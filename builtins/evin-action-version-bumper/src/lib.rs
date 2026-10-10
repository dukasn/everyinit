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

#![allow(clippy::result_large_err)]
pub mod v1;

use evin_action_sdk::{
    Action, ActionDescriptor, ActionFieldDescriptor, ActionFieldKind, ActionOutputDescriptor,
    RunnerContext,
};

const BUMPER_DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Calculate and apply the next release version.",
    fields: &[
        ActionFieldDescriptor {
            path: "initial_version",
            kind: ActionFieldKind::String,
            required: false,
            description: "Initial semantic version.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "channel",
            kind: ActionFieldKind::String,
            required: false,
            description: "Release channel.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "require_change",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Require a qualifying change.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "files",
            kind: ActionFieldKind::List,
            required: false,
            description: "Version file rewrites.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "files.path",
            kind: ActionFieldKind::String,
            required: true,
            description: "Version file path.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "files.match",
            kind: ActionFieldKind::String,
            required: true,
            description: "Version match pattern.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "files.write",
            kind: ActionFieldKind::String,
            required: true,
            description: "Version replacement template.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "git",
            kind: ActionFieldKind::Object,
            required: false,
            description: "Git release behavior.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "git.commit",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Commit rewritten files.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "git.tag",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Create a Git tag.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "git.defer",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Stage release changes for a later commit and tag.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "git.message",
            kind: ActionFieldKind::String,
            required: false,
            description: "Release commit message.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "git.include",
            kind: ActionFieldKind::List,
            required: false,
            description: "Additional paths included in the release commit.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "stable",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Force a stable release.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "read_only",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Calculate release state without changing the checkout.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "fixed_version",
            kind: ActionFieldKind::String,
            required: false,
            description: "Pin the version used for read-only package state.",
            condition: Some("read_only=true"),
        },
        ActionFieldDescriptor {
            path: "prerelease",
            kind: ActionFieldKind::String,
            required: false,
            description: "Prerelease identifier.",
            condition: None,
        },
    ],
    outputs: &[
        ActionOutputDescriptor {
            name: "version",
            description: "Resolved release version.",
        },
        ActionOutputDescriptor {
            name: "tag",
            description: "Resolved release tag.",
        },
        ActionOutputDescriptor {
            name: "release",
            description: "Structured JSON release artifact.",
        },
    ],
};

pub struct VersionBumperAction;

#[async_trait::async_trait]
impl Action for VersionBumperAction {
    fn editor_descriptor(&self) -> Option<&'static ActionDescriptor> {
        Some(&BUMPER_DESCRIPTOR)
    }
    fn validate_config(
        &self,
        with: Option<&evin_yaml::SpannedValue>,
    ) -> Result<(), evin_yaml::ConfigError> {
        v1::validate_config(with)
    }

    async fn execute(
        &self,
        ctx: &RunnerContext,
        with: Option<evin_yaml::SpannedValue>,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        v1::execute(ctx, with).await
    }
}
