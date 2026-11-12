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
async fn descriptor_duplication_obeys_left_to_right_redirection_order() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.enable_capture();

    let script = r#"
        sh -c 'printf out; printf err >&2' > both.log 2>&1
        sh -c 'printf out; printf err >&2' 2>&1 > stdout.log
        sh -c 'printf out; printf err >&2' 2>&1 | tr 'a-z' 'A-Z'
    "#;
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("both.log")).unwrap(),
        "outerr"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("stdout.log")).unwrap(),
        "out"
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "errOUTERR");
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

    let error = engine
        .eval_str("set -C; touch marker > existing.txt\n", &mut ctx)
        .await
        .expect_err("noclobber should reject an existing target");
    assert!(
        error
            .to_string()
            .contains("Cannot overwrite existing target")
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("existing.txt")).unwrap(),
        "original\n"
    );
    assert!(!tmp.path().join("marker").exists());

    ctx.set_env("ESH_REDIR_ASSIGNMENT", "original");
    let _ = engine
        .eval_str(
            "ESH_REDIR_ASSIGNMENT=temporary echo blocked > existing.txt\n",
            &mut ctx,
        )
        .await
        .expect_err("a failing output redirection should fail the command");
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

    let status = engine
        .eval_str("echo created > new.txt\n", &mut ctx)
        .await
        .unwrap();
    assert_eq!(status, 0);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("new.txt")).unwrap(),
        "created\n"
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
