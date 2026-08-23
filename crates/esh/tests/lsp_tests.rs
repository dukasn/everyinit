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

use esh::lsp::protocol::{read_message, write_message};
use esh::lsp::{LspServer, compute_semantic_tokens};
use esh::syntax::SourcePositionIndex;
use serde_json::{Value, json};
use std::io::Cursor;

#[test]
fn test_jsonrpc_message_framing() {
    let mut buffer = Vec::new();
    let body = r#"{"jsonrpc":"2.0","method":"test","params":{}}"#;
    write_message(&mut buffer, body).expect("write message");

    let expected_prefix = format!("Content-Length: {}\r\n\r\n", body.len());
    assert!(buffer.starts_with(expected_prefix.as_bytes()));

    let mut cursor = Cursor::new(buffer);
    let decoded = read_message(&mut cursor)
        .expect("read message")
        .expect("some message");
    assert_eq!(decoded, body);
}

#[test]
fn test_utf16_unicode_position_indexing() {
    // 🦀 is 4 UTF-8 bytes and 2 UTF-16 code units
    let source = "echo '🦀'\necho 'hello'\n";
    let index = SourcePositionIndex::new(source);

    // Line 0: "echo '🦀'\n"
    // Byte offset of \n is 11, line 1 starts at 12
    let pos_line1 = index.offset_to_position(12);
    assert_eq!(pos_line1.line, 1);
    assert_eq!(pos_line1.character, 0);

    // UTF-16 position within line 0 right after 🦀:
    // "echo '" is 6 UTF-16 units. "🦀" is 2 UTF-16 units.
    // Total UTF-16 position right after quote is 6 + 2 + 1 = 9.
    let after_crab_byte_offset = 6 + 4; // 'echo '' is 6 bytes + 4 bytes crab
    let pos_crab = index.offset_to_position(after_crab_byte_offset);
    assert_eq!(pos_crab.line, 0);
    assert_eq!(pos_crab.character, 8); // 6 + 2
}

#[test]
fn test_lsp_initialize_and_capabilities() {
    let mut server = LspServer::new();
    let mut output = Vec::new();

    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "rootUri": "file:///workspace"
        }
    });

    server
        .handle_message(&init_req.to_string(), &mut output)
        .expect("handle message");

    let mut cursor = Cursor::new(output);
    let msg = read_message(&mut cursor).expect("read").expect("message");
    let resp: Value = serde_json::from_str(&msg).expect("json parse");

    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["capabilities"]["textDocumentSync"], 1);
    assert_eq!(resp["result"]["capabilities"]["hoverProvider"], true);
    assert_eq!(
        resp["result"]["capabilities"]["semanticTokensProvider"]["full"],
        true
    );
    assert_eq!(resp["result"]["serverInfo"]["name"], "esh-lsp");
}

#[test]
fn test_lsp_did_open_and_hover_lookup() {
    let mut server = LspServer::new();
    let mut output = Vec::new();

    let uri = "file:///workspace/script.esh";
    let script = "read -r -p 'Name: ' USERNAME\nset -e -u -o pipefail\n";

    // Initialize first
    let init_req = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" });
    server
        .handle_message(&init_req.to_string(), &mut output)
        .expect("init");

    // Open document
    let did_open = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "languageId": "esh",
                "version": 1,
                "text": script
            }
        }
    });
    server
        .handle_message(&did_open.to_string(), &mut output)
        .expect("did open");

    // Hover on `read` (line 0, col 1)
    let mut hover_output = Vec::new();
    let hover_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/hover",
        "params": {
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 1 }
        }
    });
    server
        .handle_message(&hover_req.to_string(), &mut hover_output)
        .expect("hover");

    let mut cursor = Cursor::new(hover_output);
    let msg = read_message(&mut cursor).expect("read").expect("message");
    let resp: Value = serde_json::from_str(&msg).expect("json parse");

    assert_eq!(resp["id"], 2);
    let hover_val = resp["result"]["contents"]["value"].as_str().unwrap();
    assert!(
        hover_val.contains("Unified input tool"),
        "Hover info for read: {}",
        hover_val
    );
}

#[test]
fn test_lsp_precise_diagnostic_span() {
    let mut server = LspServer::new();
    let mut output = Vec::new();

    let init_req = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" });
    server
        .handle_message(&init_req.to_string(), &mut output)
        .expect("init");

    let uri = "file:///workspace/syntax_err.esh";
    // Line 0: valid, Line 1: syntax error (missing fi or invalid syntax)
    let script = "echo 'hello'\nif true; then\n";

    let mut diag_output = Vec::new();
    let did_open = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "languageId": "esh",
                "version": 1,
                "text": script
            }
        }
    });
    server
        .handle_message(&did_open.to_string(), &mut diag_output)
        .expect("did open");

    let mut cursor = Cursor::new(diag_output);
    let msg = read_message(&mut cursor).expect("read").expect("message");
    let notif: Value = serde_json::from_str(&msg).expect("json parse");

    assert_eq!(notif["method"], "textDocument/publishDiagnostics");
    let diags = notif["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(!diags.is_empty(), "Should report syntax error diagnostic");

    let first_diag = &diags[0];
    let start_line = first_diag["range"]["start"]["line"].as_u64().unwrap();
    assert!(
        start_line >= 1,
        "Diagnostic error should target the offending line: {}",
        start_line
    );
}

#[test]
fn test_lsp_did_close_clears_diagnostics() {
    let mut server = LspServer::new();
    let mut output = Vec::new();

    let init_req = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" });
    server
        .handle_message(&init_req.to_string(), &mut output)
        .expect("init");

    let uri = "file:///workspace/temp.esh";
    let did_open = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "languageId": "esh",
                "version": 1,
                "text": "echo 'hi'\n"
            }
        }
    });
    server
        .handle_message(&did_open.to_string(), &mut output)
        .expect("did open");

    let mut close_output = Vec::new();
    let did_close = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didClose",
        "params": {
            "textDocument": {
                "uri": uri
            }
        }
    });
    server
        .handle_message(&did_close.to_string(), &mut close_output)
        .expect("did close");

    let mut cursor = Cursor::new(close_output);
    let msg = read_message(&mut cursor).expect("read").expect("message");
    let notif: Value = serde_json::from_str(&msg).expect("json parse");

    assert_eq!(notif["method"], "textDocument/publishDiagnostics");
    let diags = notif["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        diags.is_empty(),
        "Closed document diagnostics must be cleared"
    );
}

#[test]
fn test_lsp_semantic_tokens_computation() {
    let script = "if read -r NAME; then\n    echo \"$NAME\"\nfi";
    let tokens = compute_semantic_tokens(script);

    // Tokens must be 5-tuples (length divisible by 5)
    assert_eq!(tokens.len() % 5, 0);
    assert!(!tokens.is_empty());
}

#[test]
fn test_lsp_completions_list() {
    let mut server = LspServer::new();
    let mut output = Vec::new();

    let init_req = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" });
    server
        .handle_message(&init_req.to_string(), &mut output)
        .expect("init");

    let completion_req = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "textDocument/completion",
        "params": {
            "textDocument": { "uri": "file:///test.esh" },
            "position": { "line": 0, "character": 0 }
        }
    });
    server
        .handle_message(&completion_req.to_string(), &mut output)
        .expect("completions");

    let mut cursor = Cursor::new(output);
    let _init_msg = read_message(&mut cursor)
        .expect("read")
        .expect("init message");
    let msg = read_message(&mut cursor)
        .expect("read")
        .expect("completion message");
    let resp: Value = serde_json::from_str(&msg).expect("json parse");

    let items = resp["result"]
        .as_array()
        .expect("array of completion items");
    assert!(items.iter().any(|item| item["label"] == "read"));
    assert!(items.iter().any(|item| item["label"] == "declare"));
    assert!(items.iter().any(|item| item["label"] == "if"));
}

#[test]
fn test_lsp_document_symbols_both_syntaxes() {
    let mut server = LspServer::new();
    let mut output = Vec::new();

    let init_req = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" });
    server
        .handle_message(&init_req.to_string(), &mut output)
        .expect("init");

    let uri = "file:///workspace/functions.esh";
    let script =
        "deploy() {\n    echo 'deploying'\n}\n\nfunction rollback() {\n    echo 'rollback'\n}\n";

    let did_open = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "languageId": "esh",
                "version": 1,
                "text": script
            }
        }
    });
    server
        .handle_message(&did_open.to_string(), &mut output)
        .expect("did open");

    let mut symbols_output = Vec::new();
    let symbols_req = json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "textDocument/documentSymbol",
        "params": {
            "textDocument": { "uri": uri }
        }
    });
    server
        .handle_message(&symbols_req.to_string(), &mut symbols_output)
        .expect("symbols");

    let mut cursor = Cursor::new(symbols_output);
    let msg = read_message(&mut cursor).expect("read").expect("message");
    let resp: Value = serde_json::from_str(&msg).expect("json parse");

    let symbols = resp["result"].as_array().expect("array of symbols");
    assert!(
        symbols
            .iter()
            .any(|s| s["name"] == "deploy" && s["kind"] == 12)
    );
    assert!(
        symbols
            .iter()
            .any(|s| s["name"] == "rollback" && s["kind"] == 12)
    );
}

#[test]
fn test_lsp_malformed_json_returns_error() {
    let mut server = LspServer::new();
    let mut output = Vec::new();

    let malformed = "this is not json";
    server
        .handle_message(malformed, &mut output)
        .expect("handle message");

    let mut cursor = Cursor::new(output);
    let msg = read_message(&mut cursor).expect("read").expect("message");
    let resp: Value = serde_json::from_str(&msg).expect("json parse");

    assert_eq!(resp["error"]["code"], -32700);
}
