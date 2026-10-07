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

//! Pure documentation-catalog construction. Persistence belongs to the embedding application.

use crate::ast::{DocNode, DocSiteAst};
use crate::{render_ansi, render_markdown, render_plain};
use evin_catalog::{CATALOG_VERSION_V1, CatalogEntry, CatalogExport};
use std::collections::BTreeMap;
use std::path::Path;

pub fn build_lsp_catalog(
    site: &DocSiteAst,
    generator: impl Into<String>,
    generated_at: impl Into<String>,
) -> CatalogExport {
    let mut symbols = BTreeMap::new();
    let mut keywords = BTreeMap::new();
    for page in &site.pages {
        if extract_lsp_symbols(&page.ast, &mut symbols, &mut keywords) {
            continue;
        }
        let name = page
            .metadata
            .get("name")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                Path::new(&page.source_path)
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or(&page.slug)
                    .to_owned()
            });
        let synopsis = metadata(page, "synopsis");
        let summary = metadata(page, "summary");
        let category = page
            .metadata
            .get("category")
            .and_then(|value| value.as_str())
            .unwrap_or("general")
            .to_owned();
        let handler = page
            .metadata
            .get("handler")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| default_handler(&name));
        let body: Vec<_> = page.ast.iter().filter(|node| !matches!(node, DocNode::Heading { text, .. } if text.trim().eq_ignore_ascii_case(&name))).cloned().collect();
        let meta = symbol_meta(
            name.clone(),
            None,
            synopsis,
            summary,
            category.clone(),
            handler,
            "esh",
            &body,
        );
        if page.source_path.contains("keywords") || category == "keyword" {
            keywords.insert(name, meta);
        } else {
            symbols.insert(name, meta);
        }
    }
    if !symbols.contains_key("[")
        && let Some(test) = symbols.get("test").cloned()
    {
        symbols.insert(
            "[".to_owned(),
            CatalogEntry {
                name: "[".to_owned(),
                synopsis: "[ <expression> ]".to_owned(),
                summary: "POSIX conditional expression brackets".to_owned(),
                category: test.category,
                binding: test.binding,
                hover_markdown: test.hover_markdown,
                hover_plaintext: test.hover_plaintext,
                terminal_ansi: test.terminal_ansi,
            },
        );
    }
    CatalogExport {
        version: CATALOG_VERSION_V1,
        generator: generator.into(),
        generated_at: generated_at.into(),
        symbols,
        keywords,
    }
}

fn extract_lsp_symbols(
    nodes: &[DocNode],
    symbols: &mut BTreeMap<String, CatalogEntry>,
    keywords: &mut BTreeMap<String, CatalogEntry>,
) -> bool {
    let mut found = false;
    for node in nodes {
        match node {
            DocNode::LspSymbol {
                name,
                kind,
                synopsis,
                summary,
                category,
                binding,
                target,
                language,
                title,
                children,
                ..
            } => {
                found = true;
                let meta = symbol_meta(
                    name.clone(),
                    title.as_deref(),
                    synopsis.clone(),
                    summary.clone(),
                    category.clone(),
                    if binding.is_empty() && target.is_none() {
                        default_handler(name)
                    } else {
                        binding.clone()
                    },
                    language
                        .as_deref()
                        .unwrap_or(if target.is_some() { "text" } else { "esh" }),
                    children,
                );
                if kind == "keyword" || category == "keyword" {
                    keywords.insert(name.clone(), meta);
                } else {
                    symbols.insert(name.clone(), meta);
                }
            }
            DocNode::Directive { body, .. } | DocNode::Admonition { children: body, .. } => {
                found |= extract_lsp_symbols(body, symbols, keywords)
            }
            _ => {}
        }
    }
    found
}

fn metadata(page: &crate::ast::DocPage, key: &str) -> String {
    page.metadata
        .get(key)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned()
}
fn default_handler(name: &str) -> String {
    match name {
        "break" => "builtins::r#break".to_owned(),
        "continue" => "builtins::r#continue".to_owned(),
        "return" => "builtins::r#return".to_owned(),
        "[" => "builtins::test".to_owned(),
        _ => format!("builtins::{name}"),
    }
}
#[allow(clippy::too_many_arguments)]
fn symbol_meta(
    name: String,
    title: Option<&str>,
    synopsis: String,
    summary: String,
    category: String,
    binding: String,
    language: &str,
    body: &[DocNode],
) -> CatalogEntry {
    let markdown = render_markdown(body);
    let plain = render_plain(body);
    let ansi = render_ansi(body);
    let label = title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(&name);
    CatalogEntry {
        name: name.clone(),
        synopsis: synopsis.clone(),
        summary: summary.clone(),
        category,
        binding,
        hover_markdown: format!(
            "```{language}\n{synopsis}\n```\n\n**{label}** (`{name}`): {summary}\n\n{markdown}"
        ),
        hover_plaintext: format!("{synopsis}\n\n{label} ({name}): {summary}\n\n{plain}"),
        terminal_ansi: format!(
            "\x1b[1;36m{synopsis}\x1b[0m\n\n\x1b[1m{label}\x1b[0m ({name}): {summary}\n\n{ansi}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::build_lsp_catalog;
    use crate::MarkdownParser;
    use crate::ast::{DocFormat, DocPage, DocSiteAst};
    use std::collections::HashMap;

    #[test]
    fn generic_target_uses_its_language_and_empty_binding() {
        let page = DocPage {
            slug: "workspace".into(),
            source_path: "workspace.md".into(),
            format: DocFormat::Markdown,
            metadata: HashMap::new(),
            reference: None,
            toc: Vec::new(),
            ast: MarkdownParser::parse(
                "````lsp\ntarget: [workspace, tasks]\nlanguage: flow\ntitle: Task definitions\nsynopsis: 'tasks:'\nsummary: Define tasks\n---\nTask prose.\n````",
                true,
            )
            .ast,
        };
        let catalog = build_lsp_catalog(
            &DocSiteAst {
                version: "1".into(),
                title: None,
                reference_prefix: "REF".into(),
                pages: vec![page],
            },
            "test",
            "1970-01-01T00:00:00Z",
        );
        let entry = &catalog.symbols["workspace.tasks"];
        assert!(entry.binding.is_empty());
        assert!(entry.hover_markdown.starts_with("```flow\ntasks:"));
        assert!(
            entry
                .hover_markdown
                .contains("**Task definitions** (`workspace.tasks`):")
        );
        assert!(entry.hover_markdown.contains("Task prose."));
    }
}
