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

const RELEASE_SYNC_DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Reconcile provider release state with local tags and artifacts.",
    fields: &[
        ActionFieldDescriptor {
            path: "tag",
            kind: ActionFieldKind::String,
            required: false,
            description: "Limit reconciliation to this local Git tag.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "title",
            kind: ActionFieldKind::String,
            required: false,
            description: "Explicit title for reconciled releases; defaults to the annotated tag subject.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "plan_out",
            kind: ActionFieldKind::String,
            required: false,
            description: "Generated release-sync plan path.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "dry_run",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Skip writing generated notes and plan files.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "state",
            kind: ActionFieldKind::String,
            required: false,
            description: "Provider-normalized release state path.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "notes_dir",
            kind: ActionFieldKind::String,
            required: false,
            description: "Release notes directory.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "artifacts_dir",
            kind: ActionFieldKind::String,
            required: false,
            description: "Artifact directory.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "changelog",
            kind: ActionFieldKind::String,
            required: false,
            description: "Changelog path.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "prune_orphans",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Prune remote orphan releases.",
            condition: None,
        },
    ],
    outputs: &[
        ActionOutputDescriptor {
            name: "plan",
            description: "Generated release-sync plan.",
        },
        ActionOutputDescriptor {
            name: "delta_count",
            description: "Number of reconciled changes.",
        },
        ActionOutputDescriptor {
            name: "reconciled",
            description: "Whether reconciliation changed state.",
        },
    ],
};

pub struct ReleaseSyncAction;

#[async_trait::async_trait]
impl Action for ReleaseSyncAction {
    fn editor_descriptor(&self) -> Option<&'static ActionDescriptor> {
        Some(&RELEASE_SYNC_DESCRIPTOR)
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
