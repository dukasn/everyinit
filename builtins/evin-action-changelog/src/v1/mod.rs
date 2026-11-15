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

mod action;
pub use action::ChangelogAction;

mod config;
mod parser;
use config::ChangelogConfig;
use evin_action_sdk::RunnerContext;
use evin_flow::template::pipeline::resolve_templates;
use parser::{ParsedCommit, ReleaseBlock, parse_header};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
enum ChangelogError {
    #[error("{0}")]
    InvalidConfiguration(evin_flow::ConfigError),
    #[error("{}", evin_locales::t!("git-execution-failed", error = .0))]
    GitExecutionFailed(String),
    #[error(
        "{}",
        evin_locales::t!("file-write-failed", path = path, error = message)
    )]
    WriteFileFailed { path: String, message: String },
    #[error(
        "{}",
        evin_locales::t!("changelog-file-read-failed", path = path, error = message)
    )]
    ReadFileFailed { path: String, message: String },
    #[error("{0}")]
    InvalidSection(String),
}

pub fn validate_config(
    with: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    match ChangelogConfig::from_with(with.cloned()) {
        Ok(_) => Ok(()),
        Err(ChangelogError::InvalidConfiguration(error)) => Err(error),
        Err(error) => Err(evin_flow::ConfigError {
            message: error.to_string(),
            span: None,
        }),
    }
}

// This entry point implements the SDK Action contract, whose error type is
// Diagnostic. Boxing here would be immediately undone by the adapter.
#[allow(clippy::result_large_err)]
pub async fn execute(
    ctx: &RunnerContext,
    with: Option<evin_flow::SpannedValue>,
) -> Result<(), evin_diagnostic::Diagnostic> {
    generate_changelog(ctx, with)
        .await
        .map_err(|error| match error {
            ChangelogError::InvalidConfiguration(error) => {
                error.into_diagnostic(evin_locales::t!("changelog-invalid-config-title"), None)
            }
            error => {
                evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(error.to_string()))
            }
        })
}

async fn generate_changelog(
    ctx: &RunnerContext,
    with: Option<evin_flow::SpannedValue>,
) -> Result<(), ChangelogError> {
    let config: ChangelogConfig = ctx
        .load_config(with.as_ref())
        .map_err(ChangelogError::InvalidConfiguration)?;
    config.validate()?;

    if let Some(section) = &config.section {
        let section = resolve_templates(section, ctx).map_err(|error| {
            ChangelogError::InvalidSection(evin_locales::t!(
                "changelog-section-template-invalid",
                error = error.to_string()
            ))
        })?;
        if section.trim().is_empty() {
            return Err(ChangelogError::InvalidSection(evin_locales::t!(
                "changelog-section-empty"
            )));
        }
        let notes_path = config.notes_path.as_deref().expect("validated notes_path");
        let input_path = ctx
            .fs
            .get_safe_physical_path(&config.path)
            .map_err(|error| ChangelogError::ReadFileFailed {
                path: config.path.clone(),
                message: error.to_string(),
            })?;
        let output_path = ctx.fs.get_safe_physical_path(notes_path).map_err(|error| {
            ChangelogError::WriteFileFailed {
                path: notes_path.to_string(),
                message: error.to_string(),
            }
        })?;
        if input_path == output_path {
            return Err(ChangelogError::InvalidSection(evin_locales::t!(
                "changelog-section-paths-must-differ"
            )));
        }
        let content = ctx.fs.read_to_string(&config.path).map_err(|error| {
            ChangelogError::ReadFileFailed {
                path: config.path.clone(),
                message: error.to_string(),
            }
        })?;
        let slicer = evin_release_sync::ChangelogSlicer::parse(&content);
        let notes = slicer
            .get_unique_slice(&section)
            .map_err(|error| {
                ChangelogError::InvalidSection(evin_locales::t!(
                    "changelog-section-invalid",
                    error = error
                ))
            })?
            .ok_or_else(|| {
                ChangelogError::InvalidSection(evin_locales::t!(
                    "changelog-section-missing",
                    section = section.as_str()
                ))
            })?;
        ctx.fs
            .write(notes_path, format!("{}\n", notes.body))
            .map_err(|error| ChangelogError::WriteFileFailed {
                path: notes_path.to_string(),
                message: error.to_string(),
            })?;
        ctx.state.set_output("path", &config.path);
        ctx.state.set_output("notes_path", notes_path);
        return Ok(());
    }

    // 1. Fetch entire commit history
    let commits = ctx
        .git()
        .commits()
        .await
        .map_err(|e| ChangelogError::GitExecutionFailed(format!("{:?}", e)))?;
    ctx.log
        .debug_event(evin_action_sdk::debug::ChangelogAnalysis {
            commits_scanned: commits.len(),
        });

    let skip_regex = ctx
        .gitrules()
        .releases
        .as_ref()
        .and_then(|r| r.commit_message.as_deref())
        .and_then(|tpl| evin_flow::template::regex::compile_template(tpl).ok());

    // 2. Top-Down Parsing
    let mut blocks: Vec<ReleaseBlock> = Vec::new();
    let mut current_block = ReleaseBlock {
        version: if config.include_unreleased {
            "Unreleased".to_string()
        } else {
            "".to_string() // Will be ignored
        },
        commits: Vec::new(),
    };

    for commit in commits {
        let hash = &commit.hash;
        let subject = &commit.subject;
        let refs = &commit.refs;

        // Check if there's a tag in the refs
        if let Some(tag) = evin_action_sdk::git::extract_tag(refs) {
            // Push the current block if it has a valid version name
            if !current_block.version.is_empty()
                && (!current_block.commits.is_empty() || current_block.version != "Unreleased")
            {
                blocks.push(current_block);
            } else if current_block.version == "Unreleased"
                && current_block.commits.is_empty()
                && config.include_unreleased
            {
                // Push empty unreleased so it still shows if nothing is unreleased? Optional.
            }

            // Start new block
            current_block = ReleaseBlock {
                version: tag.to_string(),
                commits: Vec::new(),
            };
        }

        if let Some(re) = &skip_regex
            && re.is_match(subject)
        {
            continue;
        }

        let parsed = ctx.gitrules().parse_commit_message(subject);
        if !current_block.version.is_empty()
            && let Some(parsed) = parse_header(subject, hash, parsed)
        {
            current_block.commits.push(parsed);
        }
    }

    // Push the final block
    if !current_block.version.is_empty()
        && (!current_block.commits.is_empty() || current_block.version != "Unreleased")
    {
        blocks.push(current_block);
    }

    // 3. Format to Markdown
    let mut final_markdown = config.preamble.clone();

    let version_val = config
        .version
        .as_deref()
        .map(|version| resolve_templates(version, ctx))
        .transpose()
        .map_err(|e| {
            ChangelogError::InvalidConfiguration(evin_flow::ConfigError {
                message: evin_locales::t!(
                    "changelog-template-resolution-failed",
                    error = e.to_string()
                ),
                span: None,
            })
        })?;

    let tags_format = ctx.tag_format();

    let final_unreleased_title = version_val
        .map(|v| {
            let clean = v.trim_start_matches('v');
            let mut args = std::collections::HashMap::new();
            args.insert("version", clean);
            evin_flow::template::format::render(tags_format, &args)
        })
        .unwrap_or_else(|| config.unreleased_heading.clone());
    let label_suffix = config
        .label
        .as_deref()
        .map(|label| resolve_templates(label, ctx))
        .transpose()
        .map_err(|e| {
            ChangelogError::InvalidConfiguration(evin_flow::ConfigError {
                message: evin_locales::t!(
                    "changelog-template-resolution-failed",
                    error = e.to_string()
                ),
                span: None,
            })
        })?
        .map(|label| format!(" — {label}"))
        .unwrap_or_default();

    let mut pending_notes: Option<String> = None;
    for block in &blocks {
        if block.commits.is_empty() {
            continue;
        }

        if block.version == "Unreleased" {
            final_markdown.push_str(&format!(
                "## [{}]{}\n\n",
                final_unreleased_title, label_suffix
            ));
        } else {
            final_markdown.push_str(&format!("## [{}]\n\n", block.version));
        }

        let mut grouped: HashMap<String, Vec<&ParsedCommit>> = HashMap::new();
        for commit in &block.commits {
            let temp_parsed = evin_gitrules::models::ParsedCommit {
                commit_type: commit.commit_type.clone(),
                scope: commit.scope.clone(),
                subject: commit.subject.clone(),
                is_breaking: false, // Not used for changelog grouping
            };
            let section_name = if let Some(override_name) = config.sections.get(&commit.commit_type)
            {
                if override_name.eq_ignore_ascii_case("none") {
                    String::new()
                } else {
                    override_name.clone()
                }
            } else {
                ctx.gitrules().get_changelog_section(&temp_parsed)
            };
            if !section_name.is_empty() {
                grouped.entry(section_name).or_default().push(commit);
            }
        }

        let mut body = String::new();
        let mut sorted_headings: Vec<String> = grouped.keys().cloned().collect();
        sorted_headings.sort();

        for heading in sorted_headings {
            let commits = grouped.get(&heading).unwrap();
            body.push_str(&format!("### {}\n\n", heading));
            for c in commits {
                let mut entry = String::from("- ");
                if config.show_scopes
                    && let Some(scope) = &c.scope
                {
                    entry.push_str(&format!("**{}:** ", scope));
                }

                let entry_subject = ctx.gitrules().format_ticket_links(&c.subject);

                entry.push_str(&entry_subject);
                if config.show_hashes {
                    entry.push_str(&format!(" ({})", c.hash));
                }
                entry.push('\n');
                body.push_str(&entry);
            }
            body.push('\n');
        }
        final_markdown.push_str(&body);
        if block.version == "Unreleased" {
            pending_notes = Some(body);
        }
    }

    let mut rendered_blocks = Vec::new();
    let first_rendered = blocks.iter().find(|b| !b.commits.is_empty());
    if let Some(block) = first_rendered {
        let out_version = if block.version == "Unreleased" {
            &final_unreleased_title
        } else {
            &block.version
        };
        let clean_version = out_version.trim_start_matches('v');
        ctx.state.set_output("version", clean_version);
        ctx.state.set_output("tag", out_version);
    }

    for block in &blocks {
        if !block.commits.is_empty() {
            rendered_blocks.push(block.version.clone());
        }
    }

    if ctx.mode == evin_action_sdk::PipelineMode::DryRun {
        ctx.log.plan(evin_action_sdk::activity::ChangelogPlan {
            path: config.path.clone(),
            sections: format!("{rendered_blocks:?}"),
        });
        ctx.state.set_output("path", &config.path);
        ctx.mark_unverified(evin_locales::t!("changelog-dry-run-skipped-generation"));
        return Ok(());
    }

    // 4. Overwrite output file
    ctx.log
        .debug_event(evin_action_sdk::debug::ChangelogOutput {
            target: config.path.clone(),
            sections_generated: rendered_blocks.len(),
        });
    ctx.fs
        .write(
            &config.path,
            final_markdown.trim_end_matches('\n').to_string() + "\n",
        )
        .map_err(|e| ChangelogError::WriteFileFailed {
            path: config.path.clone(),
            message: e.to_string(),
        })?;

    ctx.state.set_output("path", &config.path);

    if let (Some(notes_path), Some(notes)) = (&config.notes_path, pending_notes) {
        ctx.fs
            .write(notes_path, notes.trim_end_matches('\n').to_string() + "\n")
            .map_err(|e| ChangelogError::WriteFileFailed {
                path: notes_path.clone(),
                message: e.to_string(),
            })?;
        ctx.state.set_output("notes_path", notes_path);
    }

    Ok(())
}

#[cfg(test)]
mod tests;
