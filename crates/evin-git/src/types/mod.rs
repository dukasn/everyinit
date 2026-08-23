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

use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HookError {
    #[error("Failed to parse git hook: {0}")]
    ParseError(String),
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to find .git directory: {0}")]
    GitDirectoryNotFound(String),
    #[error("Failed to create hooks dir: {0}")]
    HooksDirectoryCreateFailed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GitHook {
    ApplyPatchMsg,
    PreApplyPatch,
    PostApplyPatch,
    PreCommit,
    PreMergeCommit,
    PrepareCommitMsg,
    CommitMsg,
    PostCommit,
    PreRebase,
    PostCheckout,
    PostMerge,
    PrePush,
    PreReceive,
    Update,
    PostReceive,
    PostUpdate,
    ReferenceTransaction,
    PushToCheckout,
    PreAutoGc,
    PostRewrite,
    SendemailValidate,
    FsmonitorWatchman,
    P4Changelist,
    P4PrepareChangelist,
    P4PostChangelist,
    P4PreSubmit,
    PostIndexChange,
}

impl GitHook {
    pub fn as_str(&self) -> &'static str {
        match self {
            GitHook::ApplyPatchMsg => "applypatch-msg",
            GitHook::PreApplyPatch => "pre-applypatch",
            GitHook::PostApplyPatch => "post-applypatch",
            GitHook::PreCommit => "pre-commit",
            GitHook::PreMergeCommit => "pre-merge-commit",
            GitHook::PrepareCommitMsg => "prepare-commit-msg",
            GitHook::CommitMsg => "commit-msg",
            GitHook::PostCommit => "post-commit",
            GitHook::PreRebase => "pre-rebase",
            GitHook::PostCheckout => "post-checkout",
            GitHook::PostMerge => "post-merge",
            GitHook::PrePush => "pre-push",
            GitHook::PreReceive => "pre-receive",
            GitHook::Update => "update",
            GitHook::PostReceive => "post-receive",
            GitHook::PostUpdate => "post-update",
            GitHook::ReferenceTransaction => "reference-transaction",
            GitHook::PushToCheckout => "push-to-checkout",
            GitHook::PreAutoGc => "pre-auto-gc",
            GitHook::PostRewrite => "post-rewrite",
            GitHook::SendemailValidate => "sendemail-validate",
            GitHook::FsmonitorWatchman => "fsmonitor-watchman",
            GitHook::P4Changelist => "p4-changelist",
            GitHook::P4PrepareChangelist => "p4-prepare-changelist",
            GitHook::P4PostChangelist => "p4-post-changelist",
            GitHook::P4PreSubmit => "p4-pre-submit",
            GitHook::PostIndexChange => "post-index-change",
        }
    }
}

impl FromStr for GitHook {
    type Err = HookError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "applypatch-msg" => Ok(GitHook::ApplyPatchMsg),
            "pre-applypatch" => Ok(GitHook::PreApplyPatch),
            "post-applypatch" => Ok(GitHook::PostApplyPatch),
            "pre-commit" => Ok(GitHook::PreCommit),
            "pre-merge-commit" => Ok(GitHook::PreMergeCommit),
            "prepare-commit-msg" => Ok(GitHook::PrepareCommitMsg),
            "commit-msg" => Ok(GitHook::CommitMsg),
            "post-commit" => Ok(GitHook::PostCommit),
            "pre-rebase" => Ok(GitHook::PreRebase),
            "post-checkout" => Ok(GitHook::PostCheckout),
            "post-merge" => Ok(GitHook::PostMerge),
            "pre-push" => Ok(GitHook::PrePush),
            "pre-receive" => Ok(GitHook::PreReceive),
            "update" => Ok(GitHook::Update),
            "post-receive" => Ok(GitHook::PostReceive),
            "post-update" => Ok(GitHook::PostUpdate),
            "reference-transaction" => Ok(GitHook::ReferenceTransaction),
            "push-to-checkout" => Ok(GitHook::PushToCheckout),
            "pre-auto-gc" => Ok(GitHook::PreAutoGc),
            "post-rewrite" => Ok(GitHook::PostRewrite),
            "sendemail-validate" => Ok(GitHook::SendemailValidate),
            "fsmonitor-watchman" => Ok(GitHook::FsmonitorWatchman),
            "p4-changelist" => Ok(GitHook::P4Changelist),
            "p4-prepare-changelist" => Ok(GitHook::P4PrepareChangelist),
            "p4-post-changelist" => Ok(GitHook::P4PostChangelist),
            "p4-pre-submit" => Ok(GitHook::P4PreSubmit),
            "post-index-change" => Ok(GitHook::PostIndexChange),
            _ => Err(HookError::ParseError(s.to_string())),
        }
    }
}
