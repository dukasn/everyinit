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

use esh::runtime::traits::ScopeManager;
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
async fn standard_descriptor_aliases_route_the_expected_streams() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.enable_capture();
    std::fs::write(tmp.path().join("input.txt"), "read through fd zero\n").unwrap();

    let status = engine
        .eval_str(
            "cat 0<input.txt 1>copied.txt; echo first 1>stdout.txt; echo second 1>>stdout.txt\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("copied.txt")).unwrap(),
        "read through fd zero\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("stdout.txt")).unwrap(),
        "first\nsecond\n"
    );
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

#[tokio::test]
async fn stderr_append_redirection_preserves_existing_content_and_appends_each_command() {
    let tmp = tempdir().unwrap();
    std::fs::write(tmp.path().join("errors.log"), "existing\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "sh -c 'printf first >&2' 2>> errors.log\nsh -c 'printf second >&2' 2>> errors.log\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
    assert_eq!(
        std::fs::read(tmp.path().join("errors.log")).unwrap(),
        b"existing\nfirstsecond"
    );
}

#[tokio::test]
async fn descriptor_duplication_obeys_left_to_right_redirection_order() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.enable_capture();

    let script = r#"
        sh -c 'printf out; printf err >&2' > both.log 2>&1
        sh -c 'printf err >&2; printf out' > reversed.log 2>&1
        write_both() { printf err >&2; printf out; }
        write_both > builtin-reversed.log 2>&1
        sh -c 'printf out; printf err >&2' 2>&1 > stdout.log
        sh -c 'printf out; printf err >&2' 2>&1 | tr 'a-z' 'A-Z'
        sh -c 'printf out; printf err >&2' 2> dup-first.log 1>&2
        sh -c 'printf out; printf err >&2' 1>&2 2> stderr-later.log
    "#;
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("both.log")).unwrap(),
        "outerr"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("reversed.log")).unwrap(),
        "errout"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("builtin-reversed.log")).unwrap(),
        "errout"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("stdout.log")).unwrap(),
        "out"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("dup-first.log")).unwrap(),
        "outerr"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("stderr-later.log")).unwrap(),
        "err"
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "errOUTERR");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "out");
}

#[tokio::test]
async fn combined_output_redirections_capture_both_streams_and_append_together() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "write_both() { printf out; printf err >&2; }\nwrite_both &> combined.log\nwrite_both &>> combined.log\nsh -c 'printf external-out; printf external-err >&2' &> external.log\nsh -c 'printf external-out; printf external-err >&2' &>> external.log\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("combined.log")).unwrap(),
        "outerrouterr"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("external.log")).unwrap(),
        "external-outexternal-errexternal-outexternal-err"
    );
}

#[tokio::test]
async fn redirections_apply_to_compound_commands_and_keep_brace_scope_changes() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                VALUE=before
                { VALUE=changed; printf group; } > group.log
                if true; then printf conditional; fi > if.log
                for ITEM in first second; do printf '%s ' "$ITEM"; done > for.log
                case match in match) printf case;; esac > case.log
                (printf subshell) > subshell.log
                printf 'value=%s\n' "$VALUE"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    for (path, expected) in [
        ("group.log", "group"),
        ("if.log", "conditional"),
        ("for.log", "first second "),
        ("case.log", "case"),
        ("subshell.log", "subshell"),
    ] {
        assert_eq!(
            std::fs::read_to_string(tmp.path().join(path)).unwrap(),
            expected
        );
    }
    assert_eq!(ctx.get_captured_stdout().unwrap(), "value=changed\n");
}

#[tokio::test]
async fn here_documents_expand_only_unquoted_bodies_and_collect_multiple_delimiters() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        VALUE=world
        NUMBER=7
        cat <<EOF
Hello $VALUE
$(printf substituted)
$((NUMBER + 2))
line\
continued
\$VALUE
\\$VALUE
EOF
        cat <<'RAW'
$VALUE
$(printf literal)
RAW
        cat <<FIRST <<SECOND
discarded
FIRST
kept
SECOND
    "#;

    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "Hello world\nsubstituted\n9\nlinecontinued\n$VALUE\n\\world\n$VALUE\n$(printf literal)\nkept\n"
    );
}

#[tokio::test]
async fn here_document_delimiter_at_eof_without_line_ending_executes_body() {
    let script = "cat <<EOF\nbody\nEOF";
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    let expected = b"body\n";
    assert_eq!(ctx.get_captured_stdout().unwrap().as_bytes(), expected);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    #[cfg(unix)]
    for (shell, args) in [("bash", vec!["--posix", "-c"]), ("dash", vec!["-c"])] {
        if let Ok(reference) = std::process::Command::new(shell)
            .args(args)
            .arg(script)
            .output()
        {
            assert!(reference.status.success(), "{shell} rejected EOF delimiter");
            assert_eq!(reference.stdout, expected, "{shell} here-document body");
            assert_eq!(reference.stderr, b"", "{shell} here-document diagnostic");
        }
    }
}

#[tokio::test]
async fn unquoted_here_document_backslash_parity_controls_expansion() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"VALUE=expanded
cat <<EOF
\$VALUE
\\$VALUE
\\\$VALUE
\\\\$VALUE
EOF
"#;
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        r#"$VALUE
\expanded
\$VALUE
\\expanded
"#
    );
}

#[tokio::test]
async fn tab_stripping_here_document_flows_through_a_pipeline() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = "cat <<-END | tr 'a-z' 'A-Z'\n\tvalue\n\t\tsecond\n\tEND\n";
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "VALUE\nSECOND\n");
}

#[tokio::test]
async fn tab_stripping_here_document_removes_only_leading_tabs() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = "cat <<-END\n\tstripped\n \tspace-before-tab\n \tEND\n\tEND\n";
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "stripped\n \tspace-before-tab\n \tEND\n"
    );
}

#[tokio::test]
async fn here_document_delimiter_quote_removal_handles_mixed_and_escaped_parts() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        VALUE=expanded
        cat <<E"OF"
$VALUE
EOF
        cat <<\LITERAL
$VALUE
LITERAL
        cat <<END
\"quoted\"
END
    "#;
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "$VALUE\n$VALUE\n\\\"quoted\\\"\n"
    );
}

#[tokio::test]
async fn here_document_and_double_quoted_expansion_preserve_utf8() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf '%s\\n' \"quoted: 雪\"\ncat <<EOF\nhere-doc: 雪\nbackslash: \\雪\nEOF\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "quoted: 雪\nhere-doc: 雪\nbackslash: \\雪\n"
    );
}

#[tokio::test]
async fn noclobber_rejects_existing_files_and_force_redirection_overrides_it() {
    let tmp = tempdir().unwrap();
    std::fs::write(tmp.path().join("existing.txt"), "original\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.enable_capture();

    let status = engine
        .eval_str("set -C; touch marker > existing.txt\n", &mut ctx)
        .await
        .expect("noclobber is a failed command status");
    assert_eq!(status, 1);
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("Cannot overwrite existing target")
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("existing.txt")).unwrap(),
        "original\n"
    );
    assert!(!tmp.path().join("marker").exists());

    ctx.set_env("ESH_REDIR_ASSIGNMENT", "original");
    let status = engine
        .eval_str(
            "ESH_REDIR_ASSIGNMENT=temporary echo blocked > existing.txt || STATUS=$?\nprintf 'status=%s\\n' \"$STATUS\"\n",
            &mut ctx,
        )
        .await
        .expect("a failing output redirection should be handled by the OR list");
    assert_eq!(status, 0);
    assert!(ctx.get_captured_stdout().unwrap().ends_with("status=1\n"));
    assert_eq!(
        ctx.get_var("ESH_REDIR_ASSIGNMENT").as_deref(),
        Some("original")
    );
    assert_eq!(
        ctx.env_vars()
            .get("ESH_REDIR_ASSIGNMENT")
            .map(String::as_str),
        Some("original")
    );

    let status = engine
        .eval_str("echo forced >| existing.txt\n", &mut ctx)
        .await
        .unwrap();
    assert_eq!(status, 0);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("existing.txt")).unwrap(),
        "forced\n"
    );

    std::fs::write(tmp.path().join("descriptor-force.txt"), "original\n").unwrap();
    let status = engine
        .eval_str(
            "echo descriptor-forced 1>| descriptor-force.txt\n",
            &mut ctx,
        )
        .await
        .unwrap();
    assert_eq!(status, 0);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("descriptor-force.txt")).unwrap(),
        "descriptor-forced\n"
    );

    let status = engine
        .eval_str("echo created > new.txt\n", &mut ctx)
        .await
        .unwrap();
    assert_eq!(status, 0);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("new.txt")).unwrap(),
        "created\n"
    );

    #[cfg(unix)]
    for (shell, args) in [("bash", vec!["--posix", "-c"]), ("dash", vec!["-c"])] {
        let reference_dir = tempdir().unwrap();
        if let Ok(reference) = std::process::Command::new(shell)
            .args(args)
            .arg("set -C; printf original > file; printf forced 1>| file; cat file")
            .current_dir(reference_dir.path())
            .output()
        {
            assert!(reference.status.success(), "{shell} failed 1>| redirection");
            assert_eq!(reference.stdout, b"forced", "{shell} 1>| output");
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn noclobber_rejects_symlinks_and_force_redirection_follows_them() {
    let script = "set -C\nprintf replacement > link\nSTATUS=$?\nprintf 'status=%s target=' \"$STATUS\"\ncat target\nprintf 'forced\\n' >| link\nprintf 'forced-target='\ncat target\n";
    let tmp = tempdir().unwrap();
    std::fs::write(tmp.path().join("target"), "original\n").unwrap();
    std::os::unix::fs::symlink("target", tmp.path().join("link")).unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.enable_capture();

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    let expected = "status=1 target=original\nforced-target=forced\n";
    assert_eq!(ctx.get_captured_stdout().unwrap(), expected);
    assert!(
        tmp.path()
            .join("link")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("target")).unwrap(),
        "forced\n"
    );

    let reference_dir = tempdir().unwrap();
    std::fs::write(reference_dir.path().join("target"), "original\n").unwrap();
    std::os::unix::fs::symlink("target", reference_dir.path().join("link")).unwrap();
    let reference = std::process::Command::new("bash")
        .args(["--posix", "-c"])
        .arg(script)
        .current_dir(reference_dir.path())
        .output()
        .expect("Bash should be available for the noclobber comparison");
    assert!(
        reference.status.success(),
        "Bash failed to run the redirections"
    );
    assert_eq!(reference.stdout, expected.as_bytes());
    assert_eq!(
        std::fs::read_to_string(reference_dir.path().join("target")).unwrap(),
        "forced\n"
    );
}

#[tokio::test]
async fn output_redirection_opens_files_before_dispatch_and_expands_target_once() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();

    let status = engine
        .eval_str("false > failed-command.txt\n", &mut ctx)
        .await
        .unwrap();
    assert_eq!(status, 1);
    assert_eq!(
        std::fs::read(tmp.path().join("failed-command.txt")).unwrap(),
        b""
    );

    let status = engine
        .eval_str(
            "echo payload > \"$(echo expanded >> expansion-count.txt; printf target.txt)\"\n",
            &mut ctx,
        )
        .await
        .unwrap();
    assert_eq!(status, 0);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("target.txt")).unwrap(),
        "payload\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("expansion-count.txt")).unwrap(),
        "expanded\n"
    );
}

#[tokio::test]
async fn failed_output_redirection_prevents_stateful_builtin_dispatch() {
    let temp = tempdir().unwrap();
    std::fs::create_dir(temp.path().join("child")).unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "cd child > missing/output\nREDIRECT_STATUS=$?\nprintf 'redirection=%s\\n' \"$REDIRECT_STATUS\"\nprintf 'reached-tail\\n'\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.cwd(), temp.path().canonicalize().unwrap());
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "redirection=1\nreached-tail\n"
    );
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("missing/output")
    );
}

#[tokio::test]
async fn unquoted_redirection_expansion_keeps_spaces_and_wildcards_in_one_path() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();

    let status = engine
        .eval_str(
            "TARGET='space name.txt'; echo spaced > $TARGET\nTARGET='literal*.txt'; echo wildcard > $TARGET\nTARGET=' spaced on both sides '; echo whitespace > $TARGET\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("space name.txt")).unwrap(),
        "spaced\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("literal*.txt")).unwrap(),
        "wildcard\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join(" spaced on both sides ")).unwrap(),
        "whitespace\n"
    );
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 3);
}

#[tokio::test]
async fn descriptor_like_redirection_targets_are_ordinary_filenames() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "echo one > 1; echo two > 2; echo amp-one > '&1'; echo amp-two > '&2'\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("1")).unwrap(),
        "one\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("2")).unwrap(),
        "two\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("&1")).unwrap(),
        "amp-one\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("&2")).unwrap(),
        "amp-two\n"
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn empty_unquoted_redirection_expansion_fails_as_one_empty_path() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.enable_capture();

    let status = engine
        .eval_str("TARGET=; echo payload > $TARGET\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 1);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert!(!tmp.path().join("payload").exists());
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("Cannot open target ''"),
        "empty target should remain an empty path"
    );
}

#[tokio::test]
async fn input_redirections_restore_the_callers_buffered_stdin() {
    let tmp = tempdir().unwrap();
    std::fs::write(tmp.path().join("source.txt"), "from file\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.set_stdin(b"from caller\n".to_vec());

    let status = engine
        .eval_str(
            "read FILE_VALUE < source.txt; read CALLER_VALUE; read HERE_VALUE <<< here; read CALLER_AGAIN\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 1);
    assert_eq!(ctx.get_var("FILE_VALUE").as_deref(), Some("from file"));
    assert_eq!(ctx.get_var("CALLER_VALUE").as_deref(), Some("from caller"));
    assert_eq!(ctx.get_var("HERE_VALUE").as_deref(), Some("here"));
    assert_eq!(ctx.get_var("CALLER_AGAIN"), None);
}

#[tokio::test]
async fn here_strings_expand_as_one_value_and_append_one_newline() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "VALUE='two words *.txt'; cat <<< $VALUE; cat <<< ''; cat <<< \"$(printf 'substitution\\n\\n')\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "two words *.txt\n\nsubstitution\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn unsupported_descriptor_redirections_fail_before_any_script_command_runs() {
    let engine = Engine::new();

    for operator in [
        "3> output",
        "10>> output",
        "2>&-",
        "1>&-",
        "0> output",
        "<> file",
    ] {
        let tmp = tempdir().unwrap();
        let mut ctx = ExecutionContext::new();
        ctx.set_cwd(tmp.path()).unwrap();
        ctx.enable_capture();

        let script = format!("echo should-not-run > marker\necho invalid {operator}\n");
        let error = engine
            .eval_str(&script, &mut ctx)
            .await
            .expect_err("unsupported descriptor redirection must fail before execution");

        assert!(
            error.to_string().contains("redirection"),
            "{operator}: {error}"
        );
        assert!(!tmp.path().join("marker").exists(), "{operator}");
        assert_eq!(ctx.get_captured_stdout().unwrap(), "", "{operator}");
    }
}
