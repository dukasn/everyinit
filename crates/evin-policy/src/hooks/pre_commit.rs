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

use crate::{GitHook, PolicyError};
use evin_diagnostic::IssueCode;
use evin_gitrules::models::GitRulesRoot;
use evin_gitrules::validator::RulesValidator;
use std::path::Path;

pub struct PreCommitHook;

impl GitHook for PreCommitHook {
    fn hook_name(&self) -> &'static str {
        "pre-commit"
    }

    async fn enforce(
        &self,
        rules: &GitRulesRoot,
        repo_root: &Path,
        _payload: &[String],
    ) -> Result<(), Vec<PolicyError>> {
        let validator = RulesValidator::new(rules);
        let mut violations = Vec::new();

        match evin_git::state::get_staged_files(repo_root).await {
            Ok(raw_files) => {
                let files: Vec<evin_gitrules::validator::StagedFile> = raw_files
                    .into_iter()
                    .map(|f| evin_gitrules::validator::StagedFile {
                        path: f.path,
                        content: f.content,
                        is_binary: f.is_binary,
                        size_bytes: f.size_bytes,
                        is_lfs_tracked: f.is_lfs_tracked,
                        status: f.status,
                    })
                    .collect();

                if files.is_empty() {
                    let allow_empty = rules
                        .commits
                        .as_ref()
                        .and_then(|c| c.policies.as_ref())
                        .and_then(|p| p.allow_empty_commits)
                        .unwrap_or(false);

                    if !allow_empty {
                        violations.push(PolicyError::EmptyCommitsNotAllowed);
                        return Err(violations);
                    }
                }

                if let Err(errors) = validator.validate_staged_files(&files) {
                    violations.extend(errors.into_iter().map(|e| PolicyError::Violation {
                        error: e,
                        issue_code: IssueCode::new("issue-commit-rejected"),
                    }));
                } else {
                    let is_lfs_installed = evin_git::state::is_lfs_installed().await;
                    if let Err(errors) = validator.validate_lfs(&files, is_lfs_installed) {
                        violations.extend(errors.into_iter().map(|e| PolicyError::Violation {
                            error: e,
                            issue_code: IssueCode::new("issue-lfs-requirement-unmet"),
                        }));
                    } else {
                        let submodule_urls = evin_git::state::get_submodule_urls(repo_root)
                            .await
                            .unwrap_or_default();
                        if let Err(errors) = validator.validate_submodules(&files, &submodule_urls)
                        {
                            violations.extend(errors.into_iter().map(|e| PolicyError::Violation {
                                error: e,
                                issue_code: IssueCode::new("issue-invalid-submodule"),
                            }));
                        } else {
                            // Authorship Validation
                            let repo_root_buf = repo_root.to_path_buf();
                            let get_git_config = |key: &str| {
                                let repo_root = repo_root_buf.clone();
                                let key = key.to_string();
                                async move {
                                    tokio::process::Command::new("git")
                                        .args(["config", "--get", &key])
                                        .current_dir(repo_root)
                                        .output()
                                        .await
                                        .map(|o| {
                                            String::from_utf8_lossy(&o.stdout).trim().to_string()
                                        })
                                        .unwrap_or_default()
                                }
                            };

                            let author_email = match std::env::var("GIT_AUTHOR_EMAIL") {
                                Ok(val) => val,
                                Err(_) => get_git_config("user.email").await,
                            };
                            let committer_email = match std::env::var("GIT_COMMITTER_EMAIL") {
                                Ok(val) => val,
                                Err(_) => get_git_config("user.email").await,
                            };

                            let is_signed = get_git_config("commit.gpgsign").await == "true";

                            if let Err(errors) = validator.validate_authorship(
                                &author_email,
                                &committer_email,
                                is_signed,
                            ) {
                                violations.extend(errors.into_iter().map(|e| {
                                    PolicyError::Violation {
                                        error: e,
                                        issue_code: IssueCode::new("issue-commit-rejected"),
                                    }
                                }));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                violations.push(PolicyError::GitState(format!(
                    "Failed to read staged files: {}",
                    e
                )));
            }
        }

        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }
}
