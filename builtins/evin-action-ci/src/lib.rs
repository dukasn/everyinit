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

const CI_DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Reconcile remote CI release state.",
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
            description: "Explicit title for reconciled GitHub releases; defaults to the annotated tag subject.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "plan_out",
            kind: ActionFieldKind::String,
            required: false,
            description: "Generated CI plan path.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "dry_run",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Plan without publishing changes.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "state",
            kind: ActionFieldKind::String,
            required: false,
            description: "Persisted CI state path.",
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
            description: "Generated CI plan.",
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

pub struct CiAction;

#[async_trait::async_trait]
impl Action for CiAction {
    fn editor_descriptor(&self) -> Option<&'static ActionDescriptor> {
        Some(&CI_DESCRIPTOR)
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
