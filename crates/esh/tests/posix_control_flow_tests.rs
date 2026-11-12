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
async fn esh_negated_pipeline_allows_a_line_break_after_bang() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine.eval_str("!\nfalse\n", &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}
