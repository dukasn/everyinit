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
use std::process::Command;

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
async fn posix_scalar_parameter_operators_match_available_shells() {
    let script = r#"
        unset ESH_PARAM_UNSET ESH_PARAM_PLAIN_ASSIGN ESH_PARAM_COLON_ASSIGN
        ESH_PARAM_EMPTY=
        ESH_PARAM_VALUE=value
        printf '%s|%s|%s|%s|%s|%s\n' \
            "${ESH_PARAM_UNSET-fallback}" "${ESH_PARAM_UNSET:-fallback}" \
            "${ESH_PARAM_EMPTY-fallback}" "${ESH_PARAM_EMPTY:-fallback}" \
            "${ESH_PARAM_VALUE-fallback}" "${ESH_PARAM_VALUE:-fallback}"
        printf '%s|%s|%s|%s|%s|%s\n' \
            "${ESH_PARAM_UNSET+alternate}" "${ESH_PARAM_UNSET:+alternate}" \
            "${ESH_PARAM_EMPTY+alternate}" "${ESH_PARAM_EMPTY:+alternate}" \
            "${ESH_PARAM_VALUE+alternate}" "${ESH_PARAM_VALUE:+alternate}"
        printf '%s|%s|%s|%s\n' \
            "${ESH_PARAM_PLAIN_ASSIGN=made}" "$ESH_PARAM_PLAIN_ASSIGN" \
            "${ESH_PARAM_EMPTY=ignored}" "$ESH_PARAM_EMPTY"
        printf '%s|%s|%s|%s\n' \
            "${ESH_PARAM_COLON_ASSIGN:=filled}" "$ESH_PARAM_COLON_ASSIGN" \
            "${ESH_PARAM_EMPTY:=filled}" "$ESH_PARAM_EMPTY"
        ESH_PARAM_PATTERN=abcabc
        printf '%s|%s|%s|%s\n' \
            "${ESH_PARAM_PATTERN#a*c}" "${ESH_PARAM_PATTERN##a*c}" \
            "${ESH_PARAM_PATTERN%a*c}" "${ESH_PARAM_PATTERN%%a*c}"
    "#;
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    let expected = concat!(
        "fallback|fallback||fallback|value|value\n",
        "||alternate||alternate|alternate\n",
        "made|made||\n",
        "filled|filled|filled|filled\n",
        "abc||abc|\n"
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), expected);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    #[cfg(unix)]
    for (shell, args) in [("bash", vec!["--posix", "-c"]), ("dash", vec!["-c"])] {
        if let Ok(reference) = Command::new(shell).args(args).arg(script).output() {
            assert!(
                reference.status.success(),
                "{shell} failed to evaluate POSIX parameter operators"
            );
            assert_eq!(reference.stdout, expected.as_bytes(), "{shell} output");
            assert_eq!(reference.stderr, b"", "{shell} diagnostics");
        }
    }
}

#[tokio::test]
async fn posix_required_parameter_operators_stop_the_current_script_on_failure() {
    let cases = [
        (
            "unset with non-colon operator",
            "unset ESH_REQUIRED_VALUE\nprintf before\nprintf '%s' \"${ESH_REQUIRED_VALUE?required}\"\nprintf after\n",
            false,
            "before",
        ),
        (
            "empty with colon operator",
            "ESH_REQUIRED_VALUE=\nprintf before\nprintf '%s' \"${ESH_REQUIRED_VALUE:?required}\"\nprintf after\n",
            false,
            "before",
        ),
        (
            "empty with non-colon operator",
            "ESH_REQUIRED_VALUE=\nprintf before\nprintf '%s' \"${ESH_REQUIRED_VALUE?required}\"\nprintf after\n",
            true,
            "beforeafter",
        ),
        (
            "set with colon operator",
            "ESH_REQUIRED_VALUE=present\nprintf before\nprintf '%s' \"${ESH_REQUIRED_VALUE:?required}\"\nprintf after\n",
            true,
            "beforepresentafter",
        ),
    ];

    for (name, script, succeeds, expected_stdout) in cases {
        let engine = Engine::new();
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();
        let result = engine.eval_str(script, &mut ctx).await;

        assert_eq!(result.is_ok(), succeeds, "ESH: {name}");
        assert_eq!(
            ctx.get_captured_stdout().unwrap(),
            expected_stdout,
            "ESH: {name}"
        );

        #[cfg(unix)]
        for (shell, args) in [("bash", vec!["--posix", "-c"]), ("dash", vec!["-c"])] {
            let Ok(reference) = Command::new(shell).args(args).arg(script).output() else {
                continue;
            };
            assert_eq!(reference.status.success(), succeeds, "{shell}: {name}");
            assert_eq!(
                reference.stdout,
                expected_stdout.as_bytes(),
                "{shell}: {name} output"
            );
            if !succeeds {
                assert!(!reference.stderr.is_empty(), "{shell}: {name} diagnostic");
            }
        }
    }
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
async fn parameter_slices_saturate_decimal_values_beyond_machine_width() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "VALUE=abcd\nprintf '%s|%s\\n' \"${VALUE:999999999999999999999999999999999999}\" \"${VALUE:1:999999999999999999999999999999999999}\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "|bcd\n");
}

#[tokio::test]
async fn parameter_slices_count_unicode_characters_for_scalars_and_array_elements() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                VALUE="A雪🌍Z"
                ITEMS=("A雪🌍Z")
                printf '%s|%s|%s|%s\n' \
                    "${VALUE:1:2}" "${VALUE:2}" \
                    "${ITEMS[0]:1:2}" "${ITEMS[0]:3:99}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "雪🌍|🌍Z|雪🌍|Z\n");
}

#[tokio::test]
async fn parameter_slices_reject_malformed_offsets_and_lengths() {
    let engine = Engine::new();
    for expression in [
        "${VALUE:abc}",
        "${VALUE: 1}",
        "${VALUE:1: -1}",
        "${VALUE:1:2:3}",
        "${VALUE:}",
        "${VALUE:1:}",
    ] {
        let mut ctx = ExecutionContext::new();
        let script = format!("VALUE=abcd\nprintf '%s' \"{expression}\"\n");
        assert!(
            engine.eval_str(&script, &mut ctx).await.is_err(),
            "malformed slice {expression} should return an expansion error"
        );
    }

    for expression in ["${ITEMS[0]:abc}", "${ITEMS[0]:1: -1}", "${ITEMS[0]:1:2:3}"] {
        let mut ctx = ExecutionContext::new();
        let script = format!("ITEMS=(abcd)\nprintf '%s' \"{expression}\"\n");
        assert!(
            engine.eval_str(&script, &mut ctx).await.is_err(),
            "malformed array slice {expression} should return an expansion error"
        );
    }
}

#[tokio::test]
async fn nounset_errors_for_unset_scalar_modifier_reads() {
    let engine = Engine::new();
    for expression in [
        "${MISSING#prefix}",
        "${MISSING%tail}",
        "${MISSING:1}",
        "${MISSING/old/new}",
        "${MISSING//old/new}",
    ] {
        let mut ctx = ExecutionContext::new();
        assert!(
            engine
                .eval_str(&format!("set -u\nprintf '%s' \"{expression}\"\n"), &mut ctx)
                .await
                .is_err(),
            "set -u should reject the unset value read by {expression}"
        );
    }
}

#[tokio::test]
async fn nounset_errors_for_unset_array_element_modifier_reads() {
    let engine = Engine::new();
    for expression in ["${ITEMS[0]#prefix}", "${ITEMS[0]%tail}", "${ITEMS[0]:1}"] {
        let mut ctx = ExecutionContext::new();
        assert!(
            engine
                .eval_str(
                    &format!("set -u\ndeclare -a ITEMS\nprintf '%s' \"{expression}\"\n"),
                    &mut ctx,
                )
                .await
                .is_err(),
            "set -u should reject the unset array element read by {expression}"
        );
    }

    for expression in ["${MAP[key]#prefix}", "${MAP[key]%tail}", "${MAP[key]:1}"] {
        let mut ctx = ExecutionContext::new();
        assert!(
            engine
                .eval_str(
                    &format!("set -u\ndeclare -A MAP\nprintf '%s' \"{expression}\"\n"),
                    &mut ctx,
                )
                .await
                .is_err(),
            "set -u should reject the unset map element read by {expression}"
        );
    }
}

#[tokio::test]
async fn nounset_allows_modifier_reads_of_set_but_empty_values() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                set -u
                VALUE=
                ITEMS=("")
                declare -A MAP
                MAP[key]=""
                printf '<%s><%s><%s><%s>\n' \
                    "${VALUE#x}" "${VALUE:1}" "${VALUE/x/y}" \
                    "${ITEMS[0]%y}"
                printf '<%s>\n' "${MAP[key]#x}"
                printf 'fallbacks=<%s><%s><%s><%s>\n' \
                    "${MISSING:-scalar}" "${ITEMS[4]:-array}" \
                    "${MAP[missing]:-map}" "${VALUE:-empty}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "<><><><>\n<>\nfallbacks=<scalar><array><map><empty>\n"
    );
}

#[tokio::test]
async fn nounset_counts_declared_empty_collections_but_rejects_unset_lengths() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                set -u
                EMPTY=
                declare -a ITEMS
                ITEMS=()
                declare -A MAP
                printf 'scalar=%s indexed=%s map=%s keys=<%s><%s>\n' \
                    "${#EMPTY}" "${#ITEMS[@]}" "${#MAP[@]}" \
                    "${!ITEMS[@]}" "${!MAP[@]}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "scalar=0 indexed=0 map=0 keys=<><>\n"
    );

    for expression in ["${#MISSING}", "${#MISSING[@]}"] {
        let mut ctx = ExecutionContext::new();
        assert!(
            engine
                .eval_str(&format!("set -u\nprintf '%s' \"{expression}\"\n"), &mut ctx,)
                .await
                .is_err(),
            "set -u should reject the unset length read {expression}"
        );
    }
}

#[tokio::test]
async fn parameter_substring_replacement_uses_literal_first_or_all_matches() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"VALUE=cat.cat
PATH_VALUE=a/b/c
printf '%s|%s\n' "${PATH_VALUE/a\/b/X}" "${PATH_VALUE//a\/b/x/y}"
printf '%s|%s|%s|%s|%s\n' "${VALUE/./X}" "${VALUE//cat/dog}" "${VALUE/at/AT}" "${PATH_VALUE/\//_}" "${PATH_VALUE//\//_}"
"#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "X/c|x/y/c\ncatXcat|dog.dog|cAT.cat|a_b/c|a_b_c\n"
    );
}

#[tokio::test]
async fn parameter_replacement_operands_expand_to_literal_strings() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                VALUE=alpha-beta-alpha
                SEARCH=alpha
                REPLACEMENT=gamma
                SPECIAL_VALUE='a*a'
                SPECIAL_SEARCH='*'
                PATH_VALUE=a/b/c
                PATH_SEARCH=a/b
                PATH_REPLACEMENT=x/y
                COMMAND_VALUE=alpha
                printf '%s|%s|%s|%s|%s\n' "${VALUE/$SEARCH/$REPLACEMENT}" "${VALUE//${SEARCH}/${REPLACEMENT}}" "${SPECIAL_VALUE/$SPECIAL_SEARCH/X}" "${PATH_VALUE/$PATH_SEARCH/$PATH_REPLACEMENT}" "${COMMAND_VALUE/$(printf alpha)/$(printf result)}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "gamma-beta-alpha|gamma-beta-gamma|aXa|x/y/c|result\n"
    );
}

#[tokio::test]
async fn parameter_replacement_with_an_empty_search_leaves_the_value_unchanged() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                VALUE=abc
                EMPTY_SEARCH=
                printf '%s|%s\n' "${VALUE///X}" "${VALUE//${EMPTY_SEARCH}/X}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "abc|abc\n");
}

#[tokio::test]
async fn parameter_replacement_delimiters_ignore_nested_expansion_slashes() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                VALUE=a/b/c
                SEARCH=alpha
                WORDS=alpha-alpha
                PATH=path/to/file
                PATH_WITH_PAREN='path/to)path/to/file'
                printf '%s|%s|%s|%s|%s\n' \
                    "${VALUE/${MISSING:-a/b}/X}" \
                    "${VALUE//${MISSING:-a/b}/X}" \
                    "${WORDS/${SEARCH//x/y}/X}" \
                    "${PATH/$(printf path/to)/X}" \
                    "${PATH_WITH_PAREN/$(printf ')path/to')/X}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "X/c|X/c|X-alpha|X/file|path/toX/file\n"
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
async fn array_and_map_elements_support_parameter_default_assignment_and_alternate_operators() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                ITEMS=("")
                declare -A MAP
                MAP[empty]=""
                MAP[present]=value
                printf '<%s>|<%s>|<%s>|<%s>\n' \
                    "${ITEMS[0]:-colon}" "${ITEMS[0]-plain}" \
                    "${ITEMS[0]:+alternate}" "${ITEMS[0]+alternate}"
                printf '<%s>|<%s>|<%s>|<%s>\n' \
                    "${MAP[empty]:-colon}" "${MAP[empty]-plain}" \
                    "${MAP[empty]:+alternate}" "${MAP[empty]+alternate}"

                printf 'array=%s empty=%s missing=%s map=%s\n' \
                    "${ITEMS[0]:=filled}" "${ITEMS[0]=ignored}" \
                    "${ITEMS[2]=created}" "${MAP[added]:=inserted}"
                printf 'stored=<%s>|<%s>|<%s>\n' \
                    "${ITEMS[0]}" "${ITEMS[2]}" "${MAP[added]}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "<colon>|<>|<>|<alternate>\n<colon>|<>|<>|<alternate>\narray=filled empty=filled missing=created map=inserted\nstored=<filled>|<created>|<inserted>\n"
    );
}

#[tokio::test]
async fn array_and_map_element_required_operators_distinguish_empty_from_unset() {
    let engine = Engine::new();

    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str(
                "ITEMS=(\"\"); printf '<%s>\\n' \"${ITEMS[0]?set-empty}\"\n",
                &mut ctx,
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "<>\n");

    for script in [
        "ITEMS=(\"\"); printf '%s' \"${ITEMS[0]:?required}\"",
        "ITEMS=(); printf '%s' \"${ITEMS[0]?required}\"",
        "declare -A MAP; printf '%s' \"${MAP[missing]:?required}\"",
        "set -u; ITEMS=(); printf '%s' \"${ITEMS[2]}\"",
    ] {
        let mut ctx = ExecutionContext::new();
        assert!(
            engine.eval_str(script, &mut ctx).await.is_err(),
            "expected element expansion to fail: {script}"
        );
    }
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
async fn colon_special_parameter_operators_test_the_space_joined_value() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
                show_args() { printf '%s:' "$#"; printf '<%s>' "$@"; printf '\n'; }
                set --
                show_args "${@:-fallback}"
                show_args "${*:-fallback}"
                show_args "${@:+alternate}"
                show_args "${*:+alternate}"
                set -- ''
                show_args "${@:-fallback}"
                show_args "${*:-fallback}"
                show_args "${@:+alternate}"
                show_args "${*:+alternate}"
                set -- first second
                show_args "${@:-fallback}"
                show_args "${*:-fallback}"
                show_args "${@:+alternate}"
                show_args "${*:+alternate}"
            "#;
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    let expected = concat!(
        "1:<fallback>\n",
        "1:<fallback>\n",
        "0:<>\n",
        "1:<>\n",
        "1:<fallback>\n",
        "1:<fallback>\n",
        "1:<>\n",
        "1:<>\n",
        "2:<first><second>\n",
        "1:<first second>\n",
        "1:<alternate>\n",
        "1:<alternate>\n"
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), expected);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    #[cfg(unix)]
    if let Ok(reference) = std::process::Command::new("bash")
        .args(["--posix", "-c"])
        .arg(script)
        .output()
    {
        assert!(reference.status.success());
        assert_eq!(reference.stdout, expected.as_bytes());
    }
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
async fn positional_pattern_removal_preserves_at_fields_and_joins_star_values() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        show() {
            printf 'argc=%s' "$#"
            for item in "$@"; do printf ' <%s>' "$item"; done
            printf '\n'
        }
        set -- alpha.one.txt beta.tar.gz gamma.log
        show "${@#*.}"
        show ${@##*.}
        show "${@##*.}"
        show "${@%.*}"
        show "${@%%.*}"
        IFS=:
        show "${*#*.}"
        show "${*##*.}"
        show "${*%.*}"
        show "${*%%.*}"
        show "pre${@#*.}post"

        set -- '' beta.log ''
        show "${@#*.}"
        set --
        show "${@#*.}"
        show "${*#*.}"
    "#;
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert_eq!(
        output,
        "argc=3 <one.txt> <tar.gz> <log>\nargc=3 <txt> <gz> <log>\nargc=3 <txt> <gz> <log>\nargc=3 <alpha.one> <beta.tar> <gamma>\nargc=3 <alpha> <beta> <gamma>\nargc=1 <one.txt tar.gz log>\nargc=1 <txt gz log>\nargc=1 <alpha.one beta.tar gamma>\nargc=1 <alpha beta gamma>\nargc=1 <preone.txt tar.gz logpost>\nargc=3 <> <log> <>\nargc=0\nargc=1 <>\n"
    );

    #[cfg(unix)]
    {
        // POSIX leaves these two operators unspecified for @ and *. ESH uses
        // per-parameter removal for standalone forms but keeps embedded @ as
        // one scalar, as specified by its expansion contract.
        let reference = Command::new("bash")
            .args(["--posix", "-c", script])
            .output()
            .expect("Bash should be available for the parameter expansion comparison");
        assert!(reference.status.success());
        assert_eq!(
            reference.stdout,
            b"argc=3 <one.txt> <tar.gz> <log>\nargc=3 <txt> <gz> <log>\nargc=3 <txt> <gz> <log>\nargc=3 <alpha.one> <beta.tar> <gamma>\nargc=3 <alpha> <beta> <gamma>\nargc=1 <one.txt:tar.gz:log>\nargc=1 <txt:gz:log>\nargc=1 <alpha.one:beta.tar:gamma>\nargc=1 <alpha:beta:gamma>\nargc=3 <preone.txt> <tar.gz> <logpost>\nargc=3 <> <log> <>\nargc=0\nargc=1 <>\n"
        );
    }
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
