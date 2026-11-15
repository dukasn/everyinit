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
fn removed_cargo_option_is_rejected() {
    assert!(VersionBumperConfig::from_with(None).is_ok());
    for value in ["true", "false"] {
        let with = evin_flow::parse_yaml(&format!("cargo_pre_1_0: {value}\n")).unwrap();
        assert!(VersionBumperConfig::from_with(Some(with)).is_err());
    }
}

#[test]
fn initial_version_error_uses_fluent_message() {
    let mut config = VersionBumperConfig::default();
    config.initial_version = "invalid".to_string();
    let error = config.validate().unwrap_err();
    let detail = semver::Version::parse("invalid").unwrap_err().to_string();
    let expected = evin_locales::t!("initial-version-invalid", error = detail);
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    assert_eq!(evin_locales::render_issue(&error.code), expected);
}
