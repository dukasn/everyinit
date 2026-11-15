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

use super::compiler::{compile, issue};
use super::config::CatalogConfig;
use evin_action_sdk::{
    Action, ActionDescriptor, ActionFieldDescriptor, ActionFieldKind, ActionOutputDescriptor,
    RunnerContext,
};

const DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Compile documentation symbols and keywords into a tooling catalog.",
    fields: &[
        ActionFieldDescriptor {
            path: "source_dir",
            kind: ActionFieldKind::String,
            required: false,
            description: "Markdown and reStructuredText source directory.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "out",
            kind: ActionFieldKind::String,
            required: false,
            description: "Generated catalog JSON path.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "include",
            kind: ActionFieldKind::List,
            required: false,
            description: "Included documentation paths or globs.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "exclude",
            kind: ActionFieldKind::List,
            required: false,
            description: "Excluded documentation paths or globs.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "extract_frontmatter",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Extract Markdown YAML frontmatter.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "reference_prefix",
            kind: ActionFieldKind::String,
            required: false,
            description: "Formal reference identifier prefix.",
            condition: None,
        },
    ],
    outputs: &[
        ActionOutputDescriptor {
            name: "output_file",
            description: "Generated catalog JSON path.",
        },
        ActionOutputDescriptor {
            name: "symbol_count",
            description: "Number of exported symbols and keywords.",
        },
    ],
};

pub struct CatalogAction;

#[async_trait::async_trait]
impl Action for CatalogAction {
    fn editor_descriptor(&self) -> Option<&'static ActionDescriptor> {
        Some(&DESCRIPTOR)
    }

    fn validate_config(
        &self,
        with: Option<&evin_flow::SpannedValue>,
    ) -> Result<(), evin_flow::ConfigError> {
        evin_action_sdk::parse_config::<CatalogConfig>(with).map(|_| ())
    }

    async fn execute(
        &self,
        ctx: &RunnerContext,
        with: Option<evin_flow::SpannedValue>,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        let config: CatalogConfig = ctx.load_config(with.as_ref()).map_err(|error| {
            issue(
                evin_locales::t!("catalog-invalid-config", error = error.message),
                "invalid-config",
            )
        })?;
        let (catalog, page_count) = compile(ctx.fs.workspace_root(), &config)?;
        let output = if config.out.is_absolute() {
            config.out
        } else {
            ctx.fs.workspace_root().join(config.out)
        };
        let json = serde_json::to_string_pretty(&catalog).map_err(|error| {
            issue(
                evin_locales::t!(
                    "catalog-json-serialization-failed",
                    error = error.to_string()
                ),
                "json-error",
            )
        })?;
        ctx.fs
            .write(&output.to_string_lossy(), json)
            .map_err(|error| {
                issue(
                    evin_locales::t!("catalog-output-write-failed", error = error.to_string()),
                    "io-error",
                )
            })?;
        let entry_count = catalog.symbols.len() + catalog.keywords.len();
        ctx.state
            .set_output("output_file", &output.to_string_lossy());
        ctx.state
            .set_output("symbol_count", &entry_count.to_string());
        let _ = page_count;
        let _ = entry_count;
        Ok(())
    }
}
