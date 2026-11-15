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
fn parses_stage_modes_and_optional_tag() {
    let all = config("message: release").unwrap();
    assert_eq!(all.stage, StageMode::All);
    assert!(all.tag.is_none());

    let existing = config("stage: existing\nmessage: release\ntag: v1.0.0").unwrap();
    assert_eq!(existing.stage, StageMode::Existing);
    assert_eq!(existing.tag.as_deref(), Some("v1.0.0"));
}

#[test]
fn rejects_old_fields_and_invalid_values() {
    for yaml in [
        "stage_all_modified: true\nmessage: release",
        "commit_message: release",
        "message: ' '",
        "message: release\ntag: ' '",
        "message: '{{bad.token}}'",
        "stage: invalid\nmessage: release",
    ] {
        assert!(config(yaml).is_err(), "{yaml}");
    }
}

#[test]
fn invalid_template_syntax_uses_fluent_message() {
    let error = config("message: '{{broken}}'").unwrap_err();
    let detail = evin_flow::template::pipeline::validate_template_syntax("{{broken}}")
        .unwrap_err()
        .to_string();
    let expected = evin_locales::t!("git-release-template-syntax-invalid", error = detail);
    assert_eq!(error.message, expected);
}

// These tests would be extremely similar to Version Bumper integration tests
// so we will test the primary behavior and the recovery message.

#[test]
fn validates_push_configuration() {
    assert!(config("message: r\ntag: v1\npush:\n  prune_stale_tags: true\n  base: main").is_ok());
    let error = config("message: r\ntag: v1\npush:\n  remote: ' '\n").unwrap_err();
    let expected = evin_locales::t!("git-release-push-remote-must-not-be-empty");
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    assert_eq!(error.message, expected);
    for yaml in [
        "message: r\npush:\n  remote: ' '",
        "message: r\npush:\n  unknown: true",
    ] {
        assert!(config(yaml).is_err(), "{yaml}");
    }

    assert_fluent_config_error(
        "message: r\npush:\n  branch: ' '\n",
        evin_locales::t!("git-release-push-branch-must-not-be-empty"),
    );
    assert_fluent_config_error(
        "message: r\npush:\n  prune_stale_tags: true\n  base: main",
        evin_locales::t!("git-release-prune-stale-tags-requires-tag"),
    );
    assert_fluent_config_error(
        "message: r\ntag: v1\npush:\n  prune_stale_tags: true",
        evin_locales::t!("git-release-prune-stale-tags-requires-base"),
    );
    assert_fluent_config_error(
        "message: r\npush:\n  base: main",
        evin_locales::t!("git-release-push-base-requires-prune-stale-tags"),
    );
}

fn assert_fluent_config_error(yaml: &str, expected: String) {
    let error = config(yaml).unwrap_err();
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    assert_eq!(error.message, expected, "{yaml}");
}
