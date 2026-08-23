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

//! Pure LSP catalog construction. Persistence belongs to the embedding application.

use crate::ast::{DocNode, DocSiteAst, LspCatalogExport, LspSymbolMeta};
use crate::{render_ansi, render_markdown, render_plain};
use std::collections::BTreeMap;
use std::path::Path;

pub fn build_lsp_catalog(
    site: &DocSiteAst,
    generator: impl Into<String>,
    generated_at: impl Into<String>,
) -> LspCatalogExport {
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
            synopsis,
            summary,
            category.clone(),
            handler,
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
            LspSymbolMeta {
                name: "[".to_owned(),
                synopsis: "[ <expression> ]".to_owned(),
                summary: "POSIX conditional expression brackets".to_owned(),
                category: test.category,
                handler: test.handler,
                hover_markdown: test.hover_markdown,
                hover_plaintext: test.hover_plaintext,
                terminal_ansi: test.terminal_ansi,
            },
        );
    }
    LspCatalogExport {
        version: 1,
        generator: generator.into(),
        generated_at: generated_at.into(),
        symbols,
        keywords,
    }
}

fn extract_lsp_symbols(
    nodes: &[DocNode],
    symbols: &mut BTreeMap<String, LspSymbolMeta>,
    keywords: &mut BTreeMap<String, LspSymbolMeta>,
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
                handler,
                children,
            } => {
                found = true;
                let meta = symbol_meta(
                    name.clone(),
                    synopsis.clone(),
                    summary.clone(),
                    category.clone(),
                    if handler.is_empty() {
                        default_handler(name)
                    } else {
                        handler.clone()
                    },
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
fn symbol_meta(
    name: String,
    synopsis: String,
    summary: String,
    category: String,
    handler: String,
    body: &[DocNode],
) -> LspSymbolMeta {
    let markdown = render_markdown(body);
    let plain = render_plain(body);
    let ansi = render_ansi(body);
    LspSymbolMeta {
        name: name.clone(),
        synopsis: synopsis.clone(),
        summary: summary.clone(),
        category,
        handler,
        hover_markdown: format!("```esh\n{synopsis}\n```\n\n**{name}**: {summary}\n\n{markdown}"),
        hover_plaintext: format!("{synopsis}\n\n{name}: {summary}\n\n{plain}"),
        terminal_ansi: format!(
            "\x1b[1;36m{synopsis}\x1b[0m\n\n\x1b[1m{name}\x1b[0m: {summary}\n\n{ansi}"
        ),
    }
}
