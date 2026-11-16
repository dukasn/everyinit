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

fn matching_parenthesis(source: &str, open: usize) -> usize {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
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
                    return open + offset;
                }
            }
            _ => {}
        }
    }
    panic!("unclosed call in builtin source")
}

fn issue_code_calls(source: &str) -> Vec<(String, BTreeSet<String>)> {
    let issue_code = Regex::new(r#"(?:evin_diagnostic::)?IssueCode::new\(\s*"([^"]+)""#).unwrap();
    let mut calls = Vec::new();
    for captures in issue_code.captures_iter(source) {
        let matched = captures.get(0).unwrap();
        let key = captures[1].to_string();
        let mut cursor = matched.end();
        while source[cursor..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        {
            cursor += source[cursor..].chars().next().unwrap().len_utf8();
        }
        if source.as_bytes().get(cursor) == Some(&b',') {
            cursor += 1;
            while source[cursor..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
            {
                cursor += source[cursor..].chars().next().unwrap().len_utf8();
            }
        }
        assert_eq!(source.as_bytes().get(cursor), Some(&b')'));
        cursor += 1;

        let mut arguments = BTreeSet::new();
        loop {
            while source[cursor..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
            {
                cursor += source[cursor..].chars().next().unwrap().len_utf8();
            }
            if !source[cursor..].starts_with(".arg") {
                break;
            }
            cursor += ".arg".len();
            while source[cursor..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
            {
                cursor += source[cursor..].chars().next().unwrap().len_utf8();
            }
            assert_eq!(source.as_bytes().get(cursor), Some(&b'('));
            let open = cursor;
            cursor += 1;
            while source[cursor..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
            {
                cursor += source[cursor..].chars().next().unwrap().len_utf8();
            }
            assert_eq!(source.as_bytes().get(cursor), Some(&b'"'));
            cursor += 1;
            let name_end = source[cursor..]
                .find('"')
                .map(|offset| cursor + offset)
                .expect("IssueCode argument name is closed");
            arguments.insert(source[cursor..name_end].to_string());
            cursor = matching_parenthesis(source, open) + 1;
        }
        calls.push((key, arguments));
    }
    calls
}

#[test]
fn builtin_fluent_calls_match_english_keys_and_arguments() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let builtins = root.join("builtins");
    let messages = english_messages();
    let argument = Regex::new(r"\b([a-z][a-z0-9_]*)\s*=").unwrap();
    // These codes come from evin-doc's parser and are wrapped with a localized
    // docs-level message by the builtin instead of owning a builtin FTL entry.
    let framework_issue_codes = ["invalid-filename", "missing-translation"];
    let mut files = Vec::new();
    rust_files(&builtins, &mut files);

    let mut calls = 0;
    for path in files {
        let source = std::fs::read_to_string(&path).expect("read builtin Rust source");
        for (key, passed) in issue_code_calls(&source) {
            if framework_issue_codes.contains(&key.as_str()) {
                continue;
            }
            let declared = messages.get(&key).unwrap_or_else(|| {
                panic!(
                    "{} references IssueCode key '{key}' without an English Fluent message",
                    path.display()
                )
            });
            assert_eq!(
                &passed,
                declared,
                "IssueCode Fluent arguments differ for key '{key}' in {}",
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

#[test]
fn builtin_diagnostic_issue_codes_are_fluent_test_backed() {
    // These are defensive or framework-owned paths that cannot be exercised
    // through a stable builtin input. Keep this list small and remove entries
    // when a behavioral test is added.
    let unexercised_issue_codes = BTreeSet::from([
        "archive-blocking-task-failed",
        "catalog-json-serialization-failed",
        "catalog-output-invalid",
        "docs-temporary-output-prepare-failed",
        "invalid-file",
        "review-request-body-path-unusable",
        "tag-recovery-failed",
    ])
    .into_iter()
    .map(str::to_string)
    .collect::<BTreeSet<_>>();
    // These codes are supplied by evin-doc; the docs builtin adds localized
    // wrapper diagnostics where needed.
    let framework_issue_codes = ["invalid-filename", "missing-translation"];
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    rust_files(&root.join("builtins"), &mut files);
    let mut production_codes = BTreeSet::new();
    let mut test_references = BTreeSet::new();

    for path in files {
        let source = std::fs::read_to_string(&path).expect("read builtin Rust source");
        let standalone_test = path
            .components()
            .any(|component| component.as_os_str() == "tests");
        let test_start = source.find("#[cfg(test)]").unwrap_or(source.len());
        let production = if standalone_test {
            ""
        } else {
            &source[..test_start]
        };
        let tests = if standalone_test {
            source.as_str()
        } else {
            &source[test_start..]
        };

        production_codes.extend(issue_code_calls(production).into_iter().map(|(key, _)| key));
        test_references.extend(issue_code_calls(tests).into_iter().map(|(key, _)| key));
        test_references.extend(macro_calls(tests).into_iter().map(|(key, _)| key));
    }

    let production_codes = production_codes
        .into_iter()
        .filter(|key| !framework_issue_codes.contains(&key.as_str()))
        .collect::<BTreeSet<_>>();
    let untested = production_codes
        .difference(&test_references)
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        untested, unexercised_issue_codes,
        "update Fluent diagnostic coverage tests or document why a new issue code cannot be exercised"
    );
    assert!(
        (production_codes.len() - untested.len()) * 100 >= production_codes.len() * 90,
        "at least 90% of builtin diagnostic issue codes must have Fluent-backed test coverage"
    );
}
