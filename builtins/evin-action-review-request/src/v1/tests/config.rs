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

use super::*;

#[test]
fn decision_table() {
    assert_eq!(decide(true, false), Decision::Create);
    assert_eq!(decide(true, true), Decision::Update);
    assert_eq!(decide(false, true), Decision::Close);
    assert_eq!(decide(false, false), Decision::Nothing);
}

#[test]
fn parses_flags() {
    assert_eq!(parse_flag("true"), Ok(true));
    assert_eq!(parse_flag(" False "), Ok(false));
    assert_eq!(
        parse_flag("yes").unwrap_err(),
        evin_locales::t!("review-request-wanted-must-be-boolean", value = "yes")
    );
}

#[test]
fn invalid_template_syntax_uses_fluent_message() {
    let error = config(&format!("{BASE}title: '${{version'\n")).unwrap_err();
    let detail = evin_flow::template::pipeline::validate_template_syntax("${version")
        .unwrap_err()
        .to_string();
    assert_fluent_message(
        error.message,
        evin_locales::t!("review-request-template-syntax-invalid", error = detail),
    );
}

#[test]
fn validates_config() {
    assert!(config(&format!("{BASE}title: t")).is_ok());
    assert!(config(&format!("{BASE}wanted: false")).is_ok());
    assert!(
        config(&format!(
            "{BASE}wanted: \"${{release:bump.changed}}\"\ntitle: t"
        ))
        .is_ok()
    );
    for extra in [
        "",
        "title: t\nbody_path: ../x.md",
        "title: t\nbody_path: /x.md",
        "title: t\nunknown: 1",
        "title: t\ncontext:\n  bad-key: v",
        "title: ' '",
    ] {
        assert!(config(&format!("{BASE}{extra}")).is_err(), "{extra}");
    }
    assert!(config("branch: b\nbase: main\ntitle: t").is_err());
}

#[test]
fn configuration_errors_use_fluent_messages() {
    let error = config(&format!("{BASE}title: t\ncontext:\n  bad-key: value\n")).unwrap_err();
    assert_fluent_message(
        error.message,
        evin_locales::t!("review-request-context-key-invalid", key = "bad-key"),
    );

    let error = config(&format!("{BASE}title: ' '\n")).unwrap_err();
    assert_fluent_message(
        error.message,
        evin_locales::t!(
            "review-request-field-must-not-be-empty-when-provided",
            name = "title"
        ),
    );

    for (yaml, field) in [
        (
            "branch: ' '\nbase: main\ncommands:\n  find: echo 1\n",
            "branch",
        ),
        ("branch: b\nbase: ' '\ncommands:\n  find: echo 1\n", "base"),
        (
            "branch: b\nbase: main\ncommands:\n  find: ' '\n",
            "commands.find",
        ),
    ] {
        let error = config(yaml).unwrap_err();
        assert_fluent_message(
            error.message,
            evin_locales::t!("review-request-field-must-not-be-empty", name = field),
        );
    }

    let error = config(&format!("{BASE}body_path: ' '\ntitle: t\n")).unwrap_err();
    assert_fluent_message(
        error.message,
        evin_locales::t!(
            "review-request-field-must-not-be-empty-when-provided",
            name = "body_path"
        ),
    );

    for path in ["../x.md", "/x.md"] {
        let error = config(&format!("{BASE}title: t\nbody_path: {path}\n")).unwrap_err();
        assert_fluent_message(
            error.message,
            evin_locales::t!("review-request-body-path-must-stay-in-workspace"),
        );
    }
}

fn assert_fluent_message(actual: String, expected: String) {
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    assert_eq!(actual, expected);
}
