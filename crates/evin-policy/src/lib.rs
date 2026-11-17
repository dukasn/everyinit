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

pub mod hooks;
pub mod reporter;

use evin_diagnostic::IssueCode;
use evin_repository_policy::models::RepositoryPolicy;
use evin_repository_policy::validator::ValidationError;
use std::collections::HashMap;
use std::path::Path;

/// Errors that can occur during policy enforcement.
pub enum PolicyError {
    /// A policy validation rule was violated.
    Violation {
        error: ValidationError,
        issue_code: IssueCode,
    },
    /// An error occurred while checking git state or reading files.
    GitState(String),
    /// A required argument for the hook was missing from the payload.
    MissingArgument {
        message: String,
        hint: Option<String>,
    },
    /// Allow empty commits failure.
    EmptyCommitsNotAllowed,
}

/// A trait for git hooks that enforce policies.
pub const KNOWN_HOOKS: &[&str] = &["pre-commit", "pre-push", "commit-msg"];

pub trait GitHook: Send + Sync {
    /// The name of the git hook this trait enforces (e.g., "pre-commit").
    fn hook_name(&self) -> &'static str;

    fn enforce(
        &self,
        rules: &RepositoryPolicy,
        repo_root: &Path,
        payload: &[String],
    ) -> impl std::future::Future<Output = Result<(), Vec<PolicyError>>> + Send;
}

pub enum HookVariant {
    PreCommit(hooks::pre_commit::PreCommitHook),
    PrePush(hooks::pre_push::PrePushHook),
    CommitMsg(hooks::commit_msg::CommitMsgHook),
}

impl GitHook for HookVariant {
    fn hook_name(&self) -> &'static str {
        match self {
            HookVariant::PreCommit(h) => h.hook_name(),
            HookVariant::PrePush(h) => h.hook_name(),
            HookVariant::CommitMsg(h) => h.hook_name(),
        }
    }

    async fn enforce(
        &self,
        rules: &RepositoryPolicy,
        repo_root: &Path,
        payload: &[String],
    ) -> Result<(), Vec<PolicyError>> {
        match self {
            HookVariant::PreCommit(h) => h.enforce(rules, repo_root, payload).await,
            HookVariant::PrePush(h) => h.enforce(rules, repo_root, payload).await,
            HookVariant::CommitMsg(h) => h.enforce(rules, repo_root, payload).await,
        }
    }
}

/// The engine that manages and executes git hooks based on policies.
pub struct PolicyEngine {
    hooks: HashMap<&'static str, HookVariant>,
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyEngine {
    /// Create a new `PolicyEngine` with all default hooks registered.
    pub fn new() -> Self {
        let mut engine = Self {
            hooks: HashMap::new(),
        };
        engine.register(HookVariant::PreCommit(hooks::pre_commit::PreCommitHook));
        engine.register(HookVariant::PrePush(hooks::pre_push::PrePushHook));
        engine.register(HookVariant::CommitMsg(hooks::commit_msg::CommitMsgHook));
        engine
    }

    /// Register a custom hook.
    pub fn register(&mut self, hook: HookVariant) {
        self.hooks.insert(hook.hook_name(), hook);
    }

    /// Execute the policy enforcement for the given hook type.
    pub async fn enforce(
        &self,
        hook_type: &str,
        rules: &RepositoryPolicy,
        repo_root: &Path,
        payload: &[String],
    ) -> Result<(), Vec<PolicyError>> {
        if let Some(hook) = self.hooks.get(hook_type) {
            hook.enforce(rules, repo_root, payload).await
        } else {
            // Unhandled hook types bypass validation successfully.
            Ok(())
        }
    }
}
