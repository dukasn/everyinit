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
use evin_action_sdk::RunnerContext;
use evin_yaml::template::pipeline::{
    resolve_templates_with_self, validate_template_syntax_with_self,
};
use regex::Regex;
use semver::{Prerelease, Version};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

mod compute;
use compute::compute_version;

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum FileConfig {
    Auto(String),
    Custom(CustomFileConfig),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct CustomFileConfig {
    pub path: String,
    #[serde(rename = "match")]
    pub match_pattern: String,
    pub write: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(default, deny_unknown_fields)]
pub struct GitConfig {
    pub commit: bool,
    pub tag: bool,
    pub message: Option<String>,
    pub include: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VersionBumperConfig {
    pub files: Vec<FileConfig>,
    pub git: GitConfig,
    pub initial_version: String,
    pub stable: bool,
    pub prerelease: Option<String>,
    pub channel: Option<String>,
    pub require_change: bool,
}

impl Default for VersionBumperConfig {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            git: GitConfig::default(),
            initial_version: "0.1.0".to_string(),
            stable: false,
            prerelease: None,
            channel: None,
            require_change: false,
        }
    }
}

impl VersionBumperConfig {
    pub fn from_with(
        with: Option<evin_yaml::SpannedValue>,
    ) -> Result<Self, evin_yaml::ConfigError> {
        let with = with.unwrap_or_else(|| {
            evin_yaml::SpannedValue::Mapping(Default::default(), evin_yaml::Span::default())
        });
        let config: Self = evin_action_sdk::parse_config(Some(&with))?;
        config.validate().map_err(|e| evin_yaml::ConfigError {
            message: e.to_string(),
            span: None,
        })?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), evin_diagnostic::Diagnostic> {
        Version::parse(&self.initial_version)
            .map_err(|e| format!("initial_version must be valid SemVer: {e}"))?;
        if self.stable && self.prerelease.is_some() {
            return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                "stable-and-prerelease-conflict",
            )));
        }
        if self.stable && self.channel.is_some() {
            return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                "stable-and-channel-conflict",
            )));
        }
        if self.prerelease.is_some() && self.channel.is_some() {
            return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                "prerelease-and-channel-conflict",
            )));
        }
        if let Some(prerelease) = &self.prerelease {
            if prerelease.trim().is_empty() {
                return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                    "prerelease-must-not-be-empty",
                )));
            }
            Prerelease::new(&format!("{prerelease}.1"))
                .map_err(|e| format!("prerelease is invalid: {e}"))?;
        }
        if self.git.tag && !self.git.commit {
            return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                "git-tag-requires-git-commit",
            )));
        }
        if let Some(msg) = &self.git.message {
            if msg.trim().is_empty() {
                return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                    "git-message-must-not-be-empty",
                )));
            }
            validate_tokens(msg, &["version"])?;
        }
        if self.git.include.iter().any(|path| path.trim().is_empty()) {
            return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                "git-include-paths-must-not-be-empty",
            )));
        }
        for file in &self.files {
            validate_file(file)?;
        }
        Ok(())
    }
}

fn validate_tokens(text: &str, allowed: &[&str]) -> Result<(), evin_diagnostic::Diagnostic> {
    validate_template_syntax_with_self(text, allowed)
        .map_err(|e| e.to_string().into())
        .and_then(|_| {
            static RE: OnceLock<Regex> = OnceLock::new();
            let regex = RE.get_or_init(|| Regex::new(r"\$\{(?P<body>[^{}]+)\}").unwrap());
            for captures in regex.captures_iter(text) {
                let body = captures.name("body").unwrap().as_str();
                if !allowed.contains(&body) {
                    return Err(evin_locales::diagnostic(
                        evin_diagnostic::IssueCode::new(
                            "issue-version-bumper-unsupported-template",
                        )
                        .arg("template", body),
                    ));
                }
            }
            Ok(())
        })
}

fn resolve_local_template(
    text: &str,
    ctx: &RunnerContext,
    values: &HashMap<String, String>,
) -> Result<String, String> {
    resolve_templates_with_self(text, ctx, values)
        .map_err(|e| e.to_string())
        .map_err(|e| e.to_string())
}

fn manifest_kind(path: &str) -> Option<&'static str> {
    match Path::new(path).file_name()?.to_str()? {
        "Cargo.toml" | "pyproject.toml" => Some("toml"),
        "package.json" => Some("json"),
        _ => None,
    }
}

fn toml_version_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?m)^version\s*=\s*["']([^"']+)["']"#).unwrap())
}

fn json_version_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?m)"version"\s*:\s*["']([^"']+)["']"#).unwrap())
}

fn validate_file(file: &FileConfig) -> Result<(), evin_diagnostic::Diagnostic> {
    match file {
        FileConfig::Auto(path) => {
            if path.trim().is_empty() {
                return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                    "file-path-must-not-be-empty",
                )));
            }
            manifest_kind(path).ok_or_else(|| {
                format!("automatic version detection is unsupported for '{path}'")
            })?;
        }
        FileConfig::Custom(config) => {
            if config.path.trim().is_empty() {
                return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                    "custom-file-path-must-not-be-empty",
                )));
            }
            if config.match_pattern.trim().is_empty() {
                return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                    "custom-file-match-must-not-be-empty",
                )));
            }
            if config.write.trim().is_empty() {
                return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                    "custom-file-write-must-not-be-empty",
                )));
            }
            validate_tokens(&config.match_pattern, &["previous_version"])?;
            validate_tokens(&config.write, &["version"])?;
            let pattern = config
                .match_pattern
                .replace("${previous_version}", &regex::escape("0.0.0"));
            Regex::new(&pattern).map_err(|e| format!("custom file match is invalid: {e}"))?;
        }
    }
    Ok(())
}

pub fn validate_config(
    with: Option<&evin_yaml::SpannedValue>,
) -> Result<(), evin_yaml::ConfigError> {
    VersionBumperConfig::from_with(with.cloned()).map(|_| ())
}

async fn resolve_channel_var(
    var_name: &str,
    ctx: &RunnerContext,
) -> Result<String, (String, String)> {
    match var_name {
        "n" => Ok("{n}".to_string()),
        "workspace.name" => ctx.workspace_name.clone().ok_or_else(|| {
            (
                "Missing workspace name".to_string(),
                "The {workspace.name} variable is required but no workspace name is configured."
                    .to_string(),
            )
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
            .map_err(|_| {
                (
                    "Failed to resolve git branch".to_string(),
                    "The {git.branch} variable requires a valid git branch.".to_string(),
                )
            }),
        "git.short_sha" => ctx.git().short_sha().await.map_err(|_| {
            (
                "Failed to resolve git short SHA".to_string(),
                "The {git.short_sha} variable requires a valid git commit.".to_string(),
            )
        }),
        "date.ymd" => Ok(chrono::Local::now().format("%Y%m%d").to_string()),
        s if s.starts_with("env.") => {
            let env_name = &s[4..];
            let env_val = std::env::var(env_name).unwrap_or_else(|_| "".to_string());
            if env_val.is_empty() {
                Err((
                    "Missing required environment variable".to_string(),
                    format!(
                        "The environment variable '{}' is required by this template but is not set.",
                        env_name
                    ),
                ))
            } else {
                Ok(env_val)
            }
        }
        _ => Err((
            format!("Unknown variable '{{{}}}' in channel template", var_name),
            String::new(),
        )),
    }
}

async fn hydrate_channel(
    ctx: &RunnerContext,
    _channel_name: &str,
    template: &str,
) -> Result<String, String> {
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
                Err((msg, help)) => {
                    if help.is_empty() {
                        return Err(msg);
                    }
                    missing_vars.push((var_content, i, end_idx - i + 1, msg, help));
                }
            }
        } else {
            hydrated.push(c);
        }
    }

    if let Some((_missing_name, span_start, span_length, msg, help_msg)) = missing_vars.first() {
        if let Ok(source) = std::fs::read_to_string(".evin/gitrules.flow") {
            // Find the offset of this string in the file
            if let Some(file_offset) = source.find(template) {
                ctx.log.diagnostic(
                    evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(msg.clone()))
                        .with_hint(help_msg.clone())
                        .with_file_span(
                            ".evin/gitrules.flow",
                            file_offset + span_start,
                            *span_length,
                            Some("missing variable here".to_string()),
                        ),
                );
            }
        }
        return Err(msg.clone());
    }

    Ok(hydrated)
}

pub async fn execute(
    ctx: &RunnerContext,
    with: Option<evin_yaml::SpannedValue>,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let config: VersionBumperConfig = ctx
        .load_config(with.as_ref())
        .map_err(|e| evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(e.to_string())))?;
    config
        .validate()
        .map_err(|e| evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(e.to_string())))?;
    let package_policies = ctx
        .gitrules()
        .releases
        .as_ref()
        .map(|release| release.packages.clone())
        .unwrap_or_default();
    evin_action_sdk::release::validate_policies(&package_policies).map_err(|e| e.to_string())?;
    if !package_policies.is_empty() && (!config.git.commit || !config.git.tag) {
        return Err(
            "package-managed releases require git.commit and git.tag to be true"
                .to_string()
                .into(),
        );
    }
    if !package_policies.is_empty() && ctx.mode != evin_action_sdk::PipelineMode::DryRun {
        if ctx
            .git()
            .has_staged_changes()
            .await
            .map_err(|e| e.to_string())?
        {
            return Err(
                "package release requires an empty Git index before generated files are staged"
                    .to_string()
                    .into(),
            );
        }
        let dirty = evin_action_sdk::release::dirty_paths(ctx.fs.workspace_root())
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|path| evin_action_sdk::release::path_is_watched(path, &package_policies))
            .collect::<Vec<_>>();
        if !dirty.is_empty() {
            return Err(format!(
                "package release requires a clean worktree and index; dirty paths: {}",
                dirty.join(", ")
            )
            .into());
        }
    }
    let commits = ctx.git().commits().await.map_err(|e| e.to_string())?;

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
                format!(
                    "Prerelease channel '{}' is not defined in gitrules.flow",
                    channel_name
                )
            })?;
        let start_at_one = prerelease_config.map(|p| p.start_at_one).unwrap_or(false);
        let hydrated = hydrate_channel(ctx, channel_name, template).await?;
        (Some(hydrated), start_at_one)
    } else {
        (None, false)
    };

    let mut semantic_change = !found_tag;
    let version = if !found_tag {
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
        ) {
            Some(version) => {
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

    if !found_tag {
        previous_version = "0.0.0".to_string();
    }

    let self_values = HashMap::from([
        ("version".to_string(), version.clone()),
        ("previous_version".to_string(), previous_version.clone()),
    ]);

    let mut args = std::collections::HashMap::new();
    args.insert("version", version.as_str());
    let version_with_v = evin_yaml::template::format::render(ctx.tag_format(), &args);

    if semantic_change && ctx.git().has_tag(&version_with_v).await.unwrap_or(false) {
        return Err(evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("calculated-tag-already-exists")
                .arg("version", &version)
                .arg("tag", &version_with_v),
        ));
    }

    if config.git.commit {
        ctx.git()
            .verify_identity()
            .await
            .map_err(|e| e.to_string())?;
    }
    if config.git.tag && semantic_change {
        ctx.git()
            .validate_tag_name(&version_with_v)
            .await
            .map_err(|e| e.to_string())?;
    }

    ctx.log.debug_event(evin_action_sdk::debug::VersionOutput {
        count: config.files.len(),
    });
    let mut modified_files = Vec::new();
    for file in if semantic_change {
        config.files.as_slice()
    } else {
        &[]
    } {
        let (path, regex, replacement) = match file {
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
                (path.clone(), regex, replacement)
            }
            FileConfig::Custom(custom) => {
                let mut regex_values = self_values.clone();
                regex_values.insert(
                    "previous_version".to_string(),
                    regex::escape(&previous_version),
                );
                let pattern = resolve_local_template(&custom.match_pattern, ctx, &regex_values)?;
                let regex = Regex::new(&pattern).map_err(|e| e.to_string())?;
                let replacement = resolve_local_template(&custom.write, ctx, &self_values)?;
                (custom.path.clone(), regex, replacement)
            }
        };

        let content = ctx.fs.read_to_string(&path).map_err(|e| e.to_string())?;
        let new_content = regex.replace(&content, replacement.as_str()).to_string();
        if content != new_content {
            if ctx.mode == evin_action_sdk::PipelineMode::DryRun {
                ctx.log.plan(evin_action_sdk::activity::VersionUpdatePlan {
                    version: version.clone(),
                    path: path.clone(),
                });
                modified_files.push(path);
            } else {
                ctx.fs
                    .write(&path, new_content.as_bytes())
                    .map_err(|e| e.to_string())?;
                modified_files.push(path);
            }
        }
    }

    let mut lock_changed = false;
    let mut release_json = None;
    if !package_policies.is_empty() {
        if ctx.mode != evin_action_sdk::PipelineMode::DryRun {
            let mut planned = modified_files.clone();
            if semantic_change {
                planned.extend(config.git.include.clone());
            }
            if !planned.is_empty() {
                ctx.git()
                    .stage_files(&planned)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        let fingerprints =
            evin_action_sdk::release::fingerprint_index(ctx.fs.workspace_root(), &package_policies)
                .await
                .map_err(|e| e.to_string())?;
        let previous_lock = evin_action_sdk::release::read_lock(ctx.fs.workspace_root())
            .map_err(|e| e.to_string())?;
        let lock = evin_action_sdk::release::resolve_lock(
            previous_lock.as_ref(),
            evin_action_sdk::release::ReleaseIdentity {
                version: version.clone(),
                tag: version_with_v.clone(),
            },
            &package_policies,
            &fingerprints,
        )
        .map_err(|e| e.to_string())?;
        lock_changed = previous_lock.as_ref() != Some(&lock);
        release_json = Some(
            serde_json::to_string(
                &lock
                    .artifact(&package_policies)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?,
        );
        if lock_changed && ctx.mode != evin_action_sdk::PipelineMode::DryRun {
            evin_action_sdk::release::write_lock(ctx.fs.workspace_root(), &lock)
                .map_err(|e| e.to_string())?;
            ctx.git()
                .stage_files(&[evin_action_sdk::release::RELEASE_LOCK_PATH.to_string()])
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    if !semantic_change && !lock_changed {
        ctx.state.set_output("version", &version);
        ctx.state.set_output("tag", &version_with_v);
        if let Some(release) = release_json {
            ctx.state.set_output("release", &release);
        }
        if config.require_change {
            return Err(evin_diagnostic::Diagnostic::new(
                "No qualifying commits or package state changes since the previous release",
                evin_diagnostic::IssueCode::new("no-version-change"),
            ));
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
            evin_yaml::template::format::render(&m, &args)
        } else {
            return Err(evin_diagnostic::Diagnostic::new(
                "Action Failed",
                evin_diagnostic::IssueCode::new("git-message-must-not-be-empty"),
            ));
        };
        let commit_message = if semantic_change {
            resolve_local_template(&commit_msg_template, ctx, &self_values)?
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
            ctx.mark_unverified("version bumping and git operations bypassed in dry-run");
            ctx.state.set_output("version", &version);
            ctx.state.set_output("tag", &version_with_v);
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
                .map_err(|e| e.to_string())?;
        }

        if !ctx.git().has_staged_changes().await.unwrap_or(false) {
            return Err(evin_diagnostic::Diagnostic::new(
                "Action Failed",
                evin_diagnostic::IssueCode::new("no-staged-changes"),
            ));
        }

        let require_signatures = ctx.require_signatures();

        ctx.git()
            .commit(&commit_message, require_signatures)
            .await
            .map_err(|e| e.to_string())?;

        if config.git.tag && semantic_change {
            let result = ctx.git().create_tag(&version_with_v, &commit_message).await;
            if let Err(e) = result {
                let commit_id = ctx
                    .git()
                    .get_last_commit_id()
                    .await
                    .unwrap_or_else(|_| "HEAD".to_string());
                return Err(evin_diagnostic::Diagnostic::new(
                    "Action Failed",
                    evin_diagnostic::IssueCode::new("tag-recovery-failed")
                        .arg("commit_id", commit_id)
                        .arg("tag", &version_with_v)
                        .arg("error", e.to_string()),
                ));
            }
        }
    }

    ctx.state.set_output("version", &version);
    ctx.state.set_output("tag", &version_with_v);
    if let Some(release) = release_json {
        ctx.state.set_output("release", &release);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use evin_action_sdk::{
        Logger, PipelineMode, RunnerContext, git::GitProvider, secrets::SecretsProvider,
    };
    use evin_gitrules::{PackageReleasePolicy, PackageRevisionPolicy};
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn git(root: &Path, args: &[&str]) {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .status()
                .unwrap()
                .success()
        );
    }

    #[tokio::test]
    async fn package_only_release_commits_lock_with_trailer_and_no_tag() {
        let temp = tempfile::TempDir::new().unwrap();
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "Test"][..],
            &["config", "user.email", "test@example.test"][..],
            &["config", "commit.gpgSign", "false"][..],
            &["config", "tag.gpgSign", "false"][..],
        ] {
            git(temp.path(), args);
        }
        std::fs::create_dir(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src/app"), "one\n").unwrap();
        git(temp.path(), &["add", "."]);
        git(temp.path(), &["commit", "-qm", "feat: initial"]);
        git(temp.path(), &["tag", "-a", "v1.0.0", "-m", "v1.0.0"]);
        std::fs::write(temp.path().join("src/app"), "two\n").unwrap();
        git(temp.path(), &["add", "src/app"]);
        git(temp.path(), &["commit", "-qm", "ci: rebuild package"]);

        let secrets = Arc::new(SecretsProvider::default());
        let logger = Logger::new(
            "bump".into(),
            tokio::sync::mpsc::unbounded_channel().0,
            secrets.clone(),
        );
        let provider = Arc::new(GitProvider::new(
            temp.path().to_path_buf(),
            PipelineMode::Normal,
            logger,
        ));
        let mut ctx = RunnerContext::new(
            None,
            "bump".into(),
            false,
            PipelineMode::Normal,
            tokio::sync::mpsc::unbounded_channel().0,
            vec![],
            None,
            HashMap::new(),
            temp.path().to_path_buf(),
            HashMap::new(),
            provider,
            secrets,
        );
        let mut rules = evin_gitrules::GitRulesRoot::default();
        rules.releases.as_mut().unwrap().packages = BTreeMap::from([(
            "app".into(),
            PackageReleasePolicy {
                watch: vec!["src/**".into()],
                targets: vec!["deb".into()],
                revision: PackageRevisionPolicy::default(),
            },
        )]);
        ctx.gitrules = Arc::new(rules);
        let with =
            evin_yaml::parse_yaml("initial_version: 1.0.0\ngit:\n  commit: true\n  tag: true\n")
                .unwrap();
        execute(&ctx, Some(with)).await.unwrap();

        let subject = std::process::Command::new("git")
            .args(["log", "-1", "--format=%s%n%b"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        let message = String::from_utf8_lossy(&subject.stdout);
        assert!(message.contains(evin_action_sdk::release::PACKAGE_RELEASE_SUBJECT));
        assert!(message.contains(evin_action_sdk::release::PACKAGE_RELEASE_TRAILER));
        let tags = std::process::Command::new("git")
            .args(["tag", "--list"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&tags.stdout).trim(), "v1.0.0");
        assert!(
            temp.path()
                .join(evin_action_sdk::release::RELEASE_LOCK_PATH)
                .exists()
        );
        assert!(ctx.state.outputs.lock().unwrap().contains_key("release"));
    }
}
