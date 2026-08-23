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
async fn test_redirection_stdout_and_append() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();

    let script = r#"
        echo "Line 1" > output.txt
        echo "Line 2" >> output.txt
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let content = std::fs::read_to_string(tmp.path().join("output.txt")).unwrap();
    assert_eq!(content, "Line 1\nLine 2\n");
}

#[tokio::test]
async fn test_redirection_stdin() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.enable_capture();

    std::fs::write(tmp.path().join("source.txt"), "hello from file\n").unwrap();

    let script = "read -r LINE < source.txt; echo \"$LINE\"";
    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("hello from file"));
}

#[tokio::test]
async fn test_redirection_dev_null_virtualization() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        echo "This should be discarded" > /dev/null
        echo "Visible line"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert_eq!(output, "Visible line\n");
}

#[tokio::test]
async fn test_redirection_stderr() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();

    let script = "cd /nonexistent_dir_path_87654 2> err.log || echo fallback";
    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let err_file = tmp.path().join("err.log");
    assert!(err_file.exists());
    let content = std::fs::read_to_string(err_file).unwrap();
    assert!(content.contains("Directory does not exist"));
}
