// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 of the License.

//! Evin's workspace-aware Language Server Protocol adapter.

use crate::commands::builtin_actions;
use evin_catalog::{CatalogEntry, CatalogExport};
use evin_config::semantic::{
    CatalogTarget, STANDARD_SEMANTIC_TOKEN_TYPES, SemanticKind, SourceRange,
    WorkspaceAnalysisSession,
};
use evin_config::source::{OverlaySource, normalize};
use evin_config::{WorkspaceSemanticAnalysis, WorkspaceSnapshot};
use evin_diagnostic::{Diagnostic as EvinDiagnostic, FileLocation, Severity};
use evin_lsp::protocol::{
    CompletionItem, Diagnostic, DocumentSymbol, Hover, JsonRpcError, JsonRpcNotification,
    JsonRpcRequest, JsonRpcResponse, Location, MarkupContent, Position, PublishDiagnosticsParams,
    Range, SemanticTokens, TextEdit, WorkspaceSymbol, read_message, write_message,
};
use evin_lsp::{DocumentStore, LspDocument, SourcePositionIndex};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static CONFIG_CATALOG: LazyLock<CatalogExport> = LazyLock::new(|| {
    CatalogExport::from_json(include_str!("../assets/config-catalog.json"))
        .expect("embedded Evin configuration catalog must be valid")
});

fn catalog_entry(target: Option<&CatalogTarget>) -> Option<&'static CatalogEntry> {
    target.and_then(|target| CONFIG_CATALOG.symbols.get(target.as_str()))
}

fn catalog_augmented_markdown(local: &str, target: Option<&CatalogTarget>) -> String {
    let Some(target) = target else {
        return local.into();
    };
    let mut candidate = target.as_str();
    loop {
        if let Some(entry) = CONFIG_CATALOG.symbols.get(candidate) {
            return format!("{local}\n\n{}", entry.hover_markdown);
        }
        let Some((parent, _)) = candidate.rsplit_once('.') else {
            return local.into();
        };
        candidate = parent;
    }
}

#[derive(Default)]
pub struct EvinLspServer {
    initialized: bool,
    shutdown_requested: bool,
    root: Option<PathBuf>,
    source: OverlaySource,
    documents: DocumentStore,
    session: WorkspaceAnalysisSession,
    findings: Vec<evin_config::analysis::AnalysisFinding>,
    snippet_support: bool,
    published: BTreeSet<String>,
}

impl EvinLspServer {
    pub fn handle_message<W: Write>(&mut self, message: &str, writer: &mut W) -> io::Result<()> {
        let value: Value = match serde_json::from_str(message) {
            Ok(value) => value,
            Err(error) => return self.error(writer, Value::Null, -32700, error.to_string()),
        };
        if value.get("id").is_some() {
            match serde_json::from_value::<JsonRpcRequest>(value) {
                Ok(request) => self.handle_request(request, writer),
                Err(error) => self.error(writer, Value::Null, -32600, error.to_string()),
            }
        } else {
            match serde_json::from_value::<JsonRpcNotification>(value) {
                Ok(notification) => self.handle_notification(notification, writer),
                Err(error) => self.error(writer, Value::Null, -32600, error.to_string()),
            }
        }
    }

    fn handle_request<W: Write>(
        &mut self,
        request: JsonRpcRequest,
        writer: &mut W,
    ) -> io::Result<()> {
        if !self.initialized && request.method != "initialize" {
            return self.error(writer, request.id, -32002, "Server not initialized");
        }
        match request.method.as_str() {
            "initialize" => {
                let params = request.params.unwrap_or_else(|| json!({}));
                let root = match workspace_root(&params) {
                    Ok(root) => root,
                    Err(message) => return self.error(writer, request.id, -32602, message),
                };
                self.root = Some(root.clone());
                self.source = OverlaySource::for_root(root);
                self.snippet_support = params
                    .pointer("/capabilities/textDocument/completion/completionItem/snippetSupport")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.initialized = true;
                self.response(
                    writer,
                    request.id,
                    json!({
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
                    }),
                )?;
                self.rebuild_and_publish(writer)
            }
            "shutdown" => {
                self.shutdown_requested = true;
                self.response(writer, request.id, Value::Null)
            }
            "textDocument/completion" => {
                let items = self
                    .position_from_params(request.params.as_ref())
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
                    .collect::<Vec<_>>();
                self.response(writer, request.id, serde_json::to_value(items).unwrap())
            }
            "textDocument/hover" => {
                let hover = self.position_from_params(request.params.as_ref()).and_then(
                    |(path, offset)| {
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
                                            value: format!("{markdown}\n\n_Stale local fact while the current YAML buffer is invalid._"),
                                        },
                                        range: self.range(&symbol.range),
                                    }
                                })
                            })
                    },
                );
                self.response(writer, request.id, serde_json::to_value(hover).unwrap())
            }
            "textDocument/semanticTokens/full" => {
                let data = request
                    .params
                    .as_ref()
                    .and_then(uri)
                    .and_then(|uri| self.relative_path(uri).ok())
                    .map(|path| self.semantic_token_data(&path))
                    .unwrap_or_default();
                self.response(
                    writer,
                    request.id,
                    serde_json::to_value(SemanticTokens { data }).unwrap(),
                )
            }
            "textDocument/definition" => {
                let location = self
                    .position_from_params(request.params.as_ref())
                    .and_then(|(path, offset)| self.index().definition_at(&path, offset))
                    .and_then(|range| self.location(&range));
                self.response(writer, request.id, serde_json::to_value(location).unwrap())
            }
            "textDocument/references" => {
                let include_declaration = request
                    .params
                    .as_ref()
                    .and_then(|params| params.pointer("/context/includeDeclaration"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let locations = self
                    .position_from_params(request.params.as_ref())
                    .map(|(path, offset)| {
                        let mut ranges = self.index().references_at(&path, offset);
                        if include_declaration
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
                            (&left.path, left.start, left.end).cmp(&(
                                &right.path,
                                right.start,
                                right.end,
                            ))
                        });
                        ranges.dedup();
                        ranges
                            .into_iter()
                            .filter_map(|range| self.location(&range))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                self.response(writer, request.id, serde_json::to_value(locations).unwrap())
            }
            "textDocument/documentSymbol" => {
                let symbols = request
                    .params
                    .as_ref()
                    .and_then(|params| uri(params))
                    .and_then(|uri| self.relative_path(uri).ok())
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
                self.response(writer, request.id, serde_json::to_value(symbols).unwrap())
            }
            "workspace/symbol" => {
                let query = request
                    .params
                    .as_ref()
                    .and_then(|params| params.get("query"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let symbols = self
                    .index()
                    .workspace_symbols(query)
                    .into_iter()
                    .filter_map(|symbol| {
                        Some(WorkspaceSymbol {
                            name: symbol.name.clone(),
                            kind: lsp_symbol_kind(symbol.kind),
                            location: self.location(&symbol.range)?,
                            container_name: symbol.owner.clone(),
                        })
                    })
                    .collect::<Vec<_>>();
                self.response(writer, request.id, serde_json::to_value(symbols).unwrap())
            }
            method => self.error(
                writer,
                request.id,
                -32601,
                format!("Method '{method}' not found"),
            ),
        }
    }

    fn handle_notification<W: Write>(
        &mut self,
        notification: JsonRpcNotification,
        writer: &mut W,
    ) -> io::Result<()> {
        match notification.method.as_str() {
            "initialized" => Ok(()),
            "textDocument/didOpen" => {
                let Some(params) = notification.params else {
                    return Ok(());
                };
                let Some(document) = params.get("textDocument") else {
                    return Ok(());
                };
                let Some(uri) = document.get("uri").and_then(Value::as_str) else {
                    return Ok(());
                };
                let Ok(path) = self.relative_path(uri) else {
                    return Ok(());
                };
                let text = document
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let version = document.get("version").and_then(Value::as_i64).unwrap_or(0) as i32;
                self.source.insert(&path, text.clone());
                self.documents
                    .open(LspDocument::new(uri.to_owned(), version, text));
                self.rebuild_and_publish(writer)
            }
            "textDocument/didChange" => {
                let Some(params) = notification.params else {
                    return Ok(());
                };
                let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) else {
                    return Ok(());
                };
                let Some(change) = params
                    .get("contentChanges")
                    .and_then(Value::as_array)
                    .and_then(|changes| changes.first())
                else {
                    return Ok(());
                };
                if change.get("range").is_some() {
                    return Ok(());
                }
                let Some(text) = change.get("text").and_then(Value::as_str) else {
                    return Ok(());
                };
                let Ok(path) = self.relative_path(uri) else {
                    return Ok(());
                };
                let version = params
                    .pointer("/textDocument/version")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32;
                self.source.insert(&path, text.to_owned());
                if let Some(document) = self.documents.get_mut(uri) {
                    document.update(version, text.to_owned());
                }
                self.rebuild_and_publish(writer)
            }
            "textDocument/didClose" => {
                let Some(params) = notification.params else {
                    return Ok(());
                };
                let Some(uri) = uri(&params) else {
                    return Ok(());
                };
                if let Ok(path) = self.relative_path(uri) {
                    self.source.remove(&path);
                }
                self.documents.close(uri);
                self.rebuild_and_publish(writer)
            }
            "exit" => {
                std::process::exit(if self.shutdown_requested { 0 } else { 1 });
            }
            _ => Ok(()),
        }
    }

    fn rebuild_and_publish<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        let analysis = WorkspaceSemanticAnalysis::build(
            WorkspaceSnapshot::load_sync(&self.source),
            &builtin_actions(),
        );
        self.findings = analysis.findings;
        self.session.update_index(analysis.index);
        self.publish_diagnostics(writer)
    }

    fn publish_diagnostics<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        let mut grouped = BTreeMap::<String, Vec<Diagnostic>>::new();
        let index = self.index();
        for document in index.documents() {
            grouped.entry(self.uri_for(&document.path)).or_default();
        }
        for diagnostic in &index.diagnostics {
            self.push_diagnostic(&mut grouped, diagnostic, None);
        }
        for finding in &self.findings {
            self.push_diagnostic(
                &mut grouped,
                &finding.diagnostic,
                finding.source_file.as_deref(),
            );
        }
        let current = grouped.keys().cloned().collect::<BTreeSet<_>>();
        for uri in self.published.difference(&current) {
            self.publish(writer, uri, Vec::new())?;
        }
        for (uri, diagnostics) in grouped {
            self.publish(writer, &uri, diagnostics)?;
        }
        self.published = current;
        Ok(())
    }

    fn push_diagnostic(
        &self,
        grouped: &mut BTreeMap<String, Vec<Diagnostic>>,
        diagnostic: &EvinDiagnostic,
        fallback: Option<&str>,
    ) {
        let (path, range) = match &diagnostic.location {
            Some(FileLocation::ByteSpan {
                path,
                offset,
                length,
                ..
            }) => (
                PathBuf::from(path),
                self.range_for_path(
                    Path::new(path),
                    *offset,
                    offset.saturating_add((*length).max(1)),
                ),
            ),
            Some(FileLocation::Path { path, line, column }) => {
                let path = PathBuf::from(path);
                let range = self.line_range(&path, *line, *column);
                (path, range)
            }
            None => {
                let path = fallback
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(evin_config::paths::WORKSPACE_MANIFEST));
                (path.clone(), self.line_range(&path, None, None))
            }
        };
        grouped
            .entry(self.uri_for(&path))
            .or_default()
            .push(Diagnostic {
                range: range.unwrap_or(Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 1,
                    },
                }),
                severity: Some(match diagnostic.severity {
                    Severity::Error => 1,
                    Severity::Warning => 2,
                    Severity::Info => 3,
                }),
                code: Some(diagnostic.code.key.to_owned()),
                source: Some("evin".into()),
                message: evin_locales::render_issue(&diagnostic.code),
            });
    }

    fn publish<W: Write>(
        &self,
        writer: &mut W,
        uri: &str,
        diagnostics: Vec<Diagnostic>,
    ) -> io::Result<()> {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: "textDocument/publishDiagnostics".into(),
            params: Some(
                serde_json::to_value(PublishDiagnosticsParams {
                    uri: uri.into(),
                    diagnostics,
                })
                .unwrap(),
            ),
        };
        write_message(writer, &serde_json::to_string(&notification).unwrap())
    }

    fn position_from_params(&self, params: Option<&Value>) -> Option<(PathBuf, usize)> {
        let params = params?;
        let uri = uri(params)?;
        let path = self.relative_path(uri).ok()?;
        let position = params.get("position")?;
        let position = Position {
            line: position.get("line")?.as_u64()? as u32,
            character: position.get("character")?.as_u64()? as u32,
        };
        Some((
            path.clone(),
            self.source_index(&path)?.position_to_offset(&position),
        ))
    }

    fn source_index(&self, path: &Path) -> Option<SourcePositionIndex> {
        self.index()
            .document(path)
            .map(|document| SourcePositionIndex::new(&document.text))
    }
    fn semantic_token_data(&self, path: &Path) -> Vec<u32> {
        let index = self.index();
        let Some(document) = index.document(path) else {
            return Vec::new();
        };
        if document.ast.is_none() {
            return Vec::new();
        }
        let positions = SourcePositionIndex::new(&document.text);
        let mut tokens = index
            .semantic_tokens(path)
            .into_iter()
            .filter_map(|token| {
                let range = positions.span_to_range(token.range.start, token.range.end);
                if range.start.line != range.end.line
                    || range.end.character <= range.start.character
                {
                    return None;
                }
                let token_type = STANDARD_SEMANTIC_TOKEN_TYPES
                    .iter()
                    .position(|name| *name == token.kind.lsp_name())?
                    as u32;
                Some((
                    range.start.line,
                    range.start.character,
                    range.end.character - range.start.character,
                    token_type,
                ))
            })
            .collect::<Vec<_>>();
        tokens.sort_unstable();

        let mut data = Vec::with_capacity(tokens.len() * 5);
        let (mut previous_line, mut previous_start) = (0, 0);
        let mut previous_end = (0, 0);
        for (line, start, length, token_type) in tokens {
            if (line, start) < previous_end {
                continue;
            }
            let delta_line = line - previous_line;
            let delta_start = if delta_line == 0 {
                start.saturating_sub(previous_start)
            } else {
                start
            };
            data.extend([delta_line, delta_start, length, token_type, 0]);
            previous_line = line;
            previous_start = start;
            previous_end = (line, start + length);
        }
        data
    }
    fn range(&self, range: &SourceRange) -> Option<Range> {
        self.range_for_path(&range.path, range.start, range.end)
    }
    fn range_for_path(&self, path: &Path, start: usize, end: usize) -> Option<Range> {
        Some(self.source_index(path)?.span_to_range(start, end))
    }
    fn line_range(&self, path: &Path, line: Option<usize>, column: Option<usize>) -> Option<Range> {
        self.source_index(path)?;
        let start = Position {
            line: line.unwrap_or(1).saturating_sub(1) as u32,
            character: column.unwrap_or(1).saturating_sub(1) as u32,
        };
        Some(Range { start, end: start })
    }
    fn location(&self, range: &SourceRange) -> Option<Location> {
        Some(Location {
            uri: self.uri_for(&range.path),
            range: self.range(range)?,
        })
    }
    fn document_symbol(
        &self,
        symbol: &evin_config::semantic::SemanticSymbol,
    ) -> Option<DocumentSymbol> {
        Some(DocumentSymbol {
            name: symbol.name.clone(),
            kind: lsp_symbol_kind(symbol.kind),
            range: self.range(&symbol.range)?,
            selection_range: self.range(&symbol.range)?,
        })
    }
    fn index(&self) -> &evin_config::WorkspaceSemanticIndex {
        self.session.current.as_ref().expect("initialized analysis")
    }
    fn relative_path(&self, uri: &str) -> Result<PathBuf, String> {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| "workspace is not initialized".to_string())?;
        let path = url::Url::parse(uri)
            .map_err(|error| error.to_string())?
            .to_file_path()
            .map_err(|_| "URI is not a file URI".to_string())?;
        let path = path.canonicalize().unwrap_or(path);
        let root = root.canonicalize().unwrap_or_else(|_| root.clone());
        path.strip_prefix(root)
            .map(|path| normalize(path.to_owned()))
            .map_err(|_| "document is outside the workspace root".to_string())
    }
    fn uri_for(&self, path: &Path) -> String {
        let root = self.root.as_ref().expect("initialized root");
        url::Url::from_file_path(root.join(path))
            .map(String::from)
            .unwrap_or_default()
    }
    fn response<W: Write>(&self, writer: &mut W, id: Value, result: Value) -> io::Result<()> {
        write_message(
            writer,
            &serde_json::to_string(&JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: Some(result),
                error: None,
            })
            .unwrap(),
        )
    }
    fn error<W: Write>(
        &self,
        writer: &mut W,
        id: Value,
        code: i64,
        message: impl Into<String>,
    ) -> io::Result<()> {
        write_message(
            writer,
            &serde_json::to_string(&JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code,
                    message: message.into(),
                    data: None,
                }),
            })
            .unwrap(),
        )
    }
}

pub fn run_lsp_server() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());
    let mut server = EvinLspServer::default();
    while let Some(message) = read_message(&mut reader)? {
        server.handle_message(&message, &mut writer)?;
    }
    Ok(())
}

fn workspace_root(params: &Value) -> Result<PathBuf, String> {
    match params.get("rootUri").and_then(Value::as_str) {
        Some(uri) => url::Url::parse(uri)
            .map_err(|error| error.to_string())?
            .to_file_path()
            .map_err(|_| "initialize.rootUri must be a file URI".to_string()),
        None => std::env::current_dir().map_err(|error| error.to_string()),
    }
}
fn uri(params: &Value) -> Option<&str> {
    params.pointer("/textDocument/uri").and_then(Value::as_str)
}
fn plain_text(snippet: &str) -> String {
    snippet.replace("${1}", "")
}
fn lsp_symbol_kind(kind: SemanticKind) -> u32 {
    match kind {
        SemanticKind::Task => 12,
        SemanticKind::Value | SemanticKind::EnvironmentVariable => 13,
        SemanticKind::PackagePolicy => 19,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    fn root_uri(root: &Path) -> String {
        url::Url::from_file_path(root).unwrap().into()
    }

    fn uri(root: &Path, relative: &str) -> String {
        url::Url::from_file_path(root.join(relative))
            .unwrap()
            .into()
    }

    fn bodies(bytes: Vec<u8>) -> Vec<Value> {
        let mut reader = BufReader::new(Cursor::new(bytes));
        let mut result = Vec::new();
        while let Some(message) = read_message(&mut reader).unwrap() {
            result.push(serde_json::from_str(&message).unwrap());
        }
        result
    }

    fn decode_semantic_tokens(data: &[Value]) -> Vec<(u32, u32, u32, u32)> {
        let mut decoded = Vec::new();
        let (mut line, mut start) = (0, 0);
        let (tokens, _) = data.as_chunks::<5>();
        for token in tokens {
            let delta_line = token[0].as_u64().unwrap() as u32;
            let delta_start = token[1].as_u64().unwrap() as u32;
            line += delta_line;
            start = if delta_line == 0 {
                start + delta_start
            } else {
                delta_start
            };
            decoded.push((
                line,
                start,
                token[2].as_u64().unwrap() as u32,
                token[3].as_u64().unwrap() as u32,
            ));
        }
        decoded
    }

    fn initialize(server: &mut EvinLspServer, root: &Path, snippets: bool) -> Vec<Value> {
        let mut output = Vec::new();
        server
            .handle_message(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "rootUri": root_uri(root),
                        "capabilities": { "textDocument": { "completion": { "completionItem": { "snippetSupport": snippets } } } }
                    }
                })
                .to_string(),
                &mut output,
            )
            .unwrap();
        bodies(output)
    }

    fn workspace(root: &Path) {
        std::fs::create_dir_all(root.join(".evin")).unwrap();
        std::fs::write(
            root.join(".evin/workspace.flow"),
            "workspace: { name: app, version: 1.0.0 }\nstages: [Test]\nvalues: { artifact: dist }\ntasks:\n  build:\n    stage: Test\n    steps:\n      - uses: evin/docs@v1\n        with: { site_url: '${artifact}' }\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".evin/environment.flow"),
            "version: 1\nenvironments:\n  TOKEN:\n    type: secret\n",
        )
        .unwrap();
    }

    #[test]
    fn initialize_advertises_workspace_capabilities_and_stdout_is_framed() {
        let directory = tempfile::tempdir().unwrap();
        workspace(directory.path());
        let mut server = EvinLspServer::default();
        let messages = initialize(&mut server, directory.path(), true);
        assert_eq!(messages[0]["id"], 1);
        assert_eq!(
            messages[0]["result"]["capabilities"]["definitionProvider"],
            true
        );
        assert_eq!(messages[0]["result"]["capabilities"]["textDocumentSync"], 1);
        let legend =
            messages[0]["result"]["capabilities"]["semanticTokensProvider"]["legend"]["tokenTypes"]
                .as_array()
                .unwrap();
        assert_eq!(
            legend.iter().map(Value::as_str).collect::<Vec<_>>(),
            STANDARD_SEMANTIC_TOKEN_TYPES
                .iter()
                .copied()
                .map(Some)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            messages[0]["result"]["capabilities"]["semanticTokensProvider"]["full"],
            true
        );
        assert!(
            messages
                .iter()
                .any(|message| message["method"] == "textDocument/publishDiagnostics")
        );
    }

    #[test]
    fn semantic_tokens_use_utf16_positions_and_clear_for_invalid_yaml() {
        let directory = tempfile::tempdir().unwrap();
        workspace(directory.path());
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), false);
        let document_uri = uri(directory.path(), ".evin/workspace.flow");
        let text = "workspace: { name: \"🦀\", version: 1.0.0 }\nstages: [Test]\ntasks:\n  build:\n    stage: Test\n    description: \"build\"\n    on: { hooks: [pre-commit] }\n    needs: [build]\n    watch: { paths: [\"src/**/*.rs\"] }\n    presets: [ci]\n    steps:\n      - uses: evin/docs@v1\n        with: { site_url: \"${artifact}\" }\n";
        let open = json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": { "textDocument": { "uri": document_uri, "version": 2, "text": text } } });
        server
            .handle_message(&open.to_string(), &mut Vec::new())
            .unwrap();

        let request = json!({ "jsonrpc": "2.0", "id": 20, "method": "textDocument/semanticTokens/full", "params": { "textDocument": { "uri": document_uri } } });
        let mut output = Vec::new();
        server
            .handle_message(&request.to_string(), &mut output)
            .unwrap();
        let response = bodies(output).pop().unwrap();
        let tokens = decode_semantic_tokens(response["result"]["data"].as_array().unwrap());
        assert!(!tokens.is_empty());
        assert!(
            tokens
                .windows(2)
                .all(|pair| (pair[0].0, pair[0].1) < (pair[1].0, pair[1].1))
        );

        let token_type = |name: &str| {
            STANDARD_SEMANTIC_TOKEN_TYPES
                .iter()
                .position(|kind| *kind == name)
                .unwrap() as u32
        };
        let token_for = |needle: &str| {
            let (line, byte_start, byte_end) = text
                .lines()
                .enumerate()
                .find_map(|(line, value)| {
                    value
                        .find(needle)
                        .map(|start| (line as u32, start, start + needle.len()))
                })
                .unwrap();
            let source_line = text.lines().nth(line as usize).unwrap();
            let start = source_line[..byte_start].encode_utf16().count() as u32;
            let length = source_line[byte_start..byte_end].encode_utf16().count() as u32;
            tokens
                .iter()
                .find(|token| {
                    token.0 == line && token.1 < start + length && start < token.1 + token.2
                })
                .copied()
                .unwrap_or_else(|| panic!("missing token for {needle:?}"))
        };
        assert_eq!(token_for("workspace").3, token_type("namespace"));
        assert_eq!(
            token_for("version").1,
            "workspace: { name: \"🦀\", ".encode_utf16().count() as u32
        );
        assert_eq!(token_for("build:").3, token_type("function"));
        assert_eq!(token_for("    on:").3, token_type("modifier"));
        assert_eq!(token_for("    stage:").3, token_type("property"));
        assert_eq!(token_for("Test").3, token_type("enumMember"));
        assert_eq!(token_for("evin/docs@v1").3, token_type("method"));
        assert_eq!(token_for("${artifact}").3, token_type("variable"));

        let change = json!({ "jsonrpc": "2.0", "method": "textDocument/didChange", "params": { "textDocument": { "uri": document_uri, "version": 3 }, "contentChanges": [{ "text": "workspace: [\\n" }] } });
        server
            .handle_message(&change.to_string(), &mut Vec::new())
            .unwrap();
        let mut output = Vec::new();
        server
            .handle_message(&request.to_string(), &mut output)
            .unwrap();
        let response = bodies(output).pop().unwrap();
        assert_eq!(response["result"]["data"], json!([]));
    }

    #[test]
    fn completion_honors_snippet_capability_and_environment_schema() {
        let directory = tempfile::tempdir().unwrap();
        workspace(directory.path());
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), true);
        let mut output = Vec::new();
        server.handle_message(&json!({ "jsonrpc": "2.0", "id": 2, "method": "textDocument/completion", "params": { "textDocument": { "uri": uri(directory.path(), ".evin/environment.flow") }, "position": { "line": 3, "character": 10 } } }).to_string(), &mut output).unwrap();
        let message = bodies(output).pop().unwrap();
        let items = message["result"].as_array().unwrap();
        assert!(items.iter().any(|item| item["label"] == "secret"));
        assert!(!items.iter().any(|item| item["label"] == "environments"));
        assert!(items.iter().any(|item| item["insertTextFormat"] == 2));
    }

    #[test]
    fn task_completion_replaces_prefix_and_non_reference_values_are_empty() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(directory.path().join(".evin")).unwrap();
        std::fs::write(
            directory.path().join(".evin/workspace.flow"),
            "workspace: { name: app, version: 1.0.0, include: { release: .evin/release.flow } }\nstages: [Build]\ntasks: { publish: { description: '😀', stage: Build, needs: [rel], run: echo release } }\n",
        )
        .unwrap();
        std::fs::write(
            directory.path().join(".evin/release.flow"),
            "namespace: { name: release }\ntasks: { build: { stage: Build, run: echo build } }\n",
        )
        .unwrap();
        let text = std::fs::read_to_string(directory.path().join(".evin/workspace.flow")).unwrap();
        let uri = uri(directory.path(), ".evin/workspace.flow");
        let position = |offset: usize| Position {
            line: text[..offset].bytes().filter(|byte| *byte == b'\n').count() as u32,
            character: text[text[..offset]
                .rfind('\n')
                .map(|index| index + 1)
                .unwrap_or(0)..offset]
                .encode_utf16()
                .count() as u32,
        };
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), false);
        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "id": 2, "method": "textDocument/completion", "params": {
                    "textDocument": { "uri": uri },
                    "position": position(text.find("needs: [rel]").unwrap() + "needs: [rel".len())
                }}).to_string(),
                &mut output,
            )
            .unwrap();
        let items = bodies(output).pop().unwrap()["result"]
            .as_array()
            .unwrap()
            .clone();
        let release = items
            .iter()
            .find(|item| item["label"] == "release:build")
            .unwrap();
        assert_eq!(release["textEdit"]["newText"], "release:build");
        assert_eq!(release["filterText"], "release:build");
        let range = &release["textEdit"]["range"];
        let start_offset = text.find("needs: [rel").unwrap() + "needs: [".len();
        let line_start = text[..start_offset]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let prefix = &text[line_start..start_offset];
        assert!(prefix.contains('😀'));
        assert_eq!(
            range["start"],
            json!({ "line": 2, "character": prefix.encode_utf16().count() })
        );
        assert_eq!(
            range["end"],
            json!({ "line": 2, "character": prefix.encode_utf16().count() + 3 })
        );

        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "id": 3, "method": "textDocument/completion", "params": {
                    "textDocument": { "uri": uri },
                    "position": position(text.find("echo release").unwrap() + "echo release".len())
                }}).to_string(),
                &mut output,
            )
            .unwrap();
        assert_eq!(bodies(output).pop().unwrap()["result"], json!([]));
    }

    #[test]
    fn definitions_and_full_sync_overlay_changes_use_workspace_semantics() {
        let directory = tempfile::tempdir().unwrap();
        workspace(directory.path());
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), false);
        let workspace_uri = uri(directory.path(), ".evin/workspace.flow");
        let text = std::fs::read_to_string(directory.path().join(".evin/workspace.flow")).unwrap();
        let reference = text.find("${artifact}").unwrap() + 3;
        let line = text[..reference]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count() as u32;
        let character = (reference
            - text[..reference]
                .rfind('\n')
                .map(|offset| offset + 1)
                .unwrap_or(0)) as u32;
        let mut output = Vec::new();
        server.handle_message(&json!({ "jsonrpc": "2.0", "id": 3, "method": "textDocument/definition", "params": { "textDocument": { "uri": workspace_uri }, "position": { "line": line, "character": character } } }).to_string(), &mut output).unwrap();
        assert!(bodies(output).pop().unwrap()["result"].get("uri").is_some());

        let mut output = Vec::new();
        server.handle_message(&json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": { "textDocument": { "uri": uri(directory.path(), ".evin/workspace.flow"), "version": 1, "text": "workspace: [\n" } } }).to_string(), &mut output).unwrap();
        let messages = bodies(output);
        assert!(messages.iter().any(|message| {
            message["method"] == "textDocument/publishDiagnostics"
                && message["params"]["diagnostics"]
                    .as_array()
                    .is_some_and(|diagnostics| !diagnostics.is_empty())
        }));

        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "method": "textDocument/didClose", "params": { "textDocument": { "uri": uri(directory.path(), ".evin/workspace.flow") } } }).to_string(),
                &mut output,
            )
            .unwrap();
        assert!(bodies(output).iter().any(|message| {
            message["method"] == "textDocument/publishDiagnostics"
                && message["params"]["uri"] == workspace_uri
                && message["params"]["diagnostics"] == json!([])
        }));
    }

    #[test]
    fn symbols_hover_references_and_plain_completions_use_the_semantic_index() {
        let directory = tempfile::tempdir().unwrap();
        workspace(directory.path());
        let workspace_uri = uri(directory.path(), ".evin/workspace.flow");
        let text = std::fs::read_to_string(directory.path().join(".evin/workspace.flow")).unwrap();
        let artifact = text.find("artifact:").unwrap();
        let reference = text.find("${artifact}").unwrap() + 3;
        let position = |offset: usize| Position {
            line: text[..offset].bytes().filter(|byte| *byte == b'\n').count() as u32,
            character: text[text[..offset]
                .rfind('\n')
                .map(|index| index + 1)
                .unwrap_or(0)..offset]
                .chars()
                .map(|character| character.len_utf16() as u32)
                .sum(),
        };
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), false);

        let request = |id, method, params| {
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
        };
        let mut output = Vec::new();
        server
            .handle_message(
                &request(
                    2,
                    "textDocument/completion",
                    json!({ "textDocument": { "uri": workspace_uri }, "position": { "line": 0, "character": 0 } }),
                ),
                &mut output,
            )
            .unwrap();
        let completion = bodies(output).pop().unwrap();
        assert!(
            completion["result"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| item.get("insertTextFormat").is_none())
        );
        assert!(completion["result"].as_array().unwrap().iter().any(|item| {
            item["label"] == "expose"
                && item["documentation"]["value"]
                    .as_str()
                    .is_some_and(|value| value.contains("public task interface"))
        }));

        let mut output = Vec::new();
        server
            .handle_message(
                &request(3, "textDocument/hover", json!({ "textDocument": { "uri": workspace_uri }, "position": position(reference) })),
                &mut output,
            )
            .unwrap();
        assert!(
            bodies(output).pop().unwrap()["result"]["contents"]["value"]
                .as_str()
                .is_some_and(|value| value.contains("artifact"))
        );

        let mut output = Vec::new();
        server
            .handle_message(
                &request(4, "textDocument/references", json!({ "textDocument": { "uri": workspace_uri }, "position": position(artifact), "context": { "includeDeclaration": true } })),
                &mut output,
            )
            .unwrap();
        assert!(
            bodies(output).pop().unwrap()["result"]
                .as_array()
                .is_some_and(|locations| locations.len() >= 2)
        );

        let mut output = Vec::new();
        server
            .handle_message(
                &request(
                    5,
                    "textDocument/documentSymbol",
                    json!({ "textDocument": { "uri": workspace_uri } }),
                ),
                &mut output,
            )
            .unwrap();
        assert!(
            bodies(output).pop().unwrap()["result"]
                .as_array()
                .is_some_and(|symbols| symbols.iter().any(|symbol| symbol["name"] == "build"))
        );

        let mut output = Vec::new();
        server
            .handle_message(
                &request(6, "workspace/symbol", json!({ "query": "artifact" })),
                &mut output,
            )
            .unwrap();
        assert!(
            bodies(output).pop().unwrap()["result"]
                .as_array()
                .is_some_and(|symbols| symbols.iter().any(|symbol| symbol["name"] == "artifact"))
        );
    }

    #[test]
    fn configuration_root_key_hover_and_completion_use_catalog() {
        let directory = tempfile::tempdir().unwrap();
        workspace(directory.path());
        let path = directory.path().join(".evin/workspace.flow");
        let text = std::fs::read_to_string(&path).unwrap();
        let position = |offset: usize| Position {
            line: text[..offset].bytes().filter(|byte| *byte == b'\n').count() as u32,
            character: text[text[..offset]
                .rfind('\n')
                .map(|index| index + 1)
                .unwrap_or(0)..offset]
                .chars()
                .map(|character| character.len_utf16() as u32)
                .sum(),
        };
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), false);

        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "id": 7, "method": "textDocument/completion", "params": {
                    "textDocument": { "uri": uri(directory.path(), ".evin/workspace.flow") },
                    "position": { "line": 0, "character": 0 }
                }})
                .to_string(),
                &mut output,
            )
            .unwrap();
        let completion = bodies(output).pop().unwrap();
        assert!(completion["result"].as_array().unwrap().iter().any(|item| {
            item["label"] == "expose"
                && item["documentation"]["value"]
                    .as_str()
                    .is_some_and(|value| value.contains("public task interface"))
        }));

        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "id": 8, "method": "textDocument/hover", "params": {
                    "textDocument": { "uri": uri(directory.path(), ".evin/workspace.flow") },
                    "position": position(text.find("values:").unwrap())
                }})
                .to_string(),
                &mut output,
            )
            .unwrap();
        assert!(
            bodies(output).pop().unwrap()["result"]["contents"]["value"]
                .as_str()
                .is_some_and(|value| value.contains("Define reusable template values"))
        );

        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "id": 9, "method": "textDocument/hover", "params": {
                    "textDocument": { "uri": uri(directory.path(), ".evin/workspace.flow") },
                    "position": position(text.find("artifact:").unwrap())
                }})
                .to_string(),
                &mut output,
            )
            .unwrap();
        assert!(
            bodies(output).pop().unwrap()["result"]["contents"]["value"]
                .as_str()
                .is_some_and(|value| value.contains("Configuration value."))
        );
    }

    #[test]
    fn task_schema_hover_and_completion_use_catalog() {
        let directory = tempfile::tempdir().unwrap();
        workspace(directory.path());
        let text = std::fs::read_to_string(directory.path().join(".evin/workspace.flow")).unwrap();
        let position = |offset: usize| Position {
            line: text[..offset].bytes().filter(|byte| *byte == b'\n').count() as u32,
            character: text[text[..offset]
                .rfind('\n')
                .map(|index| index + 1)
                .unwrap_or(0)..offset]
                .chars()
                .map(|character| character.len_utf16() as u32)
                .sum(),
        };
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), false);
        let stage = text.find("stage: Test").unwrap();

        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "id": 10, "method": "textDocument/completion", "params": {
                    "textDocument": { "uri": uri(directory.path(), ".evin/workspace.flow") },
                    "position": position(stage)
                }})
                .to_string(),
                &mut output,
            )
            .unwrap();
        assert!(
            bodies(output).pop().unwrap()["result"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| {
                    item["label"] == "description"
                        && item["documentation"]["value"]
                            .as_str()
                            .is_some_and(|value| {
                                value.contains("Human-readable label for the task")
                            })
                })
        );

        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "id": 11, "method": "textDocument/hover", "params": {
                    "textDocument": { "uri": uri(directory.path(), ".evin/workspace.flow") },
                    "position": position(stage)
                }})
                .to_string(),
                &mut output,
            )
            .unwrap();
        assert!(
            bodies(output).pop().unwrap()["result"]["contents"]["value"]
                .as_str()
                .is_some_and(|value| value
                    .contains("Assign the task to a declared workspace pipeline stage"))
        );
    }

    #[test]
    fn embedded_catalog_enriches_action_hover() {
        let directory = tempfile::tempdir().unwrap();
        workspace(directory.path());
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), false);
        let text = std::fs::read_to_string(directory.path().join(".evin/workspace.flow")).unwrap();
        let offset = text.find("evin/docs@v1").unwrap();
        let position = Position {
            line: text[..offset].bytes().filter(|byte| *byte == b'\n').count() as u32,
            character: text[text[..offset]
                .rfind('\n')
                .map(|index| index + 1)
                .unwrap_or(0)..offset]
                .chars()
                .map(|character| character.len_utf16() as u32)
                .sum(),
        };
        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "id": 9, "method": "textDocument/hover", "params": {
                    "textDocument": { "uri": uri(directory.path(), ".evin/workspace.flow") },
                    "position": position
                }})
                .to_string(),
                &mut output,
            )
            .unwrap();
        let hover = bodies(output).pop().unwrap();
        assert!(
            hover["result"]["contents"]["value"].as_str().is_some_and(
                |value| value.contains("Build a documentation site or tooling catalog.")
            ),
            "{hover}"
        );
    }

    #[test]
    fn gitrules_leaf_hover_keeps_type_and_uses_section_catalog() {
        let directory = tempfile::tempdir().unwrap();
        workspace(directory.path());
        let workspace_path = directory.path().join(".evin/workspace.flow");
        let workspace_text = std::fs::read_to_string(&workspace_path).unwrap();
        std::fs::write(
            &workspace_path,
            workspace_text.replace(
                "version: 1.0.0",
                "version: 1.0.0, rules: .evin/gitrules.flow",
            ),
        )
        .unwrap();
        std::fs::write(
            directory.path().join(".evin/gitrules.flow"),
            "commits:\n  message_format: '{type}: {description}'\n",
        )
        .unwrap();
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), false);
        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "id": 37, "method": "textDocument/hover", "params": {
                    "textDocument": { "uri": uri(directory.path(), ".evin/gitrules.flow") },
                    "position": { "line": 1, "character": 3 }
                }})
                .to_string(),
                &mut output,
            )
            .unwrap();
        let hover = bodies(output).pop().unwrap();
        let value = hover["result"]["contents"]["value"].as_str().unwrap();
        assert!(value.contains("message_format"), "{hover}");
        assert!(value.contains("string | string[]"), "{hover}");
        assert!(value.contains("Commit message rules"), "{hover}");
        assert_eq!(
            hover["result"]["range"]["start"],
            json!({ "line": 1, "character": 2 })
        );
        assert_eq!(
            hover["result"]["range"]["end"],
            json!({ "line": 1, "character": 16 })
        );
    }

    #[test]
    fn repository_flow_comments_never_hover_through_the_protocol() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let files = [
            ".evin/workspace.flow",
            ".evin/environment.flow",
            ".evin/gitrules.flow",
            ".evin/release.flow",
            "crates/esh/.evin/module.flow",
            "crates/evin/.evin/module.flow",
            "docs/.evin/module.flow",
        ];
        let directory = tempfile::tempdir().unwrap();
        let mut documents = Vec::new();
        for relative in files {
            let original = std::fs::read_to_string(repository.join(relative)).unwrap();
            // Inject a comment line (at the following line's indentation)
            // before every line, and a trailing comment after every line
            // that is not already a comment.
            let mut text = String::new();
            for line in original.lines() {
                let trimmed = line.trim_start();
                let indent = &line[..line.len() - trimmed.len()];
                if !line.starts_with("#!") {
                    text.push_str(&format!("{indent}# it's: a [comment] - x\n"));
                }
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    text.push_str(line);
                } else {
                    text.push_str(&format!("{line} # trailing: 'c' {{x}}"));
                }
                text.push('\n');
            }
            let path = directory.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            // Disk keeps the original text; comments only exist in the
            // unsaved editor buffer, as when a user is typing.
            std::fs::write(&path, &original).unwrap();
            documents.push((relative, original, text));
        }
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), false);
        let mut failures = Vec::new();
        let mut id = 100;
        for (relative, original, text) in &documents {
            let document_uri = uri(directory.path(), relative);
            let mut output = Vec::new();
            server
                .handle_message(
                    &json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
                        "textDocument": { "uri": document_uri, "version": 1, "text": original }
                    }})
                    .to_string(),
                    &mut output,
                )
                .unwrap();
            server
                .handle_message(
                    &json!({ "jsonrpc": "2.0", "method": "textDocument/didChange", "params": {
                        "textDocument": { "uri": document_uri, "version": 2 },
                        "contentChanges": [{ "text": text }]
                    }})
                    .to_string(),
                    &mut output,
                )
                .unwrap();
            for (line_number, line) in text.lines().enumerate() {
                let comment_start = if line.trim_start().starts_with('#') {
                    Some(0)
                } else {
                    line.find(" # trailing").map(|position| position + 1)
                };
                let Some(start) = comment_start else { continue };
                for column in start..=line.len() {
                    id += 1;
                    let mut output = Vec::new();
                    server
                        .handle_message(
                            &json!({ "jsonrpc": "2.0", "id": id, "method": "textDocument/hover", "params": {
                                "textDocument": { "uri": document_uri },
                                "position": { "line": line_number, "character": line[..column].encode_utf16().count() }
                            }})
                            .to_string(),
                            &mut output,
                        )
                        .unwrap();
                    let hover = bodies(output).pop().unwrap();
                    if !hover["result"].is_null() {
                        failures.push(format!(
                            "{relative}:{} col {column} {line:?} -> {}",
                            line_number + 1,
                            hover["result"]["contents"]["value"]
                                .as_str()
                                .unwrap_or_default()
                                .lines()
                                .next()
                                .unwrap_or_default()
                        ));
                        break;
                    }
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} comment lines hovered:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn gitrules_comment_hover_returns_null() {
        let directory = tempfile::tempdir().unwrap();
        workspace(directory.path());
        let workspace_path = directory.path().join(".evin/workspace.flow");
        let workspace_text = std::fs::read_to_string(&workspace_path).unwrap();
        std::fs::write(
            &workspace_path,
            workspace_text.replace(
                "version: 1.0.0",
                "version: 1.0.0, rules: .evin/gitrules.flow",
            ),
        )
        .unwrap();
        let gitrules_text = "#!evin/v1\nversion: 1\ncommits:\n  # The conventional commit format used for all commit messages.\n  # Can be a single string or an array of acceptable formats.\n  message_format:\n    - \"{type}: {description}\"\n";
        std::fs::write(directory.path().join(".evin/gitrules.flow"), gitrules_text).unwrap();
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), false);

        // Hover over comment line ("  # The conventional...")
        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "id": 50, "method": "textDocument/hover", "params": {
                    "textDocument": { "uri": uri(directory.path(), ".evin/gitrules.flow") },
                    "position": { "line": 3, "character": 5 }
                }})
                .to_string(),
                &mut output,
            )
            .unwrap();
        let hover = bodies(output).pop().unwrap();
        assert_eq!(hover["result"], serde_json::Value::Null, "{hover}");

        // Hover over header comment line ("#!evin/v1")
        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "id": 51, "method": "textDocument/hover", "params": {
                    "textDocument": { "uri": uri(directory.path(), ".evin/gitrules.flow") },
                    "position": { "line": 0, "character": 2 }
                }})
                .to_string(),
                &mut output,
            )
            .unwrap();
        let hover = bodies(output).pop().unwrap();
        assert_eq!(hover["result"], serde_json::Value::Null, "{hover}");

        // Now test with the full repository gitrules.flow
        let full_gitrules = std::fs::read_to_string("../../.evin/gitrules.flow").unwrap();
        std::fs::write(directory.path().join(".evin/gitrules.flow"), &full_gitrules).unwrap();
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), false);

        // Hover over comment under branches: (line 64 in file -> 0-indexed line 63)
        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "id": 52, "method": "textDocument/hover", "params": {
                    "textDocument": { "uri": uri(directory.path(), ".evin/gitrules.flow") },
                    "position": { "line": 63, "character": 5 }
                }})
                .to_string(),
                &mut output,
            )
            .unwrap();
        let hover = bodies(output).pop().unwrap();
        println!("HOVER AT LINE 63: {:?}", hover);
        assert_eq!(hover["result"], serde_json::Value::Null, "{hover}");
    }

    #[test]
    fn gitrules_unicode_hover_and_tokens_use_utf16_ranges() {
        let directory = tempfile::tempdir().unwrap();
        workspace(directory.path());
        let workspace_path = directory.path().join(".evin/workspace.flow");
        let workspace_text = std::fs::read_to_string(&workspace_path).unwrap();
        std::fs::write(
            &workspace_path,
            workspace_text.replace(
                "version: 1.0.0",
                "version: 1.0.0, rules: .evin/gitrules.flow",
            ),
        )
        .unwrap();
        let text = "commits: { fields: { café: { allowed_values: [feat] } }, message_format: \"{type}: descrição\" }\n";
        std::fs::write(directory.path().join(".evin/gitrules.flow"), text).unwrap();
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), false);
        let rules_uri = uri(directory.path(), ".evin/gitrules.flow");
        for (needle, expected) in [
            ("café", "café"),
            ("message_format", "message_format"),
            ("descrição", "\"{type}: descrição\""),
        ] {
            let offset = text.find(needle).unwrap();
            let character = text[..offset].encode_utf16().count() as u32;
            let mut output = Vec::new();
            server
                .handle_message(
                    &json!({ "jsonrpc": "2.0", "id": 39, "method": "textDocument/hover", "params": {
                        "textDocument": { "uri": rules_uri },
                        "position": { "line": 0, "character": character }
                    }})
                    .to_string(),
                    &mut output,
                )
                .unwrap();
            let hover = bodies(output).pop().unwrap();
            let start = hover["result"]["range"]["start"]["character"]
                .as_u64()
                .unwrap() as usize;
            let end = hover["result"]["range"]["end"]["character"]
                .as_u64()
                .unwrap() as usize;
            let raw_start = text
                .char_indices()
                .find(|(byte, _)| text[..*byte].encode_utf16().count() == start)
                .unwrap()
                .0;
            let raw_end = text
                .char_indices()
                .find(|(byte, _)| text[..*byte].encode_utf16().count() == end)
                .unwrap()
                .0;
            assert_eq!(&text[raw_start..raw_end], expected, "{hover}");
        }
        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "id": 40, "method": "textDocument/semanticTokens/full", "params": {
                    "textDocument": { "uri": rules_uri }
                }})
                .to_string(),
                &mut output,
            )
            .unwrap();
        let response = bodies(output).pop().unwrap();
        let tokens = decode_semantic_tokens(response["result"]["data"].as_array().unwrap());
        let cafe_start = text[..text.find("café").unwrap()].encode_utf16().count() as u32;
        let property = STANDARD_SEMANTIC_TOKEN_TYPES
            .iter()
            .position(|name| *name == "property")
            .unwrap() as u32;
        assert!(tokens.contains(&(0, cafe_start, 4, property)), "{tokens:?}");
        let raw_value = "\"{type}: descrição\"";
        let value_start = text[..text.find(raw_value).unwrap()].encode_utf16().count() as u32;
        let string = STANDARD_SEMANTIC_TOKEN_TYPES
            .iter()
            .position(|name| *name == "string")
            .unwrap() as u32;
        assert!(
            tokens.contains(&(
                0,
                value_start,
                raw_value.encode_utf16().count() as u32,
                string
            )),
            "{tokens:?}"
        );
    }

    #[test]
    fn gitrules_schema_diagnostics_keep_unaffected_leaf_hover() {
        let directory = tempfile::tempdir().unwrap();
        workspace(directory.path());
        let workspace_path = directory.path().join(".evin/workspace.flow");
        let workspace_text = std::fs::read_to_string(&workspace_path).unwrap();
        std::fs::write(
            &workspace_path,
            workspace_text.replace(
                "version: 1.0.0",
                "version: 1.0.0, rules: .evin/gitrules.flow",
            ),
        )
        .unwrap();
        std::fs::write(
            directory.path().join(".evin/gitrules.flow"),
            "commits: {}\n",
        )
        .unwrap();
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), false);
        let rules_uri = uri(directory.path(), ".evin/gitrules.flow");
        let text = "commits:\n  message_format: '{type}: {description}'\n  policies:\n    surprise: true\n    require_body: no\nreleases:\n  packages:\n    app: { watch: [] }\n";
        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
                    "textDocument": { "uri": rules_uri, "version": 1, "text": text }
                }})
                .to_string(),
                &mut output,
            )
            .unwrap();
        let published = bodies(output);
        let diagnostics = published
            .iter()
            .find(|message| {
                message["method"] == "textDocument/publishDiagnostics"
                    && message["params"]["uri"] == rules_uri
            })
            .unwrap()["params"]["diagnostics"]
            .as_array()
            .unwrap();
        let unknown = diagnostics
            .iter()
            .find(|diagnostic| diagnostic["message"].as_str().unwrap().contains("surprise"))
            .unwrap();
        assert_eq!(
            unknown["range"]["start"],
            json!({ "line": 3, "character": 4 })
        );
        assert_eq!(
            unknown["range"]["end"],
            json!({ "line": 3, "character": 12 })
        );
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic["message"]
                .as_str()
                .unwrap()
                .contains("expects boolean")
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic["message"]
                .as_str()
                .unwrap()
                .contains("Missing required field")
        }));

        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "id": 38, "method": "textDocument/hover", "params": {
                    "textDocument": { "uri": rules_uri },
                    "position": { "line": 1, "character": 3 }
                }})
                .to_string(),
                &mut output,
            )
            .unwrap();
        let hover = bodies(output).pop().unwrap();
        assert!(
            hover["result"]["contents"]["value"]
                .as_str()
                .unwrap()
                .contains("string | string[]")
        );
    }

    #[test]
    fn stale_action_hover_keeps_catalog_prose_and_warning() {
        let directory = tempfile::tempdir().unwrap();
        workspace(directory.path());
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), false);
        let path = directory.path().join(".evin/workspace.flow");
        let text = std::fs::read_to_string(&path).unwrap();
        let offset = text.find("evin/docs@v1").unwrap();
        let position = Position {
            line: text[..offset].bytes().filter(|byte| *byte == b'\n').count() as u32,
            character: text[text[..offset]
                .rfind('\n')
                .map(|index| index + 1)
                .unwrap_or(0)..offset]
                .chars()
                .map(|character| character.len_utf16() as u32)
                .sum(),
        };
        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
                    "textDocument": {
                        "uri": uri(directory.path(), ".evin/workspace.flow"),
                        "version": 1,
                        "text": format!("{text}\nbroken: [")
                    }
                }})
                .to_string(),
                &mut output,
            )
            .unwrap();
        let mut output = Vec::new();
        server
            .handle_message(
                &json!({ "jsonrpc": "2.0", "id": 10, "method": "textDocument/hover", "params": {
                    "textDocument": { "uri": uri(directory.path(), ".evin/workspace.flow") },
                    "position": position
                }})
                .to_string(),
                &mut output,
            )
            .unwrap();
        let hover = bodies(output).pop().unwrap();
        let value = hover["result"]["contents"]["value"].as_str().unwrap();
        assert!(value.contains("Build a documentation site or tooling catalog."));
        assert!(value.contains("Stale local fact"));
    }

    #[test]
    fn embedded_catalog_targets_real_semantics_or_action_fields() {
        let configuration_targets = evin_config::semantic::configuration_catalog_targets();
        let actions = builtin_actions();
        for name in CONFIG_CATALOG.symbols.keys() {
            if configuration_targets
                .iter()
                .any(|target| target.as_str() == name)
            {
                continue;
            }
            let Some(action) = name.strip_prefix("action.") else {
                panic!("catalog entry {name:?} has no semantic target");
            };
            let (action_id, field) = action.split_once(".with.").unwrap_or((action, ""));
            let descriptor = actions
                .get(action_id)
                .and_then(|action| action.editor_descriptor())
                .unwrap_or_else(|| panic!("catalog action {action_id:?} is not built in"));
            assert!(
                field.is_empty()
                    || descriptor
                        .fields
                        .iter()
                        .any(|candidate| candidate.path == field),
                "catalog field {field:?} is not declared by {action_id}"
            );
        }
        for target in &configuration_targets {
            let mut candidate = target.as_str();
            loop {
                if CONFIG_CATALOG.symbols.contains_key(candidate) {
                    break;
                }
                let (parent, _) = candidate
                    .rsplit_once('.')
                    .unwrap_or_else(|| panic!("no catalog parent for {}", target.as_str()));
                candidate = parent;
            }
        }
    }

    #[test]
    fn malformed_messages_return_json_rpc_errors() {
        let mut server = EvinLspServer::default();
        let mut output = Vec::new();
        server.handle_message("{", &mut output).unwrap();
        assert_eq!(bodies(output)[0]["error"]["code"], -32700);
    }

    #[test]
    fn invalid_evin_headers_publish_one_precise_diagnostic() {
        let directory = tempfile::tempdir().unwrap();
        workspace(directory.path());
        let workspace_uri = uri(directory.path(), ".evin/workspace.flow");
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), false);

        let mut output = Vec::new();
        server
            .handle_message(
                &json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/didOpen",
                    "params": {
                        "textDocument": {
                            "uri": workspace_uri,
                            "version": 1,
                            "text": "#!evin/v2\nworkspace: [\n"
                        }
                    }
                })
                .to_string(),
                &mut output,
            )
            .unwrap();
        let diagnostics = bodies(output)
            .into_iter()
            .find(|message| {
                message["method"] == "textDocument/publishDiagnostics"
                    && message["params"]["uri"] == workspace_uri
            })
            .unwrap()["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .clone();
        let headers = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic["code"] == "issue-evin-header-invalid")
            .collect::<Vec<_>>();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0]["range"]["start"]["line"], 0);
        assert_eq!(headers[0]["range"]["start"]["character"], 0);
        assert_eq!(headers[0]["range"]["end"]["character"], 9);
    }

    #[test]
    fn rejects_non_file_workspace_root() {
        let mut server = EvinLspServer::default();
        let mut output = Vec::new();
        server.handle_message(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "rootUri": "untitled:workspace" } }).to_string(), &mut output).unwrap();
        assert_eq!(bodies(output)[0]["error"]["code"], -32602);
    }
}
