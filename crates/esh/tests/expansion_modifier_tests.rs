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

#[tokio::test]
async fn test_expansion_default_modifier() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        echo "port: ${PORT:-8080}"
        PORT=3000
        echo "port set: ${PORT:-8080}"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "port: 8080");
    assert_eq!(lines[1], "port set: 3000");
}

#[tokio::test]
async fn test_expansion_assign_modifier() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        echo "target: ${TARGET:=./build}"
        echo "target after assign: $TARGET"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "target: ./build");
    assert_eq!(lines[1], "target after assign: ./build");
}

#[tokio::test]
async fn test_expansion_length_modifier() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        MESSAGE="hello world"
        echo "len: ${#MESSAGE}"
        UNICODE="雪"
        echo "unicode len: ${#UNICODE}"
        UNICODE_ARRAY=("雪")
        echo "array element len: ${#UNICODE_ARRAY[0]}"
        set -- "one" "two" "three"
        echo "args len: ${#@}"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "len: 11");
    assert_eq!(lines[1], "unicode len: 1");
    assert_eq!(lines[2], "array element len: 1");
    assert_eq!(lines[3], "args len: 3");
}

#[tokio::test]
async fn parameter_slices_clamp_lengths_without_integer_overflow() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let script = format!(
        "VALUE=abcd\nARRAY=(abcd)\nprintf '%s|%s\\n' \"${{VALUE:1:{}}}\" \"${{ARRAY[0]:1:{}}}\"\n",
        usize::MAX,
        usize::MAX
    );

    let status = engine.eval_str(&script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "bcd|bcd\n");
}

#[tokio::test]
async fn parameter_substring_replacement_uses_literal_first_or_all_matches() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "VALUE=cat.cat\nprintf '%s|%s|%s\\n' \"${VALUE/./X}\" \"${VALUE//cat/dog}\" \"${VALUE/at/AT}\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "catXcat|dog.dog|cAT.cat\n"
    );
}

#[tokio::test]
async fn non_colon_parameter_operators_distinguish_unset_from_empty() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                EMPTY=""
                echo "${UNSET-word}|${EMPTY-word}|${EMPTY:-word}|${UNSET+alt}|${EMPTY+alt}"
                echo "${ASSIGNED=made}:$ASSIGNED"
                echo "${EMPTY=unused}:${EMPTY:=filled}:$EMPTY"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "word||word||alt\nmade:made\n:filled:filled\n"
    );
}

#[tokio::test]
async fn nested_parameter_defaults_preserve_quoted_braces_and_assign_inner_values() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                printf '<%s>\n' "${OUTER:-${INNER:-"quoted } fallback"}}"
                printf '<%s><%s><%s>\n' "${ASSIGNED:=${INNER:=inner}}" "$ASSIGNED" "$INNER"
                printf '<%s><%s>\n' "${SPACE_DEFAULT:-space\ value}" "${DOLLAR_DEFAULT:-\$literal}"
                printf '<%s>\n' ${UNQUOTED_SPACE_DEFAULT:-space\ value}
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "<quoted } fallback>\n<inner><inner><inner>\n<space\\ value><$literal>\n<space value>\n"
    );
}

#[tokio::test]
async fn parameter_assignment_expansion_rejects_invalid_variable_names() {
    let engine = Engine::new();
    for (expression, expected_error) in [
        ("${1INVALID:=fallback}", "invalid variable name"),
        ("${10:=fallback}", "invalid variable name"),
        ("${-:=fallback}", "invalid variable name"),
        ("${1INVALID:-fallback}", "invalid parameter name"),
        ("${1INVALID:?failure}", "invalid parameter name"),
        ("${1INVALID:+fallback}", "invalid parameter name"),
    ] {
        let mut ctx = ExecutionContext::new();
        let script = format!("VALUE={expression}\n");
        let error = engine
            .eval_str(&script, &mut ctx)
            .await
            .expect_err("parameter expansion must reject a malformed name");

        assert!(error.to_string().contains(expected_error));
        assert_eq!(ctx.get_var("1INVALID"), None);
    }
}

#[tokio::test]
async fn non_colon_required_parameter_operator_errors_only_when_unset() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_env("ESH_SET_EMPTY", "");

    assert_eq!(
        engine
            .eval_str("echo ${ESH_SET_EMPTY?required}\n", &mut ctx)
            .await
            .unwrap(),
        0
    );

    let mut ctx = ExecutionContext::new();
    assert!(
        engine
            .eval_str("echo ${ESH_UNSET_REQUIRED?required}\n", &mut ctx)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn non_colon_special_parameter_operators_treat_zero_arguments_as_unset() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                show_args() { printf '%s:' "$#"; printf '<%s>' "$@"; printf '\n'; }
                set --
                show_args "${@-fallback}"
                show_args "${*-fallback}"
                show_args "${@+alternate}"
                show_args "${*+alternate}"
                set -- ''
                show_args "${@-fallback}"
                show_args "${*-fallback}"
                show_args "${@+alternate}"
                show_args "${*+alternate}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "1:<fallback>\n1:<fallback>\n0:<>\n1:<>\n1:<>\n1:<>\n1:<alternate>\n1:<alternate>\n"
    );

    for expression in ["${@?required}", "${*?required}"] {
        let mut ctx = ExecutionContext::new();
        assert!(
            engine
                .eval_str(&format!("printf '%s' {expression}\n"), &mut ctx)
                .await
                .is_err(),
            "{expression} must fail when there are no positional parameters"
        );
    }

    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_positional_args(vec!["esh".to_string(), String::new()]);
    assert_eq!(
        engine
            .eval_str(
                "printf '<%s><%s>\\n' \"${@?required}\" \"${*?required}\"\n",
                &mut ctx
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "<><>\n");
}

#[tokio::test]
async fn colon_required_parameter_operator_errors_when_unset_or_empty() {
    let engine = Engine::new();

    for initial_value in [None, Some("")] {
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();
        if let Some(value) = initial_value {
            ctx.set_env("ESH_COLON_REQUIRED", value);
        }
        ctx.set_env("ESH_REQUIRED_MESSAGE", "required expanded");

        let error = engine
            .eval_str(
                "printf before; printf '%s' \"${ESH_COLON_REQUIRED:?${ESH_REQUIRED_MESSAGE}}\"; printf after\n",
                &mut ctx,
            )
            .await
            .expect_err("colon-required expansion must fail for unset or empty values");

        assert!(error.to_string().contains("required expanded"));
        assert_eq!(ctx.get_captured_stdout().unwrap(), "before");
        assert_eq!(ctx.get_captured_stderr().unwrap(), "");
    }
}

#[tokio::test]
async fn colon_required_parameter_operator_accepts_nonempty_value() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_env("ESH_COLON_REQUIRED", "present");

    let status = engine
        .eval_str(
            "printf '<%s>\\n' \"${ESH_COLON_REQUIRED:?required}\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "<present>\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn pattern_removal_uses_shortest_longest_and_character_classes() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                VALUE="prefix-12-tail"
                echo "${VALUE#prefix-[0-9]*}|${VALUE##prefix-*-}"
                echo "${VALUE%-[[:alpha:]]*}|${VALUE%%-[[:alpha:]]*}"
                VALUE2="prefix-a-middle-z"
                echo "${VALUE2%*-*}|${VALUE2%%*-*}"
                VALUE3="prefix*literal"
                echo "${VALUE3#prefix\*}"
                VALUE4="*literal"
                echo "${VALUE4#'*'}|${VALUE4#*}"
                echo "${VALUE#*}|${VALUE##*}|${VALUE%*}|${VALUE%%*}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "2-tail|tail\nprefix-12|prefix-12\nprefix-a-middle|\nliteral\nliteral|*literal\nprefix-12-tail||prefix-12-tail|\n"
    );
}

#[tokio::test]
async fn quoted_parameter_patterns_expand_variables_as_literal_pattern_text() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                PREFIX=pre
                VALUE=prefix-tail
                echo "${VALUE#"$PREFIX"}"
                echo "${VALUE#${PREFIX}}"
                echo "${VALUE#$(printf pre)}"
                PREFIX='pre*'
                echo "${VALUE#"$PREFIX"}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "fix-tail\nfix-tail\nfix-tail\nprefix-tail\n"
    );
}
