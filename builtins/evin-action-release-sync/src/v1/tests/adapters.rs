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

use super::super::adapters::{CommandGitStateProvider, FsPlanSink, FsRemoteStateSource};
use evin_release_sync::{GitStateProvider, PlanSink, ProtocolError, RemoteStateSource};

#[test]
fn missing_state_keeps_path_as_structured_error_data() {
    let temp = tempfile::tempdir().unwrap();
    let source = FsRemoteStateSource {
        workspace_root: temp.path(),
        state_path: "missing-state.json",
    };

    let error = source.load_remote_state().unwrap_err();
    let ProtocolError::StateFileNotFound { path } = error else {
        panic!("expected missing state file error");
    };
    assert_eq!(path, temp.path().join("missing-state.json"));
}

#[test]
fn invalid_state_json_keeps_parser_error_as_structured_data() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("state.json"), "{").unwrap();
    let source = FsRemoteStateSource {
        workspace_root: temp.path(),
        state_path: "state.json",
    };

    let error = source.load_remote_state().unwrap_err();
    let ProtocolError::StateJsonInvalid { source } = error else {
        panic!("expected invalid state JSON error");
    };
    assert_eq!(source.classify(), serde_json::error::Category::Eof);
}

#[test]
fn state_read_error_keeps_path_and_io_source() {
    let temp = tempfile::tempdir().unwrap();
    let state_path = temp.path().join("state.json");
    std::fs::create_dir(&state_path).unwrap();
    let source = FsRemoteStateSource {
        workspace_root: temp.path(),
        state_path: "state.json",
    };

    let error = source.load_remote_state().unwrap_err();
    let ProtocolError::StateFileRead { path, source } = error else {
        panic!("expected state file read error");
    };
    assert_eq!(path, state_path);
    assert_eq!(source.kind(), std::io::ErrorKind::IsADirectory);
}

#[test]
fn notes_directory_error_keeps_path_and_io_source() {
    let temp = tempfile::tempdir().unwrap();
    let blocker = temp.path().join("notes");
    std::fs::write(&blocker, "file").unwrap();
    let config = super::super::config::ReleaseSyncConfig {
        notes_dir: "notes".into(),
        ..super::super::config::ReleaseSyncConfig::default()
    };
    let mut sink = FsPlanSink::new(temp.path(), &config);

    let error = sink.write_notes_file("release.md", "notes").unwrap_err();
    let ProtocolError::NotesDirectoryCreate { path, source } = error else {
        panic!("expected notes directory creation error");
    };
    assert_eq!(path, blocker);
    assert_eq!(source.kind(), std::io::ErrorKind::AlreadyExists);
}

#[test]
fn plan_directory_error_keeps_path_and_io_source() {
    let temp = tempfile::tempdir().unwrap();
    let blocker = temp.path().join("plans");
    std::fs::write(&blocker, "file").unwrap();
    let config = super::super::config::ReleaseSyncConfig {
        plan_out: "plans/plan.json".into(),
        ..super::super::config::ReleaseSyncConfig::default()
    };
    let mut sink = FsPlanSink::new(temp.path(), &config);
    let plan = evin_release_sync::ReleaseSyncPlan {
        schema: None,
        v: 1,
        provider: "test".into(),
        status: 200,
        reconciled: false,
        delta_count: 0,
        plan: Vec::new(),
    };

    let error = sink.write_plan(&plan).unwrap_err();
    let ProtocolError::PlanDirectoryCreate { path, source } = error else {
        panic!("expected plan directory creation error");
    };
    assert_eq!(path, blocker);
    assert_eq!(source.kind(), std::io::ErrorKind::AlreadyExists);
}

#[test]
fn note_write_error_keeps_path_and_io_source() {
    let temp = tempfile::tempdir().unwrap();
    let notes_dir = temp.path().join("notes");
    std::fs::create_dir(&notes_dir).unwrap();
    let destination = notes_dir.join("release.md");
    std::fs::create_dir(&destination).unwrap();
    let config = super::super::config::ReleaseSyncConfig {
        notes_dir: "notes".into(),
        ..super::super::config::ReleaseSyncConfig::default()
    };
    let mut sink = FsPlanSink::new(temp.path(), &config);

    let error = sink.write_notes_file("release.md", "notes").unwrap_err();
    let ProtocolError::NotesFileWrite { path, source } = error else {
        panic!("expected note file write error");
    };
    assert_eq!(path, destination);
    assert_eq!(source.kind(), std::io::ErrorKind::IsADirectory);
}

#[test]
fn plan_write_error_keeps_path_and_io_source() {
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("plan.json");
    std::fs::create_dir(&destination).unwrap();
    let config = super::super::config::ReleaseSyncConfig {
        plan_out: "plan.json".into(),
        ..super::super::config::ReleaseSyncConfig::default()
    };
    let mut sink = FsPlanSink::new(temp.path(), &config);
    let plan = evin_release_sync::ReleaseSyncPlan {
        schema: None,
        v: 1,
        provider: "test".into(),
        status: 200,
        reconciled: false,
        delta_count: 0,
        plan: Vec::new(),
    };

    let error = sink.write_plan(&plan).unwrap_err();
    let ProtocolError::PlanFileWrite { path, source } = error else {
        panic!("expected plan file write error");
    };
    assert_eq!(path, destination);
    assert_eq!(source.kind(), std::io::ErrorKind::IsADirectory);
}

#[test]
fn missing_configured_tag_keeps_tag_as_structured_data() {
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
    let ProtocolError::ConfiguredTagMissing { tag } = error else {
        panic!("expected missing configured tag error");
    };
    assert_eq!(tag, "v1.2.3");
}

#[test]
fn git_tag_command_error_keeps_io_source() {
    let temp = tempfile::tempdir().unwrap();
    let missing_worktree = temp.path().join("missing-worktree");
    let provider = CommandGitStateProvider {
        workspace_root: &missing_worktree,
        tag: None,
        title: None,
    };

    let error = provider.local_tags().unwrap_err();
    let ProtocolError::GitTagCommandFailed { source } = error else {
        panic!("expected git tag command error");
    };
    assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
}
