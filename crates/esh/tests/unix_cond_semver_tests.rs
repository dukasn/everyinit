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
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_bracket_and_test_conditions() {
    let tmp = tempdir().unwrap();
    let file = tmp.path().join("sample.txt");
    fs::write(&file, b"content").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let _ = ctx.set_cwd(tmp.path());

    let script = r#"
        [ -f sample.txt ] && echo "file exists"
        [ -z "" ] && echo "empty string verified"
        [ "apple" = "apple" ] && echo "strings match"
        test -d . && echo "dir verified"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("file exists"));
    assert!(output.contains("empty string verified"));
    assert!(output.contains("strings match"));
    assert!(output.contains("dir verified"));
}

#[tokio::test]
async fn test_semver_cli() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        semver -v 1.2.3 && echo "valid semver"
        semver -c 1.4.0 ">=" 1.2.0 && echo "comparison passed"
        NEXT=$(semver -b patch 1.2.3)
        echo "next patch is $NEXT"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("valid semver"));
    assert!(output.contains("comparison passed"));
    assert!(output.contains("next patch is 1.2.4"));
}

#[tokio::test]
async fn test_diff_and_assert() {
    let tmp = tempdir().unwrap();
    let f1 = tmp.path().join("a.txt");
    let f2 = tmp.path().join("b.txt");
    fs::write(&f1, "line1\nline2\n").unwrap();
    fs::write(&f2, "line1\nline3\n").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let _ = ctx.set_cwd(tmp.path());

    let script = r#"
        diff -u a.txt b.txt || echo "diff detected differences"
        assert [ -f a.txt ]
        time echo "timed command"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("diff detected differences"));
    assert!(output.contains("timed command"));
}

#[tokio::test]
async fn test_assert_reports_the_documented_message() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let code = engine
        .eval_str(
            r#"assert [ -f missing ] "release binary is missing""#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(code, 1);
    let error = ctx.get_captured_stderr().unwrap();
    assert!(error.contains("release binary is missing"));
}
