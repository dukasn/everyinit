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

pub mod v1;

use evin_action_sdk::{
    Action, ActionDescriptor, ActionFieldDescriptor, ActionFieldKind, ActionOutputDescriptor,
    RunnerContext,
};

const CHANGELOG_DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Generate a changelog section.",
    fields: &[
        ActionFieldDescriptor {
            path: "path",
            kind: ActionFieldKind::String,
            required: false,
            description: "Changelog path.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "version",
            kind: ActionFieldKind::String,
            required: false,
            description: "Release version.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "label",
            kind: ActionFieldKind::String,
            required: false,
            description: "Optional label appended to the generated pending section heading.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "notes_path",
            kind: ActionFieldKind::String,
            required: false,
            description: "Optional file receiving only the pending section body.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "include_unreleased",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Include the unreleased section.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "preamble",
            kind: ActionFieldKind::String,
            required: false,
            description: "Changelog preamble.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "show_hashes",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Show commit hashes.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "show_scopes",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Show conventional-commit scopes.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "unreleased_heading",
            kind: ActionFieldKind::String,
            required: false,
            description: "Unreleased section heading.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "sections",
            kind: ActionFieldKind::Object,
            required: false,
            description: "Change-type section labels.",
            condition: None,
        },
    ],
    outputs: &[
        ActionOutputDescriptor {
            name: "path",
            description: "Updated changelog path.",
        },
        ActionOutputDescriptor {
            name: "notes_path",
            description: "Pending section notes path, when written.",
        },
        ActionOutputDescriptor {
            name: "version",
            description: "Published version.",
        },
        ActionOutputDescriptor {
            name: "tag",
            description: "Published tag.",
        },
    ],
};

pub struct ChangelogAction;

#[async_trait::async_trait]
impl Action for ChangelogAction {
    fn editor_descriptor(&self) -> Option<&'static ActionDescriptor> {
        Some(&CHANGELOG_DESCRIPTOR)
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
