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

use super::config::CatalogConfig;
use evin_diagnostic::Diagnostic;
use std::path::Path;

pub(super) fn issue(message: impl Into<String>, code: &'static str) -> Diagnostic {
    evin_diagnostic::Diagnostic::new(message, evin_diagnostic::IssueCode::new(code))
}

#[allow(clippy::result_large_err)]
pub(super) fn compile(
    workspace: &Path,
    config: &CatalogConfig,
) -> Result<(evin_doc::CatalogExport, usize), Diagnostic> {
    let source_root = if config.source_dir.is_absolute() {
        config.source_dir.clone()
    } else {
        workspace.join(&config.source_dir)
    };
    let files =
        evin_doc::SourceResolver::resolve_all(&source_root, &config.include, &config.exclude)
            .map_err(|error| issue(error.message, error.code))?;
    let mut sources = Vec::with_capacity(files.len());
    for file in files {
        let content = std::fs::read_to_string(&file.absolute_path).map_err(|error| {
            issue(
                evin_locales::t!(
                    "catalog-source-read-failed",
                    path = file.absolute_path.display().to_string(),
                    error = error.to_string()
                ),
                "io-error",
            )
        })?;
        sources.push((file, content));
    }
    let inputs = sources
        .iter()
        .map(|(file, content)| evin_doc::DocumentInput {
            slug: file.slug.clone(),
            source_path: file.relative_path.to_string_lossy().to_string(),
            format: if file.is_rst {
                evin_doc::DocFormat::ReStructuredText
            } else {
                evin_doc::DocFormat::Markdown
            },
            is_start_page: matches!(
                file.relative_path.to_str(),
                Some("index.md") | Some("index.rst")
            ),
            content,
        });
    let prefix = config.reference_prefix.trim().to_ascii_uppercase();
    if prefix.is_empty() || !prefix.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Err(issue(
            evin_locales::t!("catalog-reference-prefix-invalid"),
            "invalid-config",
        ));
    }
    let site = evin_doc::compile_site(
        inputs,
        evin_doc::CompileOptions {
            extract_frontmatter: config.extract_frontmatter,
            reference_prefix: prefix,
            site_version: "1.0".into(),
        },
        None,
    )
    .map_err(|error| issue(error.message, error.code))?;
    let count = site.pages.len();
    let generated_at = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| "1970-01-01T00:00:00+00:00".into());
    let catalog = evin_doc::build_lsp_catalog(
        &site,
        format!("catalog-generator@{}", env!("CARGO_PKG_VERSION")),
        generated_at,
    );
    catalog.validate().map_err(|error| {
        issue(
            evin_locales::t!("catalog-output-invalid", error = error.to_string()),
            "invalid-catalog",
        )
    })?;
    Ok((catalog, count))
}
