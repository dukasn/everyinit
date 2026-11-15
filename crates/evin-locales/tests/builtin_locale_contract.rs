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

use fluent_bundle::FluentResource;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("read builtin source directory") {
        let path = entry.expect("read builtin source entry").path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn english_messages() -> BTreeMap<String, BTreeSet<String>> {
    let variables = Regex::new(r"\$([a-z][a-z0-9_-]*)").unwrap();
    let mut messages = BTreeMap::new();
    for resource in evin_io::load_ftl_resources("en") {
        FluentResource::try_new(resource.content.clone())
            .unwrap_or_else(|(_, errors)| panic!("malformed {}: {errors:?}", resource.path));

        let mut current: Option<(String, String)> = None;
        let mut insert = |current: Option<(String, String)>| {
            if let Some((key, value)) = current {
                let names = variables
                    .captures_iter(&value)
                    .map(|capture| capture[1].to_string())
                    .collect();
                assert!(
                    messages.insert(key.clone(), names).is_none(),
                    "English Fluent key '{key}' is declared more than once"
                );
            }
        };

        for line in resource.content.lines() {
            let is_entry = !line.starts_with(char::is_whitespace)
                && !line.starts_with('#')
                && line.contains(" = ");
            if is_entry {
                insert(current.take());
                let (key, value) = line.split_once(" = ").unwrap();
                current = Some((key.to_string(), value.to_string()));
            } else if let Some((_, value)) = current.as_mut()
                && line.starts_with(char::is_whitespace)
            {
                value.push('\n');
                value.push_str(line);
            }
        }
        insert(current.take());
    }
    messages
}

fn macro_calls(source: &str) -> Vec<(String, String)> {
    let marker = "evin_locales::t!";
    let mut calls = Vec::new();
    let mut search_from = 0;

    while let Some(relative_start) = source[search_from..].find(marker) {
        let marker_end = search_from + relative_start + marker.len();
        let open = source[marker_end..]
            .find('(')
            .map(|offset| marker_end + offset)
            .expect("Fluent macro call has opening parenthesis");
        let key_quote = source[open + 1..]
            .find('"')
            .map(|offset| open + 1 + offset)
            .expect("Fluent macro call has a literal key");
        let key_end = source[key_quote + 1..]
            .find('"')
            .map(|offset| key_quote + 1 + offset)
            .expect("Fluent macro key is closed");
        let key = source[key_quote + 1..key_end].to_string();

        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        let mut close = None;
        for (offset, character) in source[open..].char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    in_string = false;
                }
                continue;
            }
            match character {
                '"' => in_string = true,
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close.expect("Fluent macro call has closing parenthesis");
        calls.push((key, source[key_end + 1..close].to_string()));
        search_from = close + 1;
    }
    calls
}

fn issue_code_keys(source: &str) -> Vec<String> {
    let issue_code =
        Regex::new(r#"evin_locales::diagnostic\(\s*evin_diagnostic::IssueCode::new\(\s*"([^"]+)""#)
            .unwrap();
    issue_code
        .captures_iter(source)
        .map(|capture| capture[1].to_string())
        .collect()
}

#[test]
fn builtin_fluent_calls_match_english_keys_and_arguments() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let builtins = root.join("builtins");
    let messages = english_messages();
    let argument = Regex::new(r"\b([a-z][a-z0-9_]*)\s*=").unwrap();
    let mut files = Vec::new();
    rust_files(&builtins, &mut files);

    let mut calls = 0;
    for path in files {
        let source = std::fs::read_to_string(&path).expect("read builtin Rust source");
        for key in issue_code_keys(&source) {
            assert!(
                messages.contains_key(&key),
                "{} references IssueCode key '{key}' without an English Fluent message",
                path.display()
            );
        }
        for (key, arguments) in macro_calls(&source) {
            calls += 1;
            let declared = messages.get(&key).unwrap_or_else(|| {
                panic!("{} references missing English key '{key}'", path.display())
            });
            let passed = argument
                .captures_iter(&arguments)
                .map(|capture| capture[1].to_string())
                .collect::<BTreeSet<_>>();
            assert_eq!(
                &passed,
                declared,
                "Fluent arguments differ for key '{key}' in {}",
                path.display()
            );
        }
    }
    assert!(calls > 0, "expected to find builtin Fluent macro calls");
}
