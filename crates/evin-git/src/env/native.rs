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

use crate::env::HookEnvironment;
use crate::types::{GitHook, HookError};
use std::path::PathBuf;
use tokio::fs;

pub struct NativeGitEnv {
    hooks_dir: PathBuf,
}

impl NativeGitEnv {
    pub async fn new() -> Result<Self, HookError> {
        let output = tokio::process::Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .output()
            .await
            .map_err(|e| HookError::GitDirectoryNotFound(e.to_string()))?;

        if !output.status.success() {
            return Err(HookError::GitDirectoryNotFound(
                "Failed to execute git rev-parse".to_string(),
            ));
        }

        let git_dir_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let git_dir = PathBuf::from(git_dir_str);

        Ok(Self {
            hooks_dir: git_dir.join("hooks"),
        })
    }
}

impl HookEnvironment for NativeGitEnv {
    async fn read_hook(&self, hook: &GitHook) -> Result<Option<String>, HookError> {
        let path = self.hooks_dir.join(hook.as_str());
        if path.exists() {
            Ok(Some(fs::read_to_string(path).await?))
        } else {
            Ok(None)
        }
    }

    async fn write_hook(&self, hook: &GitHook, content: &str) -> Result<(), HookError> {
        fs::create_dir_all(&self.hooks_dir)
            .await
            .map_err(|e| HookError::HooksDirectoryCreateFailed(e.to_string()))?;
        let path = self.hooks_dir.join(hook.as_str());
        fs::write(path, content).await?;
        Ok(())
    }

    #[cfg(unix)]
    async fn make_executable(&self, hook: &GitHook) -> Result<(), HookError> {
        use std::os::unix::fs::PermissionsExt;
        let path = self.hooks_dir.join(hook.as_str());
        let mut perms = fs::metadata(&path).await?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).await?;
        Ok(())
    }

    #[cfg(unix)]
    async fn is_executable(&self, hook: &GitHook) -> Result<bool, HookError> {
        use std::os::unix::fs::PermissionsExt;
        let path = self.hooks_dir.join(hook.as_str());
        if !path.exists() {
            return Ok(false);
        }
        Ok(fs::metadata(path).await?.permissions().mode() & 0o111 != 0)
    }

    #[cfg(not(unix))]
    async fn make_executable(&self, _hook: &GitHook) -> Result<(), HookError> {
        // No-op on non-Unix platforms
        Ok(())
    }

    #[cfg(not(unix))]
    async fn is_executable(&self, hook: &GitHook) -> Result<bool, HookError> {
        Ok(self.hooks_dir.join(hook.as_str()).exists())
    }
}
