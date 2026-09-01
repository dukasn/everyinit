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

use evin_git::diff::diff_files as fetch_diff_files;
use evin_git::history::{GitCommit, fetch_all_commits};
use evin_git::mutate::stage_files as mutate_stage_files;
use evin_git::state::{
    current_branch as fetch_current_branch, is_dirty as fetch_is_dirty,
    short_sha as fetch_short_sha,
};
use std::path::PathBuf;
use tokio::sync::OnceCell;

/// Extracts a SemVer-like tag name from a git --decorate reference string.
/// For example, extracts "v1.0.0" from "HEAD -> main, tag: v1.0.0, origin/main"
pub fn extract_tag(refs: &str) -> Option<&str> {
    for reference in refs.split(',') {
        if let Some(tag) = reference.trim().strip_prefix("tag: ") {
            return Some(tag);
        }
    }
    None
}

#[derive(Debug, Clone)]
pub enum GitError {
    ExecutionFailed(String),
    CommitFailed(String),
    TagFailed(String),
    NoStagedChanges,
    TagAlreadyExists(String),
    InvalidTagName(String),
    IdentityNotConfigured,
    TagRecoveryFailed {
        commit_id: String,
        tag: String,
        error: String,
    },
}

impl From<String> for GitError {
    fn from(s: String) -> Self {
        GitError::ExecutionFailed(s)
    }
}

pub struct GitProvider {
    workspace_root: PathBuf,
    mode: crate::PipelineMode,
    log: crate::Logger,
    dry_run: evin_io::DryRunState,
    recent_commits: OnceCell<Result<Vec<GitCommit>, GitError>>,
    is_dirty_cache: OnceCell<Result<bool, GitError>>,
    current_branch_cache: OnceCell<Result<String, GitError>>,
    short_sha_cache: OnceCell<Result<String, GitError>>,
}

impl GitProvider {
    pub fn new(workspace_root: PathBuf, mode: crate::PipelineMode, log: crate::Logger) -> Self {
        Self::new_with_dry_run(workspace_root, mode, log, evin_io::DryRunState::default())
    }

    pub fn new_with_dry_run(
        workspace_root: PathBuf,
        mode: crate::PipelineMode,
        log: crate::Logger,
        dry_run: evin_io::DryRunState,
    ) -> Self {
        Self {
            workspace_root,
            mode,
            log,
            dry_run,
            recent_commits: OnceCell::new(),
            is_dirty_cache: OnceCell::new(),
            current_branch_cache: OnceCell::new(),
            short_sha_cache: OnceCell::new(),
        }
    }

    pub async fn commits(&self) -> Result<&[GitCommit], GitError> {
        let res = self
            .recent_commits
            .get_or_init(|| async {
                fetch_all_commits(&self.workspace_root)
                    .await
                    .map_err(Into::into)
            })
            .await;
        match res {
            Ok(commits) => Ok(commits),
            Err(e) => Err(e.clone()),
        }
    }

    pub async fn is_dirty(&self) -> Result<bool, GitError> {
        let res = self
            .is_dirty_cache
            .get_or_init(|| async {
                fetch_is_dirty(&self.workspace_root)
                    .await
                    .map_err(Into::into)
            })
            .await;
        match res {
            Ok(dirty) => Ok(*dirty),
            Err(e) => Err(e.clone()),
        }
    }

    pub async fn current_branch(&self) -> Result<String, GitError> {
        let res = self
            .current_branch_cache
            .get_or_init(|| async {
                fetch_current_branch(&self.workspace_root)
                    .await
                    .map_err(Into::into)
            })
            .await;
        match res {
            Ok(branch) => Ok(branch.clone()),
            Err(e) => Err(e.clone()),
        }
    }

    pub async fn short_sha(&self) -> Result<String, GitError> {
        let res = self
            .short_sha_cache
            .get_or_init(|| async {
                fetch_short_sha(&self.workspace_root)
                    .await
                    .map_err(Into::into)
            })
            .await;
        match res {
            Ok(sha) => Ok(sha.clone()),
            Err(e) => Err(e.clone()),
        }
    }

    pub async fn diff_files(&self, from: &str, to: &str) -> Result<Vec<String>, GitError> {
        fetch_diff_files(&self.workspace_root, from, to)
            .await
            .map_err(Into::into)
    }

    pub async fn stage_files(&self, files: &[String]) -> Result<(), GitError> {
        if self.mode == crate::PipelineMode::DryRun {
            if files == ["-A"] {
                self.dry_run.stage_all();
            } else {
                let paths = files.iter().filter_map(|path| {
                    let path = PathBuf::from(path);
                    let path = if path.is_absolute() {
                        path
                    } else {
                        self.workspace_root.join(path)
                    };
                    path.canonicalize().ok().or(Some(path))
                });
                self.dry_run.stage_paths(paths);
            }
            self.log.plan(crate::activity::StageFilesPlan {
                files: files.join(", "),
            });
            return Ok(());
        }
        mutate_stage_files(&self.workspace_root, files)
            .await
            .map_err(Into::into)
    }

    pub async fn commit(&self, message: &str, sign: bool) -> Result<(), GitError> {
        if self.mode == crate::PipelineMode::DryRun {
            self.log.plan(crate::activity::GitCommitPlan {
                message: message.to_string(),
                signed: sign,
            });
            self.dry_run.commit_staging();
            return Ok(());
        }
        let mut args = vec!["commit", "-m", message];
        if sign {
            args.push("-S");
        }
        let output = evin_git::cmd::git_cmd()
            .args(&args)
            .current_dir(&self.workspace_root)
            .output()
            .await
            .map_err(|e| GitError::ExecutionFailed(e.to_string()))?;

        if !output.status.success() {
            let err_str = String::from_utf8_lossy(&output.stderr);
            return Err(GitError::CommitFailed(err_str.to_string()));
        }

        Ok(())
    }

    pub async fn create_tag(&self, tag_name: &str, message: &str) -> Result<(), GitError> {
        if self.mode == crate::PipelineMode::DryRun {
            self.log.plan(crate::activity::GitTagPlan {
                tag: tag_name.to_string(),
            });
            return Ok(());
        }
        let output = evin_git::cmd::git_cmd()
            .args(["tag", "-a", tag_name, "-m", message])
            .current_dir(&self.workspace_root)
            .output()
            .await
            .map_err(|e| GitError::ExecutionFailed(e.to_string()))?;

        if !output.status.success() {
            let err_str = String::from_utf8_lossy(&output.stderr);
            return Err(GitError::TagFailed(err_str.to_string()));
        }

        Ok(())
    }

    pub async fn validate_tag_name(&self, tag: &str) -> Result<(), GitError> {
        let output = evin_git::cmd::git_cmd()
            .args(["check-ref-format", &format!("refs/tags/{}", tag)])
            .current_dir(&self.workspace_root)
            .output()
            .await
            .map_err(|e| GitError::ExecutionFailed(e.to_string()))?;

        if !output.status.success() {
            return Err(GitError::InvalidTagName(tag.to_string()));
        }
        Ok(())
    }

    pub async fn has_tag(&self, tag: &str) -> Result<bool, GitError> {
        let output = evin_git::cmd::git_cmd()
            .args(["tag", "-l", tag])
            .current_dir(&self.workspace_root)
            .output()
            .await
            .map_err(|e| GitError::ExecutionFailed(e.to_string()))?;

        let found = String::from_utf8_lossy(&output.stdout);
        Ok(found.trim() == tag)
    }

    pub async fn has_staged_changes(&self) -> Result<bool, GitError> {
        let output = evin_git::cmd::git_cmd()
            .args(["diff", "--cached", "--quiet"])
            .current_dir(&self.workspace_root)
            .output()
            .await
            .map_err(|e| GitError::ExecutionFailed(e.to_string()))?;

        // 0 = no changes, 1 = changes exist
        if !output.status.success() {
            return Ok(true);
        }

        if self.mode != crate::PipelineMode::DryRun {
            return Ok(false);
        }
        if self.dry_run.has_virtual_staged_changes() {
            return Ok(true);
        }

        let (stage_all, staged_paths) = self.dry_run.staging();
        if stage_all {
            if self.dry_run.physical_changes_consumed() {
                return Ok(false);
            }
            return self.is_dirty().await;
        }
        let staged_paths: Vec<std::path::PathBuf> = staged_paths
            .into_iter()
            .filter(|path| !self.dry_run.physical_path_consumed(path))
            .collect();
        if staged_paths.is_empty() {
            return Ok(false);
        }

        let mut command = evin_git::cmd::git_cmd();
        command.arg("status").arg("--porcelain").arg("--");
        command.args(staged_paths);
        let output = command
            .current_dir(&self.workspace_root)
            .output()
            .await
            .map_err(|e| GitError::ExecutionFailed(e.to_string()))?;
        Ok(!output.stdout.is_empty())
    }

    pub async fn get_last_commit_id(&self) -> Result<String, GitError> {
        let output = evin_git::cmd::git_cmd()
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.workspace_root)
            .output()
            .await
            .map_err(|e| GitError::ExecutionFailed(e.to_string()))?;

        if !output.status.success() {
            return Err(GitError::ExecutionFailed("Failed to read HEAD".to_string()));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Returns the author-independent commit timestamp used by reproducible
    /// artifact builders when SOURCE_DATE_EPOCH is not supplied.
    pub async fn head_commit_epoch(&self) -> Result<u64, GitError> {
        let output = evin_git::cmd::git_cmd()
            .args(["show", "-s", "--format=%ct", "HEAD"])
            .current_dir(&self.workspace_root)
            .output()
            .await
            .map_err(|e| GitError::ExecutionFailed(e.to_string()))?;
        if !output.status.success() {
            return Err(GitError::ExecutionFailed(
                "Failed to read HEAD commit timestamp".to_string(),
            ));
        }
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u64>()
            .map_err(|e| GitError::ExecutionFailed(format!("Invalid HEAD commit timestamp: {e}")))
    }

    pub async fn verify_identity(&self) -> Result<(), GitError> {
        let name_out = evin_git::cmd::git_cmd()
            .args(["config", "user.name"])
            .current_dir(&self.workspace_root)
            .output()
            .await
            .map_err(|e| GitError::ExecutionFailed(e.to_string()))?;

        let email_out = evin_git::cmd::git_cmd()
            .args(["config", "user.email"])
            .current_dir(&self.workspace_root)
            .output()
            .await
            .map_err(|e| GitError::ExecutionFailed(e.to_string()))?;

        if !name_out.status.success() || !email_out.status.success() {
            return Err(GitError::IdentityNotConfigured);
        }
        Ok(())
    }
}

impl From<GitError> for evin_diagnostic::Diagnostic {
    fn from(err: GitError) -> Self {
        match err {
            GitError::ExecutionFailed(e) => evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-git-command-failed")
                    .arg("command", "git")
                    .arg("error", e),
            ),
            GitError::CommitFailed(e) => evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-git-command-failed")
                    .arg("command", "git commit")
                    .arg("error", e),
            ),
            GitError::TagFailed(e) => evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-git-command-failed")
                    .arg("command", "git tag")
                    .arg("error", e),
            ),
            GitError::NoStagedChanges => {
                evin_locales::diagnostic(evin_diagnostic::IssueCode::new("no-staged-changes"))
            }
            GitError::TagAlreadyExists(tag) => evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("tag-already-exists").arg("tag", tag),
            ),
            GitError::InvalidTagName(tag) => evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("invalid-tag-name").arg("tag", tag),
            ),
            GitError::IdentityNotConfigured => {
                evin_locales::diagnostic(evin_diagnostic::IssueCode::new("identity-not-configured"))
            }
            GitError::TagRecoveryFailed {
                commit_id,
                tag,
                error,
            } => evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("tag-recovery-failed")
                    .arg("commit_id", commit_id)
                    .arg("tag", tag)
                    .arg("error", error),
            ),
        }
    }
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
