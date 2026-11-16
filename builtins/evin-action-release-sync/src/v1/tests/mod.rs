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
    let json_error = serde_json::from_str::<serde_json::Value>("{")
        .unwrap_err()
        .to_string();
    let cases = [
        (
            evin_release_sync::ProtocolError::UnsupportedSchemaVersion(2),
            "release-sync-schema-version-unsupported",
            evin_locales::t!("release-sync-schema-version-unsupported", version = "2"),
            Some(evin_locales::t!(
                "release-sync-schema-version-unsupported-title"
            )),
        ),
        (
            evin_release_sync::ProtocolError::StateFileNotFound {
                path: "state.json".into(),
            },
            "release-sync-state-file-not-found",
            evin_locales::t!("release-sync-state-file-not-found", path = "state.json"),
            None,
        ),
        (
            evin_release_sync::ProtocolError::StateFileRead {
                path: "state.json".into(),
                source: std::io::Error::other("permission denied"),
            },
            "release-sync-state-file-read-failed",
            evin_locales::t!(
                "release-sync-state-file-read-failed",
                path = "state.json",
                error = "permission denied"
            ),
            None,
        ),
        (
            evin_release_sync::ProtocolError::StateJsonInvalid {
                source: serde_json::from_str::<serde_json::Value>("{").unwrap_err(),
            },
            "release-sync-state-json-invalid",
            evin_locales::t!("release-sync-state-json-invalid", error = &json_error),
            None,
        ),
        (
            evin_release_sync::ProtocolError::GitTagCommandFailed {
                source: std::io::Error::other("git is missing"),
            },
            "release-sync-git-tag-command-failed",
            evin_locales::t!(
                "release-sync-git-tag-command-failed",
                error = "git is missing"
            ),
            None,
        ),
        (
            evin_release_sync::ProtocolError::GitTagFailed {
                stderr: "not a repository".into(),
            },
            "release-sync-git-tag-failed",
            evin_locales::t!("release-sync-git-tag-failed", error = "not a repository"),
            None,
        ),
        (
            evin_release_sync::ProtocolError::ConfiguredTagMissing {
                tag: "v1.2.3".into(),
            },
            "release-sync-configured-tag-missing",
            evin_locales::t!("release-sync-configured-tag-missing", tag = "v1.2.3"),
            None,
        ),
        (
            evin_release_sync::ProtocolError::ArtifactsDirectoryRead {
                path: "dist/releases".into(),
                source: std::io::Error::other("permission denied"),
            },
            "release-sync-artifacts-directory-read-failed",
            evin_locales::t!(
                "release-sync-artifacts-directory-read-failed",
                path = "dist/releases",
                error = "permission denied"
            ),
            None,
        ),
        (
            evin_release_sync::ProtocolError::ArtifactMetadataRead {
                path: "dist/releases/app.tar.gz".into(),
                source: std::io::Error::other("metadata unavailable"),
            },
            "release-sync-artifact-metadata-read-failed",
            evin_locales::t!(
                "release-sync-artifact-metadata-read-failed",
                path = "dist/releases/app.tar.gz",
                error = "metadata unavailable"
            ),
            None,
        ),
        (
            evin_release_sync::ProtocolError::ArtifactOpen {
                path: "dist/releases/app.tar.gz".into(),
                source: std::io::Error::other("file unavailable"),
            },
            "release-sync-artifact-open-failed",
            evin_locales::t!(
                "release-sync-artifact-open-failed",
                path = "dist/releases/app.tar.gz",
                error = "file unavailable"
            ),
            None,
        ),
        (
            evin_release_sync::ProtocolError::ArtifactHash {
                path: "dist/releases/app.tar.gz".into(),
                source: std::io::Error::other("read failed"),
            },
            "release-sync-artifact-hash-failed",
            evin_locales::t!(
                "release-sync-artifact-hash-failed",
                path = "dist/releases/app.tar.gz",
                error = "read failed"
            ),
            None,
        ),
        (
            evin_release_sync::ProtocolError::NotesDirectoryCreate {
                path: "dist/notes".into(),
                source: std::io::Error::other("permission denied"),
            },
            "release-sync-notes-directory-create-failed",
            evin_locales::t!(
                "release-sync-notes-directory-create-failed",
                path = "dist/notes",
                error = "permission denied"
            ),
            None,
        ),
        (
            evin_release_sync::ProtocolError::NotesFileWrite {
                path: "dist/notes/v1.md".into(),
                source: std::io::Error::other("disk full"),
            },
            "release-sync-note-write-failed",
            evin_locales::t!(
                "release-sync-note-write-failed",
                path = "dist/notes/v1.md",
                error = "disk full"
            ),
            None,
        ),
        (
            evin_release_sync::ProtocolError::PlanDirectoryCreate {
                path: "dist/plans".into(),
                source: std::io::Error::other("permission denied"),
            },
            "release-sync-plan-directory-create-failed",
            evin_locales::t!(
                "release-sync-plan-directory-create-failed",
                path = "dist/plans",
                error = "permission denied"
            ),
            None,
        ),
        (
            evin_release_sync::ProtocolError::PlanSerialize {
                source: serde_json::from_str::<serde_json::Value>("{").unwrap_err(),
            },
            "release-sync-plan-serialization-failed",
            evin_locales::t!(
                "release-sync-plan-serialization-failed",
                error = &json_error
            ),
            None,
        ),
        (
            evin_release_sync::ProtocolError::PlanFileWrite {
                path: "dist/plan.json".into(),
                source: std::io::Error::other("disk full"),
            },
            "release-sync-plan-write-failed",
            evin_locales::t!(
                "release-sync-plan-write-failed",
                path = "dist/plan.json",
                error = "disk full"
            ),
            None,
        ),
        (
            evin_release_sync::ProtocolError::Io(std::io::Error::other("disk failed")),
            "release-sync-io-failed",
            evin_locales::t!("release-sync-io-failed", error = "disk failed"),
            Some(evin_locales::t!("release-sync-io-failed-title")),
        ),
    ];

    for (error, key, expected_message, expected_title) in cases {
        let diagnostic = super::protocol_error_diagnostic(error);
        assert_eq!(diagnostic.code.key, key);
        assert_eq!(
            evin_locales::render_issue(&diagnostic.code),
            expected_message
        );
        if let Some(expected_title) = expected_title {
            assert_eq!(diagnostic.title, expected_title);
        } else {
            assert_eq!(diagnostic.title, expected_message);
        }
    }
}
