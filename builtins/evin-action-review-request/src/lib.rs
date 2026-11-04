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

const fn field(
    path: &'static str,
    kind: ActionFieldKind,
    required: bool,
    description: &'static str,
) -> ActionFieldDescriptor {
    ActionFieldDescriptor {
        path,
        kind,
        required,
        description,
        condition: None,
    }
}

const REVIEW_REQUEST_DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Keep one review request in sync with a branch.",
    fields: &[
        field("branch", ActionFieldKind::String, true, "Head branch."),
        field("base", ActionFieldKind::String, true, "Target branch."),
        field(
            "wanted",
            ActionFieldKind::Boolean,
            false,
            "Whether a review request should exist.",
        ),
        field("title", ActionFieldKind::String, false, "Request title."),
        field(
            "body_path",
            ActionFieldKind::String,
            false,
            "Markdown file used as the request body.",
        ),
        field(
            "commands",
            ActionFieldKind::Object,
            true,
            "Shell commands that talk to the forge.",
        ),
        field(
            "commands.find",
            ActionFieldKind::String,
            true,
            "Print the identifier of the open request, if any.",
        ),
        field(
            "commands.create",
            ActionFieldKind::String,
            false,
            "Create the request.",
        ),
        field(
            "commands.update",
            ActionFieldKind::String,
            false,
            "Refresh the title and body of the existing request.",
        ),
        field(
            "commands.close",
            ActionFieldKind::String,
            false,
            "Close the existing request.",
        ),
        field(
            "context",
            ActionFieldKind::Object,
            false,
            "Extra values exposed as EVIN_REVIEW_CONTEXT_<KEY>.",
        ),
    ],
    outputs: &[
        ActionOutputDescriptor {
            name: "state",
            description: "created, updated, closed, or none.",
        },
        ActionOutputDescriptor {
            name: "id",
            description: "Identifier of the request, when one exists.",
        },
    ],
};

pub struct ReviewRequestAction;

#[async_trait::async_trait]
impl Action for ReviewRequestAction {
    fn editor_descriptor(&self) -> Option<&'static ActionDescriptor> {
        Some(&REVIEW_REQUEST_DESCRIPTOR)
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
