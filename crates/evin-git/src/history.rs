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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitCommit {
    pub hash: String,
    pub subject: String,
    pub refs: String,
    pub files_changed: Vec<String>,
}

pub async fn fetch_all_commits(workspace_root: &Path) -> Result<Vec<GitCommit>, String> {
    let output = crate::cmd::git_cmd()
        .args(["log", "--name-only", "--pretty=format:|||%h|%D|%s"])
        .current_dir(workspace_root)
        .output()
        .await
        .map_err(|e| format!("Failed to execute git log: {}", e))?;

    if !output.status.success() {
        let err_str = String::from_utf8_lossy(&output.stderr);
        // If it's just not a git repository yet, we can return empty history safely
        // instead of crashing.
        if err_str.contains("not a git repository") || err_str.contains("does not have any commits")
        {
            return Ok(Vec::new());
        }
        return Err(format!("git log failed: {}", err_str));
    }

    let log_str = String::from_utf8_lossy(&output.stdout);
    let mut commits = Vec::new();

    for block in log_str.split("|||") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }

        let mut lines = block.lines();
        let metadata_line = match lines.next() {
            Some(line) => line.trim(),
            None => continue,
        };

        let mut parts = metadata_line.splitn(3, '|');
        let hash = parts.next().unwrap_or("").trim().to_string();
        let refs = parts.next().unwrap_or("").trim().to_string();
        let subject = parts.next().unwrap_or("").trim().to_string();

        let mut files_changed = Vec::new();
        for file_line in lines {
            let file_line = file_line.trim();
            if !file_line.is_empty() {
                files_changed.push(file_line.to_string());
            }
        }

        commits.push(GitCommit {
            hash,
            subject,
            refs,
            files_changed,
        });
    }

    Ok(commits)
}
