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
fn invalid_state_json_uses_fluent_message() {
    let temp = tempfile::tempdir().unwrap();
    let state_path = temp.path().join("state.json");
    std::fs::write(&state_path, "{").unwrap();
    let source = FsRemoteStateSource {
        workspace_root: temp.path(),
        state_path: "state.json",
    };

    let error = source.load_remote_state().unwrap_err();
    let ProtocolError::SchemaValidation(message) = error else {
        panic!("expected state schema validation error");
    };
    let detail = serde_json::from_str::<evin_release_sync::RemoteReleaseState>("{")
        .unwrap_err()
        .to_string();
    assert_eq!(
        message,
        evin_locales::t!("release-sync-state-json-invalid", error = detail)
    );
}

#[test]
fn state_read_error_uses_fluent_message() {
    let temp = tempfile::tempdir().unwrap();
    let state_path = temp.path().join("state.json");
    std::fs::create_dir(&state_path).unwrap();
    let source = FsRemoteStateSource {
        workspace_root: temp.path(),
        state_path: "state.json",
    };

    let error = source.load_remote_state().unwrap_err();
    let ProtocolError::StateIngestion(message) = error else {
        panic!("expected state ingestion error");
    };
    let detail = std::fs::read_to_string(&state_path)
        .unwrap_err()
        .to_string();
    assert_eq!(
        message,
        evin_locales::t!("release-sync-state-file-read-failed", error = detail)
    );
}

#[test]
fn notes_directory_error_uses_fluent_message() {
    let temp = tempfile::tempdir().unwrap();
    let blocker = temp.path().join("notes");
    std::fs::write(&blocker, "file").unwrap();
    let config = super::super::config::ReleaseSyncConfig {
        notes_dir: "notes".into(),
        ..super::super::config::ReleaseSyncConfig::default()
    };
    let mut sink = FsPlanSink::new(temp.path(), &config);

    let error = sink.write_notes_file("release.md", "notes").unwrap_err();
    let ProtocolError::PlanWrite(message) = error else {
        panic!("expected plan write error");
    };
    let detail = std::fs::create_dir_all(&blocker).unwrap_err().to_string();
    assert_eq!(
        message,
        evin_locales::t!(
            "release-sync-notes-directory-create-failed",
            path = blocker.display().to_string(),
            error = detail
        )
    );
}

#[test]
fn plan_directory_error_uses_fluent_message() {
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
    let ProtocolError::PlanWrite(message) = error else {
        panic!("expected plan write error");
    };
    let detail = std::fs::create_dir_all(&blocker).unwrap_err().to_string();
    assert_eq!(
        message,
        evin_locales::t!(
            "release-sync-plan-directory-create-failed",
            path = blocker.display().to_string(),
            error = detail
        )
    );
}

#[test]
fn note_write_error_uses_fluent_message() {
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
    let ProtocolError::PlanWrite(message) = error else {
        panic!("expected plan write error");
    };
    let detail = std::fs::write(&destination, "notes")
        .unwrap_err()
        .to_string();
    assert_eq!(
        message,
        evin_locales::t!(
            "release-sync-note-write-failed",
            path = destination.display().to_string(),
            error = detail
        )
    );
}

#[test]
fn plan_write_error_uses_fluent_message() {
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
    let ProtocolError::PlanWrite(message) = error else {
        panic!("expected plan write error");
    };
    let detail = std::fs::write(&destination, "{}").unwrap_err().to_string();
    assert_eq!(
        message,
        evin_locales::t!(
            "release-sync-plan-write-failed",
            path = destination.display().to_string(),
            error = detail
        )
    );
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

#[test]
fn git_tag_command_error_uses_fluent_message() {
    let temp = tempfile::tempdir().unwrap();
    let missing_worktree = temp.path().join("missing-worktree");
    let provider = CommandGitStateProvider {
        workspace_root: &missing_worktree,
        tag: None,
        title: None,
    };

    let error = provider.local_tags().unwrap_err();
    let ProtocolError::GitInspection(message) = error else {
        panic!("expected git inspection error");
    };
    let detail = std::process::Command::new("git")
        .args([
            "tag",
            "-l",
            "--format=%(refname:short)|%(*objectname)|%(objectname)|%(contents:subject)",
        ])
        .current_dir(&missing_worktree)
        .output()
        .unwrap_err()
        .to_string();
    assert_eq!(
        message,
        evin_locales::t!("release-sync-git-tag-command-failed", error = detail)
    );
}
