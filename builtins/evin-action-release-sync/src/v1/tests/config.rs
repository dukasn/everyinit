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

use super::super::{config::ReleaseSyncConfig, validate_config};

#[test]
fn validate_default_config() {
    assert!(validate_config(None).is_ok());
}

#[test]
fn configuration_errors_use_fluent_messages() {
    let mut config = ReleaseSyncConfig {
        tag: Some("  ".to_string()),
        ..ReleaseSyncConfig::default()
    };
    assert_fluent_message(
        config.validate().unwrap_err().message,
        evin_locales::t!("release-sync-tag-must-not-be-empty"),
    );

    config.tag = None;
    config.title = Some("  ".to_string());
    assert_fluent_message(
        config.validate().unwrap_err().message,
        evin_locales::t!("release-sync-title-must-not-be-empty"),
    );

    config.title = None;
    config.state.clear();
    assert_fluent_message(
        config.validate().unwrap_err().message,
        evin_locales::t!("release-sync-state-path-must-not-be-empty"),
    );

    config.state = ReleaseSyncConfig::default().state;
    config.plan_out.clear();
    assert_fluent_message(
        config.validate().unwrap_err().message,
        evin_locales::t!("release-sync-plan-out-path-must-not-be-empty"),
    );

    config.plan_out = ReleaseSyncConfig::default().plan_out;
    config.notes_dir.clear();
    assert_fluent_message(
        config.validate().unwrap_err().message,
        evin_locales::t!("release-sync-notes-dir-must-not-be-empty"),
    );
}

fn assert_fluent_message(actual: String, expected: String) {
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    assert_eq!(actual, expected);
}
