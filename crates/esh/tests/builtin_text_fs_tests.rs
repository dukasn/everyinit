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
async fn test_pwd_cat_head_tail() {
    let tmp = tempdir().unwrap();
    let file_path = tmp.path().join("lines.txt");
    fs::write(
        &file_path,
        "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\n",
    )
    .unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let _ = ctx.set_cwd(tmp.path());

    let script = r#"
        pwd
        cat lines.txt | head -n 3
        cat lines.txt | tail -n 2
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("line1"));
    assert!(output.contains("line2"));
    assert!(output.contains("line3"));
    assert!(output.contains("line11"));
    assert!(output.contains("line12"));
}

#[tokio::test]
async fn echo_double_dash_ends_esh_option_parsing() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str("echo -- -n -e", &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "-n -e\n");
}

#[tokio::test]
async fn head_reads_a_binary_prefix() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("binary.bin"),
        [0x28_u8, 0xb5, 0x2f, 0xfd, 4, 5],
    )
    .unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    let code = engine
        .eval_str("head -c 4 binary.bin", &mut ctx)
        .await
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        ctx.get_captured_stdout_bytes().unwrap(),
        vec![0x28, 0xb5, 0x2f, 0xfd]
    );

    let mut pipeline_ctx = ExecutionContext::new();
    pipeline_ctx.enable_capture();
    pipeline_ctx.set_cwd(tmp.path()).unwrap();
    let code = engine
        .eval_str("head -c 4 binary.bin | base64", &mut pipeline_ctx)
        .await
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(pipeline_ctx.get_captured_stdout().unwrap(), "KLUv/Q==\n");
}

#[tokio::test]
async fn head_zero_counts_emit_no_data_in_line_and_byte_modes() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("input.bin"), [0x00_u8, 0xff, b'a', b'\n']).unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("head -n 0 input.bin; head -c 0 input.bin", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout_bytes().unwrap(), b"");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn head_rejects_mixed_line_and_byte_counts_in_either_order() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine.eval_str("head -n 1 -c 2", &mut ctx).await.unwrap(),
        2
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: head: -n and -c are mutually exclusive\n"
    );

    ctx.enable_capture();
    assert_eq!(
        engine.eval_str("head -c 2 -n 1", &mut ctx).await.unwrap(),
        2
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: head: -n and -c are mutually exclusive\n"
    );
}

#[tokio::test]
async fn tail_supports_byte_counts_and_preserves_unterminated_binary_data() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("lines.txt"), b"first\nsecond").unwrap();
    fs::write(tmp.path().join("binary.bin"), [0xff, 0x00, 0x80, b'A']).unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();
    let status = engine
        .eval_str(
            "head -n 0 lines.txt\nhead -n 2 lines.txt\ntail -n 0 lines.txt\ntail -n 1 lines.txt\ntail -c 4 lines.txt\ntail -c 2 binary.bin\nprintf 'abcdef' | tail -c 3\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout_bytes().unwrap(),
        [
            b"first\nsecondsecondcond".as_slice(),
            &[0x80, b'A'],
            b"def".as_slice()
        ]
        .concat()
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn tail_rejects_missing_or_malformed_counts() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str("head -n invalid\n", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert_eq!(engine.eval_str("tail -c\n", &mut ctx).await.unwrap(), 2);
    assert_eq!(
        engine
            .eval_str("tail -n invalid\n", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        engine
            .eval_str("tail -f input.txt\n", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: head: invalid line count 'invalid'\nesh: tail: missing byte count\nesh: tail: invalid line count 'invalid'\nesh: tail: unsupported option '-f'\n"
    );
}

#[tokio::test]
async fn tail_accepts_signed_posix_counts_from_start_or_end() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str(
                "printf 'one\\ntwo\\nthree\\n' | tail -n +2; printf 'abcdef' | tail -c +3; printf 'one\\ntwo\\nthree\\n' | tail -n -1",
                &mut ctx,
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "two\nthree\ncdefthree\n"
    );
}

#[tokio::test]
async fn tail_rejects_zero_from_start_and_conflicting_count_modes() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str("tail -n +0", &mut ctx).await.unwrap(), 2);
    assert_eq!(
        engine.eval_str("tail -n 1 -c 1", &mut ctx).await.unwrap(),
        2
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("-n and -c are mutually exclusive")
    );
}

#[tokio::test]
async fn cat_head_tail_and_grep_accept_dash_as_standard_input() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("lines.txt"), b"file-one\nfile-two\n").unwrap();
    let engine = Engine::new();

    let mut cat_ctx = ExecutionContext::new();
    cat_ctx.enable_capture();
    cat_ctx.set_cwd(tmp.path()).unwrap();
    cat_ctx.set_stdin(b"pipe-data\n".to_vec());
    assert_eq!(engine.eval_str("cat -\n", &mut cat_ctx).await.unwrap(), 0);
    assert_eq!(cat_ctx.get_captured_stdout_bytes().unwrap(), b"pipe-data\n");

    let mut head_ctx = ExecutionContext::new();
    head_ctx.enable_capture();
    head_ctx.set_cwd(tmp.path()).unwrap();
    head_ctx.set_stdin(b"stdin-first\nstdin-last\n".to_vec());
    assert_eq!(
        engine
            .eval_str("head -n 1 - lines.txt\n", &mut head_ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        head_ctx.get_captured_stdout().unwrap(),
        "==> - <==\nstdin-first\n\n==> lines.txt <==\nfile-one\n"
    );

    let mut tail_ctx = ExecutionContext::new();
    tail_ctx.enable_capture();
    tail_ctx.set_cwd(tmp.path()).unwrap();
    tail_ctx.set_stdin(b"stdin-first\nstdin-last\n".to_vec());
    assert_eq!(
        engine
            .eval_str("tail -n 1 - lines.txt\n", &mut tail_ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        tail_ctx.get_captured_stdout().unwrap(),
        "==> - <==\nstdin-last\n\n==> lines.txt <==\nfile-two\n"
    );

    let mut grep_ctx = ExecutionContext::new();
    grep_ctx.enable_capture();
    grep_ctx.set_stdin(b"other\nneedle from stdin\n".to_vec());
    assert_eq!(
        engine
            .eval_str("grep needle -\n", &mut grep_ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        grep_ctx.get_captured_stdout().unwrap(),
        "needle from stdin\n"
    );
}

#[tokio::test]
async fn cat_continues_after_an_unreadable_operand() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("readable.txt"), b"still printed\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("cat missing.txt readable.txt", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "still printed\n");
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("cat: cannot open 'missing.txt'")
    );
}

#[tokio::test]
async fn cat_supports_posix_unbuffered_option_and_end_of_options() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("-input"), b"file-data\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str(
                "cat -u -- -input; cat -- -input; printf 'pipe-data\\n' | cat -uu -",
                &mut ctx
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.get_captured_stdout_bytes().unwrap(),
        b"file-data\nfile-data\npipe-data\n"
    );

    ctx.enable_capture();
    assert_eq!(engine.eval_str("cat -x", &mut ctx).await.unwrap(), 2);
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("unsupported option '-x'")
    );
}

#[tokio::test]
async fn cat_preserves_arbitrary_bytes_and_consumes_stdin_in_operand_order() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("first.bin"), [0x00, 0xff, 0x41]).unwrap();
    fs::write(tmp.path().join("last.bin"), [0x80, 0x0a]).unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.set_stdin(vec![0xfe, 0x42]);

    assert_eq!(
        engine
            .eval_str("cat first.bin - last.bin -", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.get_captured_stdout_bytes().unwrap(),
        [0x00, 0xff, 0x41, 0xfe, 0x42, 0x80, 0x0a]
    );
}

#[tokio::test]
async fn head_and_tail_accept_leading_hyphen_files_after_double_dash() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("-input"), b"first\nsecond\nthird\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("head -n 1 -- -input; tail -n +3 -- -input", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "first\nthird\n");
}

#[tokio::test]
async fn head_and_tail_continue_after_an_unreadable_operand() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("readable.txt"), b"first\nsecond\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str(
                "head -n 1 missing.txt readable.txt; tail -n 1 missing.txt readable.txt",
                &mut ctx
            )
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "==> readable.txt <==\nfirst\n==> readable.txt <==\nsecond\n"
    );
    let stderr = ctx.get_captured_stderr().unwrap();
    assert!(stderr.contains("head: cannot read 'missing.txt'"));
    assert!(stderr.contains("tail: cannot read 'missing.txt'"));
}

#[tokio::test]
async fn grep_returns_status_two_when_an_input_file_cannot_be_read() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("grep needle missing.txt\n", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("cannot read 'missing.txt'")
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("grep -s needle missing.txt\n", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn grep_accepts_explicit_multiple_patterns_and_patterns_starting_with_dash() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"a.b\naxb\n-leading\nother\n".to_vec());

    assert_eq!(
        engine
            .eval_str("grep -F -e a.b -e-leading", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "a.b\n-leading\n");

    ctx.enable_capture();
    assert_eq!(engine.eval_str("grep -e", &mut ctx).await.unwrap(), 2);
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("-e requires a pattern")
    );
}

#[tokio::test]
async fn grep_counts_selected_lines_per_input_and_quiet_suppresses_counts() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("one.txt"), "hit\nmiss\nhit again\n").unwrap();
    fs::write(tmp.path().join("two.txt"), "miss\nhit\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("grep -c hit one.txt; grep -c hit one.txt two.txt", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "2\none.txt:2\ntwo.txt:1\n"
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("grep -cq absent one.txt", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
}

#[tokio::test]
async fn grep_count_prints_zero_for_absent_stdin_and_dash_input() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine.eval_str("grep -c needle", &mut ctx).await.unwrap(),
        1
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "0\n");

    ctx.enable_capture();
    assert_eq!(
        engine.eval_str("grep -c needle -", &mut ctx).await.unwrap(),
        1
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "0\n");
}

#[tokio::test]
async fn grep_x_selects_only_whole_lines_for_multiple_patterns() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"foo\nfood\nxfoo\nbar\nfoo|bar\n".to_vec());

    assert_eq!(
        engine
            .eval_str("grep -x -e 'foo|bar'", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "foo|bar\n");

    ctx.enable_capture();
    ctx.set_stdin(b"foo\nfood\nbar\n".to_vec());
    assert_eq!(
        engine
            .eval_str("grep -Ex -e 'foo|bar'", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "foo\nbar\n");

    ctx.enable_capture();
    ctx.set_stdin(b"a.b\naXb\nxa.b\n".to_vec());
    assert_eq!(engine.eval_str("grep -xF a.b", &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "a.b\n");

    ctx.enable_capture();
    ctx.set_stdin(b"foo\nfood\n".to_vec());
    assert_eq!(engine.eval_str("grep -vx foo", &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "food\n");
}

#[tokio::test]
async fn grep_uses_basic_regex_by_default_and_extended_regex_with_e() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"a+\naa\nab\nabab\n".to_vec());

    assert_eq!(engine.eval_str("grep 'a+'", &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "a+\n");

    ctx.enable_capture();
    ctx.set_stdin(b"abab\naa\nab\n".to_vec());
    assert_eq!(
        engine
            .eval_str("grep '\\(ab\\)\\{2\\}'", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "abab\n");

    ctx.enable_capture();
    ctx.set_stdin(b"a+\naa\nab\n".to_vec());
    assert_eq!(engine.eval_str("grep -E 'a+'", &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "a+\naa\nab\n");

    ctx.enable_capture();
    ctx.set_stdin(b"aa\n".to_vec());
    assert_eq!(
        engine
            .eval_str("grep '\\(a\\)\\1'", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("backreferences")
    );
}

#[tokio::test]
async fn grep_rejects_conflicting_fixed_and_extended_matchers() {
    let engine = Engine::new();

    for command in [
        "grep -E -F pattern",
        "grep -F -E pattern",
        "grep -EF pattern",
        "grep -FE pattern",
    ] {
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();
        ctx.set_stdin(b"pattern\n".to_vec());

        assert_eq!(
            engine.eval_str(command, &mut ctx).await.unwrap(),
            2,
            "{command}"
        );
        assert_eq!(ctx.get_captured_stdout().unwrap(), "", "{command}");
        assert!(
            ctx.get_captured_stderr()
                .unwrap()
                .contains("conflicting matchers"),
            "{command}"
        );
    }
}

#[tokio::test]
async fn grep_preserves_posix_first_closing_bracket_and_named_character_classes() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"]\na\nb\n1\n".to_vec());

    assert_eq!(engine.eval_str("grep '[]a]'", &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "]\na\n");

    ctx.enable_capture();
    ctx.set_stdin(b"a\n1\n\n".to_vec());
    assert_eq!(
        engine
            .eval_str("grep '[[:digit:]]'", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "1\n");

    ctx.enable_capture();
    ctx.set_stdin(b"left\x0bright\nordinary\n".to_vec());
    assert_eq!(
        engine
            .eval_str("grep '[[:space:]]'", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap().as_bytes(),
        b"left\x0bright\n"
    );
}

#[tokio::test]
async fn grep_rejects_locale_collating_and_equivalence_classes_explicitly() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"ch\na\n".to_vec());

    assert_eq!(
        engine.eval_str("grep '[[.ch.]]'", &mut ctx).await.unwrap(),
        2
    );
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("collating and equivalence classes")
    );

    ctx.enable_capture();
    ctx.set_stdin(b"ch\na\n".to_vec());
    assert_eq!(
        engine
            .eval_str("grep -E '[[=a=]]'", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("collating and equivalence classes")
    );

    ctx.enable_capture();
    ctx.set_stdin(b"[[.ch.]]\n".to_vec());
    assert_eq!(
        engine
            .eval_str("grep -F '[[.ch.]]'", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "[[.ch.]]\n");
}

#[tokio::test]
async fn grep_l_lists_each_matching_input_and_overrides_count_output() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("one.txt"), "hit\nmiss\n").unwrap();
    fs::write(tmp.path().join("two.txt"), "miss\n").unwrap();
    fs::write(tmp.path().join("three.txt"), "hit again\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("grep -lc hit one.txt two.txt three.txt", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "one.txt\nthree.txt\n");

    ctx.enable_capture();
    ctx.set_stdin(b"miss\nhit\n".to_vec());
    assert_eq!(engine.eval_str("grep -l hit", &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "(standard input)\n");

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("grep -q hit one.txt missing.txt", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert_eq!(ctx.get_captured_stderr().unwrap_or_default(), "");
}

#[tokio::test]
async fn grep_line_numbers_reset_per_file_and_compose_with_invert() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("one.txt"), "alpha\nbeta\nalpha\n").unwrap();
    fs::write(tmp.path().join("two.txt"), "beta\nalpha\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str(
                "grep -n alpha one.txt; grep -n alpha one.txt two.txt; grep -vn alpha two.txt",
                &mut ctx,
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "1:alpha\n3:alpha\none.txt:1:alpha\none.txt:3:alpha\ntwo.txt:2:alpha\n1:beta\n"
    );
}

#[tokio::test]
async fn grep_case_insensitive_ascii_matching_composes_with_line_numbers_and_invert() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("input.txt"), "Alpha\nbeta\nALPHA\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str(
                "grep -in alpha input.txt; grep -iv alpha input.txt",
                &mut ctx
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "1:Alpha\n3:ALPHA\nbeta\n"
    );
}

#[tokio::test]
async fn grep_case_insensitive_matching_uses_unicode_case_folding() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin("Éclair\néCLAIR\nclair\n".as_bytes().to_vec());

    assert_eq!(
        engine.eval_str("grep -i 'éclair'", &mut ctx).await.unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "Éclair\néCLAIR\n");
}

#[tokio::test]
async fn grep_reads_multiple_pattern_files_and_handles_empty_pattern_sources() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("first.patterns"), "alpha\nbeta\n").unwrap();
    fs::write(tmp.path().join("second.patterns"), "gamma\n").unwrap();
    fs::write(tmp.path().join("empty.patterns"), "").unwrap();
    fs::write(
        tmp.path().join("input.txt"),
        "alpha here\nbeta here\ngamma here\ndelta\n",
    )
    .unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str(
                "grep -F -f first.patterns -f second.patterns input.txt",
                &mut ctx,
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "alpha here\nbeta here\ngamma here\n"
    );

    ctx.enable_capture();
    ctx.set_stdin(b"gamma\n".to_vec());
    assert_eq!(
        engine
            .eval_str("grep -F -f - input.txt", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "gamma here\n");

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("grep -f empty.patterns input.txt", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("grep -e '' input.txt", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "alpha here\nbeta here\ngamma here\ndelta\n"
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("grep -f missing.patterns -s input.txt", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn grep_preserves_carriage_returns_and_processes_unterminated_final_records() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"hit\r\nskip\r\nhit-final".to_vec());

    assert_eq!(engine.eval_str("grep hit", &mut ctx).await.unwrap(), 0);
    assert_eq!(
        ctx.get_captured_stdout_bytes().unwrap(),
        b"hit\r\nhit-final\n"
    );
}

#[tokio::test]
async fn test_grep_and_glob() {
    let tmp = tempdir().unwrap();
    let sub = tmp.path().join("src");
    fs::create_dir_all(&sub).unwrap();
    fs::write(
        sub.join("main.rs"),
        "fn main() {\n    println!(\"Hello Evin!\");\n}\n",
    )
    .unwrap();
    fs::write(
        sub.join("lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let _ = ctx.set_cwd(tmp.path());

    let script = r#"
        grep -i "hello" src/main.rs
        glob "src/*.rs"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("println!(\"Hello Evin!\");"));
    assert!(output.contains("src/main.rs") || output.contains("src/lib.rs"));
}

#[tokio::test]
async fn glob_requires_a_literal_leading_dot_for_hidden_paths() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("visible.txt"), "visible").unwrap();
    fs::write(tmp.path().join(".hidden.txt"), "hidden").unwrap();
    fs::create_dir_all(tmp.path().join("visible-dir")).unwrap();
    fs::create_dir_all(tmp.path().join(".hidden-dir")).unwrap();
    fs::write(tmp.path().join("visible-dir/child.txt"), "visible child").unwrap();
    fs::write(tmp.path().join(".hidden-dir/.child.txt"), "hidden child").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    let status = engine
        .eval_str(
            "glob '*'\nglob '.hidden.txt'\nif glob '[.]hidden.txt'; then echo bracket-matched; else echo bracket-skipped; fi\nglob '*/child.txt'\nif glob '.hidden-dir/*'; then echo inner-bracket-matched; else echo inner-skipped; fi\nglob '.hidden-dir/.child.txt'\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "visible-dir\nvisible.txt\n.hidden.txt\nbracket-skipped\nvisible-dir/child.txt\ninner-skipped\n.hidden-dir/.child.txt\n"
    );
}

#[tokio::test]
async fn glob_merges_multiple_patterns_in_sorted_order_and_rejects_invalid_patterns() {
    let tmp = tempdir().unwrap();
    for name in ["alpha.txt", "beta.txt", "gamma.txt"] {
        fs::write(tmp.path().join(name), name).unwrap();
    }

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    let status = engine
        .eval_str(
            "glob 'gamma.txt' 'alpha.txt' 'beta.txt' 'alpha.txt'\nglob 'alpha*' '['\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 2);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "alpha.txt\nbeta.txt\ngamma.txt\n"
    );
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .starts_with("esh: glob: invalid pattern '[':")
    );
}

#[tokio::test]
async fn glob_reports_no_matches_but_succeeds_when_any_pattern_matches() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("present.txt"), "present").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    let status = engine
        .eval_str(
            "glob 'missing*'\nNO_MATCH=$?\nglob 'missing*' 'present.txt'\nPARTIAL_MATCH=$?\nprintf 'no-match=%s partial-match=%s\\n' \"$NO_MATCH\" \"$PARTIAL_MATCH\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "present.txt\nno-match=1 partial-match=0\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn test_fixed_grep_combined_flags_and_recursive_glob() {
    let tmp = tempdir().unwrap();
    let nested = tmp.path().join("tree/one/two");
    fs::create_dir_all(&nested).unwrap();
    fs::write(tmp.path().join("literal.txt"), "a.b\naxb\n").unwrap();
    fs::write(nested.join("leaf.txt"), "leaf\n").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let _ = ctx.set_cwd(tmp.path());

    let code = engine
        .eval_str(
            r#"
                grep -Fn "a.b" literal.txt
                grep -Fq "a.b" literal.txt
                glob "tree/**"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("1:a.b"));
    assert!(output.contains("tree/one/two/leaf.txt"));
}

#[cfg(unix)]
#[tokio::test]
async fn test_executable_predicate_checks_permission_bits() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempdir().unwrap();
    let executable = tmp.path().join("tool");
    let plain = tmp.path().join("plain");
    fs::write(&executable, "#!/bin/sh\n").unwrap();
    fs::write(&plain, "text\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&plain, fs::Permissions::from_mode(0o644)).unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    let _ = ctx.set_cwd(tmp.path());

    assert_eq!(engine.eval_str("test -x tool", &mut ctx).await.unwrap(), 0);
    assert_eq!(engine.eval_str("test -x plain", &mut ctx).await.unwrap(), 1);
}
