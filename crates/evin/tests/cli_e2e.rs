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

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

fn evin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_evin"))
}

fn output_text(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn write_workspace(root: &Path, tasks: &str) {
    fs::create_dir_all(root.join(".evin")).unwrap();
    fs::write(
        root.join(".evin/workspace.yaml"),
        format!(
            "workspace:\n  name: cross-platform-e2e\n  version: \"0.1.0\"\nstages: [Test]\ntasks:\n{tasks}"
        ),
    )
    .unwrap();
}

#[test]
fn init_supports_a_destination_with_spaces() {
    let temp = tempdir().unwrap();
    let destination = temp.path().join("project with spaces");
    fs::create_dir(&destination).unwrap();

    let output = evin()
        .current_dir(temp.path())
        .arg("init")
        .arg("--workspace")
        .arg(&destination)
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", output_text(&output));
    assert!(destination.join(".evin/workspace.yaml").is_file());
}

#[test]
fn discovery_commands_emit_valid_file_uris() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().join("workspace with spaces");
    write_workspace(
        &workspace,
        "  inspect:\n    stage: Test\n    cwd: nested directory\n    run: pwd\n",
    );
    fs::create_dir(workspace.join("nested directory")).unwrap();

    let manifest = workspace
        .join(".evin/workspace.yaml")
        .canonicalize()
        .unwrap();
    let cwd = workspace.join("nested directory").canonicalize().unwrap();
    let manifest_uri = url::Url::from_file_path(manifest).unwrap().to_string();
    let cwd_uri = url::Url::from_file_path(cwd).unwrap().to_string();

    for args in [&["list"][..], &["explain", "inspect"], &["check"]] {
        let output = evin().current_dir(&workspace).args(args).output().unwrap();
        assert!(output.status.success(), "{}", output_text(&output));
        let rendered = output_text(&output);
        assert!(rendered.contains("file:///"), "{rendered}");
        assert!(!rendered.contains("file://file://"), "{rendered}");
        assert_uri_has_no_backslashes(&rendered);
        if args[0] == "explain" {
            assert!(rendered.contains(&cwd_uri), "{rendered}");
        } else {
            assert!(rendered.contains(&manifest_uri), "{rendered}");
        }
    }
}

#[test]
fn run_honors_cwd_and_inherited_environment() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().join("workspace with spaces");
    write_workspace(
        &workspace,
        "  create-artifact:\n    stage: Test\n    cwd: nested directory\n    env: [E2E_VALUE]\n    run: printf \"$E2E_VALUE\" > result.txt\n",
    );
    let nested = workspace.join("nested directory");
    fs::create_dir(&nested).unwrap();

    let output = evin()
        .current_dir(&workspace)
        .env("E2E_VALUE", "portable-value")
        .args(["run", "create-artifact"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", output_text(&output));
    assert_eq!(
        fs::read_to_string(nested.join("result.txt")).unwrap(),
        "portable-value"
    );
}

#[test]
fn focused_run_ignores_an_unrelated_invalid_action() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().join("workspace with spaces");
    write_workspace(
        &workspace,
        "  selected:\n    stage: Test\n    run: printf ok > selected.txt\n  broken:\n    stage: Test\n    steps:\n      - uses: evin/docs@v1\n        with:\n          site_url: https://example.com\n          tile: Docs\n",
    );

    let run = evin()
        .current_dir(&workspace)
        .args(["run", "selected"])
        .output()
        .unwrap();
    assert!(run.status.success(), "{}", output_text(&run));
    assert_eq!(
        fs::read_to_string(workspace.join("selected.txt")).unwrap(),
        "ok"
    );

    let check = evin()
        .current_dir(&workspace)
        .arg("check")
        .output()
        .unwrap();
    assert!(!check.status.success(), "{}", output_text(&check));
    assert!(output_text(&check).contains("tile"));
}

#[test]
fn failed_target_returns_the_evin_failure_status() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().join("workspace with spaces");
    write_workspace(&workspace, "  fail:\n    stage: Test\n    run: exit 23\n");

    let output = evin()
        .current_dir(&workspace)
        .args(["run", "fail"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "{}", output_text(&output));
    assert!(output_text(&output).contains("fail"));
}

#[cfg(windows)]
fn assert_uri_has_no_backslashes(output: &str) {
    for token in output
        .split_whitespace()
        .filter(|token| token.contains("file://"))
    {
        assert!(!token.contains('\\'), "invalid file URI: {token}");
    }
}

#[cfg(not(windows))]
fn assert_uri_has_no_backslashes(_output: &str) {}
