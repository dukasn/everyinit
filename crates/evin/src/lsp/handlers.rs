// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 of the License.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use super::catalog::{catalog_augmented_markdown, catalog_entry};
use super::protocol::*;
use super::server::{EvinLspServer, lsp_symbol_kind};
use crate::commands::builtin_actions;
use evin_config::semantic::STANDARD_SEMANTIC_TOKEN_TYPES;
use evin_config::source::OverlaySource;
use evin_lsp::LspDocument;
use evin_lsp::protocol::{
    CompletionItem, DocumentSymbol, Hover, Location, MarkupContent, SemanticTokens, TextEdit,
    WorkspaceSymbol,
};
use serde_json::{Value, json};
use std::io::{self, Write};
use std::path::PathBuf;

impl EvinLspServer {
    // -----------------------------------------------------------------------
    // Request Handlers
    // -----------------------------------------------------------------------

    pub(super) fn on_initialize(&mut self, params: InitializeParams) -> Result<Value, LspError> {
        let root = workspace_root(&params.root_uri)?;
        let canonical_root = root.canonicalize().unwrap_or_else(|_| root.clone());
        self.root = Some(root.clone());
        self.canonical_root = Some(canonical_root);
        self.source = OverlaySource::for_root(root);
        self.snippet_support = params
            .capabilities
            .pointer("/textDocument/completion/completionItem/snippetSupport")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.initialized = true;
        self.needs_initial_rebuild = true;

        Ok(json!({
            "capabilities": {
                "textDocumentSync": 1,
                "hoverProvider": true,
                "completionProvider": { "triggerCharacters": [":", "$", "{", ".", "/"] },
                "definitionProvider": true,
                "referencesProvider": true,
                "documentSymbolProvider": true,
                "workspaceSymbolProvider": true,
                "semanticTokensProvider": {
                    "legend": {
                        "tokenTypes": STANDARD_SEMANTIC_TOKEN_TYPES,
                        "tokenModifiers": []
                    },
                    "full": true
                }
            },
            "serverInfo": { "name": "evin-lsp", "version": env!("CARGO_PKG_VERSION") }
        }))
    }

    pub(super) fn on_shutdown(&mut self, _params: Value) -> Result<Value, LspError> {
        self.shutdown_requested = true;
        Ok(Value::Null)
    }

    pub(super) fn on_completion(
        &mut self,
        params: TextDocumentPositionParams,
    ) -> Result<Vec<CompletionItem>, LspError> {
        let items = self
            .position_offset(&params.text_document.uri, &params.position.into())
            .map(|(path, offset)| {
                self.index().completions_at_with_source(
                    &path,
                    offset,
                    &builtin_actions(),
                    &self.source,
                )
            })
            .unwrap_or_default()
            .into_iter()
            .map(|candidate| {
                let documentation = catalog_augmented_markdown(
                    &candidate.documentation,
                    candidate.catalog_target.as_ref(),
                );
                let new_text = if self.snippet_support {
                    candidate.snippet.clone()
                } else {
                    plain_text(&candidate.snippet)
                };
                let text_edit = candidate.replacement.as_ref().and_then(|range| {
                    Some(TextEdit {
                        range: self.range(range)?,
                        new_text: new_text.clone(),
                    })
                });
                CompletionItem {
                    label: candidate.label,
                    kind: Some(10),
                    detail: Some(candidate.detail),
                    documentation: Some(MarkupContent {
                        kind: "markdown".into(),
                        value: documentation,
                    }),
                    filter_text: candidate.filter_text,
                    insert_text: text_edit.is_none().then_some(new_text),
                    insert_text_format: self.snippet_support.then_some(2),
                    text_edit,
                }
            })
            .collect();

        Ok(items)
    }

    pub(super) fn on_hover(
        &mut self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<Hover>, LspError> {
        let hover = self
            .position_offset(&params.text_document.uri, &params.position.into())
            .and_then(|(path, offset)| {
                self.index()
                    .hover_at(&path, offset)
                    .map(|info| Hover {
                        contents: MarkupContent {
                            kind: "markdown".into(),
                            value: catalog_augmented_markdown(
                                &info.markdown,
                                info.catalog_target.as_ref(),
                            ),
                        },
                        range: self.range(&info.range),
                    })
                    .or_else(|| {
                        self.session.stale_symbol_at(&path, offset).map(|symbol| {
                            let markdown = catalog_entry(symbol.catalog_target.as_ref())
                                .map(|entry| entry.hover_markdown.clone())
                                .unwrap_or_else(|| {
                                    format!("**{}**\n\n{}", symbol.name, symbol.detail)
                                });
                            Hover {
                                contents: MarkupContent {
                                    kind: "markdown".into(),
                                    value: format!(
                                        "{markdown}\n\n_Stale local fact while the current YAML buffer is invalid._"
                                    ),
                                },
                                range: self.range(&symbol.range),
                            }
                        })
                    })
            });

        Ok(hover)
    }

    pub(super) fn on_semantic_tokens(
        &mut self,
        params: DocumentUriParams,
    ) -> Result<SemanticTokens, LspError> {
        let data = self
            .relative_path(&params.text_document.uri)
            .map(|path| self.semantic_token_data(&path))
            .unwrap_or_default();
        Ok(SemanticTokens { data })
    }

    pub(super) fn on_definition(
        &mut self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<Location>, LspError> {
        let location = self
            .position_offset(&params.text_document.uri, &params.position.into())
            .and_then(|(path, offset)| self.index().definition_at(&path, offset))
            .and_then(|range| self.location(&range));
        Ok(location)
    }

    pub(super) fn on_references(
        &mut self,
        params: ReferenceParams,
    ) -> Result<Vec<Location>, LspError> {
        let locations = self
            .position_offset(&params.text_document.uri, &params.position.into())
            .map(|(path, offset)| {
                let mut ranges = self.index().references_at(&path, offset);
                if params.context.include_declaration
                    && let Some(declaration) =
                        self.index().definition_at(&path, offset).or_else(|| {
                            self.index()
                                .node_at(&path, offset)
                                .map(|node| node.range.clone())
                        })
                {
                    ranges.push(declaration);
                }
                ranges.sort_by(|left, right| {
                    (&left.path, left.start, left.end).cmp(&(&right.path, right.start, right.end))
                });
                ranges.dedup();
                ranges
                    .into_iter()
                    .filter_map(|range| self.location(&range))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(locations)
    }

    pub(super) fn on_document_symbols(
        &mut self,
        params: DocumentUriParams,
    ) -> Result<Vec<DocumentSymbol>, LspError> {
        let symbols = self
            .relative_path(&params.text_document.uri)
            .map(|path| {
                let mut symbols = self
                    .index()
                    .document_symbols(&path)
                    .into_iter()
                    .filter_map(|symbol| self.document_symbol(symbol))
                    .collect::<Vec<_>>();
                symbols.extend(
                    self.session
                        .stale_document_symbols(&path)
                        .into_iter()
                        .filter_map(|symbol| self.document_symbol(symbol)),
                );
                symbols
            })
            .unwrap_or_default();
        Ok(symbols)
    }

    pub(super) fn on_workspace_symbols(
        &mut self,
        params: WorkspaceSymbolParams,
    ) -> Result<Vec<WorkspaceSymbol>, LspError> {
        let symbols = self
            .index()
            .workspace_symbols(&params.query)
            .into_iter()
            .filter_map(|symbol| {
                Some(WorkspaceSymbol {
                    name: symbol.name.clone(),
                    kind: lsp_symbol_kind(symbol.kind),
                    location: self.location(&symbol.range)?,
                    container_name: symbol.owner.clone(),
                })
            })
            .collect();
        Ok(symbols)
    }

    // -----------------------------------------------------------------------
    // Notification Handlers
    // -----------------------------------------------------------------------

    pub(super) fn on_did_open<W: Write>(
        &mut self,
        params: DidOpenTextDocumentParams,
        writer: &mut W,
    ) -> io::Result<()> {
        let Ok(path) = self.relative_path(&params.text_document.uri) else {
            return Ok(());
        };
        self.source.insert(&path, params.text_document.text.clone());
        self.documents.open(LspDocument::new(
            params.text_document.uri,
            params.text_document.version,
            params.text_document.text,
        ));
        self.rebuild_and_publish(writer)
    }

    pub(super) fn on_did_change<W: Write>(
        &mut self,
        params: DidChangeTextDocumentParams,
        writer: &mut W,
    ) -> io::Result<()> {
        let Some(change) = params.content_changes.into_iter().next() else {
            return Ok(());
        };
        if change.range.is_some() {
            return Ok(());
        }
        let Ok(path) = self.relative_path(&params.text_document.uri) else {
            return Ok(());
        };
        self.source.insert(&path, change.text.clone());
        if let Some(document) = self.documents.get_mut(&params.text_document.uri) {
            document.update(params.text_document.version, change.text);
        }
        self.rebuild_and_publish(writer)
    }

    pub(super) fn on_did_close<W: Write>(
        &mut self,
        params: DidCloseTextDocumentParams,
        writer: &mut W,
    ) -> io::Result<()> {
        if let Ok(path) = self.relative_path(&params.text_document.uri) {
            self.source.remove(&path);
        }
        self.documents.close(&params.text_document.uri);
        self.rebuild_and_publish(writer)
    }

    pub(super) fn on_exit<W: Write>(&mut self, _params: Value, _writer: &mut W) -> io::Result<()> {
        std::process::exit(if self.shutdown_requested { 0 } else { 1 });
    }
}

fn workspace_root(root_uri: &Option<String>) -> Result<PathBuf, LspError> {
    match root_uri.as_deref() {
        Some(uri) => url::Url::parse(uri)
            .map_err(|error| LspError::invalid_params(error.to_string()))?
            .to_file_path()
            .map_err(|_| LspError::invalid_params("initialize.rootUri must be a file URI")),
        None => {
            std::env::current_dir().map_err(|error| LspError::invalid_params(error.to_string()))
        }
    }
}

fn plain_text(snippet: &str) -> String {
    snippet.replace("${1}", "")
}
