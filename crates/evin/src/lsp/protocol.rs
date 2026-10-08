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

use evin_lsp::protocol::{
    CompletionItem, DocumentSymbol, Hover, Location, Position, SemanticTokens, WorkspaceSymbol,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Protocol Definitions & Traits
// ---------------------------------------------------------------------------

pub trait LspRequest {
    const METHOD: &'static str;
    type Params: for<'de> Deserialize<'de>;
    type Result: Serialize;
}

pub trait LspNotification {
    const METHOD: &'static str;
    type Params: for<'de> Deserialize<'de>;
}

// Request Markers
pub struct InitializeRequest;
impl LspRequest for InitializeRequest {
    const METHOD: &'static str = "initialize";
    type Params = InitializeParams;
    type Result = Value;
}

pub struct ShutdownRequest;
impl LspRequest for ShutdownRequest {
    const METHOD: &'static str = "shutdown";
    type Params = Value;
    type Result = Value;
}

pub struct CompletionRequest;
impl LspRequest for CompletionRequest {
    const METHOD: &'static str = "textDocument/completion";
    type Params = TextDocumentPositionParams;
    type Result = Vec<CompletionItem>;
}

pub struct HoverRequest;
impl LspRequest for HoverRequest {
    const METHOD: &'static str = "textDocument/hover";
    type Params = TextDocumentPositionParams;
    type Result = Option<Hover>;
}

pub struct SemanticTokensFullRequest;
impl LspRequest for SemanticTokensFullRequest {
    const METHOD: &'static str = "textDocument/semanticTokens/full";
    type Params = DocumentUriParams;
    type Result = SemanticTokens;
}

pub struct GotoDefinitionRequest;
impl LspRequest for GotoDefinitionRequest {
    const METHOD: &'static str = "textDocument/definition";
    type Params = TextDocumentPositionParams;
    type Result = Option<Location>;
}

pub struct ReferencesRequest;
impl LspRequest for ReferencesRequest {
    const METHOD: &'static str = "textDocument/references";
    type Params = ReferenceParams;
    type Result = Vec<Location>;
}

pub struct DocumentSymbolRequest;
impl LspRequest for DocumentSymbolRequest {
    const METHOD: &'static str = "textDocument/documentSymbol";
    type Params = DocumentUriParams;
    type Result = Vec<DocumentSymbol>;
}

pub struct WorkspaceSymbolRequest;
impl LspRequest for WorkspaceSymbolRequest {
    const METHOD: &'static str = "workspace/symbol";
    type Params = WorkspaceSymbolParams;
    type Result = Vec<WorkspaceSymbol>;
}

// Notification Markers
pub struct InitializedNotification;
impl LspNotification for InitializedNotification {
    const METHOD: &'static str = "initialized";
    type Params = Value;
}

pub struct DidOpenNotification;
impl LspNotification for DidOpenNotification {
    const METHOD: &'static str = "textDocument/didOpen";
    type Params = DidOpenTextDocumentParams;
}

pub struct DidChangeNotification;
impl LspNotification for DidChangeNotification {
    const METHOD: &'static str = "textDocument/didChange";
    type Params = DidChangeTextDocumentParams;
}

pub struct DidCloseNotification;
impl LspNotification for DidCloseNotification {
    const METHOD: &'static str = "textDocument/didClose";
    type Params = DidCloseTextDocumentParams;
}

pub struct ExitNotification;
impl LspNotification for ExitNotification {
    const METHOD: &'static str = "exit";
    type Params = Value;
}

// ---------------------------------------------------------------------------
// Strongly-Typed Parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct InitializeParams {
    pub root_uri: Option<String>,
    pub capabilities: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentUriParams {
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentPositionParams {
    pub text_document: TextDocumentIdentifier,
    pub position: PositionParam,
}

#[derive(Debug, Deserialize)]
pub struct TextDocumentIdentifier {
    pub uri: String,
}

#[derive(Debug, Deserialize, Copy, Clone)]
pub struct PositionParam {
    pub line: u32,
    pub character: u32,
}

impl From<PositionParam> for Position {
    fn from(param: PositionParam) -> Self {
        Position {
            line: param.line,
            character: param.character,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceParams {
    pub text_document: TextDocumentIdentifier,
    pub position: PositionParam,
    #[serde(default)]
    pub context: ReferenceContext,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ReferenceContext {
    pub include_declaration: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct WorkspaceSymbolParams {
    pub query: String,
}

pub fn deserialize_version<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<i32>::deserialize(deserializer)?.unwrap_or(0))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DidOpenTextDocumentParams {
    pub text_document: TextDocumentItem,
}

#[derive(Debug, Deserialize)]
pub struct TextDocumentItem {
    pub uri: String,
    #[serde(default, deserialize_with = "deserialize_version")]
    pub version: i32,
    pub text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DidChangeTextDocumentParams {
    pub text_document: VersionedTextDocumentIdentifier,
    pub content_changes: Vec<TextDocumentContentChangeEvent>,
}

#[derive(Debug, Deserialize)]
pub struct VersionedTextDocumentIdentifier {
    pub uri: String,
    #[serde(default, deserialize_with = "deserialize_version")]
    pub version: i32,
}

#[derive(Debug, Deserialize)]
pub struct TextDocumentContentChangeEvent {
    pub range: Option<Value>,
    pub text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DidCloseTextDocumentParams {
    pub text_document: TextDocumentIdentifier,
}

// ---------------------------------------------------------------------------
// Error Representation
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct LspError {
    pub code: i64,
    pub message: String,
}

impl LspError {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const SERVER_NOT_INITIALIZED: i64 = -32002;

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: Self::INVALID_PARAMS,
            message: message.into(),
        }
    }
}
