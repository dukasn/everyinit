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

use std::path::Path;

pub async fn is_dirty(workspace_root: &Path) -> Result<bool, String> {
    let output = crate::cmd::git_cmd()
        .args(["status", "--porcelain", "-z"])
        .current_dir(workspace_root)
        .output()
        .await
        .map_err(|e| format!("Failed to execute git status: {}", e))?;

    if !output.status.success() {
        let err_str = String::from_utf8_lossy(&output.stderr);
        // If it's not a git repository yet, consider it clean to prevent crashes on bootstrap.
        if err_str.contains("not a git repository") {
            return Ok(false);
        }
        return Err(format!("git status failed: {}", err_str));
    }

    // If there is any output, the repository is dirty.
    Ok(!output.stdout.is_empty())
}

pub async fn current_branch(workspace_root: &Path) -> Result<String, String> {
    let output = crate::cmd::git_cmd()
        .args(["branch", "--show-current"])
        .current_dir(workspace_root)
        .output()
        .await
        .map_err(|e| format!("Failed to execute git branch: {}", e))?;

    if !output.status.success() {
        let err_str = String::from_utf8_lossy(&output.stderr);
        if err_str.contains("not a git repository") {
            return Ok("".to_string());
        }
        return Err(format!("git branch failed: {}", err_str));
    }

    let mut branch = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if branch.is_empty() || branch == "HEAD" {
        // Fallback for detached HEAD in CI environments
        let ci_vars = [
            "GITHUB_HEAD_REF",
            "GITHUB_REF_NAME",
            "CI_COMMIT_BRANCH",
            "CI_MERGE_REQUEST_SOURCE_BRANCH_NAME",
            "BITBUCKET_BRANCH",
            "CIRCLE_BRANCH",
            "BRANCH_NAME",
            "GIT_BRANCH",
            "BUILD_SOURCEBRANCHNAME",
        ];

        for var in ci_vars {
            if let Ok(val) = std::env::var(var)
                && !val.trim().is_empty()
            {
                branch = val.trim().to_string();
                break;
            }
        }
    }

    Ok(branch)
}
pub async fn short_sha(workspace_root: &Path) -> Result<String, String> {
    let output = crate::cmd::git_cmd()
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(workspace_root)
        .output()
        .await
        .map_err(|e| format!("Failed to execute git rev-parse: {}", e))?;

    if !output.status.success() {
        let err_str = String::from_utf8_lossy(&output.stderr);
        if err_str.contains("not a git repository") {
            return Ok("".to_string());
        }
        return Err(format!("git rev-parse failed: {}", err_str));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub async fn is_merge_in_progress(workspace_root: &Path) -> Result<bool, String> {
    let output = crate::cmd::git_cmd()
        .args(["rev-parse", "--git-path", "MERGE_HEAD"])
        .current_dir(workspace_root)
        .output()
        .await
        .map_err(|e| format!("Failed to locate Git merge state: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git rev-parse failed while locating merge state: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let merge_head_path = String::from_utf8_lossy(&output.stdout);
    let merge_head = Path::new(merge_head_path.trim());
    let merge_head = if merge_head.is_absolute() {
        merge_head.to_path_buf()
    } else {
        workspace_root.join(merge_head)
    };
    Ok(merge_head.exists())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StagedChangeKind {
    Added,
    Modified,
    Deleted,
    TypeChanged,
}

impl StagedChangeKind {
    fn from_status(status: &str) -> Option<Self> {
        match status.chars().next()? {
            'A' => Some(Self::Added),
            'M' => Some(Self::Modified),
            'D' => Some(Self::Deleted),
            'T' => Some(Self::TypeChanged),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StagedPathChange {
    pub path: String,
    pub kind: StagedChangeKind,
    pub is_submodule: bool,
    pub creates_submodule: bool,
}

#[derive(Debug, Clone)]
pub struct StagedFileRaw {
    pub path: String,
    pub content: Option<String>,
    pub is_binary: bool,
    pub size_bytes: usize,
    pub is_lfs_tracked: bool,
    pub kind: StagedChangeKind,
}

pub async fn get_staged_path_changes(
    workspace_root: &Path,
) -> Result<Vec<StagedPathChange>, String> {
    let output = crate::cmd::git_cmd()
        .args([
            "diff",
            "--cached",
            "--raw",
            "-z",
            "--no-renames",
            "--diff-filter=ACMDT",
        ])
        .current_dir(workspace_root)
        .output()
        .await
        .map_err(|e| format!("Failed to execute git diff: {}", e))?;

    if !output.status.success() {
        return Err("git diff failed".to_string());
    }

    let mut changes = Vec::new();
    let raw_output = String::from_utf8_lossy(&output.stdout);
    let mut iter = raw_output.split('\0');
    while let Some(status) = iter.next() {
        if status.is_empty() {
            break;
        }
        let path = iter
            .next()
            .ok_or_else(|| "git diff returned a staged status without a path".to_string())?;
        let metadata = status
            .strip_prefix(':')
            .ok_or_else(|| "git diff returned an invalid raw staged entry".to_string())?;
        let mut metadata = metadata.split_whitespace();
        let old_mode = metadata
            .next()
            .ok_or_else(|| "git diff returned a staged entry without an old mode".to_string())?;
        let new_mode = metadata
            .next()
            .ok_or_else(|| "git diff returned a staged entry without a new mode".to_string())?;
        let _old_oid = metadata.next().ok_or_else(|| {
            "git diff returned a staged entry without an old object id".to_string()
        })?;
        let _new_oid = metadata.next().ok_or_else(|| {
            "git diff returned a staged entry without a new object id".to_string()
        })?;
        let status = metadata
            .next()
            .ok_or_else(|| "git diff returned a staged entry without a status".to_string())?;
        let kind = StagedChangeKind::from_status(status)
            .ok_or_else(|| format!("git diff returned an unsupported staged status: {status}"))?;
        changes.push(StagedPathChange {
            path: path.to_string(),
            kind,
            is_submodule: old_mode == "160000" || new_mode == "160000",
            creates_submodule: new_mode == "160000" && old_mode != "160000",
        });
    }

    Ok(changes)
}

pub async fn get_staged_files(workspace_root: &Path) -> Result<Vec<StagedFileRaw>, String> {
    let changes = get_staged_path_changes(workspace_root).await?;

    if changes.is_empty() {
        return Ok(vec![]);
    }

    // Check LFS tracking in bulk
    let mut is_lfs_tracked_map = std::collections::HashMap::new();
    if let Ok(mut child) = crate::cmd::git_cmd()
        .args(["check-attr", "filter", "-z", "--stdin"])
        .current_dir(workspace_root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            for change in &changes {
                if change.kind == StagedChangeKind::Deleted || change.is_submodule {
                    continue;
                }
                let _ = stdin.write_all(change.path.as_bytes()).await;
                let _ = stdin.write_all(&[0]).await;
            }
        }

        if let Ok(attr_output) = child.wait_with_output().await {
            let attr_str = String::from_utf8_lossy(&attr_output.stdout);
            let mut attr_iter = attr_str.split('\0');
            while let Some(path) = attr_iter.next() {
                if path.is_empty() {
                    break;
                }
                let _attr = attr_iter.next(); // "filter"
                let value = attr_iter.next().unwrap_or("");
                is_lfs_tracked_map.insert(path.to_string(), value == "lfs");
            }
        }
    }

    let mut staged = Vec::new();
    for StagedPathChange {
        kind,
        path,
        is_submodule,
        ..
    } in changes
    {
        if matches!(kind, StagedChangeKind::Deleted) {
            staged.push(StagedFileRaw {
                path,
                content: None,
                is_binary: false,
                size_bytes: 0,
                is_lfs_tracked: false,
                kind,
            });
            continue;
        }

        if is_submodule {
            staged.push(StagedFileRaw {
                path,
                content: None,
                is_binary: false,
                size_bytes: 0,
                is_lfs_tracked: false,
                kind,
            });
            continue;
        }

        // 1. Get exact staged file size
        let size_output = crate::cmd::git_cmd()
            .args(["cat-file", "-s", &format!(":{}", path)])
            .current_dir(workspace_root)
            .output()
            .await
            .map_err(|e| format!("git cat-file failed: {}", e))?;

        if !size_output.status.success() {
            if kind == StagedChangeKind::TypeChanged {
                staged.push(StagedFileRaw {
                    path,
                    content: None,
                    is_binary: false,
                    size_bytes: 0,
                    is_lfs_tracked: false,
                    kind,
                });
                continue;
            }
            return Err(format!("git cat-file failed for staged path: {path}"));
        }

        let size_str = String::from_utf8_lossy(&size_output.stdout)
            .trim()
            .to_string();
        let size_bytes: usize = size_str.parse().unwrap_or(0);

        let mut content = String::new();
        let mut is_binary = false;

        // 2. Size Limit Bypass
        if size_bytes <= 1_000_000 {
            let show_output = crate::cmd::git_cmd()
                .args(["show", &format!(":{}", path)])
                .current_dir(workspace_root)
                .output()
                .await
                .map_err(|e| format!("git show failed: {}", e))?;

            if !show_output.status.success() {
                return Err(format!("git show failed for staged path: {path}"));
            }
            {
                let bytes = &show_output.stdout;

                // 3. Null-byte Heuristic (Binary Bypass)
                let check_len = std::cmp::min(1024, bytes.len());
                for &b in &bytes[..check_len] {
                    if b == 0 {
                        is_binary = true;
                        break;
                    }
                }

                if !is_binary {
                    content = String::from_utf8_lossy(bytes).into_owned();
                }
            }
        }

        let is_lfs_tracked = is_lfs_tracked_map.get(&path).copied().unwrap_or(false);

        staged.push(StagedFileRaw {
            path,
            content: Some(content),
            is_binary,
            size_bytes,
            is_lfs_tracked,
            kind,
        });
    }

    Ok(staged)
}

pub async fn get_staged_submodule_additions(workspace_root: &Path) -> Result<Vec<String>, String> {
    Ok(get_staged_path_changes(workspace_root)
        .await?
        .into_iter()
        .filter(|change| change.creates_submodule)
        .map(|change| change.path)
        .collect())
}

pub async fn is_lfs_installed() -> bool {
    crate::cmd::git_cmd()
        .args(["lfs", "env"])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub async fn get_submodule_urls(repo_root: &Path) -> Result<Vec<String>, String> {
    let index_output = crate::cmd::git_cmd()
        .args(["ls-files", "--stage", "--", ".gitmodules"])
        .current_dir(repo_root)
        .output()
        .await
        .map_err(|e| format!("Failed to inspect staged .gitmodules: {e}"))?;

    if !index_output.status.success() {
        return Err(format!(
            "git ls-files failed while inspecting staged .gitmodules: {}",
            String::from_utf8_lossy(&index_output.stderr).trim()
        ));
    }
    if index_output.stdout.is_empty() {
        return Ok(vec![]);
    }

    let output = crate::cmd::git_cmd()
        .args(["config", "--blob", ":.gitmodules", "--get-regexp", "url"])
        .current_dir(repo_root)
        .output()
        .await
        .map_err(|e| format!("Failed to read staged .gitmodules: {e}"))?;
    if !output.status.success() {
        if output.status.code() == Some(1) && output.stderr.is_empty() {
            return Ok(vec![]);
        }
        return Err(format!(
            "git config failed while reading staged .gitmodules: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let mut urls = Vec::new();
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() == 2 {
            urls.push(parts[1].to_string());
        }
    }

    Ok(urls)
}

pub async fn is_force_push(
    repo_root: &Path,
    local_sha: &str,
    remote_sha: &str,
) -> Result<bool, String> {
    if remote_sha == "0000000000000000000000000000000000000000" || remote_sha.is_empty() {
        return Ok(false);
    }

    let output = crate::cmd::git_cmd()
        .args(["merge-base", "--is-ancestor", remote_sha, local_sha])
        .current_dir(repo_root)
        .output()
        .await
        .map_err(|e| format!("Failed to run merge-base: {}", e))?;

    match output.status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        code => Err(format!(
            "git merge-base failed with exit status {}: {}",
            code.map_or_else(
                || "terminated by signal".to_string(),
                |code| code.to_string()
            ),
            String::from_utf8_lossy(&output.stderr).trim()
        )),
    }
}

#[derive(Debug, Clone)]
pub struct PushRef {
    pub local_ref: String,
    pub local_sha: String,
    pub remote_ref: String,
    pub remote_sha: String,
}

pub fn get_push_refs<R: std::io::Read>(reader: &mut R) -> Result<Vec<PushRef>, String> {
    let mut stdin_str = String::new();
    reader
        .read_to_string(&mut stdin_str)
        .map_err(|e| format!("Failed to read stdin: {}", e))?;

    let mut refs = Vec::new();
    for (line_number, line) in stdin_str.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() != 4 {
            return Err(format!(
                "invalid pre-push ref record on line {}: expected 4 fields",
                line_number + 1
            ));
        }
        refs.push(PushRef {
            local_ref: parts[0].to_string(),
            local_sha: parts[1].to_string(),
            remote_ref: parts[2].to_string(),
            remote_sha: parts[3].to_string(),
        });
    }

    Ok(refs)
}

#[cfg(test)]
mod tests {
    use super::{StagedChangeKind, get_staged_files, get_staged_path_changes};
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

    fn committed_repository() -> tempfile::TempDir {
        let repo = tempdir().expect("temporary repository");
        git(repo.path(), &["init", "--quiet"]);
        git(repo.path(), &["config", "user.name", "Test User"]);
        git(repo.path(), &["config", "user.email", "test@example.com"]);
        fs::write(repo.path().join("removed.txt"), "tracked content").expect("write tracked file");
        git(repo.path(), &["add", "removed.txt"]);
        git(repo.path(), &["commit", "--quiet", "-m", "test baseline"]);
        repo
    }

    #[tokio::test]
    async fn staged_deletion_is_returned_without_blob_content() {
        let repo = committed_repository();
        fs::remove_file(repo.path().join("removed.txt")).expect("remove tracked file");
        git(repo.path(), &["add", "-u"]);

        let files = get_staged_files(repo.path())
            .await
            .expect("read staged files");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "removed.txt");
        assert_eq!(files[0].kind, StagedChangeKind::Deleted);
        assert!(files[0].content.is_none());
        assert_eq!(files[0].size_bytes, 0);
    }

    #[tokio::test]
    async fn staged_modification_retains_kind_and_blob_content() {
        let repo = committed_repository();
        fs::write(repo.path().join("removed.txt"), "updated content").expect("update tracked file");
        git(repo.path(), &["add", "removed.txt"]);

        let files = get_staged_files(repo.path())
            .await
            .expect("read staged files");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].kind, StagedChangeKind::Modified);
        assert_eq!(files[0].content.as_deref(), Some("updated content"));
    }

    #[tokio::test]
    async fn staged_rename_is_reported_as_both_path_changes() {
        let repo = committed_repository();
        git(repo.path(), &["mv", "removed.txt", "renamed.txt"]);

        let changes = get_staged_path_changes(repo.path())
            .await
            .expect("read staged path changes");
        let files = get_staged_files(repo.path())
            .await
            .expect("read staged files");

        assert!(changes.iter().any(|change| {
            change.path == "removed.txt" && change.kind == StagedChangeKind::Deleted
        }));
        assert!(changes.iter().any(|change| {
            change.path == "renamed.txt" && change.kind == StagedChangeKind::Added
        }));
        assert_eq!(files.len(), 2);
        assert!(
            files
                .iter()
                .any(|file| { file.path == "removed.txt" && file.content.is_none() })
        );
        assert!(files.iter().any(|file| {
            file.path == "renamed.txt" && file.content.as_deref() == Some("tracked content")
        }));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn staged_file_type_change_is_not_dropped() {
        let repo = committed_repository();
        fs::remove_file(repo.path().join("removed.txt")).expect("remove tracked file");
        std::os::unix::fs::symlink("target", repo.path().join("removed.txt"))
            .expect("replace file with symlink");
        git(repo.path(), &["add", "removed.txt"]);

        let files = get_staged_files(repo.path())
            .await
            .expect("read staged files");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].kind, StagedChangeKind::TypeChanged);
        assert_eq!(files[0].content.as_deref(), Some("target"));
    }

    #[tokio::test]
    async fn empty_index_returns_no_staged_files() {
        let repo = committed_repository();

        let files = get_staged_files(repo.path())
            .await
            .expect("read staged files");

        assert!(files.is_empty());
    }

    #[test]
    fn push_ref_parser_rejects_malformed_records() {
        let mut input = "refs/heads/main abc refs/heads/main".as_bytes();

        assert!(super::get_push_refs(&mut input).is_err());
    }

    #[test]
    fn push_ref_parser_preserves_each_ref_update() {
        let mut input =
            b"refs/heads/main a refs/heads/main b\nrefs/tags/v1 c refs/tags/v1 d\n".as_slice();

        let refs = super::get_push_refs(&mut input).expect("parse push refs");

        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].local_ref, "refs/heads/main");
        assert_eq!(refs[1].local_ref, "refs/tags/v1");
    }

    #[tokio::test]
    async fn force_push_check_reports_git_errors_instead_of_treating_them_as_force_pushes() {
        let repo = committed_repository();
        let result = super::is_force_push(
            repo.path(),
            "not-an-object-id",
            "0000000000000000000000000000000000000000",
        )
        .await;

        assert!(result.is_ok_and(|is_force_push| !is_force_push));

        let head = git_output(repo.path(), &["rev-parse", "HEAD"]);
        let head = String::from_utf8_lossy(&head.stdout).trim().to_string();
        let result = super::is_force_push(repo.path(), "not-an-object-id", &head).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn staged_submodule_additions_are_detected_from_gitlink_modes() {
        let repo = committed_repository();
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

        let additions = super::get_staged_submodule_additions(repo.path())
            .await
            .expect("inspect staged submodule additions");
        let files = get_staged_files(repo.path())
            .await
            .expect("read staged files");

        assert_eq!(additions, ["vendor/module"]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].kind, StagedChangeKind::Added);
        assert!(files[0].content.is_none());
    }

    #[tokio::test]
    async fn submodule_urls_come_from_the_index_not_the_working_tree() {
        let repo = committed_repository();
        let staged = "[submodule \"module\"]\n\turl = https://allowed.example/module\n";
        fs::write(repo.path().join(".gitmodules"), staged).expect("write staged config");
        git(repo.path(), &["add", ".gitmodules"]);
        fs::write(
            repo.path().join(".gitmodules"),
            "[submodule \"module\"]\n\turl = https://forbidden.example/module\n",
        )
        .expect("change worktree config");

        let urls = super::get_submodule_urls(repo.path())
            .await
            .expect("read staged submodule URLs");

        assert_eq!(urls, ["https://allowed.example/module"]);
    }

    #[tokio::test]
    async fn merge_state_lookup_finds_merge_marker() {
        let repo = committed_repository();
        assert!(
            !super::is_merge_in_progress(repo.path())
                .await
                .expect("inspect repository state")
        );

        let output = git_output(repo.path(), &["rev-parse", "--git-path", "MERGE_HEAD"]);
        assert!(output.status.success());
        let marker_path = String::from_utf8_lossy(&output.stdout);
        let marker = std::path::Path::new(marker_path.trim());
        let marker = if marker.is_absolute() {
            marker.to_path_buf()
        } else {
            repo.path().join(marker)
        };
        fs::write(marker, "merge-head\n").expect("write merge marker");

        assert!(
            super::is_merge_in_progress(repo.path())
                .await
                .expect("inspect repository merge state")
        );
    }
}
