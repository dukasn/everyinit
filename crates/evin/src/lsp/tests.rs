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

use super::{CONFIG_CATALOG, EvinLspServer};
use crate::commands::builtin_actions;
use evin_config::semantic::STANDARD_SEMANTIC_TOKEN_TYPES;
use evin_lsp::protocol::{Position, read_message};
use serde_json::{Value, json};
use std::io::{BufReader, Cursor};
use std::path::Path;

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
        "workspace: { name: app, version: 1.0.0 }\nstages: [Test]\nvalues: { artifact: dist }\ntasks:\n  build:\n    stage: Test\n    steps:\n      - uses: evin/docs@v1\n        with: { source_dir: '${artifact}' }\n",
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
fn semantic_tokens_use_utf16_positions_and_clear_for_invalid_flow() {
    let directory = tempfile::tempdir().unwrap();
    workspace(directory.path());
    let mut server = EvinLspServer::default();
    initialize(&mut server, directory.path(), false);
    let document_uri = uri(directory.path(), ".evin/workspace.flow");
    let text = "workspace: { name: \"🦀\", version: 1.0.0 }\nstages: [Test]\ntasks:\n  build:\n    stage: Test\n    description: \"build\"\n    on: { hooks: [pre-commit] }\n    needs: [build]\n    watch: { paths: [\"src/**/*.rs\"] }\n    presets: [ci]\n    steps:\n      - uses: evin/docs@v1\n        with: { source_dir: \"${artifact}\" }\n";
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
            .find(|token| token.0 == line && token.1 < start + length && start < token.1 + token.2)
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
            }})
            .to_string(),
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
            }})
            .to_string(),
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
                        .is_some_and(|value| value.contains("Human-readable label for the task"))
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
            .is_some_and(
                |value| value.contains("Assign the task to a declared workspace pipeline stage")
            )
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
        hover["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|value| value
                .contains("Compile, serve, and validate static project documentation.")),
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
    assert!(value.contains("Compile, serve, and validate static project documentation."));
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

#[test]
fn missing_output_dependency_is_published_and_clears_after_adding_needs() {
    let directory = tempfile::tempdir().unwrap();
    workspace(directory.path());
    let mut server = EvinLspServer::default();
    initialize(&mut server, directory.path(), false);
    let document_uri = uri(directory.path(), ".evin/workspace.flow");
    let original = "workspace: { name: app, version: 1.0.0 }\nstages: [Build]\ntasks:\n  bump: { stage: Build }\n  prepare:\n    stage: Build\n    steps:\n      - uses: evin/changelog@v1\n        with: { path: CHANGELOG.md, version: '${bump.version}' }\n";
    for (version, text, expected) in [
        (1, original.to_owned(), true),
        (
            2,
            original.replace("    steps:", "    needs: [bump]\n    steps:"),
            false,
        ),
    ] {
        let mut output = Vec::new();
        let notification = if version == 1 {
            json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
                "textDocument": { "uri": document_uri, "version": version, "text": text }
            }})
        } else {
            json!({ "jsonrpc": "2.0", "method": "textDocument/didChange", "params": {
                "textDocument": { "uri": document_uri, "version": version },
                "contentChanges": [{ "text": text }]
            }})
        };
        server
            .handle_message(&notification.to_string(), &mut output)
            .unwrap();
        let messages = bodies(output);
        let published = messages
            .iter()
            .find(|message| {
                message["method"] == "textDocument/publishDiagnostics"
                    && message["params"]["uri"] == document_uri
            })
            .unwrap();
        let diagnostic = published["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|diagnostic| diagnostic["code"] == "issue-task-output-missing-dependency");
        assert_eq!(diagnostic.is_some(), expected, "{published}");
        if let Some(diagnostic) = diagnostic {
            assert_eq!(diagnostic["severity"], 1);
            assert_eq!(diagnostic["range"]["start"]["line"], 8);
            assert!(diagnostic["message"].as_str().unwrap().contains("needs:"));
        }
    }
}

#[test]
fn workspace_catalog_returns_tasks_manifests_and_preserves_stale_state() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join(".evin")).unwrap();
    std::fs::create_dir_all(directory.path().join("web/.evin")).unwrap();
    std::fs::write(
        directory.path().join(".evin/workspace.flow"),
        "workspace:\n  name: demo\n  version: 0.3.0\n  modules: [web/]\nstages: [Build, Check]\ntasks:\n  lint:\n    stage: Check\n    description: Run workspace lint\n    on:\n      hooks: [pre-commit]\n    steps:\n      - run: echo lint\n  verify:\n    stage: Check\n    needs: [lint, web:build]\n    steps:\n      - run: echo verify\n",
    )
    .unwrap();
    std::fs::write(
        directory.path().join("web/.evin/module.flow"),
        "module:\n  name: web\ntasks:\n  build:\n    stage: Build\n    description: Build web bundle\n    steps:\n      - run: echo web\n",
    )
    .unwrap();

    let mut server = EvinLspServer::default();
    initialize(&mut server, directory.path(), false);

    let mut output = Vec::new();
    server
        .handle_message(
            &json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "everyinit/workspaceCatalog",
                "params": {}
            })
            .to_string(),
            &mut output,
        )
        .unwrap();
    let result = &bodies(output)[0]["result"];
    assert_eq!(result["workspaceName"], "demo");
    assert_eq!(result["workspaceVersion"], "0.3.0");
    assert_eq!(result["stages"], json!(["Build", "Check"]));
    assert_eq!(result["stale"], false);
    assert_eq!(result["diagnostics"]["errors"], 0);

    let manifests = result["manifests"].as_array().unwrap();
    assert!(
        manifests
            .iter()
            .any(|m| m["path"] == ".evin/workspace.flow" && m["kind"] == "workspace")
    );
    assert!(
        manifests
            .iter()
            .any(|m| m["path"] == "web/.evin/module.flow" && m["kind"] == "module")
    );

    let tasks = result["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 3);
    let lint = tasks.iter().find(|t| t["name"] == "lint").unwrap();
    assert_eq!(lint["stage"], "Check");
    assert_eq!(lint["description"], "Run workspace lint");
    assert_eq!(lint["hooks"], json!(["pre-commit"]));
    assert!(
        lint["location"]["uri"]
            .as_str()
            .unwrap()
            .ends_with(".evin/workspace.flow")
    );

    let web_build = tasks.iter().find(|t| t["name"] == "web:build").unwrap();
    assert_eq!(web_build["module"], "web");
    assert_eq!(web_build["shortName"], "build");
    assert_eq!(web_build["stage"], "Build");

    // Break workspace.flow temporarily and verify stale tasks remain available
    let ws_uri = uri(directory.path(), ".evin/workspace.flow");
    let mut change_out = Vec::new();
    server
        .handle_message(
            &json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": ws_uri,
                        "version": 1,
                        "text": "workspace: [\n"
                    }
                }
            })
            .to_string(),
            &mut change_out,
        )
        .unwrap();

    let mut stale_out = Vec::new();
    server
        .handle_message(
            &json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "everyinit/workspaceCatalog",
                "params": {}
            })
            .to_string(),
            &mut stale_out,
        )
        .unwrap();
    let stale_result = &bodies(stale_out)[0]["result"];
    assert_eq!(stale_result["stale"], true);
    assert_eq!(stale_result["tasks"].as_array().unwrap().len(), 3);
    assert!(stale_result["diagnostics"]["errors"].as_u64().unwrap() >= 1);
}
