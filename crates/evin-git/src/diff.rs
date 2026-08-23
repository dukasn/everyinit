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

pub async fn diff_files(
    workspace_root: &Path,
    from: &str,
    to: &str,
) -> Result<Vec<String>, String> {
    let output = crate::cmd::git_cmd()
        .args(["diff", "--name-only", from, to])
        .current_dir(workspace_root)
        .output()
        .await
        .map_err(|e| format!("Failed to execute git diff: {}", e))?;

    if !output.status.success() {
        let err_str = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git diff failed: {}", err_str));
    }

    let diff_str = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();

    for line in diff_str.lines() {
        let line = line.trim();
        if !line.is_empty() {
            files.push(line.to_string());
        }
    }

    Ok(files)
}
