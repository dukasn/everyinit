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
    assert!(parse_flag("yes").is_err());
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
