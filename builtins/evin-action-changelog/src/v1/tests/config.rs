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
fn rejects_an_empty_notes_path() {
    let with = evin_flow::parse_yaml("notes_path: \"  \"\n").unwrap();
    let error = validate_config(Some(&with)).unwrap_err();
    let expected = evin_locales::t!("notes-path-must-not-be-empty-when-provided");
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    assert_eq!(error.message, expected);
}

#[test]
fn semantic_configuration_errors_keep_fluent_issue_codes() {
    let with = evin_flow::parse_yaml("path: ' '\n").unwrap();
    let Err(ChangelogError::ConfigurationIssue(issue)) = ChangelogConfig::from_with(Some(with))
    else {
        panic!("expected a structured changelog configuration issue");
    };

    assert_eq!(issue.key, "changelog-path-must-not-be-empty");
    assert_eq!(
        evin_locales::render_issue(&issue),
        evin_locales::t!("changelog-path-must-not-be-empty")
    );
}

#[test]
fn configuration_failures_use_fluent_messages() {
    let section_error = evin_flow::template::pipeline::validate_template_syntax("{{broken}}")
        .unwrap_err()
        .to_string();
    let cases = [
        (
            "path: ' '\n",
            evin_locales::t!("changelog-path-must-not-be-empty"),
        ),
        (
            "section: ' '\nnotes_path: notes.md\n",
            evin_locales::t!("section-must-be-non-empty-and-requires-notes-path"),
        ),
        (
            "section: v1.0.0\n",
            evin_locales::t!("section-must-be-non-empty-and-requires-notes-path"),
        ),
        (
            "section: '{{broken}}'\nnotes_path: notes.md\n",
            evin_locales::t!("section-template-invalid", error = &section_error),
        ),
        (
            "unreleased_heading: ' '\n",
            evin_locales::t!("unreleased-heading-must-not-be-empty"),
        ),
        (
            "sections:\n  ' ': Maintenance\n",
            evin_locales::t!("sections-keys-must-not-be-empty"),
        ),
        (
            "version: ' '\n",
            evin_locales::t!("version-must-not-be-empty"),
        ),
        (
            "version: '{{broken}}'\n",
            evin_locales::t!("issue-changelog-version-invalid", error = &section_error),
        ),
        (
            "include_unreleased: false\n",
            evin_locales::t!("version-is-required-when-include-unreleased-is-false"),
        ),
        (
            "label: ' '\n",
            evin_locales::t!("label-must-not-be-empty-when-provided"),
        ),
        (
            "label: '{{broken}}'\n",
            evin_locales::t!("label-template-invalid", error = &section_error),
        ),
    ];

    for (yaml, expected) in cases {
        let with = evin_flow::parse_yaml(yaml).unwrap();
        let error = validate_config(Some(&with)).unwrap_err();
        assert!(
            !expected.starts_with("<missing locale key:"),
            "expected Fluent message to exist for {yaml:?}: {expected}"
        );
        assert_eq!(error.message, expected, "{yaml:?}");
    }
}
