// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

#![allow(clippy::result_large_err)]
mod action;
pub use action::VersionBumperAction;
pub mod config;
pub use config::{CustomFileConfig, FileConfig, GitConfig, VersionBumperConfig, validate_config};

use evin_action_sdk::RunnerContext;
use regex::Regex;
use semver::Version;
use std::collections::HashMap;

mod compute;
use compute::compute_version;
use config::{json_version_regex, manifest_kind, resolve_local_template, toml_version_regex};

fn file_path_error(
    key: &'static str,
    path: &str,
    error: impl std::fmt::Display,
) -> evin_diagnostic::Diagnostic {
    evin_locales::diagnostic(
        evin_diagnostic::IssueCode::new(key)
            .arg("path", path)
            .arg("error", error.to_string()),
    )
}

fn file_template_error(
    path: &str,
    field: String,
    error: impl std::fmt::Display,
) -> evin_diagnostic::Diagnostic {
    evin_locales::diagnostic(
        evin_diagnostic::IssueCode::new("version-bumper-file-template-resolution-failed")
            .arg("path", path)
            .arg("field", field)
            .arg("error", error.to_string()),
    )
}

fn fixed_version_template_error(error: impl std::fmt::Display) -> evin_diagnostic::Diagnostic {
    evin_locales::diagnostic(
        evin_diagnostic::IssueCode::new("version-bumper-fixed-version-template-resolution-failed")
            .arg("error", error.to_string()),
    )
}

fn git_message_template_error(error: impl std::fmt::Display) -> evin_diagnostic::Diagnostic {
    evin_locales::diagnostic(
        evin_diagnostic::IssueCode::new("version-bumper-git-message-template-failed")
            .arg("error", error.to_string()),
    )
}

fn release_state_error(
    operation: String,
    error: impl std::fmt::Display,
) -> evin_diagnostic::Diagnostic {
    evin_locales::diagnostic(
        evin_diagnostic::IssueCode::new("version-bumper-release-state-operation-failed")
            .arg("operation", operation)
            .arg("error", error.to_string()),
    )
}

struct ChannelVariableFailure {
    message: evin_diagnostic::IssueCode,
    hint: Option<evin_diagnostic::IssueCode>,
}

async fn resolve_channel_var(
    var_name: &str,
    ctx: &RunnerContext,
) -> Result<String, ChannelVariableFailure> {
    match var_name {
        "n" => Ok("{n}".to_string()),
        "workspace.name" => ctx
            .workspace_name
            .clone()
            .ok_or_else(|| ChannelVariableFailure {
                message: evin_diagnostic::IssueCode::new(
                    "version-bumper-workspace-name-resolution-failed",
                ),
                hint: Some(evin_diagnostic::IssueCode::new(
                    "version-bumper-workspace-name-requirement",
                )),
            }),
        "git.branch" => ctx
            .git()
            .current_branch()
            .await
            .map(|branch| {
                branch
                    .chars()
                    .map(|c| {
                        if c.is_alphanumeric() {
                            c.to_ascii_lowercase()
                        } else {
                            '-'
                        }
                    })
                    .collect()
            })
            .map_err(|_| ChannelVariableFailure {
                message: evin_diagnostic::IssueCode::new(
                    "version-bumper-git-branch-resolution-failed",
                ),
                hint: Some(evin_diagnostic::IssueCode::new(
                    "version-bumper-git-branch-requirement",
                )),
            }),
        "git.short_sha" => ctx
            .git()
            .short_sha()
            .await
            .map_err(|_| ChannelVariableFailure {
                message: evin_diagnostic::IssueCode::new(
                    "version-bumper-git-short-sha-resolution-failed",
                ),
                hint: Some(evin_diagnostic::IssueCode::new(
                    "version-bumper-git-short-sha-requirement",
                )),
            }),
        "date.ymd" => Ok(chrono::Local::now().format("%Y%m%d").to_string()),
        s if s.starts_with("env.") => {
            let env_name = &s[4..];
            let env_val = std::env::var(env_name).unwrap_or_else(|_| "".to_string());
            if env_val.is_empty() {
                Err(ChannelVariableFailure {
                    message: evin_diagnostic::IssueCode::new(
                        "version-bumper-environment-variable-missing",
                    ),
                    hint: Some(
                        evin_diagnostic::IssueCode::new(
                            "version-bumper-environment-variable-required",
                        )
                        .arg("name", env_name),
                    ),
                })
            } else {
                Ok(env_val)
            }
        }
        _ => Err(ChannelVariableFailure {
            message: evin_diagnostic::IssueCode::new("version-bumper-channel-variable-unknown")
                .arg("name", var_name),
            hint: None,
        }),
    }
}

async fn hydrate_channel(
    ctx: &RunnerContext,
    _channel_name: &str,
    template: &str,
) -> Result<String, evin_diagnostic::Diagnostic> {
    let mut hydrated = String::new();
    let mut chars = template.char_indices().peekable();
    let mut missing_vars = Vec::new();

    while let Some((i, c)) = chars.next() {
        if c == '{' {
            let mut var_content = String::new();
            let mut end_idx = i;
            while let Some(&(ni, nc)) = chars.peek() {
                end_idx = ni;
                if nc == '}' {
                    chars.next();
                    break;
                }
                var_content.push(nc);
                chars.next();
            }

            match resolve_channel_var(&var_content, ctx).await {
                Ok(val) => hydrated.push_str(&val),
                Err(failure) => {
                    if failure.hint.is_none() {
                        return Err(evin_locales::diagnostic(failure.message));
                    }
                    missing_vars.push((var_content, i, end_idx - i + 1, failure));
                }
            }
        } else {
            hydrated.push(c);
        }
    }

    if let Some((_missing_name, span_start, span_length, failure)) = missing_vars.first() {
        if let Ok(source) = std::fs::read_to_string(".evin/gitrules.flow") {
            // Find the offset of this string in the file
            if let Some(file_offset) = source.find(template) {
                let diagnostic = evin_locales::diagnostic(failure.message.clone())
                    .with_hint(evin_locales::render_issue(
                        failure.hint.as_ref().expect("hinted variable failures"),
                    ))
                    .with_file_span(
                        ".evin/gitrules.flow",
                        file_offset + span_start,
                        *span_length,
                        Some(evin_locales::t!(
                            "version-bumper-channel-variable-location-hint"
                        )),
                    );
                ctx.log.diagnostic(diagnostic.clone());
                return Err(diagnostic);
            }
        }
        return Err(evin_locales::diagnostic(failure.message.clone()).with_hint(
            evin_locales::render_issue(failure.hint.as_ref().expect("hinted variable failures")),
        ));
    }

    Ok(hydrated)
}

pub async fn execute(
    ctx: &RunnerContext,
    with: Option<evin_flow::SpannedValue>,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let mut config: VersionBumperConfig = ctx.load_config(with.as_ref()).map_err(|error| {
        error.into_diagnostic(
            evin_locales::t!("version-bumper-invalid-config-title"),
            None,
        )
    })?;
    if let Some(fixed_version) = &config.fixed_version {
        let resolved = resolve_local_template(fixed_version, ctx, &HashMap::new())
            .map_err(fixed_version_template_error)?;
        Version::parse(&resolved).map_err(|error| {
            evin_locales::t!(
                "version-bumper-fixed-version-resolution-invalid",
                error = error.to_string()
            )
        })?;
        config.fixed_version = Some(resolved);
    }
    config.validate()?;
    let package_policies = ctx
        .gitrules()
        .releases
        .as_ref()
        .map(|release| release.packages.clone())
        .unwrap_or_default();
    evin_action_sdk::release::validate_policies(&package_policies).map_err(|error| {
        evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("version-bumper-package-policy-invalid")
                .arg("error", error.to_string()),
        )
    })?;
    if !package_policies.is_empty() && !config.read_only && (!config.git.commit || !config.git.tag)
    {
        return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
            "version-bumper-package-release-requires-git",
        )));
    }
    if !package_policies.is_empty()
        && !config.read_only
        && ctx.mode != evin_action_sdk::PipelineMode::DryRun
    {
        if ctx
            .git()
            .has_staged_changes()
            .await
            .map_err(evin_diagnostic::Diagnostic::from)?
        {
            return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                "version-bumper-package-release-index-not-empty",
            )));
        }
        let dirty = evin_action_sdk::release::dirty_paths(ctx.fs.workspace_root())
            .await
            .map_err(|error| {
                release_state_error(
                    evin_locales::t!("version-bumper-release-operation-dirty-paths"),
                    error,
                )
            })?
            .into_iter()
            .filter(|path| evin_action_sdk::release::path_is_watched(path, &package_policies))
            .collect::<Vec<_>>();
        if !dirty.is_empty() {
            return Err(evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("version-bumper-package-release-worktree-dirty")
                    .arg("paths", dirty.join(", ")),
            ));
        }
    }
    let commits = ctx
        .git()
        .commits()
        .await
        .map_err(evin_diagnostic::Diagnostic::from)?;

    let mut previous_version = String::new();
    let mut impacts = Vec::new();
    let mut found_tag = false;

    for commit in commits {
        if let Some(tag) = evin_action_sdk::git::extract_tag(&commit.refs) {
            let stripped = tag.trim_start_matches('v');
            if Version::parse(stripped).is_ok() {
                previous_version = stripped.to_string();
                found_tag = true;
                break;
            }
        }
        if commit
            .message
            .lines()
            .any(|line| line.trim() == evin_action_sdk::release::PACKAGE_RELEASE_TRAILER)
        {
            continue;
        }
        if let Some(parsed) = ctx.gitrules().parse_commit_message(&commit.subject) {
            let imp = ctx.gitrules().determine_bump(&parsed);
            impacts.push(imp);
        }
    }

    let (hydrated_channel, start_at_one) = if let Some(channel_name) = &config.channel {
        let prerelease_config = ctx
            .gitrules()
            .releases
            .as_ref()
            .and_then(|r| r.prerelease.as_ref());
        let template = prerelease_config
            .and_then(|p| p.channels.get(channel_name))
            .ok_or_else(|| {
                evin_locales::diagnostic(
                    evin_diagnostic::IssueCode::new("version-bumper-channel-undefined")
                        .arg("channel", channel_name),
                )
            })?;
        let start_at_one = prerelease_config.map(|p| p.start_at_one).unwrap_or(false);
        let hydrated = hydrate_channel(ctx, channel_name, template).await?;
        (Some(hydrated), start_at_one)
    } else {
        (None, false)
    };

    let mut semantic_change = !found_tag;
    let mut version = if !found_tag {
        let v = Version::parse(&config.initial_version).unwrap();
        v.to_string()
    } else {
        match compute_version(
            &previous_version,
            &impacts,
            config.stable,
            config.prerelease.as_deref(),
            hydrated_channel.as_deref(),
            start_at_one,
            ctx.gitrules()
                .releases
                .as_ref()
                .map(|release| release.versioning.strategy)
                .unwrap_or_default(),
        ) {
            Some(version) => {
                semantic_change = true;
                let max_impact = impacts
                    .iter()
                    .max()
                    .copied()
                    .unwrap_or(evin_gitrules::models::Impact::None);
                let impact_str = format!("{:?}", max_impact).to_lowercase();
                ctx.log
                    .debug_event(evin_action_sdk::debug::VersionDiscovery {
                        previous: previous_version.clone(),
                        next: version.clone(),
                        impact: impact_str,
                    });
                version
            }
            None => {
                semantic_change = false;
                previous_version.clone()
            }
        }
    };

    if config.read_only
        && let Some(fixed_version) = &config.fixed_version
    {
        semantic_change = false;
        version.clone_from(fixed_version);
    }

    if !found_tag {
        previous_version = "0.0.0".to_string();
    }

    let self_values = HashMap::from([
        ("version".to_string(), version.clone()),
        ("previous_version".to_string(), previous_version.clone()),
    ]);

    let mut args = std::collections::HashMap::new();
    args.insert("version", version.as_str());
    let version_with_v = evin_flow::template::format::render(ctx.tag_format(), &args);

    if semantic_change
        && ctx
            .git()
            .has_tag(&version_with_v)
            .await
            .map_err(evin_diagnostic::Diagnostic::from)?
    {
        return Err(evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("calculated-tag-already-exists")
                .arg("version", &version)
                .arg("tag", &version_with_v),
        ));
    }

    if config.git.commit && !config.git.defer {
        ctx.git()
            .verify_identity()
            .await
            .map_err(evin_diagnostic::Diagnostic::from)?;
    }
    if config.git.tag && semantic_change {
        ctx.git()
            .validate_tag_name(&version_with_v)
            .await
            .map_err(evin_diagnostic::Diagnostic::from)?;
    }

    ctx.log.debug_event(evin_action_sdk::debug::VersionOutput {
        count: config.files.len(),
    });
    let mut planned_updates = Vec::new();
    let mut planned_contents: HashMap<std::path::PathBuf, String> = HashMap::new();
    for file in if semantic_change && !config.read_only {
        config.files.as_slice()
    } else {
        &[]
    } {
        let (path, regex, fallback_regex, replacement) = match file {
            FileConfig::Auto(path) => {
                let kind = manifest_kind(path).expect("validated manifest");
                let regex = if kind == "json" {
                    json_version_regex().clone()
                } else {
                    toml_version_regex().clone()
                };
                let replacement = if kind == "json" {
                    format!("\"version\": \"{version}\"")
                } else {
                    format!("version = \"{version}\"")
                };
                (path.clone(), regex, None, replacement)
            }
            FileConfig::Custom(custom) => {
                let mut regex_values = self_values.clone();
                regex_values.insert(
                    "previous_version".to_string(),
                    regex::escape(&previous_version),
                );
                let pattern = resolve_local_template(&custom.match_pattern, ctx, &regex_values)
                    .map_err(|error| {
                        file_template_error(
                            &custom.path,
                            evin_locales::t!("version-bumper-file-match-field"),
                            error,
                        )
                    })?;
                let regex = Regex::new(&pattern).map_err(|error| {
                    file_template_error(
                        &custom.path,
                        evin_locales::t!("version-bumper-file-match-field"),
                        error,
                    )
                })?;
                let fallback_regex = if !found_tag
                    && custom.match_pattern.contains("${previous_version}")
                {
                    regex_values.insert("previous_version".to_string(), regex::escape(&version));
                    let pattern = resolve_local_template(&custom.match_pattern, ctx, &regex_values)
                        .map_err(|error| {
                            file_template_error(
                                &custom.path,
                                evin_locales::t!("version-bumper-file-match-field"),
                                error,
                            )
                        })?;
                    Some(Regex::new(&pattern).map_err(|error| {
                        file_template_error(
                            &custom.path,
                            evin_locales::t!("version-bumper-file-match-field"),
                            error,
                        )
                    })?)
                } else {
                    None
                };
                let replacement = resolve_local_template(&custom.write, ctx, &self_values)
                    .map_err(|error| {
                        file_template_error(
                            &custom.path,
                            evin_locales::t!("version-bumper-file-write-field"),
                            error,
                        )
                    })?;
                (custom.path.clone(), regex, fallback_regex, replacement)
            }
        };

        let physical_path = ctx
            .fs
            .get_safe_physical_path(&path)
            .map_err(|error| file_path_error("version-bumper-file-path-unusable", &path, error))?;
        let content = match planned_contents.get(&physical_path) {
            Some(content) => content.clone(),
            None => ctx.fs.read_to_string(&path).map_err(|error| {
                file_path_error("version-bumper-file-read-failed", &path, error)
            })?,
        };
        let regex = if regex.is_match(&content) {
            &regex
        } else if let Some(fallback) = fallback_regex.as_ref().filter(|r| r.is_match(&content)) {
            fallback
        } else {
            return Err(evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("version-bumper-replacement-missing")
                    .arg("path", path),
            ));
        };
        let new_content = regex.replace(&content, replacement.as_str()).to_string();
        if content != new_content {
            planned_contents.insert(physical_path, new_content.clone());
            planned_updates.push((path, new_content));
        }
    }

    let mut modified_files = Vec::new();
    for (path, new_content) in planned_updates {
        if ctx.mode == evin_action_sdk::PipelineMode::DryRun {
            ctx.log.plan(evin_action_sdk::activity::VersionUpdatePlan {
                version: version.clone(),
                path: path.clone(),
            });
        } else {
            ctx.fs
                .write(&path, new_content.as_bytes())
                .map_err(|error| {
                    file_path_error("version-bumper-file-write-failed", &path, error)
                })?;
        }
        modified_files.push(path);
    }

    let mut lock_changed = false;
    let mut release_json = None;
    if !package_policies.is_empty() {
        if !config.read_only && ctx.mode != evin_action_sdk::PipelineMode::DryRun {
            let mut planned = modified_files.clone();
            if semantic_change {
                planned.extend(config.git.include.clone());
            }
            if !planned.is_empty() {
                ctx.git()
                    .stage_files(&planned)
                    .await
                    .map_err(evin_diagnostic::Diagnostic::from)?;
            }
        }
        let fingerprints =
            evin_action_sdk::release::fingerprint_index(ctx.fs.workspace_root(), &package_policies)
                .await
                .map_err(|error| {
                    release_state_error(
                        evin_locales::t!("version-bumper-release-operation-fingerprinting"),
                        error,
                    )
                })?;
        let previous_lock =
            evin_action_sdk::release::read_lock(ctx.fs.workspace_root()).map_err(|error| {
                release_state_error(
                    evin_locales::t!("version-bumper-release-operation-reading-lock"),
                    error,
                )
            })?;
        let lock = evin_action_sdk::release::resolve_lock(
            previous_lock.as_ref(),
            evin_action_sdk::release::ReleaseIdentity {
                version: version.clone(),
                tag: version_with_v.clone(),
            },
            &package_policies,
            &fingerprints,
        )
        .map_err(|error| {
            release_state_error(
                evin_locales::t!("version-bumper-release-operation-resolving-revisions"),
                error,
            )
        })?;
        lock_changed = previous_lock.as_ref() != Some(&lock);
        release_json = Some(
            serde_json::to_string(&lock.artifact(&package_policies).map_err(|error| {
                release_state_error(
                    evin_locales::t!("version-bumper-release-operation-building-artifact"),
                    error,
                )
            })?)
            .map_err(|error| {
                release_state_error(
                    evin_locales::t!("version-bumper-release-operation-serializing-artifact"),
                    error,
                )
            })?,
        );
        if lock_changed && !config.read_only && ctx.mode != evin_action_sdk::PipelineMode::DryRun {
            evin_action_sdk::release::write_lock(ctx.fs.workspace_root(), &lock).map_err(
                |error| {
                    release_state_error(
                        evin_locales::t!("version-bumper-release-operation-writing-lock"),
                        error,
                    )
                },
            )?;
            ctx.git()
                .stage_files(&[evin_action_sdk::release::RELEASE_LOCK_PATH.to_string()])
                .await
                .map_err(evin_diagnostic::Diagnostic::from)?;
        }
    }

    if !semantic_change && !lock_changed {
        ctx.state.set_output("version", &version);
        ctx.state.set_output("tag", &version_with_v);
        ctx.state.set_output("changed", "false");
        if let Some(release) = release_json {
            ctx.state.set_output("release", &release);
        }
        if config.require_change {
            return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                "no-version-change",
            )));
        }
        return Ok(());
    }

    if config.git.commit && (semantic_change || lock_changed) {
        let commit_msg_template = if let Some(m) = &config.git.message {
            m.clone()
        } else if let Some(m) = ctx
            .gitrules()
            .releases
            .as_ref()
            .and_then(|r| r.commit_message.clone())
        {
            let mut args = std::collections::HashMap::new();
            args.insert("version", "${version}");
            evin_flow::template::format::render(&m, &args)
        } else {
            return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                "git-message-must-not-be-empty",
            )));
        };
        let commit_message = if semantic_change {
            resolve_local_template(&commit_msg_template, ctx, &self_values)
                .map_err(git_message_template_error)?
        } else {
            format!(
                "{}\n\n{}",
                evin_action_sdk::release::PACKAGE_RELEASE_SUBJECT,
                evin_action_sdk::release::PACKAGE_RELEASE_TRAILER
            )
        };

        if ctx.mode == evin_action_sdk::PipelineMode::DryRun {
            ctx.log.plan(evin_action_sdk::activity::VersionCommitPlan {
                message: commit_message.clone(),
            });
            if config.git.tag && semantic_change {
                ctx.log.plan(evin_action_sdk::activity::VersionTagPlan {
                    tag: version_with_v.clone(),
                });
            }
            ctx.mark_unverified(evin_locales::t!("version-bumper-dry-run-skipped-mutations"));
            ctx.state.set_output("version", &version);
            ctx.state.set_output("tag", &version_with_v);
            ctx.state
                .set_output("changed", &semantic_change.to_string());
            if let Some(release) = release_json {
                ctx.state.set_output("release", &release);
            }
            return Ok(());
        }

        if package_policies.is_empty() {
            modified_files.extend(config.git.include.clone());
            ctx.git()
                .stage_files(&modified_files)
                .await
                .map_err(evin_diagnostic::Diagnostic::from)?;
        }

        let has_staged_changes = ctx
            .git()
            .has_staged_changes()
            .await
            .map_err(evin_diagnostic::Diagnostic::from)?;
        if !has_staged_changes && (found_tag || !config.git.defer) {
            return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                "no-staged-changes",
            )));
        }

        if !config.git.defer {
            let require_signatures = ctx.require_signatures();
            ctx.git()
                .commit(&commit_message, require_signatures)
                .await
                .map_err(evin_diagnostic::Diagnostic::from)?;

            if config.git.tag && semantic_change {
                let result = ctx.git().create_tag(&version_with_v, &commit_message).await;
                if let Err(e) = result {
                    let commit_id = ctx
                        .git()
                        .get_last_commit_id()
                        .await
                        .unwrap_or_else(|_| "HEAD".to_string());
                    return Err(evin_locales::diagnostic(
                        evin_diagnostic::IssueCode::new("tag-recovery-failed")
                            .arg("commit_id", commit_id)
                            .arg("tag", &version_with_v)
                            .arg("error", e.to_string()),
                    ));
                }
            }
        }
    }

    ctx.state.set_output("version", &version);
    ctx.state.set_output("tag", &version_with_v);
    ctx.state
        .set_output("changed", &semantic_change.to_string());
    if let Some(release) = release_json {
        ctx.state.set_output("release", &release);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
