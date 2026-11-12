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
    summary: "Compile, serve, and validate static project documentation.",
    fields: &[
        ActionFieldDescriptor {
            path: "mode",
            kind: ActionFieldKind::Enum(&["build", "serve"]),
            required: false,
            description: "Web build or preview mode.",
            condition: None,
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
            description: "Generated web site directory.",
            condition: None,
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
            path: "bundle",
            kind: ActionFieldKind::Enum(&["separated", "single-file"]),
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
            condition: None,
        },
        ActionFieldDescriptor {
            path: "host",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Expose the preview server to the network.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "layout_dir",
            kind: ActionFieldKind::String,
            required: false,
            description: "Custom layout directory.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "extensions_dir",
            kind: ActionFieldKind::String,
            required: false,
            description: "Documentation extensions directory.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "theme_color",
            kind: ActionFieldKind::String,
            required: false,
            description: "Browser theme color.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "background_color",
            kind: ActionFieldKind::String,
            required: false,
            description: "Browser background color.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "icon_dir",
            kind: ActionFieldKind::String,
            required: false,
            description: "Icon source directory.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "minify",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Minify generated assets.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "reference_prefix",
            kind: ActionFieldKind::String,
            required: false,
            description: "Reference-page URL prefix.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "strict_filenames",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Require strict documentation filenames.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "grammars",
            kind: ActionFieldKind::List,
            required: false,
            description: "Additional syntax grammar files.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "appearance",
            kind: ActionFieldKind::Object,
            required: false,
            description: "Documentation appearance tokens and assets.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "appearance.accent",
            kind: ActionFieldKind::String,
            required: false,
            description: "Primary accent color in #RRGGBB format.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "appearance.logo",
            kind: ActionFieldKind::String,
            required: false,
            description: "Optional logo path.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "appearance.favicon",
            kind: ActionFieldKind::String,
            required: false,
            description: "Optional favicon path.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "appearance.font_sans",
            kind: ActionFieldKind::String,
            required: false,
            description: "System font stack override for sans-serif text.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "appearance.font_mono",
            kind: ActionFieldKind::String,
            required: false,
            description: "System font stack override for code.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "appearance.custom_css",
            kind: ActionFieldKind::String,
            required: false,
            description: "CSS file appended in the overrides layer.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "version",
            kind: ActionFieldKind::Object,
            required: false,
            description: "Version registry and current release.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "version.current",
            kind: ActionFieldKind::String,
            required: true,
            description: "Version emitted by this invocation.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "version.latest",
            kind: ActionFieldKind::String,
            required: true,
            description: "Latest version shown in the version switcher.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "version.manifest",
            kind: ActionFieldKind::String,
            required: false,
            description: "Version registry path relative to the output directory.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "i18n",
            kind: ActionFieldKind::Object,
            required: false,
            description: "Localized documentation sources and UI labels.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "i18n.default_locale",
            kind: ActionFieldKind::String,
            required: true,
            description: "Fallback locale and default source tree.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "i18n.locales",
            kind: ActionFieldKind::List,
            required: true,
            description: "Locales to compile.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "i18n.dictionaries_dir",
            kind: ActionFieldKind::String,
            required: false,
            description: "Directory containing locale UI dictionaries.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "serve",
            kind: ActionFieldKind::Object,
            required: false,
            description: "Preview server settings.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "serve.host",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Expose the preview server to the network.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "serve.port",
            kind: ActionFieldKind::Integer,
            required: false,
            description: "Preview server port.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "topbar",
            kind: ActionFieldKind::Object,
            required: false,
            description: "Topbar configuration or false.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "topbar.id",
            kind: ActionFieldKind::String,
            required: false,
            description: "Topbar identifier.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "topbar.badge",
            kind: ActionFieldKind::String,
            required: false,
            description: "Topbar badge.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "topbar.message",
            kind: ActionFieldKind::String,
            required: true,
            description: "Topbar message.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "topbar.action",
            kind: ActionFieldKind::Object,
            required: false,
            description: "Topbar action.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "topbar.action.label",
            kind: ActionFieldKind::String,
            required: true,
            description: "Topbar action label.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "topbar.action.href",
            kind: ActionFieldKind::String,
            required: true,
            description: "Topbar action URL.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "topbar.dismissible",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Allow dismissing the topbar.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "diagnostic_search",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Include diagnostic search information.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "social_image",
            kind: ActionFieldKind::String,
            required: false,
            description: "Social sharing image.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "lastmod_fallback",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Fallback for last-modified metadata.",
            condition: None,
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
