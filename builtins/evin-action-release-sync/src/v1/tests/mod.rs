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
fn protocol_errors_keep_fluent_issue_keys_and_messages() {
    let cases = [
        (
            evin_release_sync::ProtocolError::UnsupportedSchemaVersion(2),
            "release-sync-schema-version-unsupported",
            evin_locales::t!("release-sync-schema-version-unsupported", version = "2"),
            evin_locales::t!("release-sync-schema-version-unsupported-title"),
        ),
        (
            evin_release_sync::ProtocolError::SchemaValidation("bad schema".into()),
            "release-sync-schema-validation-failed",
            evin_locales::t!(
                "release-sync-schema-validation-failed",
                error = "bad schema"
            ),
            evin_locales::t!("release-sync-schema-validation-failed-title"),
        ),
        (
            evin_release_sync::ProtocolError::StateIngestion("bad state".into()),
            "release-sync-state-ingestion-failed",
            evin_locales::t!("release-sync-state-ingestion-failed", error = "bad state"),
            evin_locales::t!("release-sync-state-ingestion-failed-title"),
        ),
        (
            evin_release_sync::ProtocolError::GitInspection("git failed".into()),
            "release-sync-git-inspection-failed",
            evin_locales::t!("release-sync-git-inspection-failed", error = "git failed"),
            evin_locales::t!("release-sync-git-inspection-failed-title"),
        ),
        (
            evin_release_sync::ProtocolError::NotesExtraction("notes failed".into()),
            "release-sync-notes-extraction-failed",
            evin_locales::t!(
                "release-sync-notes-extraction-failed",
                error = "notes failed"
            ),
            evin_locales::t!("release-sync-notes-extraction-failed-title"),
        ),
        (
            evin_release_sync::ProtocolError::ArtifactInspection("artifact failed".into()),
            "release-sync-artifact-inspection-failed",
            evin_locales::t!(
                "release-sync-artifact-inspection-failed",
                error = "artifact failed"
            ),
            evin_locales::t!("release-sync-artifact-inspection-failed-title"),
        ),
        (
            evin_release_sync::ProtocolError::PlanWrite("plan failed".into()),
            "release-sync-plan-operation-failed",
            evin_locales::t!("release-sync-plan-operation-failed", error = "plan failed"),
            evin_locales::t!("release-sync-plan-operation-failed-title"),
        ),
        (
            evin_release_sync::ProtocolError::Io(std::io::Error::other("disk failed")),
            "release-sync-io-failed",
            evin_locales::t!("release-sync-io-failed", error = "disk failed"),
            evin_locales::t!("release-sync-io-failed-title"),
        ),
    ];

    for (error, key, expected_message, expected_title) in cases {
        let diagnostic = super::protocol_error_diagnostic(error);
        assert_eq!(diagnostic.code.key, key);
        assert_eq!(
            evin_locales::render_issue(&diagnostic.code),
            expected_message
        );
        assert_eq!(diagnostic.title, expected_title);
    }
}
