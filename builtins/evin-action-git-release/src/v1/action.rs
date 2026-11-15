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

use evin_action_sdk::{
    Action, ActionDescriptor, ActionFieldDescriptor, ActionFieldKind, ActionOutputDescriptor,
    RunnerContext,
};

const GIT_RELEASE_DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Commit and tag a release.",
    fields: &[
        ActionFieldDescriptor {
            path: "stage",
            kind: ActionFieldKind::Enum(&["all", "existing"]),
            required: false,
            description: "Files to stage before committing.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "message",
            kind: ActionFieldKind::String,
            required: true,
            description: "Release commit message.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "tag",
            kind: ActionFieldKind::String,
            required: true,
            description: "Release tag.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "enabled",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Run the release; false skips it and reports committed false.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "push",
            kind: ActionFieldKind::Object,
            required: false,
            description: "Publish the branch and tag after committing.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "push.remote",
            kind: ActionFieldKind::String,
            required: false,
            description: "Remote that receives the refs.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "push.branch",
            kind: ActionFieldKind::String,
            required: false,
            description: "Branch to push; defaults to the current branch.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "push.force",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Overwrite the remote branch and tag.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "push.prune_stale_tags",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Delete unreleased remote tags that match the tag format.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "push.base",
            kind: ActionFieldKind::String,
            required: false,
            description: "Branch whose history defines which tags are released.",
            condition: None,
        },
    ],
    outputs: &[
        ActionOutputDescriptor {
            name: "tag",
            description: "Created release tag.",
        },
        ActionOutputDescriptor {
            name: "committed",
            description: "Whether a release commit was created.",
        },
    ],
};

pub struct GitReleaseAction;

#[async_trait::async_trait]
impl Action for GitReleaseAction {
    fn editor_descriptor(&self) -> Option<&'static ActionDescriptor> {
        Some(&GIT_RELEASE_DESCRIPTOR)
    }
    fn validate_config(
        &self,
        with: Option<&evin_flow::SpannedValue>,
    ) -> Result<(), evin_flow::ConfigError> {
        super::validate_config(with)
    }

    async fn execute(
        &self,
        ctx: &RunnerContext,
        with: Option<evin_flow::SpannedValue>,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        super::execute(ctx, with).await
    }
}
