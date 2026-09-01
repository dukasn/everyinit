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
    pub message: String,
    pub refs: String,
    pub files_changed: Vec<String>,
}

pub async fn fetch_all_commits(workspace_root: &Path) -> Result<Vec<GitCommit>, String> {
    let output = crate::cmd::git_cmd()
        .args([
            "log",
            "--name-only",
            "--pretty=format:|||%h%x1f%D%x1f%s%x1f%B%x1e",
        ])
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

        let Some((metadata, file_section)) = block.split_once('\u{1e}') else {
            continue;
        };
        let mut parts = metadata.trim().splitn(4, '\u{1f}');
        let hash = parts.next().unwrap_or("").trim().to_string();
        let refs = parts.next().unwrap_or("").trim().to_string();
        let subject = parts.next().unwrap_or("").trim().to_string();
        let message = parts.next().unwrap_or(&subject).trim().to_string();

        let mut files_changed = Vec::new();
        for file_line in file_section.lines() {
            let file_line = file_line.trim();
            if !file_line.is_empty() {
                files_changed.push(file_line.to_string());
            }
        }

        commits.push(GitCommit {
            hash,
            subject,
            message,
            refs,
            files_changed,
        });
    }

    Ok(commits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn retains_commit_body_and_files_separately() {
        let temp = tempfile::TempDir::new().unwrap();
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "Test"][..],
            &["config", "user.email", "test@example.test"][..],
            &["config", "commit.gpgSign", "false"][..],
        ] {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(temp.path())
                    .status()
                    .unwrap()
                    .success()
            );
        }
        std::fs::write(temp.path().join("file.txt"), "content").unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["add", "file.txt"])
                .current_dir(temp.path())
                .status()
                .unwrap()
                .success()
        );
        assert!(
            std::process::Command::new("git")
                .args([
                    "commit",
                    "-m",
                    "chore(release): refresh package revisions",
                    "-m",
                    "Evin-Release-Kind: package"
                ])
                .current_dir(temp.path())
                .status()
                .unwrap()
                .success()
        );
        let commits = fetch_all_commits(temp.path()).await.unwrap();
        assert_eq!(
            commits[0].subject,
            "chore(release): refresh package revisions"
        );
        assert!(commits[0].message.contains("Evin-Release-Kind: package"));
        assert_eq!(commits[0].files_changed, ["file.txt"]);
    }
}
