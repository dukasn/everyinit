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
use std::io::IsTerminal;
use std::path::Path;

pub struct PrePushHook;

impl GitHook for PrePushHook {
    fn hook_name(&self) -> &'static str {
        "pre-push"
    }

    async fn enforce(
        &self,
        rules: &GitRulesRoot,
        repo_root: &Path,
        payload: &[String],
    ) -> Result<(), Vec<PolicyError>> {
        let validator = RulesValidator::new(rules);
        let mut violations = Vec::new();
        let remote_name = payload.first().map(|s| s.as_str()).unwrap_or("");
        let mut is_force_push = false;

        // If run manually in a terminal, git won't be piping STDIN refs, so skip reading to avoid blocking forever.
        let push_refs = if !std::io::stdin().is_terminal() {
            evin_git::state::get_push_refs(&mut std::io::stdin()).unwrap_or_default()
        } else {
            // Ideally we'd emit a remark here, but for now we skip stdin ref validation.
            Vec::new()
        };

        for pref in push_refs {
            if let Ok(true) =
                evin_git::state::is_force_push(repo_root, &pref.local_sha, &pref.remote_sha).await
            {
                is_force_push = true;
            }

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
                let branch_name = pref.local_ref.strip_prefix("refs/heads/").unwrap();
                if let Err(e) = validator.validate_branch_name(branch_name) {
                    violations.push(PolicyError::Violation {
                        error: e,
                        issue_code: IssueCode::new("issue-push-rejected"),
                    });
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
                    let out = cmd.output().await.unwrap();
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

        let branch = evin_git::state::current_branch(repo_root)
            .await
            .unwrap_or_default();

        if let Err(errors) =
            validator.validate_remotes(remote_name, &branch, is_force_push, !is_force_push)
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
