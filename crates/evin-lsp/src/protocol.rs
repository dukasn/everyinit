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

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Read, Write};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: Option<u32>,
    pub code: Option<String>,
    pub source: Option<String>,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishDiagnosticsParams {
    pub uri: String,
    pub diagnostics: Vec<Diagnostic>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hover {
    pub contents: MarkupContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkupContent {
    pub kind: String,
    pub value: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionItem {
    pub label: String,
    pub kind: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<MarkupContent>,
    #[serde(rename = "filterText", skip_serializing_if = "Option::is_none")]
    pub filter_text: Option<String>,
    #[serde(rename = "insertText", skip_serializing_if = "Option::is_none")]
    pub insert_text: Option<String>,
    #[serde(rename = "insertTextFormat", skip_serializing_if = "Option::is_none")]
    pub insert_text_format: Option<u32>,
    #[serde(rename = "textEdit", skip_serializing_if = "Option::is_none")]
    pub text_edit: Option<TextEdit>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEdit {
    pub range: Range,
    #[serde(rename = "newText")]
    pub new_text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticTokens {
    pub data: Vec<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: u32,
    pub range: Range,
    pub selection_range: Range,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSymbol {
    pub name: String,
    pub kind: u32,
    pub location: Location,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
}

pub fn read_message<R: Read + BufRead>(reader: &mut R) -> io::Result<Option<String>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end_matches(&['\r', '\n'][..]);
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:")
            && let Ok(length) = value.trim().parse::<usize>()
        {
            content_length = Some(length);
        }
    }
    let Some(length) = content_length else {
        return Ok(None);
    };
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    String::from_utf8(body)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub fn write_message<W: Write>(writer: &mut W, body: &str) -> io::Result<()> {
    writer.write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())?;
    writer.write_all(body.as_bytes())?;
    writer.flush()
}
