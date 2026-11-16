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
use evin_diagnostic::{Diagnostic, IssueCode};
use std::path::Path;

fn source_issue(issue: evin_doc::DocIssue) -> Diagnostic {
    let path = issue.source_path.unwrap_or_default();
    match issue.code {
        "invalid-nav-order" => evin_locales::diagnostic(
            IssueCode::new("catalog-source-navigation-order-invalid").arg("path", path),
        ),
        "formal-reference-invalid" => evin_locales::diagnostic(
            IssueCode::new("catalog-source-formal-reference-invalid").arg("error", issue.message),
        ),
        _ => evin_locales::diagnostic(
            IssueCode::new("catalog-source-compilation-failed")
                .arg("path", path)
                .arg("error", issue.message),
        ),
    }
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
            .map_err(source_issue)?;
    let mut sources = Vec::with_capacity(files.len());
    for file in files {
        let content = std::fs::read_to_string(&file.absolute_path).map_err(|error| {
            evin_locales::diagnostic(
                IssueCode::new("catalog-source-read-failed")
                    .arg("path", file.absolute_path.display().to_string())
                    .arg("error", error.to_string()),
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
        return Err(evin_locales::diagnostic(IssueCode::new(
            "catalog-reference-prefix-invalid",
        )));
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
    .map_err(source_issue)?;
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
        evin_locales::diagnostic(
            IssueCode::new("catalog-output-invalid").arg("error", error.to_string()),
        )
    })?;
    Ok((catalog, count))
}

#[cfg(test)]
mod tests {
    use super::source_issue;

    #[test]
    fn unclassified_source_diagnostic_uses_fluent_fallback() {
        let error = source_issue(
            evin_doc::DocIssue::error("document-parse-failed", "unexpected heading structure")
                .with_source_path("guide.md"),
        );
        let expected = evin_locales::t!(
            "catalog-source-compilation-failed",
            path = "guide.md",
            error = "unexpected heading structure"
        );
        assert_eq!(error.code.key, "catalog-source-compilation-failed");
        assert_eq!(evin_locales::render_issue(&error.code), expected);
        assert_eq!(
            error.title,
            evin_locales::t!("catalog-source-compilation-failed-title")
        );
    }
}
