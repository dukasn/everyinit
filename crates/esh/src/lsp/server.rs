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

use super::hover::get_hover_documentation;
use super::protocol::{
    CompletionItem, Diagnostic, DocumentSymbol, Hover, JsonRpcError, JsonRpcNotification,
    JsonRpcRequest, JsonRpcResponse, MarkupContent, PublishDiagnosticsParams, Range,
    SemanticTokens, read_message, write_message,
};
use super::semantic_tokens::{TOKEN_TYPES, compute_semantic_tokens};
use crate::lexer::cursor::Lexer;
use crate::parser::ast::ASTNode;
use crate::parser::pratt::Parser;
use crate::planner::compiler::compile_script;
use crate::syntax::scan_tokens;
use bumpalo::Bump;
use evin_lsp::LspDocument;
use rustc_hash::FxHashMap;
use serde_json::{Value, json};
use std::io::{self, BufReader, BufWriter, Write};

pub struct LspServer {
    pub initialized: bool,
    pub shutdown_requested: bool,
    pub documents: FxHashMap<String, LspDocument>,
}

impl Default for LspServer {
    fn default() -> Self {
        Self::new()
    }
}

impl LspServer {
    pub fn new() -> Self {
        Self {
            initialized: false,
            shutdown_requested: false,
            documents: FxHashMap::default(),
        }
    }

    /// Process a single incoming JSON-RPC raw message.
    pub fn handle_message<W: Write>(&mut self, raw: &str, writer: &mut W) -> io::Result<()> {
        let parsed: Result<Value, _> = serde_json::from_str(raw);
        let val = match parsed {
            Ok(v) => v,
            Err(_) => {
                let err_resp = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: json!(null),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: "Parse error: Invalid JSON was received by the server."
                            .to_string(),
                        data: None,
                    }),
                };
                let body = serde_json::to_string(&err_resp).unwrap();
                return write_message(writer, &body);
            }
        };

        if val.get("id").is_some() {
            if let Ok(req) = serde_json::from_value::<JsonRpcRequest>(val) {
                self.handle_request(req, writer)?;
            }
        } else if let Ok(notif) = serde_json::from_value::<JsonRpcNotification>(val) {
            self.handle_notification(notif, writer)?;
        }

        Ok(())
    }

    fn handle_request<W: Write>(&mut self, req: JsonRpcRequest, writer: &mut W) -> io::Result<()> {
        let req_id = req.id;

        // Ensure initialize is handled before any other requests
        if !self.initialized && req.method != "initialize" {
            let response = JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req_id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32002,
                    message: "Server not initialized".to_string(),
                    data: None,
                }),
            };
            let body = serde_json::to_string(&response).unwrap();
            return write_message(writer, &body);
        }

        match req.method.as_str() {
            "initialize" => {
                self.initialized = true;
                let response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req_id,
                    result: Some(json!({
                        "capabilities": {
                            "textDocumentSync": 1, // Full sync
                            "hoverProvider": true,
                            "completionProvider": {
                                "triggerCharacters": ["$", "-", "."]
                            },
                            "semanticTokensProvider": {
                                "legend": {
                                    "tokenTypes": TOKEN_TYPES,
                                    "tokenModifiers": []
                                },
                                "full": true
                            },
                            "documentSymbolProvider": true
                        },
                        "serverInfo": {
                            "name": "esh-lsp",
                            "version": "1.0.0"
                        }
                    })),
                    error: None,
                };
                let body = serde_json::to_string(&response).unwrap();
                write_message(writer, &body)?;
            }

            "shutdown" => {
                self.shutdown_requested = true;
                let response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req_id,
                    result: Some(json!(null)),
                    error: None,
                };
                let body = serde_json::to_string(&response).unwrap();
                write_message(writer, &body)?;
            }

            "textDocument/hover" => {
                let params = req.params.unwrap_or(json!({}));
                let uri = params
                    .get("textDocument")
                    .and_then(|td| td.get("uri"))
                    .and_then(|u| u.as_str())
                    .unwrap_or_default();
                let line = params
                    .get("position")
                    .and_then(|p| p.get("line"))
                    .and_then(|l| l.as_u64())
                    .unwrap_or(0) as usize;
                let col_utf16 = params
                    .get("position")
                    .and_then(|p| p.get("character"))
                    .and_then(|c| c.as_u64())
                    .unwrap_or(0) as usize;

                let mut hover_result: Option<Hover> = None;
                if let Some(doc) = self.documents.get(uri)
                    && let Some(doc_line) = doc.text.lines().nth(line)
                {
                    let tokens = scan_tokens(doc_line, line);
                    for tok in tokens {
                        if col_utf16 >= tok.col_utf16
                            && col_utf16 <= tok.col_utf16 + tok.length_utf16
                            && let Some(doc_md) = get_hover_documentation(tok.text)
                        {
                            let line_offset =
                                doc.index.position_to_offset(&super::protocol::Position {
                                    line: line as u32,
                                    character: 0,
                                });
                            let start_offset = line_offset + tok.col;
                            let end_offset = start_offset + tok.text.len();
                            let range = doc.index.span_to_range(start_offset, end_offset);

                            hover_result = Some(Hover {
                                contents: MarkupContent {
                                    kind: "markdown".to_string(),
                                    value: doc_md,
                                },
                                range: Some(range),
                            });
                            break;
                        }
                    }
                }

                let response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req_id,
                    result: Some(serde_json::to_value(hover_result).unwrap_or(json!(null))),
                    error: None,
                };
                let body = serde_json::to_string(&response).unwrap();
                write_message(writer, &body)?;
            }

            "textDocument/semanticTokens/full" => {
                let params = req.params.unwrap_or(json!({}));
                let uri = params
                    .get("textDocument")
                    .and_then(|td| td.get("uri"))
                    .and_then(|u| u.as_str())
                    .unwrap_or_default();

                let data = if let Some(doc) = self.documents.get(uri) {
                    compute_semantic_tokens(&doc.text)
                } else {
                    Vec::new()
                };

                let response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req_id,
                    result: Some(
                        serde_json::to_value(SemanticTokens { data }).unwrap_or(json!(null)),
                    ),
                    error: None,
                };
                let body = serde_json::to_string(&response).unwrap();
                write_message(writer, &body)?;
            }

            "textDocument/completion" => {
                let completions = self.get_completions();
                let response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req_id,
                    result: Some(serde_json::to_value(completions).unwrap_or(json!([]))),
                    error: None,
                };
                let body = serde_json::to_string(&response).unwrap();
                write_message(writer, &body)?;
            }

            "textDocument/documentSymbol" => {
                let params = req.params.unwrap_or(json!({}));
                let uri = params
                    .get("textDocument")
                    .and_then(|td| td.get("uri"))
                    .and_then(|u| u.as_str())
                    .unwrap_or_default();

                let symbols = if let Some(doc) = self.documents.get(uri) {
                    self.extract_symbols(doc)
                } else {
                    Vec::new()
                };

                let response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req_id,
                    result: Some(serde_json::to_value(symbols).unwrap_or(json!([]))),
                    error: None,
                };
                let body = serde_json::to_string(&response).unwrap();
                write_message(writer, &body)?;
            }

            _ => {
                let response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req_id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32601,
                        message: format!("Method '{}' not found", req.method),
                        data: None,
                    }),
                };
                let body = serde_json::to_string(&response).unwrap();
                write_message(writer, &body)?;
            }
        }
        Ok(())
    }

    fn handle_notification<W: Write>(
        &mut self,
        notif: JsonRpcNotification,
        writer: &mut W,
    ) -> io::Result<()> {
        match notif.method.as_str() {
            "initialized" => {}
            "textDocument/didOpen" => {
                if let Some(params) = notif.params
                    && let Some(text_doc) = params.get("textDocument")
                {
                    let uri = text_doc
                        .get("uri")
                        .and_then(|u| u.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let version = text_doc
                        .get("version")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(1) as i32;
                    let text = text_doc
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let doc = LspDocument::new(uri.clone(), version, text);
                    self.publish_diagnostics(&doc, writer)?;
                    self.documents.insert(uri, doc);
                }
            }
            "textDocument/didChange" => {
                if let Some(params) = notif.params {
                    let uri = params
                        .get("textDocument")
                        .and_then(|td| td.get("uri"))
                        .and_then(|u| u.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let version = params
                        .get("textDocument")
                        .and_then(|td| td.get("version"))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(1) as i32;
                    if let Some(changes) = params.get("contentChanges").and_then(|c| c.as_array())
                        && let Some(first_change) = changes.first()
                        && let Some(text) = first_change.get("text").and_then(|t| t.as_str())
                    {
                        let updated_doc = if let Some(doc) = self.documents.get_mut(&uri) {
                            doc.update(version, text.to_string());
                            Some(doc.clone())
                        } else {
                            None
                        };
                        if let Some(ref doc) = updated_doc {
                            self.publish_diagnostics(doc, writer)?;
                        }
                    }
                }
            }
            "textDocument/didClose" => {
                if let Some(params) = notif.params {
                    let uri = params
                        .get("textDocument")
                        .and_then(|td| td.get("uri"))
                        .and_then(|u| u.as_str())
                        .unwrap_or_default();
                    self.documents.remove(uri);
                    // Clear diagnostics on close
                    let notif = JsonRpcNotification {
                        jsonrpc: "2.0".to_string(),
                        method: "textDocument/publishDiagnostics".to_string(),
                        params: Some(
                            serde_json::to_value(PublishDiagnosticsParams {
                                uri: uri.to_string(),
                                diagnostics: Vec::new(),
                            })
                            .unwrap(),
                        ),
                    };
                    let body = serde_json::to_string(&notif).unwrap();
                    write_message(writer, &body)?;
                }
            }
            "exit" => {
                if self.shutdown_requested {
                    std::process::exit(0);
                } else {
                    std::process::exit(1);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn publish_diagnostics<W: Write>(&self, doc: &LspDocument, writer: &mut W) -> io::Result<()> {
        let mut diagnostics = Vec::new();

        if let Err(pe) = compile_script(&doc.text) {
            let span_start = pe.span.start as usize;
            let span_end = pe.span.end as usize;
            let range = doc
                .index
                .span_to_range(span_start, span_end.max(span_start + 1));

            diagnostics.push(Diagnostic {
                range,
                severity: Some(1), // Error
                code: Some("syntax-error".to_string()),
                source: Some("esh".to_string()),
                message: pe.message,
            });
        }

        let notif = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "textDocument/publishDiagnostics".to_string(),
            params: Some(
                serde_json::to_value(PublishDiagnosticsParams {
                    uri: doc.uri.clone(),
                    diagnostics,
                })
                .unwrap(),
            ),
        };
        let body = serde_json::to_string(&notif).unwrap();
        write_message(writer, &body)
    }

    fn get_completions(&self) -> Vec<CompletionItem> {
        let mut items = Vec::with_capacity(
            crate::catalog::BUILTIN_CATALOG.len() + crate::catalog::KEYWORD_CATALOG.len(),
        );

        for kw in crate::catalog::KEYWORD_CATALOG {
            items.push(CompletionItem {
                label: kw.name.to_string(),
                kind: Some(14), // Keyword
                detail: Some(kw.synopsis.to_string()),
                documentation: Some(super::protocol::MarkupContent {
                    kind: "markdown".to_string(),
                    value: kw.doc_markdown.to_string(),
                }),
                insert_text: Some(kw.name.to_string()),
                insert_text_format: None,
            });
        }

        for bi in crate::catalog::BUILTIN_CATALOG {
            items.push(CompletionItem {
                label: bi.name.to_string(),
                kind: Some(3), // Function
                detail: Some(bi.synopsis.to_string()),
                documentation: Some(super::protocol::MarkupContent {
                    kind: "markdown".to_string(),
                    value: bi.doc_markdown.to_string(),
                }),
                insert_text: Some(bi.name.to_string()),
                insert_text_format: None,
            });
        }

        items
    }

    fn extract_symbols(&self, doc: &LspDocument) -> Vec<DocumentSymbol> {
        let mut symbols = Vec::new();

        // 1. Try AST-backed extraction first
        if let Ok(tokens) = Lexer::new(&doc.text).tokenize_all() {
            let arena = Bump::new();
            if let Ok(ast) = Parser::new(&arena, tokens).parse_script() {
                self.collect_ast_symbols(ast, doc, &mut symbols);
                if !symbols.is_empty() {
                    return symbols;
                }
            }
        }

        // 2. Recovery fallback: scanner-based extraction for incomplete editor buffers
        for (idx, line) in doc.text.lines().enumerate() {
            let trimmed = line.trim();
            if let Some(pos) = trimmed.find("()") {
                let name = trimmed[..pos].trim();
                if !name.is_empty() && !name.contains(' ') {
                    let line_start_offset =
                        doc.index.position_to_offset(&super::protocol::Position {
                            line: idx as u32,
                            character: 0,
                        });
                    let name_col = line.find(name).unwrap_or(0);
                    let name_start = line_start_offset + name_col;
                    let name_end = name_start + name.len();

                    symbols.push(DocumentSymbol {
                        name: name.to_string(),
                        kind: 12, // Function
                        range: Range {
                            start: doc.index.offset_to_position(line_start_offset),
                            end: doc.index.offset_to_position(line_start_offset + line.len()),
                        },
                        selection_range: Range {
                            start: doc.index.offset_to_position(name_start),
                            end: doc.index.offset_to_position(name_end),
                        },
                    });
                }
            } else if let Some(rest) = trimmed.strip_prefix("function ") {
                let name = rest
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_end_matches("()");
                if !name.is_empty() {
                    let line_start_offset =
                        doc.index.position_to_offset(&super::protocol::Position {
                            line: idx as u32,
                            character: 0,
                        });
                    let name_col = line.find(name).unwrap_or(0);
                    let name_start = line_start_offset + name_col;
                    let name_end = name_start + name.len();

                    symbols.push(DocumentSymbol {
                        name: name.to_string(),
                        kind: 12, // Function
                        range: Range {
                            start: doc.index.offset_to_position(line_start_offset),
                            end: doc.index.offset_to_position(line_start_offset + line.len()),
                        },
                        selection_range: Range {
                            start: doc.index.offset_to_position(name_start),
                            end: doc.index.offset_to_position(name_end),
                        },
                    });
                }
            }
        }

        symbols
    }

    fn collect_ast_symbols(
        &self,
        node: &ASTNode<'_>,
        doc: &LspDocument,
        symbols: &mut Vec<DocumentSymbol>,
    ) {
        match node {
            ASTNode::FunctionDeclaration { name, body, span } => {
                let full_range = doc
                    .index
                    .span_to_range(span.start as usize, span.end as usize);
                let name_start = span.start as usize;
                let name_end = name_start + name.len();
                let selection_range = doc.index.span_to_range(name_start, name_end);

                symbols.push(DocumentSymbol {
                    name: (*name).to_string(),
                    kind: 12, // Function
                    range: full_range,
                    selection_range,
                });

                self.collect_ast_symbols(body, doc, symbols);
            }
            ASTNode::StatementList { statements, .. }
            | ASTNode::Block { statements, .. }
            | ASTNode::Subshell { statements, .. } => {
                for stmt in statements {
                    self.collect_ast_symbols(stmt, doc, symbols);
                }
            }
            ASTNode::Pipeline { stages, .. } => {
                for stage in stages {
                    self.collect_ast_symbols(stage, doc, symbols);
                }
            }
            ASTNode::ParallelPipeline { branches, .. } => {
                for branch in branches {
                    self.collect_ast_symbols(branch, doc, symbols);
                }
            }
            ASTNode::LogicalChain { left, right, .. } => {
                self.collect_ast_symbols(left, doc, symbols);
                self.collect_ast_symbols(right, doc, symbols);
            }
            ASTNode::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
                ..
            } => {
                self.collect_ast_symbols(condition, doc, symbols);
                self.collect_ast_symbols(then_branch, doc, symbols);
                for (c, b) in elif_branches {
                    self.collect_ast_symbols(c, doc, symbols);
                    self.collect_ast_symbols(b, doc, symbols);
                }
                if let Some(eb) = else_branch {
                    self.collect_ast_symbols(eb, doc, symbols);
                }
            }
            ASTNode::While {
                condition, body, ..
            } => {
                self.collect_ast_symbols(condition, doc, symbols);
                self.collect_ast_symbols(body, doc, symbols);
            }
            ASTNode::For { body, .. } => {
                self.collect_ast_symbols(body, doc, symbols);
            }
            _ => {}
        }
    }
}

/// Run the LSP server over standard input and standard output.
pub fn run_lsp_server() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());
    let mut server = LspServer::new();

    while let Ok(Some(msg)) = read_message(&mut reader) {
        server.handle_message(&msg, &mut writer)?;
    }

    Ok(())
}
