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
