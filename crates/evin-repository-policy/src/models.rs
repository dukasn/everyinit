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

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum FormatList {
    Single(String),
    Multiple(Vec<String>),
}

impl FormatList {
    pub fn iter(&self) -> impl Iterator<Item = &String> {
        match self {
            FormatList::Single(s) => std::slice::from_ref(s).iter(),
            FormatList::Multiple(v) => v.iter(),
        }
    }
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct RepositoryPolicy {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub name: String,
    pub description: Option<String>,
    pub commits: Option<CommitsNamespace>,
    pub branches: Option<BranchesNamespace>,
    pub releases: Option<ReleasesNamespace>,
    pub authorship: Option<AuthorshipNamespace>,
    pub history: Option<HistoryNamespace>,
    pub tags: Option<TagsNamespace>,
    pub pull_requests: Option<PullRequestsNamespace>,
    pub files: Option<FilesNamespace>,
    pub lfs: Option<LfsNamespace>,
    pub submodules: Option<SubmodulesNamespace>,
    pub remotes: Option<RemotesNamespace>,
}

impl Default for RepositoryPolicy {
    fn default() -> Self {
        Self {
            schema: "https://dl.everyinit.dev/schema/v1/policy".to_string(),
            name: "Default Repository Policy".to_string(),
            description: Some(
                "Provides standard conventional commits and safety defaults.".to_string(),
            ),
            commits: Some(CommitsNamespace::default()),
            branches: Some(BranchesNamespace::default()),
            releases: Some(ReleasesNamespace::default()),
            authorship: Some(AuthorshipNamespace::default()),
            history: Some(HistoryNamespace::default()),
            tags: Some(TagsNamespace::default()),
            pull_requests: Some(PullRequestsNamespace::default()),
            files: Some(FilesNamespace::default()),
            lfs: Some(LfsNamespace::default()),
            submodules: Some(SubmodulesNamespace::default()),
            remotes: Some(RemotesNamespace::default()),
        }
    }
}
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParsedCommit {
    pub commit_type: String,
    pub scope: Option<String>,
    pub subject: String,
    pub is_breaking: bool,
}

impl RepositoryPolicy {
    pub fn parse_commit_message(&self, message: &str) -> Option<ParsedCommit> {
        let commits_ns = self.commits.as_ref()?;
        let format_list = commits_ns.message_format.as_ref()?;

        let subject_line = message
            .lines()
            .find(|l| !l.is_empty() && !l.starts_with('#'))
            .unwrap_or(message);

        for format in format_list.iter() {
            let compiled = match evin_flow::template::regex::compile_template(format) {
                Ok(c) => c,
                Err(_) => continue,
            };

            if let Some(captures) = compiled.captures(subject_line) {
                let commit_type = captures
                    .name("type")
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                let scope = captures.name("scope").map(|m| m.as_str().to_string());
                let clean_subject = captures
                    .name("description")
                    .or_else(|| captures.name("subject"))
                    .map(|s| s.as_str().to_string())
                    .unwrap_or_else(|| subject_line.to_string());
                let is_breaking = captures.name("breaking").is_some()
                    || subject_line.starts_with("BREAKING CHANGE");

                return Some(ParsedCommit {
                    commit_type,
                    scope,
                    subject: clean_subject,
                    is_breaking,
                });
            }
        }
        None
    }

    /// Evaluates a parsed commit against configured change types
    /// to determine its semantic versioning impact.
    pub fn determine_bump(&self, commit: &ParsedCommit) -> Impact {
        if let Some(releases) = &self.releases {
            if commit.is_breaking
                && releases
                    .policies
                    .as_ref()
                    .and_then(|p| p.breaking_change_bump.as_ref())
                    == Some(&VersionBump::Major)
            {
                return Impact::Major;
            }
            if let Some(change_type) = releases.change_types.get(&commit.commit_type) {
                return match change_type.bump {
                    VersionBump::Major => Impact::Major,
                    VersionBump::Minor => Impact::Minor,
                    VersionBump::Patch => Impact::Patch,
                    VersionBump::None => Impact::None,
                };
            }
        }

        // Core-managed fallback strategy
        if commit.is_breaking {
            Impact::Major
        } else if commit.commit_type == "feat" {
            Impact::Minor
        } else if commit.commit_type == "fix"
            || commit.commit_type == "chore"
            || commit.commit_type == "refactor"
            || commit.commit_type == "perf"
        {
            Impact::Patch
        } else {
            Impact::None
        }
    }

    /// Maps a commit type to its human-readable changelog section heading.
    pub fn get_changelog_section(&self, commit: &ParsedCommit) -> String {
        self.releases
            .as_ref()
            .and_then(|r| r.change_types.get(&commit.commit_type))
            .and_then(|cfg| cfg.changelog_section.clone())
            .unwrap_or_else(|| {
                if commit.commit_type == "feat" {
                    "Features".to_string()
                } else if commit.commit_type == "fix" {
                    "Bug Fixes".to_string()
                } else {
                    "Other Changes".to_string()
                }
            })
    }

    /// Parses text to automatically turn ticket references into markdown links.
    pub fn format_ticket_links(&self, text: &str) -> String {
        let Some(releases) = &self.releases else {
            return text.to_string();
        };
        let Some(changelog) = &releases.changelog else {
            return text.to_string();
        };
        let Some(ticket_linking) = &changelog.ticket_linking else {
            return text.to_string();
        };

        let Some(prefix) = &ticket_linking.prefix else {
            return text.to_string();
        };
        let Some(url_template) = &ticket_linking.url_template else {
            return text.to_string();
        };

        if let Ok(re) = regex::Regex::new(&format!(
            r"({})(?P<id>[a-zA-Z0-9\-]+)",
            regex::escape(prefix)
        )) {
            return re
                .replace_all(text, |caps: &regex::Captures<'_>| {
                    let ticket = caps.get(0).map_or("", |m| m.as_str());
                    let url = url_template.replace("{ticket}", ticket);
                    format!("[{ticket}]({url})")
                })
                .to_string();
        }
        text.to_string()
    }
}

#[derive(
    schemars::JsonSchema, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum Impact {
    None = 0,
    Patch = 1,
    Minor = 2,
    Major = 3,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct CommitsNamespace {
    pub message_format: Option<FormatList>,
    pub ignore_patterns: Vec<String>,
    pub fields: HashMap<String, FieldConstraint>,
    pub policies: Option<CommitPolicies>,
}

impl Default for CommitsNamespace {
    fn default() -> Self {
        let mut fields = HashMap::new();
        fields.insert(
            "type".to_string(),
            FieldConstraint {
                must_be: None,
                allowed_values: Some(vec![
                    "feat".to_string(),
                    "fix".to_string(),
                    "chore".to_string(),
                    "docs".to_string(),
                    "refactor".to_string(),
                    "perf".to_string(),
                    "test".to_string(),
                    "ci".to_string(),
                    "build".to_string(),
                ]),
                pattern: None,
                max_length: None,
            },
        );
        fields.insert(
            "scope".to_string(),
            FieldConstraint {
                must_be: Some("lowercase-with-hyphens".to_string()),
                allowed_values: None,
                pattern: None,
                max_length: None,
            },
        );

        Self {
            message_format: Some(FormatList::Multiple(vec![
                "{type}({scope}?){breaking?}: {description}".to_string(),
                "{type}: {description}".to_string(),
            ])),
            ignore_patterns: vec!["^Merge branch".to_string(), "^Revert".to_string()],
            fields,
            policies: Some(CommitPolicies {
                require_body: Some(false),
                max_body_line_length: Some(100),
                allow_empty_commits: Some(false),
                require_ticket_reference: Some(false),
            }),
        }
    }
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FieldConstraint {
    pub must_be: Option<String>,
    pub allowed_values: Option<Vec<String>>,
    pub pattern: Option<String>,
    pub max_length: Option<usize>,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CommitPolicies {
    pub require_body: Option<bool>,
    pub max_body_line_length: Option<usize>,
    pub allow_empty_commits: Option<bool>,
    pub require_ticket_reference: Option<bool>,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct BranchesNamespace {
    pub name_format: Option<FormatList>,
    pub fields: HashMap<String, FieldConstraint>,
    pub policies: Option<BranchPolicies>,
}

impl Default for BranchesNamespace {
    fn default() -> Self {
        let mut fields = HashMap::new();
        fields.insert(
            "type".to_string(),
            FieldConstraint {
                must_be: None,
                allowed_values: Some(vec![
                    "feat".to_string(),
                    "fix".to_string(),
                    "chore".to_string(),
                    "docs".to_string(),
                    "refactor".to_string(),
                    "test".to_string(),
                ]),
                pattern: None,
                max_length: None,
            },
        );
        fields.insert(
            "ticket".to_string(),
            FieldConstraint {
                must_be: Some("alphanumeric-with-hyphens".to_string()),
                allowed_values: None,
                pattern: None,
                max_length: None,
            },
        );

        Self {
            name_format: Some(FormatList::Multiple(vec![
                "{type}/{ticket}-{description}".to_string(),
                "{type}/{description}".to_string(),
            ])),
            fields,
            policies: Some(BranchPolicies {
                block_push_to_main: Some(false),
            }),
        }
    }
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct PrereleaseConfig {
    pub start_at_one: bool,
    pub channels: HashMap<String, String>,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BranchPolicies {
    pub block_push_to_main: Option<bool>,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct ReleasesNamespace {
    pub change_types: HashMap<String, ChangeTypeDefinition>,
    pub versioning: VersioningConfig,
    pub changelog: Option<ChangelogConfig>,
    pub policies: Option<ReleasePolicies>,
    pub prerelease: Option<PrereleaseConfig>,
    pub commit_message: Option<String>,
    #[serde(default)]
    pub packages: BTreeMap<String, PackageReleasePolicy>,
}

impl Default for ReleasesNamespace {
    fn default() -> Self {
        let mut change_types = HashMap::new();
        change_types.insert(
            "feat".to_string(),
            ChangeTypeDefinition {
                bump: VersionBump::Minor,
                changelog_section: Some("Features".to_string()),
            },
        );
        change_types.insert(
            "fix".to_string(),
            ChangeTypeDefinition {
                bump: VersionBump::Patch,
                changelog_section: Some("Bug Fixes".to_string()),
            },
        );
        change_types.insert(
            "perf".to_string(),
            ChangeTypeDefinition {
                bump: VersionBump::Patch,
                changelog_section: Some("Performance Improvements".to_string()),
            },
        );
        change_types.insert(
            "docs".to_string(),
            ChangeTypeDefinition {
                bump: VersionBump::Patch,
                changelog_section: Some("Documentation".to_string()),
            },
        );
        change_types.insert(
            "refactor".to_string(),
            ChangeTypeDefinition {
                bump: VersionBump::Patch,
                changelog_section: Some("Refactoring".to_string()),
            },
        );
        change_types.insert(
            "chore".to_string(),
            ChangeTypeDefinition {
                bump: VersionBump::Patch,
                changelog_section: Some("Miscellaneous".to_string()),
            },
        );
        change_types.insert(
            "test".to_string(),
            ChangeTypeDefinition {
                bump: VersionBump::None,
                changelog_section: Some("Miscellaneous".to_string()),
            },
        );
        change_types.insert(
            "ci".to_string(),
            ChangeTypeDefinition {
                bump: VersionBump::None,
                changelog_section: Some("Miscellaneous".to_string()),
            },
        );
        change_types.insert(
            "build".to_string(),
            ChangeTypeDefinition {
                bump: VersionBump::Patch,
                changelog_section: Some("Miscellaneous".to_string()),
            },
        );

        let mut channels = HashMap::new();
        channels.insert("alpha".to_string(), "alpha.{n}".to_string());
        channels.insert("beta".to_string(), "beta.{n}".to_string());
        channels.insert("rc".to_string(), "rc.{n}".to_string());
        channels.insert("canary".to_string(), "canary.{n}".to_string());
        channels.insert("dev".to_string(), "dev.{n}".to_string());
        channels.insert("preview".to_string(), "preview.{n}".to_string());
        channels.insert("nightly".to_string(), "nightly.{n}".to_string());
        channels.insert("test".to_string(), "test.{n}".to_string());

        Self {
            change_types,
            versioning: VersioningConfig::default(),
            changelog: Some(ChangelogConfig {
                ticket_linking: None,
                host_integration: None,
            }),
            policies: Some(ReleasePolicies {
                breaking_change_bump: Some(VersionBump::Major),
                pre_release_prefix: Some("rc".to_string()),
            }),
            prerelease: Some(PrereleaseConfig {
                start_at_one: false,
                channels,
            }),
            commit_message: Some("chore(release): {version}".to_string()),
            packages: BTreeMap::new(),
        }
    }
}

#[derive(
    schemars::JsonSchema, Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq,
)]
#[serde(rename_all = "lowercase")]
pub enum VersioningStrategy {
    #[default]
    Semver,
    Cargo,
}

#[derive(schemars::JsonSchema, Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct VersioningConfig {
    pub strategy: VersioningStrategy,
}

#[derive(
    schemars::JsonSchema, Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq,
)]
#[serde(rename_all = "lowercase")]
pub enum PackageRevisionScope {
    #[default]
    Package,
    Target,
}

#[derive(schemars::JsonSchema, Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct PackageRevisionPolicy {
    pub scope: PackageRevisionScope,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackageReleasePolicy {
    pub watch: Vec<String>,
    pub targets: Vec<String>,
    #[serde(default)]
    pub revision: PackageRevisionPolicy,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
pub enum VersionBump {
    Major,
    Minor,
    Patch,
    None,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ChangeTypeDefinition {
    pub bump: VersionBump,
    pub changelog_section: Option<String>,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
#[derive(Default)]
pub struct TicketLinking {
    pub prefix: Option<String>,
    pub url_template: Option<String>,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ChangelogConfig {
    pub ticket_linking: Option<TicketLinking>,
    pub host_integration: Option<String>,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReleasePolicies {
    pub breaking_change_bump: Option<VersionBump>,
    pub pre_release_prefix: Option<String>,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct AuthorshipNamespace {
    pub allowed_emails: Vec<String>,
    pub policies: Option<AuthorshipPolicies>,
}

impl Default for AuthorshipNamespace {
    fn default() -> Self {
        Self {
            allowed_emails: vec![],
            policies: Some(AuthorshipPolicies {
                require_signatures: Some(false),
                block_identity_forging: Some(false),
            }),
        }
    }
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AuthorshipPolicies {
    pub require_signatures: Option<bool>,
    pub block_identity_forging: Option<bool>,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct HistoryNamespace {
    pub content_blocks: Vec<ContentBlock>,
    pub policies: Option<HistoryPolicies>,
}

impl Default for HistoryNamespace {
    fn default() -> Self {
        Self {
            content_blocks: vec![],
            policies: Some(HistoryPolicies {
                require_linear_history: Some(false),
                max_staged_file_size: Some("50MB".to_string()),
            }),
        }
    }
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ContentBlock {
    pub pattern: String,
    pub error_message: String,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HistoryPolicies {
    pub require_linear_history: Option<bool>,
    pub max_staged_file_size: Option<String>,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct TagsNamespace {
    pub name_format: Option<FormatList>,
    pub policies: Option<TagPolicies>,
}

impl Default for TagsNamespace {
    fn default() -> Self {
        Self {
            name_format: Some(FormatList::Single("v{version}".to_string())),
            policies: Some(TagPolicies {
                require_annotated: Some(true),
                block_deletion: Some(true),
            }),
        }
    }
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TagPolicies {
    pub require_annotated: Option<bool>,
    pub block_deletion: Option<bool>,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct PullRequestsNamespace {
    pub title_format: Option<FormatList>,
    pub policies: Option<PullRequestPolicies>,
}

impl Default for PullRequestsNamespace {
    fn default() -> Self {
        Self {
            title_format: Some(FormatList::Single(
                "{type}({scope}?): {description}".to_string(),
            )),
            policies: Some(PullRequestPolicies {
                require_assignee: Some(false),
                require_linked_issue: Some(false),
                block_wip_prefixes: vec!["WIP:".to_string(), "DRAFT:".to_string()],
            }),
        }
    }
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PullRequestPolicies {
    pub require_assignee: Option<bool>,
    pub require_linked_issue: Option<bool>,
    #[serde(default)]
    pub block_wip_prefixes: Vec<String>,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct FilesNamespace {
    pub naming_convention: Option<String>,
    pub ignore: Vec<String>,
    pub content_rules: Vec<ContentRule>,
    pub policies: Option<FilesPolicies>,
}

impl Default for FilesNamespace {
    fn default() -> Self {
        Self {
            naming_convention: None,
            ignore: vec![],
            content_rules: vec![],
            policies: Some(FilesPolicies {
                block_paths: vec![],
            }),
        }
    }
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ContentRule {
    pub name: String,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    pub require_pattern: Option<String>,
    pub forbid_pattern: Option<String>,
    pub error_message: String,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilesPolicies {
    #[serde(default)]
    pub block_paths: Vec<String>,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct LfsNamespace {
    pub policies: Option<LfsPolicies>,
}

impl Default for LfsNamespace {
    fn default() -> Self {
        Self {
            policies: Some(LfsPolicies {
                require_tracking_for: vec![
                    ".psd".to_string(),
                    ".mp4".to_string(),
                    ".zip".to_string(),
                    ".tar.gz".to_string(),
                    ".mov".to_string(),
                ],
                require_lfs_installed: Some(false),
            }),
        }
    }
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LfsPolicies {
    #[serde(default)]
    pub require_tracking_for: Vec<String>,
    pub require_lfs_installed: Option<bool>,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct SubmodulesNamespace {
    pub policies: Option<SubmodulePolicies>,
}

impl Default for SubmodulesNamespace {
    fn default() -> Self {
        Self {
            policies: Some(SubmodulePolicies {
                allow_creation: Some(false),
                allowed_hosts: vec![],
            }),
        }
    }
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SubmodulePolicies {
    pub allow_creation: Option<bool>,
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(default)]
pub struct RemotesNamespace {
    pub policies: Option<RemotePolicies>,
}

impl Default for RemotesNamespace {
    fn default() -> Self {
        Self {
            policies: Some(RemotePolicies {
                allowed_push_remotes: vec!["origin".to_string()],
                block_force_push: vec!["main".to_string(), "master".to_string()],
                require_up_to_date: Some(true),
            }),
        }
    }
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RemotePolicies {
    #[serde(default)]
    pub allowed_push_remotes: Vec<String>,
    #[serde(default)]
    pub block_force_push: Vec<String>,
    pub require_up_to_date: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_commit_message_conventional() {
        let rules = RepositoryPolicy::default();
        let parsed = rules
            .parse_commit_message("feat(auth): add OAuth2 login")
            .unwrap();

        assert_eq!(parsed.commit_type, "feat");
        assert_eq!(parsed.scope.as_deref(), Some("auth"));
        assert_eq!(parsed.subject, "add OAuth2 login");
        assert!(!parsed.is_breaking);
    }

    #[test]
    fn test_parse_commit_message_breaking_change() {
        let rules = RepositoryPolicy::default();
        let parsed = rules
            .parse_commit_message("fix(api)!: breaking change in response")
            .unwrap();

        assert_eq!(parsed.commit_type, "fix");
        assert!(parsed.is_breaking);
    }

    #[test]
    fn test_determine_bump() {
        let rules = RepositoryPolicy::default();

        let feat = rules.parse_commit_message("feat: new feature").unwrap();
        assert_eq!(rules.determine_bump(&feat), Impact::Minor);

        let fix = rules.parse_commit_message("fix: minor bug").unwrap();
        assert_eq!(rules.determine_bump(&fix), Impact::Patch);

        let breaking = rules.parse_commit_message("fix!: breaking").unwrap();
        assert_eq!(rules.determine_bump(&breaking), Impact::Major);
    }

    #[test]
    fn test_get_changelog_section() {
        let rules = RepositoryPolicy::default();

        let feat = rules.parse_commit_message("feat: add feature").unwrap();
        assert_eq!(rules.get_changelog_section(&feat), "Features");

        let fix = rules.parse_commit_message("fix: fix bug").unwrap();
        assert_eq!(rules.get_changelog_section(&fix), "Bug Fixes");
    }

    #[test]
    fn test_format_ticket_links() {
        let mut rules = RepositoryPolicy::default();

        let mut releases = ReleasesNamespace::default();
        let changelog = ChangelogConfig {
            ticket_linking: Some(TicketLinking {
                prefix: Some("PROJ-".to_string()),
                url_template: Some("https://jira.example.com/browse/{ticket}".to_string()),
            }),
            host_integration: None,
        };
        releases.changelog = Some(changelog);
        rules.releases = Some(releases);

        let formatted = rules.format_ticket_links("Fixes PROJ-123 in backend");
        assert_eq!(
            formatted,
            "Fixes [PROJ-123](https://jira.example.com/browse/PROJ-123) in backend"
        );
    }
}
