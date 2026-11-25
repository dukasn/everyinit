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
use evin_repository_policy::models::RepositoryPolicy;
use evin_repository_policy::validator::{RepositoryPolicyValidator, ValidationError};
use std::io::IsTerminal;
use std::path::Path;

pub struct PrePushHook;

impl GitHook for PrePushHook {
    fn hook_name(&self) -> &'static str {
        "pre-push"
    }

    async fn enforce(
        &self,
        rules: &RepositoryPolicy,
        repo_root: &Path,
        payload: &[String],
    ) -> Result<(), Vec<PolicyError>> {
        let validator = RepositoryPolicyValidator::new(rules);
        let mut violations = Vec::new();
        let remote_name = payload.first().map(|s| s.as_str()).unwrap_or("");

        // If run manually in a terminal, git won't be piping STDIN refs, so skip reading to avoid blocking forever.
        let push_refs = if !std::io::stdin().is_terminal() {
            match evin_git::state::get_push_refs(&mut std::io::stdin()) {
                Ok(refs) if !refs.is_empty() => refs,
                Ok(_) => {
                    return Err(vec![PolicyError::GitState(
                        "pre-push hook received no ref updates".to_string(),
                    )]);
                }
                Err(error) => {
                    return Err(vec![PolicyError::GitState(format!(
                        "Failed to read pre-push refs: {error}"
                    ))]);
                }
            }
        } else {
            // Ideally we'd emit a remark here, but for now we skip stdin ref validation.
            Vec::new()
        };

        let mut branch_push_seen = false;
        let mut remote_error_reported = false;
        for pref in push_refs {
            // Validate Branch & Tag Names & Deletions
            if pref.local_sha == "0000000000000000000000000000000000000000"
                && pref.remote_ref.starts_with("refs/tags/")
            {
                if let Err(e) = validator.validate_tag_deletion(&pref.remote_ref) {
                    violations.push(PolicyError::Violation {
                        error: e,
                        issue_code: IssueCode::new("issue-push-rejected"),
                    });
                }
            } else if pref.local_ref.starts_with("refs/heads/") {
                branch_push_seen = true;
                let Some(branch_name) = pref.local_ref.strip_prefix("refs/heads/") else {
                    continue;
                };
                if let Err(e) = validator.validate_branch_name(branch_name) {
                    violations.push(PolicyError::Violation {
                        error: e,
                        issue_code: IssueCode::new("issue-push-rejected"),
                    });
                }

                let is_force_push = if pref.local_sha == "0000000000000000000000000000000000000000"
                {
                    false
                } else {
                    match evin_git::state::is_force_push(
                        repo_root,
                        &pref.local_sha,
                        &pref.remote_sha,
                    )
                    .await
                    {
                        Ok(is_force_push) => is_force_push,
                        Err(error) => {
                            violations.push(PolicyError::GitState(format!(
                                "Failed to determine whether branch '{branch_name}' is up to date: {error}"
                            )));
                            continue;
                        }
                    }
                };

                if let Err(errors) = validator.validate_remotes(
                    remote_name,
                    branch_name,
                    is_force_push,
                    !is_force_push,
                ) {
                    for error in errors {
                        if matches!(error, ValidationError::RemoteNotAllowed { .. }) {
                            if !remote_error_reported {
                                violations.push(PolicyError::Violation {
                                    error,
                                    issue_code: IssueCode::new("issue-push-restricted"),
                                });
                                remote_error_reported = true;
                            }
                        } else {
                            violations.push(PolicyError::Violation {
                                error,
                                issue_code: IssueCode::new("issue-push-restricted"),
                            });
                        }
                    }
                }
            } else if pref.local_ref.starts_with("refs/tags/") {
                let tag_name = pref.local_ref.strip_prefix("refs/tags/").unwrap();
                // Check if tag is annotated using `git cat-file -t`
                let is_annotated = if pref.local_sha == "0000000000000000000000000000000000000000" {
                    false
                } else {
                    let mut cmd = tokio::process::Command::new("git");
                    cmd.current_dir(repo_root);
                    cmd.args(["cat-file", "-t", &pref.local_sha]);
                    let out = cmd.output().await.map_err(|error| {
                        vec![PolicyError::GitState(format!(
                            "Failed to inspect pushed tag '{tag_name}': {error}"
                        ))]
                    })?;
                    if !out.status.success() {
                        return Err(vec![PolicyError::GitState(format!(
                            "Failed to inspect pushed tag '{tag_name}': {}",
                            String::from_utf8_lossy(&out.stderr).trim()
                        ))]);
                    }
                    let obj_type = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    obj_type == "tag"
                };

                if let Err(e) = validator.validate_tag_name(tag_name, is_annotated) {
                    violations.push(PolicyError::Violation {
                        error: e,
                        issue_code: IssueCode::new("issue-push-rejected"),
                    });
                }
            }
        }

        if !branch_push_seen
            && let Err(errors) = validator.validate_remotes(remote_name, "", false, true)
        {
            violations.extend(errors.into_iter().map(|e| PolicyError::Violation {
                error: e,
                issue_code: IssueCode::new("issue-push-restricted"),
            }));
        }

        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }
}
