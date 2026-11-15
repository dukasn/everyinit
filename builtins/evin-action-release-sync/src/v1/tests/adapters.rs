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

use super::super::adapters::{CommandGitStateProvider, FsRemoteStateSource};
use evin_release_sync::{GitStateProvider, ProtocolError, RemoteStateSource};

#[test]
fn missing_state_diagnostic_uses_fluent_message() {
    let temp = tempfile::tempdir().unwrap();
    let source = FsRemoteStateSource {
        workspace_root: temp.path(),
        state_path: "missing-state.json",
    };
    let error = source.load_remote_state().unwrap_err();
    let ProtocolError::StateIngestion(message) = error else {
        panic!("expected state ingestion error");
    };
    let expected = evin_locales::t!(
        "release-sync-state-file-not-found",
        path = temp.path().join("missing-state.json").display().to_string()
    );
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    assert_eq!(message, expected);
}

#[test]
fn missing_configured_tag_uses_fluent_message() {
    let temp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let provider = CommandGitStateProvider {
        workspace_root: temp.path(),
        tag: Some("v1.2.3"),
        title: None,
    };
    let error = provider.local_tags().unwrap_err();
    let ProtocolError::GitInspection(message) = error else {
        panic!("expected Git inspection error");
    };
    let expected = evin_locales::t!("release-sync-configured-tag-missing", tag = "v1.2.3");
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    assert_eq!(message, expected);
}
