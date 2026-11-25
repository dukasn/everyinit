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

pub struct PreCommitHook;

impl GitHook for PreCommitHook {
    fn hook_name(&self) -> &'static str {
        "pre-commit"
    }

    async fn enforce(
        &self,
        rules: &RepositoryPolicy,
        repo_root: &Path,
        _payload: &[String],
    ) -> Result<(), Vec<PolicyError>> {
        let validator = RepositoryPolicyValidator::new(rules);
        let mut violations = Vec::new();

        match evin_git::state::get_staged_files(repo_root).await {
            Ok(raw_files) => {
                let files: Vec<evin_repository_policy::validator::StagedFile> = raw_files
                    .into_iter()
                    .map(|f| evin_repository_policy::validator::StagedFile {
                        path: f.path,
                        content: f.content,
                        is_binary: f.is_binary,
                        size_bytes: f.size_bytes,
                        is_lfs_tracked: f.is_lfs_tracked,
                        kind: match f.kind {
                            evin_git::state::StagedChangeKind::Added => {
                                evin_repository_policy::validator::StagedChangeKind::Added
                            }
                            evin_git::state::StagedChangeKind::Modified => {
                                evin_repository_policy::validator::StagedChangeKind::Modified
                            }
                            evin_git::state::StagedChangeKind::Deleted => {
                                evin_repository_policy::validator::StagedChangeKind::Deleted
                            }
                            evin_git::state::StagedChangeKind::TypeChanged => {
                                evin_repository_policy::validator::StagedChangeKind::TypeChanged
                            }
                        },
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
                        let submodule_urls =
                            match evin_git::state::get_submodule_urls(repo_root).await {
                                Ok(urls) => urls,
                                Err(error) => {
                                    return Err(vec![PolicyError::GitState(format!(
                                        "Failed to read staged submodule URLs: {error}"
                                    ))]);
                                }
                            };
                        let staged_submodule_additions =
                            match evin_git::state::get_staged_submodule_additions(repo_root).await {
                                Ok(paths) => paths,
                                Err(error) => {
                                    return Err(vec![PolicyError::GitState(format!(
                                        "Failed to inspect staged submodule additions: {error}"
                                    ))]);
                                }
                            };
                        if let Err(errors) = validator
                            .validate_submodules(&submodule_urls, &staged_submodule_additions)
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

#[cfg(test)]
mod tests {
    use super::PreCommitHook;
    use crate::{GitHook, PolicyError};
    use evin_repository_policy::{
        models::{FilesNamespace, FilesPolicies, RepositoryPolicy},
        validator::ValidationError,
    };
    use std::{fs, process::Command};
    use tempfile::tempdir;

    fn git_output(repo: &std::path::Path, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .args(args)
            .current_dir(repo)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_PREFIX")
            .output()
            .expect("git should run")
    }

    fn git(repo: &std::path::Path, args: &[&str]) {
        let output = git_output(repo, args);
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn repository_with_staged_deletion() -> tempfile::TempDir {
        let repo = tempdir().expect("temporary repository");
        git(repo.path(), &["init", "--quiet"]);
        git(repo.path(), &["config", "user.name", "Test User"]);
        git(repo.path(), &["config", "user.email", "test@example.com"]);
        fs::write(repo.path().join("removed.txt"), "tracked content").expect("write tracked file");
        git(repo.path(), &["add", "removed.txt"]);
        git(repo.path(), &["commit", "--quiet", "-m", "test baseline"]);
        fs::remove_file(repo.path().join("removed.txt")).expect("remove tracked file");
        git(repo.path(), &["add", "-u"]);
        repo
    }

    #[tokio::test]
    async fn deletion_only_change_is_not_rejected_as_empty() {
        let repo = repository_with_staged_deletion();
        let result = PreCommitHook
            .enforce(&RepositoryPolicy::default(), repo.path(), &[])
            .await;

        assert!(result.is_ok(), "deletion should pass pre-commit");
    }

    #[tokio::test]
    async fn deletion_only_change_still_obeys_path_policies() {
        let repo = repository_with_staged_deletion();
        let rules = RepositoryPolicy {
            files: Some(FilesNamespace {
                policies: Some(FilesPolicies {
                    block_paths: vec!["removed.txt".to_string()],
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = PreCommitHook.enforce(&rules, repo.path(), &[]).await;

        assert!(result.is_err(), "blocked deletion path should be rejected");
    }

    #[tokio::test]
    async fn empty_index_is_still_rejected() {
        let repo = repository_with_staged_deletion();
        git(repo.path(), &["reset", "--quiet"]);

        let result = PreCommitHook
            .enforce(&RepositoryPolicy::default(), repo.path(), &[])
            .await;

        assert!(matches!(
            result,
            Err(errors) if errors.iter().any(|error| matches!(error, PolicyError::EmptyCommitsNotAllowed))
        ));
    }

    #[tokio::test]
    async fn staged_submodule_creation_reaches_submodule_policy() {
        let repo = repository_with_staged_deletion();
        let child = tempdir().expect("temporary submodule repository");
        git(child.path(), &["init", "--quiet"]);
        git(child.path(), &["config", "user.name", "Test User"]);
        git(child.path(), &["config", "user.email", "test@example.com"]);
        fs::write(child.path().join("file.txt"), "submodule content")
            .expect("write submodule file");
        git(child.path(), &["add", "file.txt"]);
        git(
            child.path(),
            &["commit", "--quiet", "-m", "submodule baseline"],
        );
        let output = git_output(child.path(), &["rev-parse", "HEAD"]);
        assert!(output.status.success());
        let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let cache_info = format!("160000,{commit},vendor/module");
        git(
            repo.path(),
            &["update-index", "--add", "--cacheinfo", &cache_info],
        );

        let result = PreCommitHook
            .enforce(&RepositoryPolicy::default(), repo.path(), &[])
            .await;

        assert!(matches!(
            result,
            Err(errors) if errors.iter().any(|error| matches!(error, PolicyError::Violation { error: ValidationError::SubmoduleCreationBlocked, .. }))
        ));
    }
}
