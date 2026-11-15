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
