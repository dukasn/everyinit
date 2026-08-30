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

//! Evin action adapter for the pure `evin-doc` LSP catalog builder.

use crate::v1::DocsConfig;
use evin_doc::DocSiteAst;
use std::path::{Path, PathBuf};

fn reproducible_generated_at() -> String {
    std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
        .map(|timestamp| timestamp.to_rfc3339())
        .unwrap_or_else(|| "1970-01-01T00:00:00+00:00".to_string())
}

pub fn build_lsp_catalog(
    fs: &evin_action_sdk::fs::WorkspaceFs,
    config: &DocsConfig,
    site: &DocSiteAst,
    logger: &evin_action_sdk::Logger,
) -> Result<PathBuf, evin_diagnostic::Diagnostic> {
    let generator = format!("docs-generator@{}", env!("CARGO_PKG_VERSION"));
    let generated_at = reproducible_generated_at();
    let catalog = evin_doc::build_lsp_catalog(site, generator, generated_at);
    let out_path = if Path::new(&config.out).is_absolute() {
        PathBuf::from(&config.out)
    } else {
        fs.workspace_root().join(&config.out)
    };
    let json = serde_json::to_string_pretty(&catalog).map_err(|error| {
        evin_diagnostic::Diagnostic::new(
            format!("Failed to serialize LSP catalog JSON: {error}"),
            evin_diagnostic::IssueCode::new("json-error"),
        )
    })?;
    let out_request = out_path.to_string_lossy();
    let unchanged = fs
        .read_to_string(&out_request)
        .map(|current| current == json)
        .unwrap_or(false);
    if !unchanged {
        fs.write(&out_request, json).map_err(|error| {
            evin_diagnostic::Diagnostic::new(
                format!(
                    "Failed to write LSP catalog to {}: {error}",
                    out_path.display()
                ),
                evin_diagnostic::IssueCode::new("io-error"),
            )
        })?;
    }
    evin_action_sdk::action_event!(
        logger,
        evin_action_sdk::activity::DocsGenerated {
            output: out_path.display().to_string()
        }
    );
    Ok(out_path)
}
