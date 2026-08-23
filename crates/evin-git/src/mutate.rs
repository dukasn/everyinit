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

pub async fn stage_files(workspace_root: &Path, files: &[String]) -> Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }

    let mut args = vec!["add".to_string()];
    args.extend_from_slice(files);

    let output = crate::cmd::git_cmd()
        .args(&args)
        .current_dir(workspace_root)
        .output()
        .await
        .map_err(|e| format!("Failed to execute git add: {}", e))?;

    if !output.status.success() {
        let err_str = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git add failed: {}", err_str));
    }

    Ok(())
}

pub async fn commit(workspace_root: &Path, message: &str) -> Result<(), String> {
    let output = crate::cmd::git_cmd()
        .args(["commit", "-m", message])
        .current_dir(workspace_root)
        .output()
        .await
        .map_err(|e| format!("Failed to execute git commit: {}", e))?;

    if !output.status.success() {
        let err_str = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git commit failed: {}", err_str));
    }

    Ok(())
}

pub async fn create_tag(
    workspace_root: &Path,
    tag_name: &str,
    message: &str,
) -> Result<(), String> {
    let output = crate::cmd::git_cmd()
        .args(["tag", "-a", tag_name, "-m", message])
        .current_dir(workspace_root)
        .output()
        .await
        .map_err(|e| format!("Failed to execute git tag: {}", e))?;

    if !output.status.success() {
        let err_str = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git tag failed: {}", err_str));
    }

    Ok(())
}
