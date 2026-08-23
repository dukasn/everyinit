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
