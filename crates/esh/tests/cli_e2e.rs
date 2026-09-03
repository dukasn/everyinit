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
#[cfg(not(windows))]
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

fn esh() -> Command {
    Command::new(env!("CARGO_BIN_EXE_esh"))
}

fn output_text(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn inline_mode_uses_the_requested_working_directory() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().join("workspace with spaces");
    fs::create_dir(&workspace).unwrap();

    let output = esh()
        .current_dir(&workspace)
        .args(["-c", "mkdir -p nested; printf portable > nested/result.txt"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", output_text(&output));
    assert_eq!(
        fs::read_to_string(workspace.join("nested/result.txt")).unwrap(),
        "portable"
    );
}

#[test]
fn script_mode_preserves_zero_positionals_and_shift() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().join("workspace with spaces");
    fs::create_dir(&workspace).unwrap();
    let script = workspace.join("arguments.esh");
    fs::write(
        &script,
        "printf '%s|%s|%s\\n' \"$0\" \"$1\" \"$2\"\nshift\nprintf '%s|%s\\n' \"$1\" \"$2\"\n",
    )
    .unwrap();

    let output = esh()
        .arg(&script)
        .args(["first value", "second value", "third value"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", output_text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        [
            format!("{}|first value|second value", script.to_string_lossy()),
            "second value|third value".to_string(),
        ]
    );
}

#[test]
fn dry_run_has_no_side_effects() {
    let temp = tempdir().unwrap();
    let script = temp.path().join("dry run.esh");
    let artifact = temp.path().join("should-not-exist.txt");
    fs::write(&script, "printf changed > should-not-exist.txt\n").unwrap();

    let output = esh()
        .current_dir(temp.path())
        .args(["--dry-run"])
        .arg(&script)
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", output_text(&output));
    assert!(!artifact.exists());
    assert!(!output.stdout.is_empty());
}

#[test]
fn exit_builtin_sets_the_process_status() {
    let status = esh().args(["-c", "exit 23"]).status().unwrap();
    assert_eq!(status.code(), Some(23));
}

#[test]
fn path_output_uses_script_separators() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().join("workspace with spaces");
    fs::create_dir(&workspace).unwrap();

    let output = esh()
        .current_dir(&workspace)
        .args([
            "-c",
            "mkdir -p nested/dir; touch nested/dir/file.txt; pwd; path -j nested dir file.txt; realpath nested/dir; glob 'nested/**/*.txt'; mktemp -d",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", output_text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().nth(1), Some("nested/dir/file.txt"));
    assert!(stdout.contains("nested/dir/file.txt"));
    assert_no_native_separators(&stdout);
}

#[cfg(windows)]
fn assert_no_native_separators(output: &str) {
    assert!(
        !output.contains('\\'),
        "path output was not canonical: {output}"
    );
    assert!(
        output
            .lines()
            .next()
            .is_some_and(|line| line.contains(":/"))
    );
}

#[cfg(not(windows))]
fn assert_no_native_separators(output: &str) {
    assert!(
        output
            .lines()
            .next()
            .is_some_and(|line| Path::new(line).is_absolute())
    );
}
