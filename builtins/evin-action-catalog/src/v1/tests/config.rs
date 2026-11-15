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

use super::super::config::CatalogConfig;

#[test]
fn configuration_is_closed_and_has_no_site_url_requirement() {
    let valid = evin_flow::parse_yaml("source_dir: docs\nout: dist/catalog.json\n").unwrap();
    evin_action_sdk::parse_config::<CatalogConfig>(Some(&valid)).unwrap();

    let invalid =
        evin_flow::parse_yaml("source_dir: docs\nsite_url: https://example.com\n").unwrap();
    assert!(evin_action_sdk::parse_config::<CatalogConfig>(Some(&invalid)).is_err());
}
