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

use evin_action_sdk::RunnerContext;
use evin_yaml::template::pipeline::{resolve_templates, validate_template_syntax};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;
use thiserror::Error;

fn default_path() -> String {
    "CHANGELOG.md".to_string()
}

fn default_include_unreleased() -> bool {
    true
}

fn default_preamble() -> String {
    "# Changelog\nAll notable changes to this project will be documented in this file.\n\n"
        .to_string()
}

fn default_show_hashes() -> bool {
    true
}

fn default_show_scopes() -> bool {
    true
}

fn default_unreleased_heading() -> String {
    "Unreleased".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChangelogConfig {
    #[serde(default = "default_path")]
    path: String,
    #[serde(default = "default_include_unreleased")]
    include_unreleased: bool,
    #[serde(default = "default_preamble")]
    preamble: String,
    #[serde(default = "default_show_hashes")]
    show_hashes: bool,
    #[serde(default = "default_show_scopes")]
    show_scopes: bool,
    #[serde(default = "default_unreleased_heading")]
    unreleased_heading: String,
    #[serde(default)]
    sections: HashMap<String, String>,
    #[serde(default)]
    version: Option<String>,
}

impl Default for ChangelogConfig {
    fn default() -> Self {
        Self {
            path: default_path(),
            include_unreleased: default_include_unreleased(),
            preamble: default_preamble(),
            show_hashes: default_show_hashes(),
            show_scopes: default_show_scopes(),
            unreleased_heading: default_unreleased_heading(),
            sections: HashMap::new(),
            version: None,
        }
    }
}

impl ChangelogConfig {
    fn from_with(with: Option<evin_yaml::SpannedValue>) -> Result<Self, ChangelogError> {
        let config: Self = evin_action_sdk::parse_config(with.as_ref()).map_err(|e| {
            ChangelogError::InvalidConfiguration(evin_yaml::ConfigError {
                message: e.to_string(),
                span: None,
            })
        })?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ChangelogError> {
        if self.path.trim().is_empty() {
            return Err(ChangelogError::InvalidConfiguration(
                evin_yaml::ConfigError {
                    message: evin_locales::t!("changelog-path-must-not-be-empty"),
                    span: None,
                },
            ));
        }
        if self.unreleased_heading.trim().is_empty() {
            return Err(ChangelogError::InvalidConfiguration(
                evin_yaml::ConfigError {
                    message: evin_locales::t!("unreleased-heading-must-not-be-empty"),
                    span: None,
                },
            ));
        }
        if self.sections.keys().any(|key| key.trim().is_empty()) {
            return Err(ChangelogError::InvalidConfiguration(
                evin_yaml::ConfigError {
                    message: evin_locales::t!("sections-keys-must-not-be-empty"),
                    span: None,
                },
            ));
        }
        if let Some(version) = &self.version {
            let is_empty = version.trim().is_empty();
            if is_empty {
                return Err(ChangelogError::InvalidConfiguration(
                    evin_yaml::ConfigError {
                        message: evin_locales::t!("version-must-not-be-empty"),
                        span: None,
                    },
                ));
            }
            validate_template_syntax(version).map_err(|e| {
                ChangelogError::InvalidConfiguration(evin_yaml::ConfigError {
                    message: evin_locales::t!(
                        "issue-changelog-version-invalid",
                        error = e.to_string()
                    ),
                    span: None,
                })
            })?;
        } else if !self.include_unreleased {
            return Err(ChangelogError::InvalidConfiguration(
                evin_yaml::ConfigError {
                    message: evin_locales::t!(
                        "version-is-required-when-include-unreleased-is-false"
                    ),
                    span: None,
                },
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
enum ChangelogError {
    #[error("{0}")]
    InvalidConfiguration(evin_yaml::ConfigError),
    #[error("{}", evin_locales::t!("git-execution-failed", error = .0))]
    GitExecutionFailed(String),
    #[error(
        "{}",
        evin_locales::t!("file-write-failed", path = path, error = message)
    )]
    WriteFileFailed { path: String, message: String },
}

#[derive(Debug)]
struct ParsedCommit {
    commit_type: String,
    scope: Option<String>,
    subject: String,
    hash: String,
}

#[derive(Debug)]
struct ReleaseBlock {
    version: String,
    commits: Vec<ParsedCommit>,
}

pub fn validate_config(
    with: Option<&evin_yaml::SpannedValue>,
) -> Result<(), evin_yaml::ConfigError> {
    ChangelogConfig::from_with(with.cloned())
        .map(|_| ())
        .map_err(|e| evin_yaml::ConfigError {
            message: e.to_string(),
            span: None,
        })
}

pub async fn execute(
    ctx: &RunnerContext,
    with: Option<evin_yaml::SpannedValue>,
) -> Result<(), evin_diagnostic::Diagnostic> {
    generate_changelog(ctx, with)
        .await
        .map_err(|e| e.to_string().into())
}

async fn generate_changelog(
    ctx: &RunnerContext,
    with: Option<evin_yaml::SpannedValue>,
) -> Result<(), ChangelogError> {
    let config: ChangelogConfig = ctx
        .load_config(with.as_ref())
        .map_err(ChangelogError::InvalidConfiguration)?;
    config.validate()?;

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
        .and_then(|tpl| evin_yaml::template::regex::compile_template(tpl).ok());

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
            ChangelogError::InvalidConfiguration(evin_yaml::ConfigError {
                message: e.to_string(),
                span: None,
            })
        })?;

    let tags_format = ctx.tag_format();

    let final_unreleased_title = version_val
        .map(|v| {
            let clean = v.trim_start_matches('v');
            let mut args = std::collections::HashMap::new();
            args.insert("version", clean);
            evin_yaml::template::format::render(tags_format, &args)
        })
        .unwrap_or_else(|| config.unreleased_heading.clone());

    for block in &blocks {
        if block.commits.is_empty() {
            continue;
        }

        if block.version == "Unreleased" {
            final_markdown.push_str(&format!("## [{}]\n\n", final_unreleased_title));
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

        let mut sorted_headings: Vec<String> = grouped.keys().cloned().collect();
        sorted_headings.sort();

        for heading in sorted_headings {
            let commits = grouped.get(&heading).unwrap();
            final_markdown.push_str(&format!("### {}\n\n", heading));
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
                final_markdown.push_str(&entry);
            }
            final_markdown.push('\n');
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
        ctx.mark_unverified("changelog generation bypassed in dry-run");
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

    Ok(())
}

fn parse_header(
    subject: &str,
    hash: &str,
    parsed: Option<evin_gitrules::models::ParsedCommit>,
) -> Option<ParsedCommit> {
    if let Some(p) = parsed {
        return Some(ParsedCommit {
            commit_type: p.commit_type,
            scope: p.scope,
            subject: p.subject,
            hash: hash.to_string(),
        });
    }

    static HEADER_REGEX: OnceLock<Regex> = OnceLock::new();
    let default_regex = HEADER_REGEX.get_or_init(|| {
        Regex::new(
            r"^(?<type>[a-zA-Z]+)(?:\((?<scope>[a-zA-Z0-9-]+)\))?(?<breaking>!)?: (?<subject>.+)$",
        )
        .expect("commit header regex is valid")
    });

    if let Some(captures) = default_regex.captures(subject) {
        let commit_type = captures.name("type")?.as_str().to_string();
        let scope = captures.name("scope").map(|s| s.as_str().to_string());
        let clean_subject = captures.name("subject")?.as_str().to_string();

        Some(ParsedCommit {
            commit_type,
            scope,
            subject: clean_subject,
            hash: hash.to_string(),
        })
    } else {
        None
    }
}
