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
    let issue = parse_flag("yes").unwrap_err();
    assert_eq!(issue.key, "review-request-wanted-must-be-boolean");
    assert_eq!(
        evin_locales::render_issue(&issue),
        evin_locales::t!("review-request-wanted-must-be-boolean", value = "yes")
    );
}

#[test]
fn invalid_template_syntax_uses_fluent_message() {
    let detail = evin_flow::template::pipeline::validate_template_syntax("${version")
        .unwrap_err()
        .to_string();
    assert_fluent_config_error(
        &format!("{BASE}title: '${{version'\n"),
        "review-request-template-syntax-invalid",
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
    assert!(config(&format!("{BASE}title: t\nunknown: 1")).is_err());
    assert_fluent_config_error(
        BASE,
        "review-request-title-required",
        evin_locales::t!("review-request-title-required"),
    );
}

#[test]
fn configuration_errors_use_fluent_messages() {
    assert_fluent_config_error(
        &format!("{BASE}title: t\ncontext:\n  bad-key: value\n"),
        "review-request-context-key-invalid",
        evin_locales::t!("review-request-context-key-invalid", key = "bad-key"),
    );

    assert_fluent_config_error(
        &format!("{BASE}title: ' '\n"),
        "review-request-field-must-not-be-empty-when-provided",
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
        assert_fluent_config_error(
            yaml,
            "review-request-field-must-not-be-empty",
            evin_locales::t!("review-request-field-must-not-be-empty", name = field),
        );
    }

    assert_fluent_config_error(
        &format!("{BASE}body_path: ' '\ntitle: t\n"),
        "review-request-field-must-not-be-empty-when-provided",
        evin_locales::t!(
            "review-request-field-must-not-be-empty-when-provided",
            name = "body_path"
        ),
    );

    for path in ["../x.md", "/x.md"] {
        assert_fluent_config_error(
            &format!("{BASE}title: t\nbody_path: {path}\n"),
            "review-request-body-path-must-stay-in-workspace",
            evin_locales::t!("review-request-body-path-must-stay-in-workspace"),
        );
    }
}

fn assert_fluent_config_error(yaml: &str, key: &str, expected: String) {
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    let with = evin_flow::parse_yaml(yaml).expect("valid YAML");
    let parsed: ReviewRequestConfig = evin_action_sdk::parse_config(Some(&with)).unwrap();
    let issue = parsed.validate().unwrap_err();
    assert_eq!(issue.key, key, "{yaml}");
    assert_eq!(evin_locales::render_issue(&issue), expected, "{yaml}");

    let error = ReviewRequestConfig::from_with(Some(with)).unwrap_err();
    assert_eq!(error.message, expected, "{yaml}");
}
