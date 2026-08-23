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

#[derive(Debug, Clone)]
pub struct StagedFileRaw {
    pub path: String,
    pub content: String,
    pub is_binary: bool,
    pub size_bytes: usize,
    pub is_lfs_tracked: bool,
    pub status: String,
}

pub async fn get_staged_files(workspace_root: &Path) -> Result<Vec<StagedFileRaw>, String> {
    let output = crate::cmd::git_cmd()
        .args([
            "diff",
            "--cached",
            "--name-status",
            "-z",
            "--diff-filter=ACM",
        ])
        .current_dir(workspace_root)
        .output()
        .await
        .map_err(|e| format!("Failed to execute git diff: {}", e))?;

    if !output.status.success() {
        return Err("git diff failed".to_string());
    }

    let mut files_info = Vec::new();
    let names_output = String::from_utf8_lossy(&output.stdout);
    let mut iter = names_output.split('\0');
    while let Some(status) = iter.next() {
        if status.is_empty() {
            break;
        }
        if let Some(path) = iter.next() {
            files_info.push((status.to_string(), path.to_string()));
        }
    }

    if files_info.is_empty() {
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
            for (_, path) in &files_info {
                let _ = stdin.write_all(path.as_bytes()).await;
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
    for (status, path) in files_info {
        // 1. Get exact staged file size
        let size_output = crate::cmd::git_cmd()
            .args(["cat-file", "-s", &format!(":{}", path)])
            .current_dir(workspace_root)
            .output()
            .await
            .map_err(|e| format!("git cat-file failed: {}", e))?;

        if !size_output.status.success() {
            continue; // Could not stat, maybe deleted concurrently
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

            if show_output.status.success() {
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
            content,
            is_binary,
            size_bytes,
            is_lfs_tracked,
            status,
        });
    }

    Ok(staged)
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
    let output = crate::cmd::git_cmd()
        .args(["config", "--file", ".gitmodules", "--get-regexp", "url"])
        .current_dir(repo_root)
        .output()
        .await
        .map_err(|e| format!("Failed to execute git config: {}", e))?;

    if !output.status.success() {
        return Ok(vec![]);
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

    let status = crate::cmd::git_cmd()
        .args(["merge-base", "--is-ancestor", remote_sha, local_sha])
        .current_dir(repo_root)
        .status()
        .await
        .map_err(|e| format!("Failed to run merge-base: {}", e))?;

    Ok(!status.success())
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
    for line in stdin_str.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() == 4 {
            refs.push(PushRef {
                local_ref: parts[0].to_string(),
                local_sha: parts[1].to_string(),
                remote_ref: parts[2].to_string(),
                remote_sha: parts[3].to_string(),
            });
        }
    }

    Ok(refs)
}
