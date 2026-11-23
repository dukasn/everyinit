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
#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::process::{Command, Stdio};

#[tokio::test]
async fn test_for_loop_explicit_words() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        RESULT=""
        for item in "apple" "banana" "cherry"; do
            RESULT="${RESULT}:${item}"
        done
        echo "$RESULT"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert_eq!(output.trim(), ":apple:banana:cherry");
}

#[tokio::test]
async fn empty_function_and_case_commands_are_successful_noops() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "noop() {}\nnoop\nprintf 'function=%s\\n' \"$?\"\ncase value in esac\nprintf 'case=%s\\n' \"$?\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "function=0\ncase=0\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn select_reads_choices_repeats_for_invalid_input_and_preserves_remaining_stdin() {
    let script = "PS3='choose> '\nCOUNT=0\nselect choice in alpha beta; do\n  COUNT=$((COUNT + 1))\n  printf '<%s>:<%s>\\n' \"$choice\" \"$REPLY\"\n  if test \"$COUNT\" -eq 4; then break; fi\ndone\nread AFTER\nprintf 'after=<%s>\\n' \"$AFTER\"\n";
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"2\ninvalid\n  +2 \n1\nremaining\n".to_vec());

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("select execution should succeed");

    assert_eq!(status, 0);
    let expected_stdout =
        b"<beta>:<2>\n<>:<invalid>\n<beta>:<  +2 >\n<alpha>:<1>\nafter=<remaining>\n";
    let expected_stderr = b"1) alpha\n2) beta\nchoose> choose> choose> choose> ";
    assert_eq!(
        ctx.get_captured_stdout().unwrap().as_bytes(),
        expected_stdout
    );
    assert_eq!(
        ctx.get_captured_stderr().unwrap().as_bytes(),
        expected_stderr
    );

    #[cfg(unix)]
    if let Ok(mut reference) = Command::new("bash")
        .args(["--posix", "-c"])
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        reference
            .stdin
            .take()
            .unwrap()
            .write_all(b"2\ninvalid\n  +2 \n1\nremaining\n")
            .unwrap();
        let reference = reference.wait_with_output().unwrap();
        assert!(reference.status.success(), "Bash select loop failed");
        assert_eq!(reference.stdout, expected_stdout);
        assert_eq!(reference.stderr, expected_stderr);
    }
}

#[tokio::test]
async fn select_without_an_in_clause_uses_positional_parameters() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"2\n".to_vec());

    let status = engine
        .eval_str(
            "set -- first second\nselect choice\ndo printf '%s\\n' \"$choice\"; break; done\n",
            &mut ctx,
        )
        .await
        .expect("select with positional parameters should succeed");

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "second\n");
}

#[tokio::test]
async fn select_without_positionals_skips_the_menu_and_preserves_stdin() {
    let script = "set --\nPS3='choose> '\nselect choice; do\n  printf 'unexpected-body:<%s>\\n' \"$choice\"\ndone\nread AFTER\nprintf 'after=<%s>\\n' \"$AFTER\"\n";
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"1\n".to_vec());

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "after=<1>\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    #[cfg(unix)]
    {
        let mut reference = Command::new("bash")
            .args(["--posix", "-c", script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Bash should be available for the empty-select comparison");
        reference.stdin.take().unwrap().write_all(b"1\n").unwrap();
        let reference = reference.wait_with_output().unwrap();
        assert!(reference.status.success());
        assert_eq!(reference.stdout, b"after=<1>\n");
        assert_eq!(reference.stderr, b"");
    }
}

#[tokio::test]
async fn select_with_an_empty_in_list_does_not_fall_back_to_positionals() {
    let script = "set -- positional\nPS3='choose> '\nselect choice in; do\n  printf 'unexpected-body:<%s>\\n' \"$choice\"\ndone\nread AFTER\nprintf 'after=<%s>\\n' \"$AFTER\"\n";
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"1\n".to_vec());

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    let expected_stdout = b"after=<1>\n";
    assert_eq!(
        ctx.get_captured_stdout().unwrap().as_bytes(),
        expected_stdout
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    #[cfg(unix)]
    {
        use std::io::Write;

        if let Ok(mut child) = std::process::Command::new("bash")
            .args(["--posix", "-c"])
            .arg(script)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            child.stdin.take().unwrap().write_all(b"1\n").unwrap();
            let reference = child.wait_with_output().unwrap();
            assert!(
                reference.status.success(),
                "Bash failed to run empty select list"
            );
            assert_eq!(reference.stdout, expected_stdout);
            assert_eq!(reference.stderr, b"");
        }
    }
}

#[tokio::test]
async fn select_ends_cleanly_at_eof_without_running_its_body() {
    let script = "select choice in only; do echo unreachable; done\nprintf 'status=%s\\n' \"$?\"\necho after\n";
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(Vec::new());

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("select EOF should end the loop normally");

    assert_eq!(status, 0);
    let expected_stdout = b"\nstatus=1\nafter\n";
    let expected_stderr = b"1) only\n#? ";
    assert_eq!(
        ctx.get_captured_stdout().unwrap().as_bytes(),
        expected_stdout
    );
    assert_eq!(
        ctx.get_captured_stderr().unwrap().as_bytes(),
        expected_stderr
    );

    #[cfg(unix)]
    if let Ok(reference) = Command::new("bash")
        .args(["--posix", "-c"])
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
    {
        assert!(reference.status.success(), "Bash select EOF loop failed");
        assert_eq!(reference.stdout, expected_stdout);
        assert_eq!(reference.stderr, expected_stderr);
    }

    let after_selection_script = "select choice in only; do printf 'picked=%s\\n' \"$choice\"; done\nprintf 'status=%s\\n' \"$?\"\n";
    let mut after_selection_ctx = ExecutionContext::new();
    after_selection_ctx.enable_capture();
    after_selection_ctx.set_stdin(b"1\n".to_vec());
    assert_eq!(
        engine
            .eval_str(after_selection_script, &mut after_selection_ctx)
            .await
            .unwrap(),
        0
    );
    let after_selection_stdout = b"picked=only\n\nstatus=1\n";
    let after_selection_stderr = b"1) only\n#? #? ";
    assert_eq!(
        after_selection_ctx
            .get_captured_stdout()
            .unwrap()
            .as_bytes(),
        after_selection_stdout
    );
    assert_eq!(
        after_selection_ctx
            .get_captured_stderr()
            .unwrap()
            .as_bytes(),
        after_selection_stderr
    );

    #[cfg(unix)]
    if let Ok(mut reference) = Command::new("bash")
        .args(["--posix", "-c"])
        .arg(after_selection_script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        reference.stdin.take().unwrap().write_all(b"1\n").unwrap();
        let reference = reference.wait_with_output().unwrap();
        assert!(reference.status.success(), "Bash select after input failed");
        assert_eq!(reference.stdout, after_selection_stdout);
        assert_eq!(reference.stderr, after_selection_stderr);
    }
}

#[tokio::test]
async fn select_continue_and_multilevel_break_follow_loop_control_rules() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"1\n2\n1\n".to_vec());

    let status = engine
        .eval_str(
            "COUNT=0\nfor outer in first second; do\n  select choice in a b; do\n    COUNT=$((COUNT + 1))\n    if test \"$COUNT\" -eq 1; then continue; fi\n    printf '%s:%s\\n' \"$outer\" \"$choice\"\n    break 2\n  done\ndone\necho after\n",
            &mut ctx,
        )
        .await
        .expect("select should honor continue and multilevel break");

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "first:b\nafter\n");
}

#[tokio::test]
async fn select_reexpands_ps3_for_each_prompt() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"1\n1\n".to_vec());

    let status = engine
        .eval_str(
            "PS3=first\nCOUNT=0\nselect choice in only; do\n  COUNT=$((COUNT + 1))\n  if test \"$COUNT\" -eq 1; then PS3=second; continue; fi\n  break\ndone\n",
            &mut ctx,
        )
        .await
        .expect("select should read the current PS3 value for each prompt");

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "1) only\nfirstsecond");
}

#[tokio::test]
async fn select_loop_variable_named_reply_receives_the_selected_item() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"2\n".to_vec());

    let status = engine
        .eval_str(
            "select REPLY in alpha beta; do printf '%s\\n' \"$REPLY\"; break; done\n",
            &mut ctx,
        )
        .await
        .expect("select should allow REPLY as its loop variable");

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "beta\n");
}

#[tokio::test]
async fn for_loop_accepts_reserved_words_as_contextual_variable_names() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let keywords = [
        "if", "then", "elif", "else", "fi", "for", "in", "while", "until", "do", "done", "case",
        "esac",
    ];
    let mut script = String::new();
    let mut expected = String::new();
    for keyword in keywords {
        script.push_str(&format!(
            "for {keyword} in value; do printf '{keyword}=<%s>\\n' \"${{{keyword}}}\"; done\n"
        ));
        expected.push_str(&format!("{keyword}=<value>\n"));
    }
    let status = engine.eval_str(&script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), expected);
}

#[tokio::test]
async fn test_for_loop_splits_standalone_command_substitution_by_line() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let code = engine
        .eval_str(
            r#"
                RESULT=""
                for item in $(printf 'alpha\nbeta\ngamma\n'); do
                    RESULT="${RESULT}:${item}"
                done
                echo "$RESULT"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap().trim(),
        ":alpha:beta:gamma"
    );
}

#[tokio::test]
async fn for_loop_line_lists_preserve_spaces_and_keep_quoted_or_scalar_values_whole() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let code = engine
        .eval_str(
            r#"
                RESULT=""
                for item in $(printf 'alpha beta\nsecond\n'); do
                    RESULT="${RESULT}|${item}"
                done
                printf 'lines=<%s>\n' "$RESULT"

                RESULT=""
                for item in "$(printf 'left\nright\n')"; do
                    RESULT="${RESULT}|${item}"
                done
                printf 'quoted=<%s>\n' "$RESULT"

                VALUE=$(printf 'left\nright\n')
                RESULT=""
                for item in $VALUE; do
                    RESULT="${RESULT}|${item}"
                done
                printf 'scalar=<%s>\n' "$RESULT"

                RESULT=""
                for item in $(printf 'first\n\nlast\n'); do
                    RESULT="${RESULT}|${item}"
                done
                printf 'blank-line=<%s>\n' "$RESULT"

                RESULT=none
                for item in $(printf ''); do
                    RESULT=unexpected
                done
                printf 'empty-output=<%s>\n' "$RESULT"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "lines=<|alpha beta|second>\nquoted=<|left\nright>\nscalar=<|left\nright>\nblank-line=<|first||last>\nempty-output=<none>\n"
    );
}

#[tokio::test]
async fn test_for_loop_expands_array_values_as_individual_items() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let code = engine
        .eval_str(
            r#"
                FRUIT=(apple banana cherry)
                RESULT=""
                for fruit in "${FRUIT[@]}"; do
                    RESULT="${RESULT}:${fruit}"
                done
                echo "$RESULT"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap().trim(),
        ":apple:banana:cherry"
    );
}

#[tokio::test]
async fn for_loop_over_sparse_array_values_keeps_empty_and_space_boundaries() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                declare -a ITEMS
                ITEMS[0]="two words"
                ITEMS[2]=""
                ITEMS[5]="*.txt"
                TRACE=
                for item in "${ITEMS[@]}"; do
                    TRACE="${TRACE}[${item}]"
                done
                INDEXES=
                for index in "${!ITEMS[@]}"; do
                    INDEXES="${INDEXES}${index},"
                done
                printf 'values=%s count=%s indexes=%s\n' "$TRACE" "${#ITEMS[@]}" "$INDEXES"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "values=[two words][][*.txt] count=3 indexes=0,2,5,\n"
    );
}

#[tokio::test]
async fn test_for_loop_default_positional_params() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        process_all() {
            local OUT=""
            for arg; do
                OUT="${OUT}_${arg}"
            done
            echo "$OUT"
        }
        process_all "one" "two" "three"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert_eq!(output.trim(), "_one_two_three");
}

#[tokio::test]
async fn test_for_loop_break_and_continue() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        LOG=""
        for i in "1" "2" "3" "4" "5"; do
            [ "$i" = "2" ] && continue
            [ "$i" = "4" ] && break
            LOG="${LOG}${i}"
        done
        echo "$LOG"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert_eq!(output.trim(), "13");
}

#[tokio::test]
async fn loop_control_clamps_excess_levels_and_rejects_invalid_levels() {
    let engine = Engine::new();

    let mut break_ctx = ExecutionContext::new();
    break_ctx.enable_capture();
    let break_status = engine
        .eval_str(
            "for outer in A B; do for inner in x y; do break 99; done; echo wrong; done; echo after-break\n",
            &mut break_ctx,
        )
        .await
        .unwrap();
    assert_eq!(break_status, 0);
    assert_eq!(break_ctx.get_captured_stdout().unwrap(), "after-break\n");

    let mut continue_ctx = ExecutionContext::new();
    continue_ctx.enable_capture();
    let continue_status = engine
        .eval_str(
            "TRACE=; for outer in A B; do for inner in x y; do TRACE=\"${TRACE}${outer}${inner}\"; continue 99; done; done; echo \"$TRACE\"; echo after-continue\n",
            &mut continue_ctx,
        )
        .await
        .unwrap();
    assert_eq!(continue_status, 0);
    assert_eq!(
        continue_ctx.get_captured_stdout().unwrap(),
        "AxBx\nafter-continue\n"
    );

    for script in [
        "for item in x; do break nope; done",
        "for item in x; do continue 0; done",
        "for item in x; do break 1 2; done",
    ] {
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();
        assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 2);
        assert_ne!(ctx.get_captured_stderr().unwrap(), "");
    }
}

#[tokio::test]
async fn loop_control_outside_a_loop_returns_status_two() {
    let engine = Engine::new();

    for (script, expected) in [
        (
            "break",
            "only meaningful in a `for', `while', or `until' loop",
        ),
        (
            "continue",
            "only meaningful in a `for', `while', or `until' loop",
        ),
    ] {
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();
        assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 2);
        assert!(
            ctx.get_captured_stderr().unwrap().contains(expected),
            "{script}"
        );
    }
}

#[tokio::test]
async fn if_elif_and_else_choose_one_branch_and_preserve_its_status() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                if false; then
                    echo wrong-if
                elif true; then
                    echo elif
                    false
                else
                    echo wrong-else
                fi
                printf 'elif-status=%s\n' "$?"
                if false; then
                    echo wrong-second-if
                else
                    echo else
                fi
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "elif\nelif-status=1\nelse\n"
    );
}

#[tokio::test]
async fn if_conditions_run_lists_left_to_right_and_use_the_final_status() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                TRACE=
                if TRACE="$TRACE-a"; false; then
                    echo wrong-first
                elif TRACE="$TRACE-b"; true; false; then
                    echo wrong-elif
                elif TRACE="$TRACE-c"; true; then
                    echo selected
                else
                    echo wrong-else
                fi
                printf 'trace=%s status=%s\n' "$TRACE" "$?"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "selected\ntrace=-a-b-c status=0\n"
    );
}

#[tokio::test]
async fn and_or_lists_short_circuit_left_to_right_and_preserve_status() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                TRACE=
                false && TRACE="${TRACE}a" || TRACE="${TRACE}b" && TRACE="${TRACE}c"
                printf 'chain=%s status=%s\n' "$TRACE" "$?"

                TRACE=
                true || TRACE="${TRACE}wrong-or"
                false && TRACE="${TRACE}wrong-and"
                printf 'skipped=<%s> status=%s\n' "$TRACE" "$?"

                false || false || TRACE=fallback
                printf 'fallback=%s status=%s\n' "$TRACE" "$?"

                true && false || true && false
                printf 'final-status=%s\n' "$?"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "chain=bc status=0\nskipped=<> status=1\nfallback=fallback status=0\nfinal-status=1\n"
    );
}

#[tokio::test]
async fn parallel_joins_bind_inside_and_or_operands() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                false && printf 'wrong-and-left\n' &| printf 'wrong-and-right\n'
                printf 'and-skip-status=%s\n' "$?"

                true || printf 'wrong-or-left\n' &| printf 'wrong-or-right\n'
                printf 'or-skip-status=%s\n' "$?"

                true && printf 'and-left\n' &| printf 'and-right\n'
                printf 'and-join-status=%s\n' "$?"

                false || printf 'or-left\n' &| printf 'or-right\n'
                printf 'or-join-status=%s\n' "$?"

                TRACE=
                true || printf 'wrong-or-chain-left\n' &| printf 'wrong-or-chain-right\n' && TRACE=and-after-skipped-join
                printf 'or-chain=%s:%s\n' "$TRACE" "$?"

                TRACE=
                false && printf 'wrong-and-chain-left\n' &| printf 'wrong-and-chain-right\n' || TRACE=or-after-skipped-join
                printf 'and-chain=%s:%s\n' "$TRACE" "$?"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "and-skip-status=1\nor-skip-status=0\nand-left\nand-right\nand-join-status=0\nor-left\nor-right\nor-join-status=0\nor-chain=and-after-skipped-join:0\nand-chain=or-after-skipped-join:0\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn loop_control_propagates_through_only_executed_and_or_branches() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                TRACE=
                for item in first second third fourth; do
                    if [ "$item" = first ]; then
                        true || break
                        TRACE="${TRACE}1"
                    elif [ "$item" = second ]; then
                        false && break
                        TRACE="${TRACE}2"
                    elif [ "$item" = third ]; then
                        false || continue
                        TRACE=wrong-after-continue
                    else
                        true && break
                        TRACE=wrong-after-break
                    fi
                    TRACE="${TRACE},"
                done
                printf 'trace=%s\n' "$TRACE"

                TRACE=
                skip_first() { true || continue; TRACE="${TRACE}a"; }
                skip_second() { false || continue; TRACE=wrong-after-function-continue; }
                for item in first second third; do
                    if [ "$item" = first ]; then
                        skip_first
                    elif [ "$item" = second ]; then
                        skip_second
                    else
                        TRACE="${TRACE}c"
                        break
                    fi
                    TRACE="${TRACE}x"
                done
                printf 'function-trace=%s\n' "$TRACE"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "trace=1,2,\nfunction-trace=axc\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
    assert!(!ctx.in_loop());
}

#[tokio::test]
async fn fail_fast_fallback_does_not_intercept_exit_or_function_return() {
    let engine = Engine::new();

    let mut exit_ctx = ExecutionContext::new();
    exit_ctx.enable_capture();
    let exit_status = engine
        .eval_str(
            "{ printf 'before-exit\\n'; exit 23; } || { printf 'wrong-exit-fallback\\n'; }; printf 'wrong-after-exit\\n'\n",
            &mut exit_ctx,
        )
        .await
        .unwrap();
    assert_eq!(exit_status, 23);
    assert_eq!(exit_ctx.get_captured_stdout().unwrap(), "before-exit\n");
    assert!(exit_ctx.is_exiting());

    let mut return_ctx = ExecutionContext::new();
    return_ctx.enable_capture();
    let return_status = engine
        .eval_str(
            "finish() { { printf 'before-return\\n'; return 17; } || { printf 'wrong-return-fallback\\n'; }; printf 'wrong-after-return\\n'; }; finish; printf 'after-function:%s\\n' \"$?\"\n",
            &mut return_ctx,
        )
        .await
        .unwrap();
    assert_eq!(return_status, 0);
    assert_eq!(
        return_ctx.get_captured_stdout().unwrap(),
        "before-return\nafter-function:17\n"
    );
    assert!(!return_ctx.is_returning());
}

#[tokio::test]
async fn exit_and_return_in_conditions_stop_before_the_branch_or_loop_body() {
    let engine = Engine::new();

    let mut exit_ctx = ExecutionContext::new();
    exit_ctx.enable_capture();
    let exit_status = engine
        .eval_str(
            "if exit 19; then printf 'wrong-then\\n'; else printf 'wrong-else\\n'; fi; printf 'wrong-after-if\\n'\n",
            &mut exit_ctx,
        )
        .await
        .unwrap();
    assert_eq!(exit_status, 19);
    assert_eq!(exit_ctx.get_captured_stdout().unwrap(), "");
    assert!(exit_ctx.is_exiting());

    let mut return_ctx = ExecutionContext::new();
    return_ctx.enable_capture();
    let status = engine
        .eval_str(
            "finish() { while return 23; do printf 'wrong-body\\n'; done; printf 'wrong-after-loop\\n'; }; finish; printf 'after-function:%s\\n' \"$?\"\n",
            &mut return_ctx,
        )
        .await
        .unwrap();
    assert_eq!(status, 0);
    assert_eq!(
        return_ctx.get_captured_stdout().unwrap(),
        "after-function:23\n"
    );
    assert!(!return_ctx.is_returning());
}

#[tokio::test]
async fn negated_exit_keeps_the_requested_status_in_shells_and_subshells() {
    let engine = Engine::new();

    let mut shell_ctx = ExecutionContext::new();
    shell_ctx.enable_capture();
    let shell_status = engine
        .eval_str(
            "while ! exit 7; do printf 'unreachable-body\\n'; done\nprintf 'unreachable-tail\\n'\n",
            &mut shell_ctx,
        )
        .await
        .unwrap();
    assert_eq!(shell_status, 7);
    assert_eq!(shell_ctx.get_captured_stdout().unwrap(), "");

    let mut subshell_ctx = ExecutionContext::new();
    subshell_ctx.enable_capture();
    let subshell_status = engine
        .eval_str(
            "( ! exit 9; printf 'unreachable\\n' )\nprintf 'subshell=%s\\n' \"$?\"\n",
            &mut subshell_ctx,
        )
        .await
        .unwrap();
    assert_eq!(subshell_status, 0);
    assert_eq!(subshell_ctx.get_captured_stdout().unwrap(), "subshell=9\n");
}

#[tokio::test]
async fn negated_return_keeps_its_requested_function_status() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "finish() { while ! return 11; do printf 'unreachable-body\\n'; done; printf 'unreachable-tail\\n'; }\nfinish\nprintf 'function=%s\\n' \"$?\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "function=11\n");
}

#[tokio::test]
async fn isolated_pipeline_and_parallel_stages_report_a_pending_exit_status() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "true | ( ! exit 7 )\nprintf 'pipeline=%s\\n' \"$?\"\n! exit 9 &| true\nprintf 'parallel=%s\\n' \"$?\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "pipeline=7\nparallel=9\n"
    );
}

#[tokio::test]
async fn loop_frames_are_unwound_when_expansion_conditions_or_bodies_fail() {
    let engine = Engine::new();

    for script in [
        "set -u\nfor item in \"${MISSING:?required}\"; do :; done\n",
        "set -u\nwhile printf '%s' \"${MISSING:?required}\"; do :; done\n",
        "set -u\nwhile true; do printf '%s' \"${MISSING:?required}\"; done\n",
    ] {
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();
        assert!(engine.eval_str(script, &mut ctx).await.is_err(), "{script}");
        assert!(!ctx.in_loop(), "loop scope leaked after {script}");
        assert_eq!(engine.eval_str("break", &mut ctx).await.unwrap(), 2);
        assert!(!ctx.in_loop(), "break changed loop scope after {script}");
    }
}

#[tokio::test]
async fn runtime_errors_unwind_nested_function_and_loop_frames() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let error = engine
        .eval_str(
            "set -u\nVALUE=outer\nfail_inside_loop() { local VALUE=inner; for item in first; do printf '%s' \"${MISSING:?required}\"; done; }\nfail_inside_loop\n",
            &mut ctx,
        )
        .await
        .expect_err("an unset required value should fail during the function loop");

    assert!(error.to_string().contains("required"));
    assert!(!ctx.in_loop(), "loop frame leaked after runtime error");
    assert!(
        !ctx.in_function(),
        "function frame leaked after runtime error"
    );
    assert!(
        !ctx.is_returning(),
        "return state leaked after runtime error"
    );
    assert!(!ctx.is_exiting(), "exit state leaked after runtime error");
    assert_eq!(
        ctx.get_var("VALUE").as_deref(),
        Some("outer"),
        "local shadow leaked after runtime error"
    );

    let recovered = engine
        .eval_str("printf 'recovered:%s:%s\\n' \"$?\" \"$VALUE\"\n", &mut ctx)
        .await
        .expect("the same execution context should remain usable after the error");
    assert_eq!(recovered, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "recovered:1:outer\n");
}

#[tokio::test]
async fn loop_control_in_functions_targets_the_callers_active_loops() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                stop_outer() { echo stop:$1; break; echo wrong-stop; }
                for item in first second; do
                    stop_outer "$item"
                    echo wrong-after-stop
                done
                TRACE=
                skip_two() { continue 2; echo wrong-skip; }
                for outer in A B; do
                    for inner in 1 2; do
                        TRACE="${TRACE}${outer}${inner}"
                        skip_two
                    done
                    TRACE="${TRACE}x"
                done
                printf 'trace=%s\n' "$TRACE"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "stop:first\ntrace=A1B1\n"
    );
    assert!(!ctx.in_loop());
}

#[tokio::test]
async fn return_is_function_scoped_and_exit_statuses_are_validated() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str("return 7", &mut ctx).await.unwrap(), 2);
    assert!(!ctx.is_returning());

    let status = engine
        .eval_str(
            "finish() { echo before; return 7; echo wrong; }; finish; echo after:$?\n",
            &mut ctx,
        )
        .await
        .unwrap();
    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "before\nafter:7\n");

    for script in [
        "exit nope",
        "exit 1 2",
        "exit 256",
        "exit -1",
        "finish() { return nope; }; finish",
        "finish() { return 256; }; finish",
        "finish() { return -1; }; finish",
    ] {
        let mut bad_ctx = ExecutionContext::new();
        bad_ctx.enable_capture();
        assert_eq!(engine.eval_str(script, &mut bad_ctx).await.unwrap(), 2);
        assert!(!bad_ctx.is_exiting());
        assert!(!bad_ctx.is_returning());
    }

    let mut range_ctx = ExecutionContext::new();
    range_ctx.enable_capture();
    let status = engine
        .eval_str(
            "exit 256\necho after-exit:$?\nfinish() { return -1; echo after-return:$?; }; finish\n",
            &mut range_ctx,
        )
        .await
        .unwrap();
    assert_eq!(status, 0);
    assert_eq!(
        range_ctx.get_captured_stdout().unwrap(),
        "after-exit:2\nafter-return:2\n"
    );
    assert_eq!(
        range_ctx.get_captured_stderr().unwrap(),
        "esh: exit: status must be between 0 and 255\nesh: return: status must be between 0 and 255\n"
    );

    let mut boundary_ctx = ExecutionContext::new();
    assert_eq!(
        engine
            .eval_str("exit 255", &mut boundary_ctx)
            .await
            .unwrap(),
        255
    );
    assert!(boundary_ctx.is_exiting());

    let mut return_boundary_ctx = ExecutionContext::new();
    assert_eq!(
        engine
            .eval_str("finish() { return 255; }; finish", &mut return_boundary_ctx)
            .await
            .unwrap(),
        255
    );
}

#[tokio::test]
async fn test_while_and_until_loops() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        # while loop counting up to 3
        COUNT=0
        WHILE_OUT=""
        while [ "$COUNT" -lt 3 ]; do
            COUNT=$(( COUNT + 1 ))
            WHILE_OUT="${WHILE_OUT}${COUNT}"
        done

        # until loop counting from 3 down to 0
        UNTIL_OUT=""
        until [ "$COUNT" -eq 0 ]; do
            UNTIL_OUT="${UNTIL_OUT}${COUNT}"
            COUNT=$(( COUNT - 1 ))
        done

        echo "WHILE=$WHILE_OUT UNTIL=$UNTIL_OUT"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert_eq!(output.trim(), "WHILE=123 UNTIL=321");
}

#[tokio::test]
async fn loops_skip_empty_bodies_and_keep_the_last_executed_status() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                TRACE=
                for item in; do TRACE=wrong; done
                while false; do TRACE=wrong; done
                until true; do TRACE=wrong; done
                printf 'skipped=%s status=%s\n' "$TRACE" "$?"

                for item in first last; do
                    printf '%s ' "$item"
                    [ "$item" = first ]
                done
                printf 'for-status=%s\n' "$?"

                COUNT=0
                while [ "$COUNT" -lt 2 ]; do
                    COUNT=$((COUNT + 1))
                    [ "$COUNT" -lt 2 ]
                done
                printf 'while-status=%s count=%s\n' "$?" "$COUNT"

                COUNT=0
                until [ "$COUNT" -eq 2 ]; do
                    COUNT=$((COUNT + 1))
                    [ "$COUNT" -lt 2 ]
                done
                printf 'until-status=%s count=%s\n' "$?" "$COUNT"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "skipped= status=0\nfirst last for-status=1\nwhile-status=1 count=2\nuntil-status=1 count=2\n"
    );
}

#[tokio::test]
async fn empty_array_expansions_leave_the_for_variable_unchanged() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                declare -a EMPTY
                ITEM=preserved
                for ITEM in "${EMPTY[@]}"; do ITEM=wrong; done
                printf 'empty-array=<%s> status=%s\n' "$ITEM" "$?"

                EMPTY[2]=value
                unset 'EMPTY[2]'
                for ITEM in "${EMPTY[@]}"; do ITEM=wrong; done
                printf 'sparse-empty-array=<%s> status=%s\n' "$ITEM" "$?"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "empty-array=<preserved> status=0\nsparse-empty-array=<preserved> status=0\n"
    );
}

#[tokio::test]
async fn test_nested_loops_multi_level_break() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        TRACE=""
        for x in "A" "B" "C"; do
            for y in "1" "2" "3"; do
                TRACE="${TRACE}(${x}${y})"
                [ "$x" = "B" ] && [ "$y" = "2" ] && break 2
            done
        done
        echo "$TRACE"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert_eq!(output.trim(), "(A1)(A2)(A3)(B1)(B2)");
}

#[tokio::test]
async fn loop_control_crosses_if_and_case_bodies_for_while_and_until() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                TRACE=
                OUTER=0
                while [ "$OUTER" -lt 3 ]; do
                    OUTER=$((OUTER + 1))
                    INNER=0
                    while [ "$INNER" -lt 3 ]; do
                        INNER=$((INNER + 1))
                        if [ "$OUTER" -eq 2 ] && [ "$INNER" -eq 2 ]; then
                            case skip in
                                skip) continue 2 ;;
                            esac
                        fi
                        TRACE="${TRACE}${OUTER}${INNER},"
                    done
                done

                UNTIL=0
                until [ "$UNTIL" -ge 5 ]; do
                    UNTIL=$((UNTIL + 1))
                    case "$UNTIL" in
                        2) continue ;;
                        4) break ;;
                    esac
                    TRACE="${TRACE}u${UNTIL},"
                done
                printf 'trace=%s until=%s\n' "$TRACE" "$UNTIL"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "trace=11,12,13,21,31,32,33,u1,u3, until=4\n"
    );
    assert!(
        !ctx.in_loop(),
        "loop depth leaked after nested control transfer"
    );
}

#[tokio::test]
async fn break_and_continue_status_and_flags_do_not_leak_after_loop_completion() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                for item in first second; do
                    continue
                    echo unreachable-continue-tail
                done
                printf 'continue-status=%s\n' "$?"

                for item in first second; do
                    break
                    echo unreachable-break-tail
                done
                printf 'break-status=%s\n' "$?"

                echo after-loop
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "continue-status=0\nbreak-status=0\nafter-loop\n"
    );
    assert!(!ctx.in_loop());
}

#[tokio::test]
async fn negated_break_and_continue_transfer_control_while_inverting_status() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        TRACE=
        for item in first second; do
            ! break
            TRACE="${TRACE}wrong-break-tail"
        done
        printf 'break=<%s> status=%s\n' "$TRACE" "$?"

        TRACE=
        for item in first second; do
            ! continue
            TRACE="${TRACE}wrong-continue-tail"
        done
        printf 'continue=<%s> status=%s\n' "$TRACE" "$?"

        TRACE=
        for outer in first second; do
            for inner in a b; do
                TRACE="${TRACE}${outer}${inner}"
                ! continue 2 || TRACE=wrong-or-branch
                TRACE=wrong-after-continue
            done
            TRACE=wrong-after-inner
        done
        printf 'multilevel=<%s> status=%s\n' "$TRACE" "$?"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("negated loop transfers should unwind their active loops");

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "break=<> status=1\ncontinue=<> status=1\nmultilevel=<firstaseconda> status=1\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
    assert!(!ctx.in_loop());

    #[cfg(unix)]
    if let Ok(reference) = Command::new("bash")
        .args(["--posix", "-c"])
        .arg(script)
        .output()
    {
        assert!(
            reference.status.success(),
            "Bash loop transfer probe failed"
        );
        assert_eq!(
            reference.stdout,
            ctx.get_captured_stdout().unwrap().as_bytes()
        );
        assert_eq!(
            reference.stderr,
            ctx.get_captured_stderr().unwrap().as_bytes()
        );
    }
}

#[tokio::test]
async fn loop_control_in_test_lists_is_handled_before_running_the_body() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                while break; do
                    echo wrong-break-body
                done

                until break; do
                    echo wrong-until-break-body
                done

                COUNT=0
                TRACE=
                while COUNT=$((COUNT + 1));
                    [ "$COUNT" -ne 2 ] || continue
                    [ "$COUNT" -le 4 ]; do
                    TRACE="${TRACE}${COUNT},"
                done
                COUNT_TRACE=$TRACE

                COUNT=0
                TRACE=
                until COUNT=$((COUNT + 1));
                    [ "$COUNT" -ne 2 ] || continue
                    [ "$COUNT" -ge 4 ]; do
                    TRACE="${TRACE}${COUNT},"
                done
                UNTIL_TRACE=$TRACE

                TRACE=
                for ITEM in first second; do
                    if [ "$ITEM" = first ] && continue; then
                        echo wrong-if-body
                    fi
                    TRACE="${TRACE}${ITEM},"
                done
                while true; do
                    if break; then
                        echo wrong-break-if-body
                    fi
                done

                OUTER=0
                NESTED_TRACE=
                while [ "$OUTER" -lt 3 ]; do
                    OUTER=$((OUTER + 1))
                    INNER=0
                    while INNER=$((INNER + 1));
                        [ "$INNER" -ne 2 ] || continue 2
                        [ "$INNER" -le 4 ]; do
                        NESTED_TRACE="${NESTED_TRACE}${OUTER}${INNER},"
                    done
                    echo wrong-outer-tail
                done
                printf 'count=%s trace=%s until-trace=%s if-trace=%s nested-trace=%s\n' "$COUNT" "$COUNT_TRACE" "$UNTIL_TRACE" "$TRACE" "$NESTED_TRACE"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "count=4 trace=1,3,4, until-trace=1,3, if-trace=second, nested-trace=11,21,31,\n"
    );
    assert!(!ctx.in_loop());
}

#[tokio::test]
async fn function_loop_control_in_conditions_reaches_the_enclosing_loop() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                break_from_condition() { break; echo wrong-after-break; }
                continue_from_condition() { continue; echo wrong-after-continue; }

                TRACE=
                for item in first second; do
                    if break_from_condition; then
                        TRACE=wrong-if-then
                    else
                        TRACE=wrong-if-else
                    fi
                    TRACE=wrong-after-if
                done
                TRACE="${TRACE}B"

                for item in first second; do
                    if continue_from_condition; then
                        TRACE=wrong-continue-then
                    else
                        TRACE=wrong-continue-else
                    fi
                    TRACE=wrong-after-continue-if
                done
                TRACE="${TRACE}C"

                while break_from_condition; do
                    TRACE=wrong-while-body
                done
                TRACE="${TRACE}W"

                printf '%s\n' "$TRACE"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "BCW\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
    assert!(!ctx.in_loop());
}

#[tokio::test]
async fn loop_control_does_not_inherit_or_escape_isolated_contexts() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                TRACE=
                for item in first second; do
                    (break)
                    VALUE=$(continue)
                    break | cat
                    continue 2>/dev/null | cat
                    TRACE="${TRACE}${item},"
                done
                printf 'trace=%s\n' "$TRACE"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "trace=first,second,\n");
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        concat!(
            "esh: break: only meaningful in a `for', `while', or `until' loop\n",
            "esh: continue: only meaningful in a `for', `while', or `until' loop\n",
            "esh: break: only meaningful in a `for', `while', or `until' loop\n",
            "esh: break: only meaningful in a `for', `while', or `until' loop\n",
            "esh: continue: only meaningful in a `for', `while', or `until' loop\n",
            "esh: break: only meaningful in a `for', `while', or `until' loop\n",
        )
    );
    assert!(!ctx.in_loop());
}

#[tokio::test]
async fn parallel_branches_do_not_inherit_or_escape_loop_control() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                TRACE=
                for ITEM in first second; do
                    break &| continue &| printf 'joined-branch\n'
                    TRACE="${TRACE}${ITEM},"
                done
                printf 'trace=%s\n' "$TRACE"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "joined-branch\njoined-branch\ntrace=first,second,\n"
    );
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        concat!(
            "esh: break: only meaningful in a `for', `while', or `until' loop\n",
            "esh: continue: only meaningful in a `for', `while', or `until' loop\n",
            "esh: break: only meaningful in a `for', `while', or `until' loop\n",
            "esh: continue: only meaningful in a `for', `while', or `until' loop\n",
        )
    );
    assert!(!ctx.in_loop());
}

#[tokio::test]
async fn isolated_contexts_keep_nested_control_flow_and_exit_local() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                TRACE=
                for outer in first second; do
                    (
                        for inner in a b; do
                            printf '%s%s\n' "$outer" "$inner"
                            break
                            echo wrong-inner-tail
                        done
                        exit 9
                        echo wrong-after-subshell-exit
                    )
                    TRACE="${TRACE}!"
                done

                command_sub_return() {
                    VALUE=$(return 11)
                    printf 'substitution-status=%s value=<%s>\n' "$?" "$VALUE"
                    printf 'function-continued\n'
                }
                command_sub_return
                printf 'trace=%s\n' "$TRACE"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "firsta\nseconda\nsubstitution-status=11 value=<>\nfunction-continued\ntrace=!!\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
    assert!(!ctx.is_exiting());
    assert!(!ctx.is_returning());
    assert!(!ctx.in_loop());
}

#[tokio::test]
async fn test_case_pattern_matching() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        classify() {
            case "$1" in
                *.tar.gz|*.tgz)
                    echo "gzipped-tar"
                    ;;
                *.zip)
                    echo "zip-archive"
                    ;;
                v[0-9].*)
                    echo "semver-release"
                    ;;
                *)
                    echo "unknown"
                    ;;
            esac
        }

        echo "1: $(classify "bundle.tar.gz")"
        echo "2: $(classify "archive.zip")"
        echo "3: $(classify "v2.1.0")"
        echo "4: $(classify "photo.png")"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.trim().lines().collect();
    assert_eq!(
        lines,
        vec![
            "1: gzipped-tar",
            "2: zip-archive",
            "3: semver-release",
            "4: unknown",
        ]
    );
}

#[tokio::test]
async fn case_patterns_treat_leading_dots_and_slashes_as_ordinary_characters() {
    let script = r#"
        case .hidden in
            .*) DOT=matched ;;
            *) DOT=missed ;;
        esac
        case src/deep/file in
            src/*) SLASH=matched ;;
            *) SLASH=missed ;;
        esac
        case src/deep/file in
            src/*/file) NESTED=matched ;;
            *) NESTED=missed ;;
        esac
        printf '%s:%s:%s\n' "$DOT" "$SLASH" "$NESTED"
    "#;
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    let expected = b"matched:matched:matched\n";
    assert_eq!(ctx.get_captured_stdout().unwrap().as_bytes(), expected);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    #[cfg(unix)]
    for (shell, args) in [("bash", vec!["--posix", "-c"]), ("dash", vec!["-c"])] {
        if let Ok(reference) = std::process::Command::new(shell)
            .args(args)
            .arg(script)
            .output()
        {
            assert!(
                reference.status.success(),
                "{shell} failed to match case patterns"
            );
            assert_eq!(reference.stdout, expected, "{shell} case pattern result");
        }
    }
}

#[tokio::test]
async fn case_patterns_support_posix_named_classes_and_negation() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                for item in ABx a2x 42x; do
                    case "$item" in
                        [[:alpha:]][!0-9]?)
                            echo "$item:match"
                            ;;
                        *)
                            echo "$item:no"
                            ;;
                    esac
                done
                case "*" in
                    '*')
                        echo "single-quoted literal"
                        ;;
                    *)
                        echo "single-quoted wildcard"
                        ;;
                esac
                case anything in
                    "*")
                        echo "double-quoted wildcard"
                        ;;
                    *)
                        echo "double-quoted literal"
                        ;;
                esac
                case "*" in
                    \*)
                        echo "escaped literal"
                        ;;
                    *)
                        echo "escaped wildcard"
                        ;;
                esac
                STAR="*"
                case anything in
                    $STAR)
                        echo "unquoted expansion wildcard"
                        ;;
                    *)
                        echo "unquoted expansion literal"
                        ;;
                esac
                case anything in
                    "$STAR")
                        echo "quoted expansion wildcard"
                        ;;
                    *)
                        echo "quoted expansion literal"
                        ;;
                esac
                case a in
                    [^b])
                        echo "caret-negation extension"
                        ;;
                    *)
                        echo "caret-literal behavior"
                        ;;
                esac
                case '^' in
                    [\^a])
                        echo "escaped-caret-literal"
                        ;;
                    *)
                        echo "escaped-caret-missed"
                        ;;
                esac
                VALUE=abc
                echo "caret-removal=${VALUE#[^b]}"
                VALUE='^tail'
                echo "escaped-caret-removal=${VALUE#[\^a]}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "ABx:match\na2x:no\n42x:no\nsingle-quoted literal\ndouble-quoted literal\nescaped literal\nunquoted expansion wildcard\nquoted expansion literal\ncaret-negation extension\nescaped-caret-literal\ncaret-removal=bc\nescaped-caret-removal=tail\n"
    );
}

#[tokio::test]
async fn case_and_removal_patterns_use_ascii_named_classes_and_unicode_ranges() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                case 'é' in
                    [[:alpha:]])
                        echo nonascii-alpha
                        ;;
                    *)
                        echo ascii-class
                        ;;
                esac
                case 'å' in
                    [à-å])
                        echo unicode-range
                        ;;
                    *)
                        echo missed-range
                        ;;
                esac
                VALUE=ätail
                printf '%s\n' "${VALUE#[à-å]}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "ascii-class\nunicode-range\ntail\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn test_case_fallthrough_and_resume() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        TARGET="alpha"
        RESULT=""

        case "$TARGET" in
            alpha)
                RESULT="${RESULT}A"
                ;&
            beta)
                RESULT="${RESULT}B"
                ;;
            *)
                RESULT="${RESULT}C"
                ;;
        esac

        echo "$RESULT"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert_eq!(output.trim(), "AB");
}

#[tokio::test]
async fn loop_control_in_case_arms_stops_fallthrough_and_pattern_resumption() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                TRACE=
                for item in break; do
                    case value in
                        value)
                            TRACE="${TRACE}B"
                            break
                            ;&
                        *)
                            TRACE="${TRACE}x"
                            ;;
                    esac
                    TRACE="${TRACE}y"
                done

                for item in first second; do
                    case value in
                        value)
                            TRACE="${TRACE}C"
                            continue
                            ;;&
                        value)
                            TRACE="${TRACE}x"
                            ;;
                    esac
                    TRACE="${TRACE}y"
                done
                printf '%s\n' "$TRACE"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "BCC\n");
}

#[tokio::test]
async fn case_delimiters_can_follow_commands_on_the_same_line() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "case value in value) printf A;& value) printf B;; esac\ncase value in value) printf C;;& value) printf D;; esac\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "ABCD");
}

#[tokio::test]
async fn esh_case_resume_next_rematches_following_patterns() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                RESULT=
                case alpha in
                    a*)
                        RESULT=prefix
                        ;;&
                    al*)
                        RESULT="$RESULT-matched"
                        ;;&
                    beta)
                        RESULT=incorrect
                        ;;
                    *)
                        RESULT="$RESULT-fallback"
                        ;;
                esac
                printf '%s\n' "$RESULT"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "prefix-matched-fallback\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn case_no_match_and_matched_arm_status_are_observable() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                case unmatched in matched) echo wrong;; esac
                printf 'unmatched=%s\n' "$?"
                case matched in
                    matched)
                        true
                        false
                        ;;
                esac
                printf 'matched=%s\n' "$?"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "unmatched=0\nmatched=1\n"
    );
}

#[tokio::test]
async fn empty_case_arms_obey_break_fallthrough_and_resume_delimiters() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                TRACE=
                case value in
                    value) ;;
                    *) TRACE=wrong-break-fallthrough ;;
                esac
                TRACE="${TRACE}A"

                case value in
                    value) ;&
                    unmatched) TRACE="${TRACE}B" ;;
                esac

                case value in
                    value) ;;&
                    unmatched) TRACE=wrong-resume-match ;;
                    value) TRACE="${TRACE}C" ;;
                esac
                printf '%s\n' "$TRACE"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "ABC\n");
}

#[tokio::test]
async fn return_exits_only_the_current_nested_function_and_skips_loop_tail() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                inner() {
                    for item in first second; do
                        printf 'inner=%s\n' "$item"
                        return 23
                        echo wrong-inner-tail
                    done
                    echo wrong-inner-after-loop
                }
                outer() {
                    inner
                    printf 'outer-status=%s\n' "$?"
                    return 7
                    echo wrong-outer-tail
                }
                outer
                printf 'top-status=%s\n' "$?"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "inner=first\nouter-status=23\ntop-status=7\n"
    );
    assert!(!ctx.is_returning());
    assert!(!ctx.in_loop());
}

#[tokio::test]
async fn return_propagates_through_every_compound_control_flow_body() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"1\n".to_vec());

    let script = r#"
        from_if() {
            if true; then printf 'if\n'; return 11; fi
            printf 'wrong-if-tail\n'
        }
        from_for() {
            for item in one; do printf 'for:%s\n' "$item"; return 12; done
            printf 'wrong-for-tail\n'
        }
        from_while() {
            while true; do printf 'while\n'; return 13; done
            printf 'wrong-while-tail\n'
        }
        from_until() {
            until false; do printf 'until\n'; return 14; done
            printf 'wrong-until-tail\n'
        }
        from_case() {
            case value in value) printf 'case\n'; return 15 ;; esac
            printf 'wrong-case-tail\n'
        }
        from_select() {
            select item in one; do printf 'select:%s\n' "$item"; return 16; done
            printf 'wrong-select-tail\n'
        }

        from_if; printf 'if-status=%s\n' "$?"
        from_for; printf 'for-status=%s\n' "$?"
        from_while; printf 'while-status=%s\n' "$?"
        from_until; printf 'until-status=%s\n' "$?"
        from_case; printf 'case-status=%s\n' "$?"
        from_select; printf 'select-status=%s\n' "$?"
    "#;

    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "if\nif-status=11\nfor:one\nfor-status=12\nwhile\nwhile-status=13\nuntil\nuntil-status=14\ncase\ncase-status=15\nselect:one\nselect-status=16\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "1) one\n#? ");
    assert!(!ctx.is_returning());
    assert!(!ctx.in_loop());

    #[cfg(unix)]
    if let Ok(mut reference) = Command::new("bash")
        .args(["--posix", "-c"])
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        reference.stdin.take().unwrap().write_all(b"1\n").unwrap();
        let reference = reference.wait_with_output().unwrap();
        assert!(
            reference.status.success(),
            "Bash control flow matrix failed"
        );
        assert_eq!(
            reference.stdout,
            ctx.get_captured_stdout().unwrap().as_bytes()
        );
        assert_eq!(
            reference.stderr,
            ctx.get_captured_stderr().unwrap().as_bytes()
        );
    }
}

#[tokio::test]
async fn shell_control_transfers_propagate_through_brace_groups() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        TRACE=
        for item in first second; do
            {
                printf 'item=%s\n' "$item"
                if [ "$item" = first ]; then continue; fi
                break
                printf 'wrong-group-tail\n'
            }
            printf 'wrong-loop-tail\n'
        done
        printf 'loop-status=%s\n' "$?"

        finish() {
            {
                printf 'return-before\n'
                return 17
                printf 'wrong-return-group-tail\n'
            }
            printf 'wrong-return-function-tail\n'
        }
        finish
        printf 'return-status=%s\n' "$?"

        {
            printf 'exit-before\n'
            exit 23
            printf 'wrong-exit-group-tail\n'
        }
        printf 'wrong-after-exit\n'
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("control transfers should propagate through brace groups");

    assert_eq!(status, 23);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "item=first\nitem=second\nloop-status=0\nreturn-before\nreturn-status=17\nexit-before\n"
    );
    assert!(ctx.is_exiting());
    assert!(!ctx.is_returning());
    assert!(!ctx.in_loop());

    #[cfg(unix)]
    if let Ok(reference) = Command::new("bash")
        .args(["--posix", "-c"])
        .arg(script)
        .output()
    {
        assert_eq!(reference.status.code(), Some(23));
        assert_eq!(
            reference.stdout,
            ctx.get_captured_stdout().unwrap().as_bytes()
        );
        assert_eq!(
            reference.stderr,
            ctx.get_captured_stderr().unwrap().as_bytes()
        );
    }
}

#[tokio::test]
async fn exit_propagates_through_every_compound_control_flow_body() {
    let cases = [
        (
            "if",
            "if true; then printf 'if\\n'; exit 21; fi\nprintf 'wrong-tail\\n'\n",
            21,
            b"if\n".as_slice(),
        ),
        (
            "for",
            "for item in one; do printf 'for:%s\\n' \"$item\"; exit 22; done\nprintf 'wrong-tail\\n'\n",
            22,
            b"for:one\n".as_slice(),
        ),
        (
            "while",
            "while true; do printf 'while\\n'; exit 23; done\nprintf 'wrong-tail\\n'\n",
            23,
            b"while\n".as_slice(),
        ),
        (
            "until",
            "until false; do printf 'until\\n'; exit 24; done\nprintf 'wrong-tail\\n'\n",
            24,
            b"until\n".as_slice(),
        ),
        (
            "case",
            "case value in value) printf 'case\\n'; exit 25 ;; esac\nprintf 'wrong-tail\\n'\n",
            25,
            b"case\n".as_slice(),
        ),
        (
            "select",
            "select item in one; do printf 'select:%s\\n' \"$item\"; exit 26; done\nprintf 'wrong-tail\\n'\n",
            26,
            b"select:one\n".as_slice(),
        ),
    ];

    for (construct, script, expected_status, expected_stdout) in cases {
        let engine = Engine::new();
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();
        ctx.set_stdin(b"1\n".to_vec());

        let status = engine
            .eval_str(script, &mut ctx)
            .await
            .unwrap_or_else(|error| panic!("ESH {construct} exit failed: {error}"));

        assert_eq!(status, expected_status, "ESH {construct} exit status");
        assert_eq!(
            ctx.get_captured_stdout().unwrap().as_bytes(),
            expected_stdout,
            "ESH {construct} output"
        );
        assert!(ctx.is_exiting(), "ESH {construct} did not set exit state");

        #[cfg(unix)]
        if let Ok(mut reference) = Command::new("bash")
            .args(["--posix", "-c"])
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            reference.stdin.take().unwrap().write_all(b"1\n").unwrap();
            let reference = reference.wait_with_output().unwrap();
            assert_eq!(
                reference.status.code(),
                Some(expected_status),
                "Bash {construct} exit status"
            );
            assert_eq!(reference.stdout, expected_stdout, "Bash {construct} output");
            assert_eq!(
                reference.stderr,
                ctx.get_captured_stderr().unwrap().as_bytes(),
                "Bash {construct} diagnostics"
            );
        }
    }
}

#[tokio::test]
async fn esh_negated_pipeline_allows_a_line_break_after_bang() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "!\nprintf 'pipeline-output\\n' |\ncat\nprintf 'negated-status=%s\\n' \"$?\"\n",
            &mut ctx,
        )
        .await
        .expect("newline after negation should continue the pipeline");

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "pipeline-output\nnegated-status=1\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}
