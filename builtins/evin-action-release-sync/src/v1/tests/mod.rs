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

mod adapters;
mod config;

#[test]
fn unsupported_remote_state_version_uses_fluent_diagnostic() {
    let diagnostic = super::protocol_error_diagnostic(
        evin_release_sync::ProtocolError::UnsupportedSchemaVersion(2),
    );
    assert_eq!(
        diagnostic.code.key,
        "release-sync-schema-version-unsupported"
    );
    assert_eq!(
        evin_locales::render_issue(&diagnostic.code),
        evin_locales::t!("release-sync-schema-version-unsupported", version = "2")
    );
    assert_eq!(
        diagnostic.title,
        evin_locales::t!("release-sync-schema-version-unsupported-title")
    );
}
