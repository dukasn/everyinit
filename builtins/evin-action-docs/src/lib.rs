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
    InvocationFlag, InvocationFlags, RunnerContext,
};

const DOCS_DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Parse, validate, and publish structured documentation data.",
    fields: &[
        ActionFieldDescriptor {
            path: "source_dir",
            kind: ActionFieldKind::String,
            required: false,
            description: "Documentation source root.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "out",
            kind: ActionFieldKind::String,
            required: false,
            description: "Generated data package directory.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "include",
            kind: ActionFieldKind::List,
            required: false,
            description: "Included documentation selectors.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "exclude",
            kind: ActionFieldKind::List,
            required: false,
            description: "Excluded documentation paths.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "asset_dir",
            kind: ActionFieldKind::String,
            required: false,
            description: "Source directory for referenced local media.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "extract_frontmatter",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Extract page frontmatter metadata.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "strict_filenames",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Require ordered route-safe filenames.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "reference_prefix",
            kind: ActionFieldKind::String,
            required: false,
            description: "Formal reference identifier prefix.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "validate_links",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Validate document links and anchors.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "i18n",
            kind: ActionFieldKind::Object,
            required: false,
            description: "Locale inventory and default locale.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "i18n.default_locale",
            kind: ActionFieldKind::String,
            required: false,
            description: "Default documentation locale.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "i18n.locales",
            kind: ActionFieldKind::List,
            required: false,
            description: "Locales included in the data package.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "versions",
            kind: ActionFieldKind::List,
            required: false,
            description: "Version source directories to include.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "versions.id",
            kind: ActionFieldKind::String,
            required: true,
            description: "Path-safe version identifier.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "versions.source_dir",
            kind: ActionFieldKind::String,
            required: false,
            description: "Source root for this version.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "latest_version",
            kind: ActionFieldKind::String,
            required: false,
            description: "Version whose routes omit a version prefix.",
            condition: None,
        },
    ],
    outputs: &[
        ActionOutputDescriptor {
            name: "manifest_path",
            description: "Generated route and content manifest.",
        },
        ActionOutputDescriptor {
            name: "page_count",
            description: "Number of generated page variants.",
        },
    ],
};

pub struct DocsAction;

#[async_trait::async_trait]
impl Action for DocsAction {
    fn editor_descriptor(&self) -> Option<&'static ActionDescriptor> {
        Some(&DOCS_DESCRIPTOR)
    }

    fn invocation_flags(&self) -> &'static [InvocationFlag] {
        &[]
    }

    fn validate_config(
        &self,
        with: Option<&evin_flow::SpannedValue>,
    ) -> Result<(), evin_flow::ConfigError> {
        v1::headless::validate_config(with)
    }

    async fn execute(
        &self,
        ctx: &RunnerContext,
        with: Option<evin_flow::SpannedValue>,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        v1::headless::execute(ctx, with).await
    }

    async fn execute_with_invocation(
        &self,
        ctx: &RunnerContext,
        with: Option<evin_flow::SpannedValue>,
        _flags: &InvocationFlags,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        v1::headless::execute(ctx, with).await
    }
}
