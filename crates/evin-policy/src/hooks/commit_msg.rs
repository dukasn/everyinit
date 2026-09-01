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

pub struct CommitMsgHook;

impl GitHook for CommitMsgHook {
    fn hook_name(&self) -> &'static str {
        "commit-msg"
    }

    async fn enforce(
        &self,
        rules: &GitRulesRoot,
        repo_root: &Path,
        payload: &[String],
    ) -> Result<(), Vec<PolicyError>> {
        let validator = RulesValidator::new(rules);
        let mut violations = Vec::new();

        if let Err(errors) = validator.validate_history_integrity(repo_root) {
            violations.extend(errors.into_iter().map(|e| PolicyError::Violation {
                error: e,
                issue_code: IssueCode::new("issue-commit-rejected"),
            }));
        }

        let commit_msg_path = payload.first().map(|s| s.as_str()).unwrap_or("");
        if commit_msg_path.is_empty() {
            violations.push(PolicyError::MissingArgument {
                message:
                    "The commit-msg hook requires a path to the commit message file as an argument."
                        .to_string(),
                hint: Some("Try running: evin hook run commit-msg <path/to/file>".to_string()),
            });
            return Err(violations);
        }

        match tokio::fs::read_to_string(commit_msg_path).await {
            Ok(source) => {
                if let Err(errors) = validator.validate_commit_message(&source) {
                    violations.extend(errors.into_iter().map(|e| PolicyError::Violation {
                        error: e,
                        issue_code: IssueCode::new("issue-commit-rejected"),
                    }));
                }
                let has_trailer = source
                    .lines()
                    .any(|line| line.trim() == evin_action_sdk::release::PACKAGE_RELEASE_TRAILER);
                let subject = source.lines().next().unwrap_or_default().trim();
                let staged = std::process::Command::new("git")
                    .args(["diff", "--cached", "--name-only", "--diff-filter=ACMR"])
                    .current_dir(repo_root)
                    .output()
                    .ok()
                    .filter(|output| output.status.success())
                    .map(|output| {
                        String::from_utf8_lossy(&output.stdout)
                            .lines()
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let lock_only = staged == [evin_action_sdk::release::RELEASE_LOCK_PATH];
                if (has_trailer || lock_only)
                    && (!has_trailer
                        || !lock_only
                        || subject != evin_action_sdk::release::PACKAGE_RELEASE_SUBJECT)
                {
                    violations.push(PolicyError::GitState(format!(
                        "package-only release commits must use '{}' and '{}' and stage only {}",
                        evin_action_sdk::release::PACKAGE_RELEASE_SUBJECT,
                        evin_action_sdk::release::PACKAGE_RELEASE_TRAILER,
                        evin_action_sdk::release::RELEASE_LOCK_PATH,
                    )));
                }
            }
            Err(e) => {
                violations.push(PolicyError::GitState(format!(
                    "Cannot read commit message file '{}': {}",
                    commit_msg_path, e
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
