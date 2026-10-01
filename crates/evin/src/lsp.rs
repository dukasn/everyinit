// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 of the License.

//! Evin's workspace-aware Language Server Protocol adapter.

use crate::commands::builtin_actions;
use evin_config::semantic::{SemanticKind, SourceRange, WorkspaceAnalysisSession};
use evin_config::source::{OverlaySource, normalize};
use evin_config::{WorkspaceSemanticAnalysis, WorkspaceSnapshot};
use evin_diagnostic::{Diagnostic as EvinDiagnostic, FileLocation, Severity};
use evin_lsp::protocol::{
    CompletionItem, Diagnostic, DocumentSymbol, Hover, JsonRpcError, JsonRpcNotification,
    JsonRpcRequest, JsonRpcResponse, Location, MarkupContent, Position, PublishDiagnosticsParams,
    Range, WorkspaceSymbol, read_message, write_message,
};
use evin_lsp::{DocumentStore, LspDocument, SourcePositionIndex};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

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
                            "completionProvider": { "triggerCharacters": [":", "$", "."] },
                            "definitionProvider": true,
                            "referencesProvider": true,
                            "documentSymbolProvider": true,
                            "workspaceSymbolProvider": true
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
                        self.index()
                            .completions_at(&path, offset, &builtin_actions())
                    })
                    .unwrap_or_default()
                    .into_iter()
                    .map(|candidate| CompletionItem {
                        label: candidate.label,
                        kind: Some(10),
                        detail: Some(candidate.detail),
                        documentation: Some(MarkupContent {
                            kind: "markdown".into(),
                            value: candidate.documentation,
                        }),
                        insert_text: Some(if self.snippet_support {
                            candidate.snippet
                        } else {
                            plain_text(&candidate.snippet)
                        }),
                        insert_text_format: self.snippet_support.then_some(2),
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
                                    value: info.markdown,
                                },
                                range: self.range(&info.range),
                            })
                            .or_else(|| {
                                self.session.stale_symbol_at(&path, offset).map(|symbol| Hover {
                                    contents: MarkupContent {
                                        kind: "markdown".into(),
                                        value: format!("**{}**\n\n{}\n\n_Stale local fact while the current YAML buffer is invalid._", symbol.name, symbol.detail),
                                    },
                                    range: self.range(&symbol.range),
                                })
                            })
                    },
                );
                self.response(writer, request.id, serde_json::to_value(hover).unwrap())
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
                    .unwrap_or_else(|| PathBuf::from(".evin/workspace.yaml"));
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
                message: diagnostic.to_string(),
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
            root.join(".evin/workspace.yaml"),
            "workspace: { name: app, version: 1.0.0 }\nstages: []\nvalues: { artifact: dist }\ntasks:\n  build:\n    steps:\n      - uses: evin/docs@v1\n        with: { site_url: '${artifact}' }\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".evin/environment.yaml"),
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
        assert!(
            messages
                .iter()
                .any(|message| message["method"] == "textDocument/publishDiagnostics")
        );
    }

    #[test]
    fn completion_honors_snippet_capability_and_environment_schema() {
        let directory = tempfile::tempdir().unwrap();
        workspace(directory.path());
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), true);
        let mut output = Vec::new();
        server.handle_message(&json!({ "jsonrpc": "2.0", "id": 2, "method": "textDocument/completion", "params": { "textDocument": { "uri": uri(directory.path(), ".evin/environment.yaml") }, "position": { "line": 0, "character": 0 } } }).to_string(), &mut output).unwrap();
        let message = bodies(output).pop().unwrap();
        let items = message["result"].as_array().unwrap();
        assert!(items.iter().any(|item| item["label"] == "environments"));
        assert!(items.iter().any(|item| item["label"] == "secret"));
        assert!(items.iter().any(|item| item["insertTextFormat"] == 2));
    }

    #[test]
    fn definitions_and_full_sync_overlay_changes_use_workspace_semantics() {
        let directory = tempfile::tempdir().unwrap();
        workspace(directory.path());
        let mut server = EvinLspServer::default();
        initialize(&mut server, directory.path(), false);
        let workspace_uri = uri(directory.path(), ".evin/workspace.yaml");
        let text = std::fs::read_to_string(directory.path().join(".evin/workspace.yaml")).unwrap();
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
        server.handle_message(&json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": { "textDocument": { "uri": uri(directory.path(), ".evin/workspace.yaml"), "version": 1, "text": "workspace: [\n" } } }).to_string(), &mut output).unwrap();
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
                &json!({ "jsonrpc": "2.0", "method": "textDocument/didClose", "params": { "textDocument": { "uri": uri(directory.path(), ".evin/workspace.yaml") } } }).to_string(),
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
        let workspace_uri = uri(directory.path(), ".evin/workspace.yaml");
        let text = std::fs::read_to_string(directory.path().join(".evin/workspace.yaml")).unwrap();
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
        let workspace_uri = uri(directory.path(), ".evin/workspace.yaml");
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
