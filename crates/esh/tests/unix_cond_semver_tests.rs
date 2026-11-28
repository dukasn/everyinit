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
use std::process::Command;
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

#[tokio::test]
async fn test_and_bracket_apply_posix_boolean_precedence() {
    let script = r#"
        test alpha -o '' -a ''
        TEST_AND_OR=$?
        test ! '' -o alpha -a beta
        TEST_NOT_OR=$?
        test ! alpha -o alpha
        TEST_FOUR_OPERANDS=$?
        [ alpha -o '' -a '' ]
        BRACKET_AND_OR=$?
        [ ! '' -o alpha -a beta ]
        BRACKET_NOT_OR=$?
        [ ! alpha -o alpha ]
        BRACKET_FOUR_OPERANDS=$?
        printf '%s:%s:%s:%s:%s:%s\n' "$TEST_AND_OR" "$TEST_NOT_OR" "$TEST_FOUR_OPERANDS" "$BRACKET_AND_OR" "$BRACKET_NOT_OR" "$BRACKET_FOUR_OPERANDS"
    "#;
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    let expected = b"0:0:1:0:0:1\n";
    assert_eq!(ctx.get_captured_stdout().unwrap().as_bytes(), expected);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    #[cfg(unix)]
    for (shell, args) in [("bash", vec!["--posix", "-c"]), ("dash", vec!["-c"])] {
        if let Ok(reference) = Command::new(shell).args(args).arg(script).output() {
            assert!(
                reference.status.success(),
                "{shell} failed to evaluate test expression"
            );
            assert_eq!(reference.stdout, expected, "{shell} test precedence");
            assert_eq!(reference.stderr, b"", "{shell} test precedence diagnostic");
        }
    }
}

#[tokio::test]
async fn test_and_bracket_match_posix_operand_count_forms() {
    let script = r#"
        test
        EMPTY_TEST=$?
        [ ]
        EMPTY_BRACKET=$?
        test !
        SINGLE_NOT=$?
        test ! alpha
        UNARY_NOT_TRUE=$?
        test ! ''
        UNARY_NOT_EMPTY=$?
        test ! alpha = alpha
        NEGATED_BINARY_EQUAL=$?
        [ ! alpha = beta ]
        NEGATED_BINARY_UNEQUAL=$?
        printf '%s:%s:%s:%s:%s:%s:%s\n' "$EMPTY_TEST" "$EMPTY_BRACKET" "$SINGLE_NOT" "$UNARY_NOT_TRUE" "$UNARY_NOT_EMPTY" "$NEGATED_BINARY_EQUAL" "$NEGATED_BINARY_UNEQUAL"
    "#;
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    let expected = b"1:1:0:1:0:1:0\n";
    assert_eq!(ctx.get_captured_stdout().unwrap().as_bytes(), expected);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    #[cfg(unix)]
    for (shell, args) in [("bash", vec!["--posix", "-c"]), ("dash", vec!["-c"])] {
        if let Ok(reference) = Command::new(shell).args(args).arg(script).output() {
            assert!(
                reference.status.success(),
                "{shell} failed to evaluate test operand-count forms"
            );
            assert_eq!(reference.stdout, expected, "{shell} operand-count forms");
            assert_eq!(
                reference.stderr, b"",
                "{shell} operand-count forms diagnostic"
            );
        }
    }
}

#[tokio::test]
async fn test_and_bracket_accept_double_equals_as_string_equality_extension() {
    let script = r#"
        if test alpha == alpha; then TEST_EQUAL=true; else TEST_EQUAL=false; fi
        if [ alpha == beta ]; then BRACKET_EQUAL=true; else BRACKET_EQUAL=false; fi
        printf '%s:%s\n' "$TEST_EQUAL" "$BRACKET_EQUAL"
    "#;
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    let stdout = ctx.get_captured_stdout().unwrap();
    assert_eq!(stdout, "true:false\n");

    if let Ok(reference) = Command::new("bash")
        .args(["--posix", "-c", script])
        .output()
    {
        assert!(reference.status.success());
        assert_eq!(reference.stdout, stdout.as_bytes());
        assert_eq!(reference.stderr, b"");
    }
}

#[tokio::test]
async fn test_t_is_an_operand_when_three_argument_form_selects_a_binary_expression() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "test -t = -t; EQUAL=$?\ntest -t = x; UNEQUAL=$?\ntest -t -a x; AND=$?\ntest -t -o ''; OR=$?\nprintf '%s:%s:%s:%s\\n' \"$EQUAL\" \"$UNEQUAL\" \"$AND\" \"$OR\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "0:1:0:0\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
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
    let script = r#"
            if test missing -nt present; then printf T; else printf F; fi
            if test present -nt missing; then printf T; else printf F; fi
            if test missing -ot present; then printf T; else printf F; fi
            if test present -ot missing; then printf T; else printf F; fi
            if test missing -nt missing; then printf T; else printf F; fi
            if test missing -ot missing; then printf T; else printf F; fi
            printf '\n'
            "#;
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "FTTFFF\n");

    #[cfg(unix)]
    for (shell, args) in [("bash", vec!["--posix", "-c"]), ("dash", vec!["-c"])] {
        if cfg!(target_os = "macos") && shell == "dash" {
            // Darwin's bundled dash reports false for both missing-path time
            // comparisons, while ESH implements the documented missing-path
            // behavior asserted above.
            continue;
        }
        if let Ok(reference) = Command::new(shell)
            .current_dir(tmp.path())
            .args(args)
            .arg(script)
            .output()
        {
            assert!(reference.status.success(), "{shell} file-time predicates");
            assert_eq!(reference.stdout, b"FTTFFF\n", "{shell} file-time results");
        }
    }
}

#[tokio::test]
async fn file_predicates_treat_empty_path_operands_as_missing() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("present"), b"content").unwrap();

    let script = r#"
        for OP in -e -a -f -d -x -r -w -g -u -b -c -p -S -s -L -h; do
            test "$OP" ""
            printf '%s' "$?"
        done
        test "" -nt present; printf ':%s' "$?"
        test present -nt ""; printf ':%s' "$?"
        test "" -ot present; printf ':%s' "$?"
        test present -ot ""; printf ':%s' "$?"
        test "" -ef ""; printf ':%s\n' "$?"
    "#;
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    let expected = b"1111111111111111:1:0:0:1:1\n";
    assert_eq!(ctx.get_captured_stdout().unwrap().as_bytes(), expected);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    #[cfg(unix)]
    // Dash parses `-a` as an expression operator in this two-argument form,
    // unlike Bash POSIX mode and ESH's file-predicate handling.
    for (shell, args) in [("bash", vec!["--posix", "-c"])] {
        if let Ok(reference) = Command::new(shell)
            .current_dir(tmp.path())
            .args(args)
            .arg(script)
            .output()
        {
            assert!(reference.status.success(), "{shell} empty path predicates");
            assert_eq!(reference.stdout, expected, "{shell} empty path results");
            assert_eq!(reference.stderr, b"", "{shell} empty path diagnostics");
        }
    }
}

#[tokio::test]
async fn test_file_time_predicates_compare_existing_timestamps_and_ties() {
    use std::time::{Duration, SystemTime};

    let tmp = tempdir().unwrap();
    for name in ["older", "newer", "same"] {
        fs::write(tmp.path().join(name), b"content").unwrap();
    }
    for (name, seconds) in [("older", 1_600_000_000), ("newer", 1_700_000_000)] {
        fs::File::options()
            .write(true)
            .open(tmp.path().join(name))
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)),
            )
            .unwrap();
    }

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    let script = "test older -nt newer; OLD_NEWER=$?\ntest newer -nt older; NEW_NEWER=$?\ntest older -ot newer; OLD_OLDER=$?\ntest newer -ot older; NEW_OLDER=$?\ntest same -nt same; SAME_NEWER=$?\ntest same -ot same; SAME_OLDER=$?\nprintf '%s:%s:%s:%s:%s:%s\\n' \"$OLD_NEWER\" \"$NEW_NEWER\" \"$OLD_OLDER\" \"$NEW_OLDER\" \"$SAME_NEWER\" \"$SAME_OLDER\"\n";
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "1:0:0:1:1:1\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    #[cfg(unix)]
    for (shell, args) in [("bash", vec!["--posix", "-c"]), ("dash", vec!["-c"])] {
        if let Ok(reference) = Command::new(shell)
            .current_dir(tmp.path())
            .args(args)
            .arg(script)
            .output()
        {
            assert!(reference.status.success(), "{shell} file-time predicates");
            assert_eq!(
                reference.stdout, b"1:0:0:1:1:1\n",
                "{shell} file-time results"
            );
        }
    }
}

#[tokio::test]
async fn test_file_identity_predicate_matches_path_aliases_and_hard_links_only() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("original"), b"same contents").unwrap();
    fs::hard_link(tmp.path().join("original"), tmp.path().join("hard-link")).unwrap();
    fs::write(tmp.path().join("copy"), b"same contents").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    let script = "test original -ef ./original && printf alias; test original -ef hard-link && printf hard-link; test original -ef copy || printf separate\nprintf '\\n'\n";
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "aliashard-linkseparate\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    #[cfg(unix)]
    for (shell, args) in [("bash", vec!["--posix", "-c"]), ("dash", vec!["-c"])] {
        if let Ok(reference) = Command::new(shell)
            .current_dir(tmp.path())
            .args(args)
            .arg(script)
            .output()
        {
            assert!(
                reference.status.success(),
                "{shell} file-identity predicate"
            );
            assert_eq!(reference.stdout, b"aliashard-linkseparate\n");
        }
    }
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
async fn test_parenthesis_operand_respects_three_argument_binary_precedence() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        test '('
        SINGLE=$?
        test '(' '!=' '!='
        BINARY_NOT_EQUAL=$?
        test '(' '=' '('
        BINARY_EQUAL=$?
        test '(' -a x
        BINARY_AND=$?
        test '(' -n ')'
        PARENTHESIZED_UNARY_LOOKING_WORD=$?
        test '(' '' ')'
        PARENTHESIZED_EMPTY_WORD=$?
        test ! '(' -n ')'
        NEGATED_PARENTHESIZED_WORD=$?
        [ '(' != x ]
        BRACKET_NOT_EQUAL=$?
        printf '%s:%s:%s:%s:%s:%s:%s:%s\n' "$SINGLE" "$BINARY_NOT_EQUAL" "$BINARY_EQUAL" "$BINARY_AND" "$PARENTHESIZED_UNARY_LOOKING_WORD" "$PARENTHESIZED_EMPTY_WORD" "$NEGATED_PARENTHESIZED_WORD" "$BRACKET_NOT_EQUAL"
    "#;
    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    let expected = b"0:0:0:0:0:1:1:0\n";
    assert_eq!(ctx.get_captured_stdout().unwrap().as_bytes(), expected);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    #[cfg(unix)]
    // Dash gives parenthesis-looking operands special meaning in these
    // ambiguous three-argument forms; Bash POSIX mode matches the adopted
    // operand precedence exercised here.
    for (shell, args) in [("bash", vec!["--posix", "-c"])] {
        if let Ok(reference) = Command::new(shell).args(args).arg(script).output() {
            assert!(reference.status.success(), "{shell} parenthesis operands");
            assert_eq!(reference.stdout, expected, "{shell} parenthesis results");
            assert_eq!(reference.stderr, b"", "{shell} parenthesis diagnostics");
        }
    }
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
