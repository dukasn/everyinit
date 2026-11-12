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
async fn test_dirname_basename_realpath() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        dirname /usr/local/bin/esh
        basename /usr/local/bin/esh
        basename -s .tar.gz /dist/app-v1.0.0.tar.gz
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "/usr/local/bin");
    assert_eq!(lines[1], "esh");
    assert_eq!(lines[2], "app-v1.0.0");
}

#[tokio::test]
async fn dirname_and_basename_reduce_all_slash_operands_to_root() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "dirname ///\nbasename ///\ndirname //\nbasename //\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "/\n/\n/\n/\n");
}

#[tokio::test]
async fn dirname_and_basename_accept_double_dash_before_hyphen_paths() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "dirname -- -name/leaf\nbasename -- -name/leaf\ndirname -- -name\nbasename -- -name\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "-name\nleaf\n.\n-name\n"
    );
}

#[tokio::test]
async fn basename_supports_posix_multiple_operands_and_suffix_option() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "basename -a /src/one.rs /src/two.rs\nbasename -s .rs /src/one.rs /src/two.rs\nbasename /src/one.rs .rs\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "one.rs\ntwo.rs\none\ntwo\none\n"
    );
}

#[tokio::test]
async fn basename_rejects_missing_suffix_and_extra_operands_without_multiple_mode() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("basename -s\nbasename one two three\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 2);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    let stderr = ctx.get_captured_stderr().unwrap();
    assert!(stderr.contains("option requires a suffix"));
    assert!(stderr.contains("too many operands"));
}

#[tokio::test]
async fn test_cut_tr_wc() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        echo "alpha:beta:gamma:delta" | cut -d: -f2,4
        echo "hello world" | tr '[:lower:]' '[:upper:]'
        echo -n "word1 word2 word3" | wc -w
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "beta:delta");
    assert_eq!(lines[1], "HELLO WORLD");
    assert_eq!(lines[2].trim(), "3");
}

#[tokio::test]
async fn cut_supports_character_ranges_field_order_stdin_and_validation() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf 'éx:yz\\nplain\\na::c\\nundelimited\\n' | cut -c2-4; printf 'a::c\\nplain\\nundelimited\\n' | cut -d: -f3,1; printf 'a::c\\nplain\\n' | cut -s -d: -f2,3; printf 'a\\tone\\na\\ttwo\\nb\\tthree\\n' | cut -sf1",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "x:y\nlai\n::c\nnde\na:c\nplain\nundelimited\n:c\na\na\nb\n"
    );

    ctx.enable_capture();
    assert_eq!(engine.eval_str("cut -f0", &mut ctx).await.unwrap(), 2);
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("invalid selection list")
    );
}

#[tokio::test]
async fn cut_byte_mode_preserves_and_can_split_utf8_bytes() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str("printf 'éx\\n' | cut -b2", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout_bytes().unwrap(), b"\xa9\n");
}

#[tokio::test]
async fn cut_accepts_blank_separated_lists_and_non_splitting_byte_mode() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str(
                "printf 'abcdef\\n' | cut -c '1 3-4'; printf 'éx\\n' | cut -b1 -n; printf 'éx\\n' | cut -b2 -n; printf 'éx\\n' | cut -b1-2 -n; printf 'éx\\n' | cut -b1,3 -n",
                &mut ctx,
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "acd\n\né\né\nx\n");

    ctx.enable_capture();
    assert_eq!(engine.eval_str("cut -c 1 -n", &mut ctx).await.unwrap(), 2);
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("-n requires byte selection")
    );
}

#[tokio::test]
async fn cut_continues_after_an_unreadable_file_operand() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("readable.txt"), "first:second\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("cut -d: -f2 missing.txt readable.txt", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "second\n");
    assert!(ctx.get_captured_stderr().unwrap().contains("missing.txt"));
}

#[tokio::test]
async fn tr_squeezes_deletes_and_expands_unicode_ranges() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf 'aaabbbcc\\n' | tr -s abc; printf 'aaabbbcc\\n' | tr -ds a b; printf 'éêë\\n' | tr 'é-ê' XY; printf 'hello\\n' | tr '[:lower:]' '[:upper:]'; printf 'a\\nb\\n' | tr '\\012' ','; printf a | tr a '\\141'; printf 'abc xyz!\\n' | tr -cs '[:alpha:]' '[\\n*]'; printf ab | tr ab '[x*2]'",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "abc\nbcc\nXYë\nHELLO\na,b,aabc\nxyz\nxx"
    );

    ctx.enable_capture();
    assert_eq!(engine.eval_str("tr -ds a", &mut ctx).await.unwrap(), 2);
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("invalid number of operands")
    );

    ctx.enable_capture();
    assert_eq!(engine.eval_str("tr -z a", &mut ctx).await.unwrap(), 2);
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("unsupported option '-z'")
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("tr '[:unsupported:]' x", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("invalid character set")
    );

    ctx.enable_capture();
    assert_eq!(engine.eval_str("tr '\\400' x", &mut ctx).await.unwrap(), 2);
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("invalid character set")
    );
}

#[tokio::test]
async fn tr_complements_character_sets_for_translation_deletion_and_squeezing() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf 'abcxyz' | tr -c abc X; printf 'abcxyz' | tr -cd abc; printf 'xyzz' | tr -cs a",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "abcXXXabcxyz");
}

#[tokio::test]
async fn tr_lowercase_and_uppercase_complement_options_share_esh_unicode_semantics() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str(
                "printf 'abcxyz' | tr -c abc X; printf 'abcxyz' | tr -C abc X",
                &mut ctx,
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "abcXXXabcXXX");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn tr_rejects_combining_mutually_exclusive_complement_options() {
    let engine = Engine::new();
    for script in ["tr -cC abc X", "tr -c -C abc X"] {
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();

        assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 2);
        assert_eq!(ctx.get_captured_stdout().unwrap(), "");
        assert_eq!(
            ctx.get_captured_stderr().unwrap(),
            "esh: tr: -c and -C are mutually exclusive\n"
        );
    }
}

#[tokio::test]
async fn tr_complement_translation_counts_repeated_exclusions_once() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"b".to_vec());

    let mut replacements = "x".repeat(98);
    replacements.replace_range(96..97, "y");
    replacements.replace_range(97..98, "z");
    let script = format!("tr -c aa '{replacements}'");

    assert_eq!(engine.eval_str(&script, &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "z");
}

#[tokio::test]
async fn tr_repeated_translation_source_uses_its_first_mapping() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str("printf 'ab' | tr aab 123", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "13");
}

#[tokio::test]
async fn tr_pads_a_short_replacement_set_with_its_last_character() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str("printf 'abc' | tr abc XY", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "XYY");
}

#[tokio::test]
async fn tr_keeps_character_classes_and_escaped_hyphens_out_of_ranges() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str(
                "printf '9:-a' | tr '[:digit:]-a' X; printf '%s' '-az' | tr 'a\\-z' X",
                &mut ctx
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "X:XXXXX");
}

#[tokio::test]
async fn tr_translates_using_esh_ordered_class_in_the_second_set() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str("printf ab | tr ab '[:digit:]'", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "01");
}

#[tokio::test]
async fn tr_processes_nul_bytes_for_translation_deletion_and_squeezing() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    ctx.set_stdin(vec![b'a', 0, b'b']);
    assert_eq!(engine.eval_str("tr '\\000' X", &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout_bytes().unwrap(), b"aXb");

    ctx.enable_capture();
    ctx.set_stdin(vec![b'a', 0, 0, b'b']);
    assert_eq!(engine.eval_str("tr -d '\\000'", &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout_bytes().unwrap(), b"ab");

    ctx.enable_capture();
    ctx.set_stdin(vec![b'a', 0, 0, b'b']);
    assert_eq!(engine.eval_str("tr -s '\\000'", &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout_bytes().unwrap(), b"a\0b");
}

#[tokio::test]
async fn tr_unicode_ranges_skip_the_surrogate_code_point_gap() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let first = char::from_u32(0xd7ff).unwrap();
    let last = char::from_u32(0xe000).unwrap();
    let script = format!("printf '{first}{last}' | tr '{first}-{last}' X");

    assert_eq!(engine.eval_str(&script, &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout(), Some("XX".to_string()));
}

#[tokio::test]
async fn test_sha256sum_and_md5sum() {
    let tmp = tempdir().unwrap();
    let file = tmp.path().join("data.txt");
    fs::write(&file, b"hello world").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let _ = ctx.set_cwd(tmp.path());

    let script = r#"
        sha256sum data.txt | cut -d' ' -f1
        md5sum data.txt | cut -d' ' -f1
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines[0],
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
    assert_eq!(lines[1], "5eb63bbbe01eeed093cb22bb8f5acdc3");
}

#[tokio::test]
async fn test_sha256sum_check_manifest_reports_matches_and_mismatches() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("data.txt"), b"hello world").unwrap();
    fs::write(
        tmp.path().join("SHA256SUMS"),
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9  data.txt\n",
    )
    .unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("sha256sum -c SHA256SUMS", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "data.txt: OK\n");

    fs::write(
        tmp.path().join("SHA256SUMS"),
        "0000000000000000000000000000000000000000000000000000000000000000  data.txt\n",
    )
    .unwrap();
    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("sha256sum --check SHA256SUMS", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "data.txt: FAILED\n");
}

#[tokio::test]
async fn test_sort_and_uniq() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let code = engine
        .eval_str(
            r#"
                printf 'pear\napple\napple\nbanana\n' | sort -u
                printf 'apple\napple\nbanana\npear\npear\n' | uniq -c
                printf 'apple\napple\nbanana\npear\npear\n' | uniq -d
                printf 'apple\napple\nbanana\npear\npear\n' | uniq -u
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "apple\nbanana\npear\n2 apple\n1 banana\n2 pear\napple\npear\nbanana\n"
    );
}

#[tokio::test]
async fn sort_numeric_compares_decimal_keys_exactly_and_deduplicates_by_key() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf '10\\n2\\n-3\\n1.50\\n1.5\\nword\\n-0\\n0\\n' | sort -n; printf '1.50\\n1.5\\n2\\n10\\n' | sort -nu; printf '9007199254740993\\n9007199254740992\\n' | sort -n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "-3\n-0\n0\nword\n1.5\n1.50\n2\n10\n1.50\n2\n10\n9007199254740992\n9007199254740993\n"
    );
}

#[tokio::test]
async fn sort_fold_case_uses_unicode_keys_and_unique_keeps_first_input() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf 'b\\na\\nA\\nB\\n' | sort -f; printf 'b\\na\\nA\\nB\\n' | sort -fu; printf 'ä\\nÄ\\n' | sort -fu",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "A\na\nB\nb\na\nb\nä\n");
}

#[tokio::test]
async fn sort_stable_preserves_input_order_for_equal_comparison_keys() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str(
                "printf 'a\\nA\\n' | sort -sf; printf 'z:1\\na:1\\n' | sort -s -t: -k2,2",
                &mut ctx,
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "a\nA\nz:1\na:1\n");
}

#[tokio::test]
async fn sort_check_mode_reports_disorder_and_checks_unique_keys() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str(
                "printf 'a\\nb\\n' | sort -c; printf 'b\\na\\n' | sort -c; printf 'b\\na\\n' | sort -C; printf 'a\\nA\\n' | sort -cfu",
                &mut ctx,
            )
            .await
            .unwrap(),
        1
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: sort: input is not in sorted order\nesh: sort: input is not in sorted order\n"
    );
}

#[tokio::test]
async fn sort_output_file_supports_distinct_and_in_place_paths() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("input.txt"), "pear\napple\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str(
                "sort -o result.txt input.txt; sort -o input.txt input.txt",
                &mut ctx,
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert_eq!(
        fs::read_to_string(tmp.path().join("result.txt")).unwrap(),
        "apple\npear\n"
    );
    assert_eq!(
        fs::read_to_string(tmp.path().join("input.txt")).unwrap(),
        "apple\npear\n"
    );
}

#[tokio::test]
async fn sort_stops_on_unreadable_operand_without_partial_output() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("before.txt"), "zeta\n").unwrap();
    fs::write(tmp.path().join("after.txt"), "alpha\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("sort before.txt missing.txt after.txt", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("sort: cannot read 'missing.txt'")
    );
}

#[tokio::test]
async fn sort_output_file_reports_usage_and_write_errors() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("input.txt"), "b\na\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("sort -o absent/target input.txt", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("cannot write 'absent/target'")
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("sort -o first -o second input.txt", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("sort -c -o output input.txt", &mut ctx)
            .await
            .unwrap(),
        2
    );
    ctx.enable_capture();
    assert_eq!(engine.eval_str("sort -c -C", &mut ctx).await.unwrap(), 2);
}

#[tokio::test]
async fn sort_check_mode_rejects_merge_output_and_multiple_file_modes() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    for script in [
        "sort -c -m",
        "sort -c -o output",
        "sort -c first.txt second.txt",
    ] {
        ctx.enable_capture();
        assert_eq!(
            engine.eval_str(script, &mut ctx).await.unwrap(),
            2,
            "{script}"
        );
        assert_eq!(ctx.get_captured_stdout().unwrap(), "", "{script}");
    }
}

#[tokio::test]
async fn sort_merge_combines_presorted_runs_with_keys_and_options() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("left.txt"), "b:1\nd:2\n").unwrap();
    fs::write(tmp.path().join("right.txt"), "a:1\nc:2\n").unwrap();
    fs::write(tmp.path().join("stable-left.txt"), "z:1\ny:2\n").unwrap();
    fs::write(tmp.path().join("stable-right.txt"), "a:1\nb:2\n").unwrap();
    fs::write(tmp.path().join("unique-left.txt"), "b:1\n").unwrap();
    fs::write(tmp.path().join("unique-right.txt"), "a:1\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str(
                "sort -m -t: -k2,2n left.txt right.txt; sort -ms -t: -k2,2n stable-left.txt stable-right.txt; sort -mu -t: -k2,2n unique-left.txt unique-right.txt",
                &mut ctx,
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "a:1\nb:1\nc:2\nd:2\nz:1\na:1\ny:2\nb:2\nb:1\n"
    );
}

#[tokio::test]
async fn sort_dictionary_order_ignores_punctuation_and_deduplicates_by_key() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str(
                "printf 'a-1\\na 1\\na1\\nb_2\\nb2\\n' | sort -d; printf 'a1\\na-1\\na 1\\nb_2\\nb2\\n' | sort -du",
                &mut ctx,
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "a 1\na-1\na1\nb2\nb_2\na 1\na1\nb_2\n"
    );

    ctx.enable_capture();
    assert_eq!(engine.eval_str("sort -dn", &mut ctx).await.unwrap(), 2);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
}

#[tokio::test]
async fn sort_ignores_unicode_controls_and_rejects_numeric_control_modes() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str("printf 'a\\001\\na\\n' | sort -iu", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout_bytes().unwrap(), b"a\x01\n");

    ctx.enable_capture();
    assert_eq!(engine.eval_str("sort -in", &mut ctx).await.unwrap(), 2);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
}

#[tokio::test]
async fn sort_supports_field_keys_ranges_modifiers_and_separators() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf 'x:10\nx:2\ny:1\n' | sort -t: -k2,2n; printf 'b:2\na:10\na:2\n' | sort -t : -k1,1 -k2,2n; printf 'ax\nay\nbx\n' | sort -k1.2,1.2; printf 'a:02\na:2\n' | sort -t: -u -k2,2n; printf 'b:2\na:10\na:2\n' | sort --field-separator=: --key=1,1 --key=2,2n; printf '10\n2\nA\na\n' | sort -n -k1,1f; printf 'a\nb\n' | sort -k1,1r; printf 'x: b\nx:a\n' | sort -t: -k2,2b; printf 'x: b\nx:a\n' | sort -b -t: -k2,2; printf 'x:10\nx: 2\n' | sort -b -t: -k2.2,2.2; printf 'a:1\na:2\nb:1\n' | sort -r -t: -k1,1; printf 'x: aZ\nx:bY\n' | sort -t: -k2.2b,2.2; printf '10:a\n2:b\n' | sort -r -t: -k1,1n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "y:1\nx:2\nx:10\na:2\na:10\nb:2\nax\nbx\nay\na:02\na:2\na:10\nb:2\n10\n2\nA\na\nb\na\nx: b\nx:a\nx:a\nx: b\nx: 2\nx:10\nb:1\na:2\na:1\nx: aZ\nx:bY\n2:b\n10:a\n"
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("printf 'a\nb\n' | sort -k0", &mut ctx)
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn uniq_skips_fields_and_characters_only_for_comparison() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf '1 alpha\\n2 alpha\\n3 beta\\n' | uniq -f 1; printf 'AAdup\\nBBdup\\nCCother\\n' | uniq -s2; printf '1 abdup\\n2 cbdup\\n' | uniq -f1 -s2",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "1 alpha\n3 beta\nAAdup\nCCother\n1 abdup\n"
    );

    ctx.enable_capture();
    assert_eq!(engine.eval_str("uniq -f 0", &mut ctx).await.unwrap(), 2);
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("invalid field count '0'")
    );

    ctx.enable_capture();
    assert_eq!(engine.eval_str("uniq -cd", &mut ctx).await.unwrap(), 2);
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("choose only one of -c, -d, or -u")
    );
}

#[tokio::test]
async fn uniq_saturates_large_skip_counts_at_the_end_of_each_line() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let maximum = usize::MAX;
    let script = format!(
        "printf 'a:x\\nb:y\\nc:z\\n' | uniq -f {maximum}; printf 'a:x\\nb:y\\nc:z\\n' | uniq -s {maximum}; printf 'a:x\\nb:y\\nc:z\\n' | uniq -f 1 -s {maximum}"
    );

    assert_eq!(engine.eval_str(&script, &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "a:x\na:x\na:x\n");
}

#[tokio::test]
async fn uniq_supports_posix_input_and_output_operands() {
    let engine = Engine::new();
    let tmp = tempdir().unwrap();
    let input = tmp.path().join("input.txt");
    let output = tmp.path().join("output.txt");
    fs::write(&input, "a\na\nb\n").unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let script = format!("uniq '{}' '{}'", input.display(), output.display());
    assert_eq!(engine.eval_str(&script, &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert_eq!(fs::read_to_string(output).unwrap(), "a\nb\n");

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("printf 'x\\nx\\ny\\n' | uniq -c - -", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "2 x\n1 y\n");

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("uniq first second third", &mut ctx)
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn sort_and_uniq_preserve_carriage_returns_and_unterminated_records() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let code = engine
        .eval_str(
            "printf 'z\\r\\na\\na' | sort -ru; printf 'same\\r\\nsame' | uniq -c",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        ctx.get_captured_stdout_bytes().unwrap(),
        b"z\r\na\n1 same\r\n1 same\n"
    );
}

#[tokio::test]
async fn sort_terminates_each_incomplete_input_file_record_before_the_next_file() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("first"), "z").unwrap();
    fs::write(tmp.path().join("second"), "a\n").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("sort first second", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "a\nz\n");
}

#[tokio::test]
async fn sort_default_fields_keep_the_preceding_blank_separator_in_the_next_key() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str("printf 'y\\tb\\nx a\\n' | sort -k 2,2", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "y\tb\nx a\n");
}

#[tokio::test]
async fn cut_handles_selection_positions_beyond_record_length() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let maximum = usize::MAX;
    let script = format!(
        "printf 'abc\\n' | cut -b 1,{maximum}; printf 'abc\\n' | cut -c {maximum}; printf 'a:b\\n' | cut -d : -f {maximum}"
    );

    assert_eq!(engine.eval_str(&script, &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "a\n\n\n");
}

#[tokio::test]
async fn wc_counts_newline_bytes_unicode_and_stdin_operands() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let code = engine
        .eval_str(
            "printf 'é x\\nlast' | wc -l -w -m -c; printf 'é x\\nlast' | wc -; printf 'é x\\nlast' | wc -lwmc -; printf 'é x\\nlast' | wc -c -m",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "1 3 8 9\n1 3 9\n1 3 8 9\n8 9\n"
    );
}

#[tokio::test]
async fn wc_words_use_locale_independent_unicode_whitespace() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str("printf 'one two three\\n' | wc -w", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "3\n");
}

#[tokio::test]
async fn wc_character_count_uses_lossy_unicode_decoding_for_invalid_utf8() {
    let tmp = tempdir().unwrap();
    let input = tmp.path().join("invalid-utf8.bin");
    fs::write(&input, [0xc3, 0xa9, 0xff, b'A']).unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str(&format!("wc -m -c '{}'", input.display()), &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!("3 4 {}\n", input.display())
    );
}

#[tokio::test]
async fn wc_supports_end_of_options_and_multifile_totals() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("-first"), "one\n").unwrap();
    fs::write(tmp.path().join("second"), "two\nthree\n").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("wc -l -- -first second", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "1 -first\n2 second\n3 total\n"
    );
}

#[tokio::test]
async fn wc_continues_after_unreadable_operands_and_totals_readable_files() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("readable"), "one two\n").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("wc -w missing readable\n", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "2 readable\n2 total\n");
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("cannot read 'missing'")
    );
}

#[tokio::test]
async fn wc_rejects_unsupported_options() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let code = engine.eval_str("wc -z", &mut ctx).await.unwrap();

    assert_eq!(code, 2);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("unsupported option '-z'")
    );
}
