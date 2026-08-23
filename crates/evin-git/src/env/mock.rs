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
use std::collections::HashMap;
use std::sync::Mutex;

pub struct MockGitEnv {
    hooks: Mutex<HashMap<GitHook, String>>,
    executable_flags: Mutex<HashMap<GitHook, bool>>,
}

impl Default for MockGitEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl MockGitEnv {
    pub fn new() -> Self {
        Self {
            hooks: Mutex::new(HashMap::new()),
            executable_flags: Mutex::new(HashMap::new()),
        }
    }

    pub fn hook_is_executable(&self, hook: &GitHook) -> bool {
        self.executable_flags
            .lock()
            .unwrap()
            .get(hook)
            .copied()
            .unwrap_or(false)
    }
}

impl HookEnvironment for MockGitEnv {
    async fn read_hook(&self, hook: &GitHook) -> Result<Option<String>, HookError> {
        let hooks = self.hooks.lock().unwrap();
        Ok(hooks.get(hook).cloned())
    }

    async fn write_hook(&self, hook: &GitHook, content: &str) -> Result<(), HookError> {
        let mut hooks = self.hooks.lock().unwrap();
        hooks.insert(hook.clone(), content.to_string());
        Ok(())
    }

    async fn make_executable(&self, hook: &GitHook) -> Result<(), HookError> {
        let mut flags = self.executable_flags.lock().unwrap();
        flags.insert(hook.clone(), true);
        Ok(())
    }

    async fn is_executable(&self, hook: &GitHook) -> Result<bool, HookError> {
        Ok(self.hook_is_executable(hook))
    }
}
