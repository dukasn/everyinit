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

use super::dispatcher::{NotificationDispatcher, RequestDispatcher};
use super::protocol::*;
use crate::commands::builtin_actions;
use evin_config::semantic::{
    STANDARD_SEMANTIC_TOKEN_TYPES, SemanticKind, SourceRange, WorkspaceAnalysisSession,
};
use evin_config::source::{OverlaySource, normalize};
use evin_config::{WorkspaceSemanticAnalysis, WorkspaceSnapshot};
use evin_diagnostic::{Diagnostic as EvinDiagnostic, FileLocation, Severity};
use evin_lsp::protocol::{
    Diagnostic, DocumentSymbol, JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse,
    Location, Position, PublishDiagnosticsParams, Range, write_message,
};
use evin_lsp::{DocumentStore, SourcePositionIndex};
use serde::Serialize;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct EvinLspServer {
    pub(super) initialized: bool,
    pub(super) shutdown_requested: bool,
    pub(super) needs_initial_rebuild: bool,
    pub(super) root: Option<PathBuf>,
    pub(super) canonical_root: Option<PathBuf>,
    pub(super) source: OverlaySource,
    pub(super) documents: DocumentStore,
    pub(super) session: WorkspaceAnalysisSession,
    pub(super) findings: Vec<evin_config::analysis::AnalysisFinding>,
    pub(super) snippet_support: bool,
    pub(super) published: BTreeSet<String>,
    pub(super) source_indices: RefCell<HashMap<PathBuf, SourcePositionIndex>>,
}

impl EvinLspServer {
    pub fn handle_message<W: Write>(&mut self, message: &str, writer: &mut W) -> io::Result<()> {
        let value: Value = match serde_json::from_str(message) {
            Ok(value) => value,
            Err(error) => {
                return self.respond_error(
                    writer,
                    Value::Null,
                    LspError::PARSE_ERROR,
                    error.to_string(),
                );
            }
        };

        if value.get("id").is_some() {
            match serde_json::from_value::<JsonRpcRequest>(value) {
                Ok(request) => self.handle_request(request, writer),
                Err(error) => self.respond_error(
                    writer,
                    Value::Null,
                    LspError::INVALID_REQUEST,
                    error.to_string(),
                ),
            }
        } else {
            match serde_json::from_value::<JsonRpcNotification>(value) {
                Ok(notification) => self.handle_notification(notification, writer),
                Err(error) => self.respond_error(
                    writer,
                    Value::Null,
                    LspError::INVALID_REQUEST,
                    error.to_string(),
                ),
            }
        }
    }

    fn handle_request<W: Write>(
        &mut self,
        request: JsonRpcRequest,
        writer: &mut W,
    ) -> io::Result<()> {
        if !self.initialized && request.method != InitializeRequest::METHOD {
            return self.respond_error(
                writer,
                request.id,
                LspError::SERVER_NOT_INITIALIZED,
                "Server not initialized",
            );
        }

        RequestDispatcher::new(request, self, writer)
            .on::<InitializeRequest, _>(Self::on_initialize)?
            .on::<ShutdownRequest, _>(Self::on_shutdown)?
            .on::<CompletionRequest, _>(Self::on_completion)?
            .on::<HoverRequest, _>(Self::on_hover)?
            .on::<SemanticTokensFullRequest, _>(Self::on_semantic_tokens)?
            .on::<GotoDefinitionRequest, _>(Self::on_definition)?
            .on::<ReferencesRequest, _>(Self::on_references)?
            .on::<DocumentSymbolRequest, _>(Self::on_document_symbols)?
            .on::<WorkspaceSymbolRequest, _>(Self::on_workspace_symbols)?
            .finish()
    }

    fn handle_notification<W: Write>(
        &mut self,
        notification: JsonRpcNotification,
        writer: &mut W,
    ) -> io::Result<()> {
        NotificationDispatcher::new(notification, self, writer)
            .on::<InitializedNotification, _>(|_, _, _| Ok(()))?
            .on::<DidOpenNotification, _>(Self::on_did_open)?
            .on::<DidChangeNotification, _>(Self::on_did_change)?
            .on::<DidCloseNotification, _>(Self::on_did_close)?
            .on::<ExitNotification, _>(Self::on_exit)?
            .finish()
    }

    // -----------------------------------------------------------------------
    // Semantic Analysis & Diagnostics
    // -----------------------------------------------------------------------

    pub(super) fn rebuild_and_publish<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.source_indices.borrow_mut().clear();
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

    // -----------------------------------------------------------------------
    // Helper Lookups & Geometry
    // -----------------------------------------------------------------------

    pub(super) fn position_offset(
        &self,
        uri: &str,
        position: &Position,
    ) -> Option<(PathBuf, usize)> {
        let path = self.relative_path(uri).ok()?;
        let offset = self.with_source_index(&path, |index| index.position_to_offset(position))?;
        Some((path, offset))
    }

    pub(super) fn with_source_index<R>(
        &self,
        path: &Path,
        f: impl FnOnce(&SourcePositionIndex) -> R,
    ) -> Option<R> {
        let mut cache = self.source_indices.borrow_mut();
        if !cache.contains_key(path) {
            let doc = self.index().document(path)?;
            cache.insert(path.to_owned(), SourcePositionIndex::new(&doc.text));
        }
        cache.get(path).map(f)
    }

    pub(super) fn semantic_token_data(&self, path: &Path) -> Vec<u32> {
        let index = self.index();
        let Some(document) = index.document(path) else {
            return Vec::new();
        };
        if document.ast.is_none() {
            return Vec::new();
        }

        let mut tokens = self
            .with_source_index(path, |positions| {
                index
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
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

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

    pub(super) fn range(&self, range: &SourceRange) -> Option<Range> {
        self.range_for_path(&range.path, range.start, range.end)
    }

    pub(super) fn range_for_path(&self, path: &Path, start: usize, end: usize) -> Option<Range> {
        self.with_source_index(path, |index| index.span_to_range(start, end))
    }

    pub(super) fn line_range(
        &self,
        path: &Path,
        line: Option<usize>,
        column: Option<usize>,
    ) -> Option<Range> {
        self.with_source_index(path, |_| ())?;
        let start = Position {
            line: line.unwrap_or(1).saturating_sub(1) as u32,
            character: column.unwrap_or(1).saturating_sub(1) as u32,
        };
        Some(Range { start, end: start })
    }

    pub(super) fn location(&self, range: &SourceRange) -> Option<Location> {
        Some(Location {
            uri: self.uri_for(&range.path),
            range: self.range(range)?,
        })
    }

    pub(super) fn document_symbol(
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

    pub(super) fn index(&self) -> &evin_config::WorkspaceSemanticIndex {
        self.session.current.as_ref().expect("initialized analysis")
    }

    pub(super) fn relative_path(&self, uri: &str) -> Result<PathBuf, String> {
        let canonical_root = self
            .canonical_root
            .as_ref()
            .ok_or_else(|| "workspace is not initialized".to_string())?;
        let path = url::Url::parse(uri)
            .map_err(|error| error.to_string())?
            .to_file_path()
            .map_err(|_| "URI is not a file URI".to_string())?;
        let path = path.canonicalize().unwrap_or(path);
        path.strip_prefix(canonical_root)
            .map(|path| normalize(path.to_owned()))
            .map_err(|_| "document is outside the workspace root".to_string())
    }

    pub(super) fn uri_for(&self, path: &Path) -> String {
        let root = self.root.as_ref().expect("initialized root");
        url::Url::from_file_path(root.join(path))
            .map(String::from)
            .unwrap_or_default()
    }

    pub(super) fn respond<W: Write, T: Serialize>(
        &self,
        writer: &mut W,
        id: Value,
        result: &T,
    ) -> io::Result<()> {
        let result_value = serde_json::to_value(result).map_err(io::Error::other)?;
        let response = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result_value),
            error: None,
        };
        write_message(writer, &serde_json::to_string(&response).unwrap())
    }

    pub(super) fn respond_error<W: Write>(
        &self,
        writer: &mut W,
        id: Value,
        code: i64,
        message: impl Into<String>,
    ) -> io::Result<()> {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        };
        write_message(writer, &serde_json::to_string(&response).unwrap())
    }
}

pub(super) fn lsp_symbol_kind(kind: SemanticKind) -> u32 {
    match kind {
        SemanticKind::Task => 12,
        SemanticKind::Value | SemanticKind::EnvironmentVariable => 13,
        SemanticKind::PackagePolicy => 19,
        _ => 2,
    }
}
