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

use esh::{Engine, ExecutionContext};
use std::path::Path;

#[tokio::test]
async fn test_mktemp_file_and_directory() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        mktemp
        mktemp -d
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 2);

    let file_path = Path::new(lines[0]);
    let dir_path = Path::new(lines[1]);

    assert!(file_path.exists());
    assert!(dir_path.exists());
    assert!(dir_path.is_dir());

    // Cleanup generated paths
    let _ = std::fs::remove_file(file_path);
    let _ = std::fs::remove_dir(dir_path);
}

#[tokio::test]
async fn test_mktemp_template_uses_its_parent_directory() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let directory = tempfile::tempdir_in(".").expect("create relative temporary directory");
    let template = directory.path().join("CHANGELOG.md.tmp.XXXXXX");
    let script = format!("mktemp '{}'", template.display());

    let status = engine
        .eval_str(&script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let created_path = Path::new(output.trim());
    assert!(created_path.exists());
    assert_eq!(
        created_path
            .parent()
            .expect("temporary file should have a parent")
            .canonicalize()
            .expect("canonicalize created file parent"),
        directory
            .path()
            .canonicalize()
            .expect("canonicalize temp directory")
    );
    assert!(
        created_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("CHANGELOG.md.tmp."))
    );
}
