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
use evin_repository_policy::validator::RepositoryPolicyValidator;
use std::path::Path;

pub struct CommitMsgHook;

impl GitHook for CommitMsgHook {
    fn hook_name(&self) -> &'static str {
        "commit-msg"
    }

    async fn enforce(
        &self,
        rules: &RepositoryPolicy,
        repo_root: &Path,
        payload: &[String],
    ) -> Result<(), Vec<PolicyError>> {
        let validator = RepositoryPolicyValidator::new(rules);
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
                let staged = match evin_git::state::get_staged_path_changes(repo_root).await {
                    Ok(staged) => staged,
                    Err(error) => {
                        violations.push(PolicyError::GitState(format!(
                            "Failed to read staged paths: {error}"
                        )));
                        return Err(violations);
                    }
                };
                let lock_only = staged.len() == 1
                    && staged[0].path == evin_action_sdk::release::RELEASE_LOCK_PATH;
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

#[cfg(test)]
mod tests {
    use super::CommitMsgHook;
    use crate::{GitHook, PolicyError};
    use evin_repository_policy::models::RepositoryPolicy;
    use std::{fs, process::Command};
    use tempfile::tempdir;

    fn git(repo: &std::path::Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("git should run");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[tokio::test]
    async fn staged_deletion_prevents_package_lock_only_classification() {
        let repo = tempdir().expect("temporary repository");
        let lock_path = repo
            .path()
            .join(evin_action_sdk::release::RELEASE_LOCK_PATH);
        fs::create_dir_all(lock_path.parent().unwrap()).expect("create lock directory");
        fs::write(&lock_path, "{}\n").expect("write release lock");
        fs::write(repo.path().join("removed.txt"), "tracked content").expect("write tracked file");
        git(repo.path(), &["init", "--quiet"]);
        git(repo.path(), &["config", "user.name", "Test User"]);
        git(repo.path(), &["config", "user.email", "test@example.com"]);
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "--quiet", "-m", "test baseline"]);

        fs::write(&lock_path, "{\"revision\": 1}\n").expect("update release lock");
        fs::remove_file(repo.path().join("removed.txt")).expect("remove tracked file");
        git(repo.path(), &["add", "-u"]);

        let message_path = repo.path().join("COMMIT_EDITMSG");
        fs::write(
            &message_path,
            format!(
                "{}\n\n{}\n",
                evin_action_sdk::release::PACKAGE_RELEASE_SUBJECT,
                evin_action_sdk::release::PACKAGE_RELEASE_TRAILER
            ),
        )
        .expect("write commit message");
        let payload = vec![message_path.to_string_lossy().into_owned()];

        let result = CommitMsgHook
            .enforce(&RepositoryPolicy::default(), repo.path(), &payload)
            .await;

        assert!(matches!(
            result,
            Err(errors) if errors.iter().any(|error| matches!(error, PolicyError::GitState(message) if message.contains("stage only")))
        ));
    }
}
