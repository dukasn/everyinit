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
use tempfile::tempdir;

#[tokio::test]
async fn test_path_dirname_and_basename() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        path -d /foo/bar/baz.txt
        path -b /foo/bar/baz.txt
        path -e /foo/bar/baz.txt
        path -s /foo/bar/baz.txt
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "/foo/bar");
    assert_eq!(lines[1], "baz.txt");
    assert_eq!(lines[2], "txt");
    assert_eq!(lines[3], "baz");
}

#[tokio::test]
async fn test_path_join() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = "path -j src compiler mod.rs";
    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert_eq!(output.trim(), "src/compiler/mod.rs");
}

#[tokio::test]
async fn test_path_exists_and_checks() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.enable_capture();

    let script = r#"
        mkdir -p test_dir
        touch test_dir/test_file.txt

        path --exists test_dir && echo "dir exists"
        path --is-dir test_dir && echo "is a dir"
        path --is-file test_dir/test_file.txt && echo "is a file"
        path --exists nonexistent_item || echo "not exists"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("dir exists"));
    assert!(output.contains("is a dir"));
    assert!(output.contains("is a file"));
    assert!(output.contains("not exists"));
}
