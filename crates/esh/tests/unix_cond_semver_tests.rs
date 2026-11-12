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
async fn test_supports_boolean_precedence_parentheses_and_permission_predicates() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("readable.txt"), b"data").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            tmp.path().join("readable.txt"),
            fs::Permissions::from_mode(0o400),
        )
        .unwrap();
    }

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();
    let script = r#"
        test ! && echo "lone bang is a non-empty string"
        test "" -o alpha -a beta && echo "and binds tighter than or"
        test '(' alpha -a beta ')' && echo "parentheses work"
        test 10 -gt 2 && echo "integer comparison works"
        test alpha '<' beta && echo "string ordering works"
        test 'β' '>' 'α' && echo "unicode ordering works"
        [ -a readable.txt -a -r readable.txt -a ! -w readable.txt ] && echo "file tests work"
        test -x . && echo "directory is searchable"
    "#;
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "lone bang is a non-empty string\nand binds tighter than or\nparentheses work\ninteger comparison works\nstring ordering works\nunicode ordering works\nfile tests work\ndirectory is searchable\n"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn test_supports_special_file_and_set_id_predicates() {
    use std::ffi::CString;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    let tmp = tempdir().unwrap();
    let fifo = tmp.path().join("pipe");
    let fifo_c = CString::new(fifo.to_str().unwrap()).unwrap();
    // SAFETY: `fifo_c` is a valid NUL-terminated path and mkfifo does not
    // retain the pointer after returning.
    assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);

    let setgid = tmp.path().join("setgid");
    let setuid = tmp.path().join("setuid");
    let socket = tmp.path().join("socket");
    let _listener = UnixListener::bind(&socket).unwrap();
    fs::write(&setgid, b"").unwrap();
    fs::write(&setuid, b"").unwrap();
    fs::set_permissions(&setgid, fs::Permissions::from_mode(0o2644)).unwrap();
    fs::set_permissions(&setuid, fs::Permissions::from_mode(0o4644)).unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();
    let status = engine
        .eval_str(
            "test -p pipe && echo fifo; test -c /dev/null && echo char; test -S socket && echo socket; test ! -S pipe && echo not-socket; test -g setgid && echo setgid; test -u setuid && echo setuid; test ! -b pipe && echo not-block\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "fifo\nchar\nsocket\nnot-socket\nsetgid\nsetuid\nnot-block\n"
    );
}

#[tokio::test]
async fn test_file_time_predicates_handle_missing_paths_explicitly() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("present"), b"data").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();
    let status = engine
        .eval_str(
            r#"
            if test missing -nt present; then printf T; else printf F; fi
            if test present -nt missing; then printf T; else printf F; fi
            if test missing -ot present; then printf T; else printf F; fi
            if test present -ot missing; then printf T; else printf F; fi
            if test missing -nt missing; then printf T; else printf F; fi
            if test missing -ot missing; then printf T; else printf F; fi
            printf '\n'
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "FTTFFF\n");
}

#[tokio::test]
async fn test_reports_malformed_and_non_numeric_expressions() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("test 1 -gt nope\n", &mut ctx)
        .await
        .unwrap();
    assert_eq!(status, 2);
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: test: expected an integer, got 'nope'\n"
    );

    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let status = engine
        .eval_str("test left -unknown right\n", &mut ctx)
        .await
        .unwrap();
    assert_eq!(status, 2);
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("unknown binary operator '-unknown'")
    );
}

#[tokio::test]
async fn test_explicitly_rejects_unsupported_terminal_predicate() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "test -t\nSINGLE=$?\ntest -t 0\nTEST_STATUS=$?\n[ -t 1 ]\nBRACKET_STATUS=$?\nprintf '%s:%s:%s\\n' \"$SINGLE\" \"$TEST_STATUS\" \"$BRACKET_STATUS\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "0:2:2\n");
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: test: terminal predicate '-t' is unsupported\nesh: test: terminal predicate '-t' is unsupported\n"
    );
}

#[tokio::test]
async fn bracket_requires_closing_token_without_changing_test_arguments() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str("test ]", &mut ctx).await.unwrap(), 0);
    assert_eq!(engine.eval_str("[ -n value ]", &mut ctx).await.unwrap(), 0);
    assert_eq!(engine.eval_str("[ -n value", &mut ctx).await.unwrap(), 2);
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("missing closing ']'")
    );
}

#[tokio::test]
async fn test_single_parenthesis_is_a_nonempty_operand() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();

    assert_eq!(engine.eval_str("test '('", &mut ctx).await.unwrap(), 0);
    assert_eq!(engine.eval_str("[ '(' ]", &mut ctx).await.unwrap(), 0);
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

    let mut legacy_ctx = ExecutionContext::new();
    assert_eq!(
        engine
            .eval_str("semver bump patch 1.2.3", &mut legacy_ctx)
            .await
            .unwrap(),
        1
    );
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
