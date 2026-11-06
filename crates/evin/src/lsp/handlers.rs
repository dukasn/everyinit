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
use evin_config::semantic::{DocumentKind, STANDARD_SEMANTIC_TOKEN_TYPES, SemanticKind};
use evin_config::source::{OverlaySource, normalize};
use evin_diagnostic::{Diagnostic as EvinDiagnostic, FileLocation, Severity};
use evin_lsp::LspDocument;
use evin_lsp::protocol::{
    CompletionItem, DocumentSymbol, Hover, Location, MarkupContent, Position, Range,
    SemanticTokens, TextEdit, WorkspaceSymbol,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

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
                                        "{markdown}\n\n_Stale local fact while the current Flow buffer is invalid._"
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

    pub(super) fn on_workspace_catalog(
        &mut self,
        _params: Value,
    ) -> Result<WorkspaceCatalogResult, LspError> {
        let Some(index) = self.session.current.as_ref() else {
            return Ok(WorkspaceCatalogResult::default());
        };

        let mut counts_by_path = BTreeMap::<PathBuf, (usize, usize)>::new();
        let mut total_errors = 0_usize;
        let mut total_warnings = 0_usize;

        let mut record_diag = |diagnostic: &EvinDiagnostic, fallback: Option<&str>| {
            let raw_path = match &diagnostic.location {
                Some(FileLocation::ByteSpan { path, .. })
                | Some(FileLocation::Path { path, .. }) => PathBuf::from(path),
                None => fallback
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(evin_config::paths::WORKSPACE_MANIFEST)),
            };
            let entry = counts_by_path.entry(normalize(raw_path)).or_default();
            match diagnostic.severity {
                Severity::Error => {
                    entry.0 += 1;
                    total_errors += 1;
                }
                Severity::Warning => {
                    entry.1 += 1;
                    total_warnings += 1;
                }
                Severity::Info => {}
            }
        };

        for diagnostic in &index.diagnostics {
            record_diag(diagnostic, None);
        }
        for finding in &self.findings {
            record_diag(&finding.diagnostic, finding.source_file.as_deref());
        }

        let manifests = index
            .documents()
            .map(|document| {
                let (errors, warnings) = counts_by_path
                    .get(&document.path)
                    .copied()
                    .unwrap_or((0, 0));
                let kind = match document.kind {
                    DocumentKind::Workspace => "workspace",
                    DocumentKind::Module => "module",
                    DocumentKind::Namespace => "namespace",
                    DocumentKind::GitRules => "gitrules",
                    DocumentKind::Environment => "environment",
                    DocumentKind::Unknown => "unknown",
                };
                CatalogManifestItem {
                    path: document.path.to_string_lossy().replace('\\', "/"),
                    uri: self.uri_for(&document.path),
                    kind: kind.to_string(),
                    errors,
                    warnings,
                }
            })
            .collect::<Vec<_>>();

        let Some(config) = &self.config else {
            return Ok(WorkspaceCatalogResult {
                stale: self.config_stale,
                diagnostics: CatalogDiagnosticCounts {
                    errors: total_errors,
                    warnings: total_warnings,
                },
                manifests,
                ..Default::default()
            });
        };

        let exposed: HashSet<&str> = config.expose.iter().map(String::as_str).collect();
        let mut sorted_tasks: Vec<_> = config.tasks.iter().collect();
        sorted_tasks.sort_by(|left, right| left.name.cmp(&right.name));

        let tasks = sorted_tasks
            .into_iter()
            .map(|task| {
                let (module, short_name) = match task.name.split_once(':') {
                    Some((module, rest)) => (Some(module.to_string()), rest.to_string()),
                    None => (None, task.name.clone()),
                };
                let source_path = task
                    .source_file
                    .clone()
                    .unwrap_or_else(|| evin_config::paths::WORKSPACE_MANIFEST.to_string())
                    .replace('\\', "/");

                let symbol_range = index
                    .symbols
                    .iter()
                    .find(|sym| sym.kind == SemanticKind::Task && sym.name == task.name)
                    .map(|sym| &sym.range)
                    .or_else(|| {
                        self.session
                            .stale_symbols
                            .values()
                            .flatten()
                            .find(|sym| sym.kind == SemanticKind::Task && sym.name == task.name)
                            .map(|sym| &sym.range)
                    });

                let location = symbol_range
                    .and_then(|range| self.location(range))
                    .or_else(|| {
                        let uri = self.uri_for(Path::new(&source_path));
                        (!uri.is_empty()).then_some(Location {
                            uri,
                            range: Range {
                                start: Position {
                                    line: 0,
                                    character: 0,
                                },
                                end: Position {
                                    line: 0,
                                    character: 0,
                                },
                            },
                        })
                    });

                let description = task
                    .description
                    .as_deref()
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .map(String::from);

                CatalogTaskItem {
                    name: task.name.clone(),
                    short_name,
                    module,
                    description,
                    stage: task.stage.clone(),
                    needs: task.needs.clone().unwrap_or_default(),
                    hooks: task
                        .on
                        .as_ref()
                        .and_then(|on| on.hooks.clone())
                        .unwrap_or_default(),
                    has_watch: task.watch.is_some(),
                    watch_paths: task
                        .watch
                        .as_ref()
                        .map(|watch| watch.paths.clone())
                        .unwrap_or_default(),
                    presets: task.presets.clone().unwrap_or_default(),
                    exposed: exposed.contains(task.name.as_str()),
                    source_path,
                    location,
                }
            })
            .collect();

        Ok(WorkspaceCatalogResult {
            workspace_name: Some(config.workspace_name.clone()),
            workspace_version: Some(config.workspace_version.clone()),
            stages: config.stages.clone(),
            stale: self.config_stale,
            diagnostics: CatalogDiagnosticCounts {
                errors: total_errors,
                warnings: total_warnings,
            },
            manifests,
            tasks,
        })
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
