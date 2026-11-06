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

const INVOCATION_FLAGS: &[InvocationFlag] = &[InvocationFlag::new("host")];
const DOCS_DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Build, serve, or catalog project documentation.",
    fields: &[
        ActionFieldDescriptor {
            path: "target",
            kind: ActionFieldKind::Enum(&["web", "lsp"]),
            required: false,
            description: "Documentation output target.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "mode",
            kind: ActionFieldKind::Enum(&["build", "serve"]),
            required: false,
            description: "Web build or preview mode.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "site_url",
            kind: ActionFieldKind::String,
            required: true,
            description: "Canonical site URL.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "source_dir",
            kind: ActionFieldKind::String,
            required: false,
            description: "Documentation source directory.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "out",
            kind: ActionFieldKind::String,
            required: false,
            description: "LSP catalog output path.",
            condition: Some("target=lsp"),
        },
        ActionFieldDescriptor {
            path: "language",
            kind: ActionFieldKind::String,
            required: false,
            description: "Documentation language.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "asset_dir",
            kind: ActionFieldKind::String,
            required: false,
            description: "Static asset directory.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "include",
            kind: ActionFieldKind::List,
            required: false,
            description: "Included documentation paths.",
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
            path: "extract_frontmatter",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Extract page frontmatter.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "title",
            kind: ActionFieldKind::String,
            required: false,
            description: "Site title.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "theme",
            kind: ActionFieldKind::String,
            required: false,
            description: "Documentation theme.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "theme_overrides",
            kind: ActionFieldKind::Object,
            required: false,
            description: "Theme variable overrides.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "bundle",
            kind: ActionFieldKind::String,
            required: false,
            description: "Asset bundle strategy.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "validate_links",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Validate documentation links.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "port",
            kind: ActionFieldKind::Integer,
            required: false,
            description: "Preview server port.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "host",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Expose the preview server to the network.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "layout_dir",
            kind: ActionFieldKind::String,
            required: false,
            description: "Custom layout directory.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "favicon",
            kind: ActionFieldKind::String,
            required: false,
            description: "Favicon path.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "logo",
            kind: ActionFieldKind::String,
            required: false,
            description: "Logo path.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "extensions_dir",
            kind: ActionFieldKind::String,
            required: false,
            description: "Documentation extensions directory.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "theme_color",
            kind: ActionFieldKind::String,
            required: false,
            description: "Browser theme color.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "background_color",
            kind: ActionFieldKind::String,
            required: false,
            description: "Browser background color.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "icon_dir",
            kind: ActionFieldKind::String,
            required: false,
            description: "Icon source directory.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "minify",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Minify generated assets.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "reference_prefix",
            kind: ActionFieldKind::String,
            required: false,
            description: "Reference-page URL prefix.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "strict_filenames",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Require strict documentation filenames.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "topbar",
            kind: ActionFieldKind::Object,
            required: false,
            description: "Topbar configuration or false.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "topbar.id",
            kind: ActionFieldKind::String,
            required: false,
            description: "Topbar identifier.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "topbar.variant",
            kind: ActionFieldKind::String,
            required: true,
            description: "Topbar presentation variant.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "topbar.badge",
            kind: ActionFieldKind::String,
            required: false,
            description: "Topbar badge.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "topbar.message",
            kind: ActionFieldKind::String,
            required: true,
            description: "Topbar message.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "topbar.action",
            kind: ActionFieldKind::Object,
            required: false,
            description: "Topbar action.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "topbar.action.label",
            kind: ActionFieldKind::String,
            required: true,
            description: "Topbar action label.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "topbar.action.href",
            kind: ActionFieldKind::String,
            required: true,
            description: "Topbar action URL.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "topbar.dismissible",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Allow dismissing the topbar.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "presentation",
            kind: ActionFieldKind::Object,
            required: false,
            description: "Site presentation configuration.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "presentation.variant",
            kind: ActionFieldKind::String,
            required: true,
            description: "Site presentation variant.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "diagnostic_search",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Include diagnostic search information.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "metrics",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Emit documentation metrics.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "metrics_budget_kb",
            kind: ActionFieldKind::Integer,
            required: false,
            description: "Metrics budget in KiB.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "social_image",
            kind: ActionFieldKind::String,
            required: false,
            description: "Social sharing image.",
            condition: Some("target=web"),
        },
        ActionFieldDescriptor {
            path: "lastmod_fallback",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Fallback for last-modified metadata.",
            condition: Some("target=web"),
        },
    ],
    outputs: &[
        ActionOutputDescriptor {
            name: "output_dir",
            description: "Generated web output directory.",
        },
        ActionOutputDescriptor {
            name: "page_count",
            description: "Generated web page count.",
        },
        ActionOutputDescriptor {
            name: "output_file",
            description: "Generated LSP catalog file.",
        },
        ActionOutputDescriptor {
            name: "symbol_count",
            description: "Generated LSP catalog symbol count.",
        },
        ActionOutputDescriptor {
            name: "server_url",
            description: "Preview server URL.",
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
        INVOCATION_FLAGS
    }

    fn validate_config(
        &self,
        with: Option<&evin_flow::SpannedValue>,
    ) -> Result<(), evin_flow::ConfigError> {
        v1::validate_config(with)
    }

    async fn execute(
        &self,
        ctx: &RunnerContext,
        with: Option<evin_flow::SpannedValue>,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        v1::execute(ctx, with).await
    }

    async fn execute_with_invocation(
        &self,
        ctx: &RunnerContext,
        with: Option<evin_flow::SpannedValue>,
        flags: &InvocationFlags,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        v1::execute_with_invocation(ctx, with, flags).await
    }
}
