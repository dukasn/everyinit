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
        "message: '{{bad.token}}'",
        "stage: invalid\nmessage: release",
    ] {
        assert!(config(yaml).is_err(), "{yaml}");
    }

    assert_fluent_config_error(
        "message: ' '",
        "message-must-not-be-empty",
        evin_locales::t!("message-must-not-be-empty"),
    );
    assert_fluent_config_error(
        "message: release\ntag: ' '",
        "tag-must-not-be-empty-when-provided",
        evin_locales::t!("tag-must-not-be-empty-when-provided"),
    );
}

#[test]
fn invalid_template_syntax_uses_fluent_message() {
    let detail = evin_flow::template::pipeline::validate_template_syntax("{{broken}}")
        .unwrap_err()
        .to_string();
    assert_fluent_config_error(
        "message: '{{broken}}'",
        "git-release-template-syntax-invalid",
        evin_locales::t!("git-release-template-syntax-invalid", error = detail),
    );
}

// These tests would be extremely similar to Version Bumper integration tests
// so we will test the primary behavior and the recovery message.

#[test]
fn validates_push_configuration() {
    assert!(config("message: r\ntag: v1\npush:\n  prune_stale_tags: true\n  base: main").is_ok());
    assert_fluent_config_error(
        "message: r\ntag: v1\npush:\n  remote: ' '\n",
        "git-release-push-remote-must-not-be-empty",
        evin_locales::t!("git-release-push-remote-must-not-be-empty"),
    );
    for yaml in [
        "message: r\npush:\n  remote: ' '",
        "message: r\npush:\n  unknown: true",
    ] {
        assert!(config(yaml).is_err(), "{yaml}");
    }

    assert_fluent_config_error(
        "message: r\npush:\n  branch: ' '\n",
        "git-release-push-branch-must-not-be-empty",
        evin_locales::t!("git-release-push-branch-must-not-be-empty"),
    );
    assert_fluent_config_error(
        "message: r\npush:\n  prune_stale_tags: true\n  base: main",
        "git-release-prune-stale-tags-requires-tag",
        evin_locales::t!("git-release-prune-stale-tags-requires-tag"),
    );
    assert_fluent_config_error(
        "message: r\ntag: v1\npush:\n  prune_stale_tags: true",
        "git-release-prune-stale-tags-requires-base",
        evin_locales::t!("git-release-prune-stale-tags-requires-base"),
    );
    assert_fluent_config_error(
        "message: r\npush:\n  base: main",
        "git-release-push-base-requires-prune-stale-tags",
        evin_locales::t!("git-release-push-base-requires-prune-stale-tags"),
    );
}

fn assert_fluent_config_error(yaml: &str, key: &str, expected: String) {
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    let with = evin_flow::parse_yaml(yaml).expect("valid YAML");
    let parsed: GitReleaseConfig = evin_action_sdk::parse_config(Some(&with)).unwrap();
    let issue = parsed.validate().unwrap_err();
    assert_eq!(issue.key, key, "{yaml}");
    assert_eq!(evin_locales::render_issue(&issue), expected, "{yaml}");

    let error = GitReleaseConfig::from_with(Some(with)).unwrap_err();
    assert_eq!(error.message, expected, "{yaml}");
}
