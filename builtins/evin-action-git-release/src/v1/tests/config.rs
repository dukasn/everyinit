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

// These tests would be extremely similar to Version Bumper integration tests
// so we will test the primary behavior and the recovery message.

#[test]
fn validates_push_configuration() {
    assert!(config("message: r\ntag: v1\npush:\n  prune_stale_tags: true\n  base: main").is_ok());
    for yaml in [
        "message: r\npush:\n  remote: ' '",
        "message: r\ntag: v1\npush:\n  prune_stale_tags: true",
        "message: r\npush:\n  prune_stale_tags: true\n  base: main",
        "message: r\npush:\n  base: main",
        "message: r\npush:\n  unknown: true",
    ] {
        assert!(config(yaml).is_err(), "{yaml}");
    }
}
