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

use esh::planner::compiler::compile_script;
use esh::runtime::context::ExecutionContext;
use esh::runtime::traits::{Executable, ScopeManager};
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_set_errexit_aborts_on_failure() {
    let script = r#"
        set -e
        A=1
        false
        A=2
    "#;

    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    let res = plan.execute(&mut ctx).await;

    assert_eq!(ctx.get_var("A").as_deref(), Some("1"));
    assert!(ctx.is_exiting());
    assert_eq!(res.unwrap_or(1), 1);
}

#[tokio::test]
async fn test_set_errexit_shielded_in_conditions() {
    let script = r#"
        set -e
        if false; then
            A=unreachable
        else
            A=reached_else
        fi
        
        false || B=fallback_ok
        
        while false; do
            C=loop_never
        done
        C=after_while
    "#;

    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    let res = plan.execute(&mut ctx).await;

    assert_eq!(res.unwrap(), 0);
    assert_eq!(ctx.get_var("A").as_deref(), Some("reached_else"));
    assert_eq!(ctx.get_var("B").as_deref(), Some("fallback_ok"));
    assert_eq!(ctx.get_var("C").as_deref(), Some("after_while"));
}

#[tokio::test]
async fn errexit_stops_failed_function_body_and_enclosing_block() {
    let script = "set -e\nfail_inside() { echo before; false; echo unreachable-function; }\nfail_inside\necho unreachable-caller\n";
    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = plan.execute(&mut ctx).await.unwrap();

    assert_eq!(status, 1);
    assert!(ctx.is_exiting());
    assert_eq!(ctx.get_captured_stdout().unwrap(), "before\n");
}

#[tokio::test]
async fn errexit_is_shielded_inside_functions_used_as_conditions() {
    let script = r#"set -e
checked() { echo checked-before; false; echo checked-after; }
if checked; then echo if-then; fi
chained() { echo chained-before; false; echo chained-after; }
chained || echo fallback
echo done
"#;
    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = plan.execute(&mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert!(!ctx.is_exiting());
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "checked-before\nchecked-after\nif-then\nchained-before\nchained-after\ndone\n"
    );
}

#[tokio::test]
async fn errexit_is_shielded_inside_functions_used_as_loop_tests() {
    let script = r#"set -e
COUNT=0
while_check() { false; COUNT=$((COUNT + 1)); [ "$COUNT" -lt 2 ]; }
while while_check; do echo while-body; done
COUNT=0
until_check() { false; COUNT=$((COUNT + 1)); [ "$COUNT" -ge 2 ]; }
until until_check; do echo until-body; done
failed_check() { echo failed-check; false; }
if failed_check; then echo unexpected-if; else echo if-else; fi
failed_left() { echo failed-left; false; }
failed_left || echo fallback
echo done
"#;
    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = plan.execute(&mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert!(!ctx.is_exiting());
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "while-body\nuntil-body\nfailed-check\nif-else\nfailed-left\nfallback\ndone\n"
    );
}

#[tokio::test]
async fn errexit_stops_after_parallel_join_with_a_failed_branch() {
    let script = "set -e\nfalse &| echo joined-output\necho unreachable-caller\n";
    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = plan.execute(&mut ctx).await.unwrap();

    assert_eq!(status, 1);
    assert!(ctx.is_exiting());
    assert_eq!(ctx.get_captured_stdout().unwrap(), "joined-output\n");
}

#[tokio::test]
async fn errexit_applies_to_the_result_of_an_and_or_expression() {
    for script in [
        "set -e\nfalse && echo skipped\necho unreachable\n",
        "set -e\ntrue && false\necho unreachable\n",
        "set -e\nfalse || false\necho unreachable\n",
    ] {
        let plan = compile_script(script).expect("compile");
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();

        assert_eq!(plan.execute(&mut ctx).await.unwrap(), 1, "{script}");
        assert!(ctx.is_exiting(), "{script}");
        assert_eq!(ctx.get_captured_stdout().unwrap(), "", "{script}");
    }
}

#[tokio::test]
async fn errexit_checks_the_result_of_negated_pipelines() {
    let cases = [
        ("set -e\n! false\necho reached\n", 0, "reached\n", false),
        ("set -e\n! true\necho unreachable\n", 1, "", true),
    ];

    for (script, expected_status, expected_stdout, expected_exiting) in cases {
        let plan = compile_script(script).expect("compile");
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();

        let status = plan.execute(&mut ctx).await.unwrap();

        assert_eq!(status, expected_status, "{script}");
        assert_eq!(
            ctx.get_captured_stdout().unwrap(),
            expected_stdout,
            "{script}"
        );
        assert_eq!(ctx.is_exiting(), expected_exiting, "{script}");
    }
}

#[tokio::test]
async fn errexit_shields_failures_inside_negated_function_but_checks_inverted_status() {
    let script = "set -e\nwork() { printf 'before\\n'; false; printf 'after\\n'; }\n! work\nprintf 'unreachable\\n'\n";
    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = plan.execute(&mut ctx).await.unwrap();

    assert_eq!(status, 1);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "before\nafter\n");
    assert!(ctx.is_exiting());
}

#[tokio::test]
async fn negated_command_errors_restore_errexit_shield() {
    let plan = compile_script("set -u\n! printf '%s' \"${MISSING:?required}\"\n").unwrap();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert!(plan.execute(&mut ctx).await.is_err());
    assert!(!ctx.is_errexit_shielded());
}

#[tokio::test]
async fn conditional_execution_errors_restore_errexit_shield() {
    for script in [
        "set -u\nif echo \"$ESH_MISSING_FOR_IF\"; then :; fi\n",
        "set -u\nwhile echo \"$ESH_MISSING_FOR_WHILE\"; do :; done\n",
        "set -u\necho \"$ESH_MISSING_FOR_AND_OR\" || :\n",
    ] {
        let plan = compile_script(script).expect("compile");
        let mut ctx = ExecutionContext::new();

        assert!(plan.execute(&mut ctx).await.is_err(), "{script}");
        assert!(
            !ctx.is_errexit_shielded(),
            "conditional failure leaked errexit shield: {script}"
        );
    }
}

#[tokio::test]
async fn test_set_nounset() {
    let script = r#"
        set -u
        DEFINED="hello"
        A="$DEFINED"
        B="$UNDEFINED_VAR"
        C="unreachable"
    "#;

    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    let res = plan.execute(&mut ctx).await;

    assert!(res.is_err());
    assert_eq!(ctx.get_var("A").as_deref(), Some("hello"));
    assert_eq!(ctx.get_var("C"), None);
}

#[tokio::test]
async fn nounset_applies_to_length_expansion_of_unset_names() {
    let plan = compile_script("set -u\nVALUE=${#UNDEFINED_VALUE}\n").expect("compile");
    let mut ctx = ExecutionContext::new();

    assert!(plan.execute(&mut ctx).await.is_err());
    assert_eq!(ctx.get_var("VALUE"), None);
}

#[tokio::test]
async fn nounset_applies_to_arithmetic_reads_but_respects_short_circuit_and_assignment() {
    let plan = compile_script(
        "set -u\nSAFE_AND=$((0 && MISSING))\nSAFE_OR=$((1 || MISSING))\nCREATED=$((MISSING=4))\nprintf '%s:%s:%s\\n' \"$SAFE_AND\" \"$SAFE_OR\" \"$CREATED\"\n",
    )
    .expect("compile script");
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    assert_eq!(plan.execute(&mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "0:1:4\n");

    let plan = compile_script("set -u\nVALUE=$((MISSING + 1))\n").expect("compile script");
    let mut ctx = ExecutionContext::new();
    let error = plan
        .execute(&mut ctx)
        .await
        .expect_err("arithmetic read of unset variable must fail under nounset");
    assert!(error.to_string().contains("MISSING: unbound variable"));
    assert_eq!(ctx.get_var("VALUE"), None);
}

#[tokio::test]
async fn test_set_pipefail() {
    let script = r#"
        set -o pipefail
        false | true
    "#;

    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    let res = plan.execute(&mut ctx).await.unwrap();

    assert_eq!(res, 1);
}

#[tokio::test]
async fn pipefail_returns_the_rightmost_nonzero_pipeline_status() {
    let script = "set -o pipefail\nfalse | sh -c 'exit 7' | true\n";
    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();

    assert_eq!(plan.execute(&mut ctx).await.unwrap(), 7);
}

#[tokio::test]
async fn test_set_xtrace_stderr() {
    let script = r#"
        set -x
        echo "hello xtrace"
    "#;

    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    plan.execute(&mut ctx).await.expect("execute");

    let stderr = ctx.get_captured_stderr().unwrap_or_default();
    assert!(stderr.contains("+ echo hello xtrace"));
}

#[tokio::test]
async fn set_dash_disables_xtrace_without_changing_positionals() {
    let engine = esh::Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "set -- first second\nset -x\nset -\nprintf '%s:%s:%s\\n' \"$1\" \"$2\" \"$-\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "first:second:\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "+ set -\n");
}

#[tokio::test]
async fn set_reports_enabled_options_and_supports_positional_terminators() {
    let script = r#"
        set -euo pipefail
        echo "flags:$-"
        set +e
        echo "flags:$-"
        set -o
        set +o
        set value -x
        echo "first:$1 second:$2 flags:$-"
        set -- -x
        echo "literal:$1 flags:$-"
    "#;
    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    plan.execute(&mut ctx).await.expect("execute");

    let stdout = ctx.get_captured_stdout().unwrap();
    assert!(stdout.starts_with("flags:eu\nflags:u\n"));
    assert!(stdout.contains("errexit off\nnoglob off\nnounset on\npipefail on\n"));
    assert!(stdout.contains("set +o errexit\nset +o noglob\nset -o nounset\nset -o pipefail\n"));
    assert!(stdout.contains("first:value second:-x flags:u\n"));
    assert!(stdout.contains("literal:-x flags:u\n"));
}

#[tokio::test]
async fn allexport_exports_scalar_assignments_until_disabled() {
    let engine = esh::Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"set -a
AUTO_EXPORTED=visible
AUTO_ARRAY=(left right)
export_from_local() { local LOCAL_AUTO_EXPORTED=inside; sh -c 'printf "local=%s\\n" "$LOCAL_AUTO_EXPORTED"'; }
export_from_local
set +a
NOT_AUTO_EXPORTED=private
sh -c 'printf "%s:%s:%s\n" "$AUTO_EXPORTED" "${NOT_AUTO_EXPORTED-unset}" "${AUTO_ARRAY-unset}"'
printf 'flags=%s\n' "$-"
"#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "local=inside\nvisible:unset:unset\nflags=\n"
    );
    assert_eq!(ctx.get_var("AUTO_EXPORTED").as_deref(), Some("visible"));
    assert_eq!(ctx.get_var("NOT_AUTO_EXPORTED").as_deref(), Some("private"));
    assert!(ctx.env_vars().contains_key("AUTO_EXPORTED"));
    assert!(!ctx.env_vars().contains_key("LOCAL_AUTO_EXPORTED"));
    assert!(!ctx.env_vars().contains_key("NOT_AUTO_EXPORTED"));
    assert!(!ctx.env_vars().contains_key("AUTO_ARRAY"));
}

#[tokio::test]
async fn allexport_is_reported_and_replayed_as_a_shell_option() {
    let engine = esh::Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "set -o allexport\nprintf '%s\\n' \"$-\"\nset +o\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.starts_with("a\n"));
    assert!(output.contains("set -o allexport\n"));
}

#[tokio::test]
async fn set_plus_o_output_restores_the_recorded_option_state_when_sourced() {
    let engine = esh::Engine::new();
    let temp = tempdir().unwrap();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(temp.path()).unwrap();

    let status = engine
        .eval_str(
            "set -u -o pipefail\nset +o > option-state\nset +u +o pipefail\n. ./option-state\nfalse | true\nprintf '%s:%s\\n' \"$-\" \"$?\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "u:1\n");
}

#[tokio::test]
async fn set_plus_o_round_trips_every_adopted_option() {
    let engine = esh::Engine::new();
    let temp = tempdir().unwrap();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(temp.path()).unwrap();

    let status = engine
        .eval_str(
            "set -a -e -u -f -x -C -o pipefail\nset +o >| option-state\nset +a +e +u +f +x +C +o pipefail\n. ./option-state\nif false | true; then PIPE=0; else PIPE=$?; fi\nprintf 'PIPE:%s\\n' \"$PIPE\"\nset -o\nset +e +x\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    let stdout = ctx.get_captured_stdout().unwrap();
    assert!(stdout.starts_with("PIPE:1\n"));
    for option in [
        "allexport",
        "errexit",
        "noglob",
        "nounset",
        "pipefail",
        "xtrace",
        "noclobber",
    ] {
        assert!(
            stdout.contains(&format!("{option} on\n")),
            "{option}: {stdout}"
        );
    }
}

#[tokio::test]
async fn set_requires_double_dash_before_dash_prefixed_numeric_parameters() {
    let script = r#"
        set -1
        INVALID_STATUS=$?
        set -- -1
        printf '%s:<%s>\n' "$INVALID_STATUS" "$1"
    "#;
    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = plan.execute(&mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "2:<-1>\n");
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("unknown option '1'")
    );
}

#[tokio::test]
async fn set_rejects_posix_options_outside_the_esh_script_contract() {
    let engine = esh::Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "set -- original\nfor OPTION in h k m n p t v; do set -$OPTION; printf '%s:%s:%s\\n' \"$OPTION\" \"$?\" \"$1\"; done\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "h:2:original\nk:2:original\nm:2:original\nn:2:original\np:2:original\nt:2:original\nv:2:original\n"
    );
    let stderr = ctx.get_captured_stderr().unwrap();
    for option in ['h', 'k', 'm', 'n', 'p', 't', 'v'] {
        assert!(stderr.contains(&format!("unknown option '{option}'")));
    }
}

#[tokio::test]
async fn dollar_dash_lists_enabled_short_options_in_esh_order() {
    let plan = compile_script("set -aeufxC\nprintf '%s\\n' \"$-\"\n").expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    plan.execute(&mut ctx).await.expect("execute");

    assert_eq!(ctx.get_captured_stdout().unwrap(), "aeufxC\n");
}

#[tokio::test]
async fn noglob_flag_is_redundant_and_explicit_glob_remains_available() {
    let temp = tempdir().unwrap();
    fs::write(temp.path().join("alpha.txt"), "").unwrap();
    fs::write(temp.path().join("beta.txt"), "").unwrap();

    let engine = esh::Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(temp.path()).unwrap();

    let status = engine
        .eval_str(
            "VALUE='*.txt'\nset -f\nprintf '<%s>\\n' $VALUE\nglob '*.txt'\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("-").as_deref(), Some("f"));
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "<*.txt>\nalpha.txt\nbeta.txt\n"
    );
}

#[tokio::test]
async fn set_rejects_unknown_options_with_status_two() {
    let plan = compile_script("set -z\n").expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = plan.execute(&mut ctx).await.unwrap();
    assert_eq!(status, 2);
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: set: unknown option 'z'\n"
    );
}

#[tokio::test]
async fn set_rejects_invalid_option_groups_without_partially_changing_options() {
    let plan = compile_script("set -euZ\n").expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = plan.execute(&mut ctx).await.unwrap();

    assert_eq!(status, 2);
    assert!(!ctx.options().errexit);
    assert!(!ctx.options().nounset);
    assert!(!ctx.is_exiting());
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: set: unknown option 'Z'\n"
    );
}

#[tokio::test]
async fn test_read_multi_var_and_custom_ifs() {
    let script = r#"
        IFS=":"
        read -r USER PASS UID GID REST <<< "alice:secret123:1001:1001:Alice Developer:/home/alice:/bin/esh"
    "#;

    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    plan.execute(&mut ctx).await.expect("execute");

    assert_eq!(ctx.get_var("USER").as_deref(), Some("alice"));
    assert_eq!(ctx.get_var("PASS").as_deref(), Some("secret123"));
    assert_eq!(ctx.get_var("UID").as_deref(), Some("1001"));
    assert_eq!(ctx.get_var("GID").as_deref(), Some("1001"));
    assert_eq!(
        ctx.get_var("REST").as_deref(),
        Some("Alice Developer:/home/alice:/bin/esh")
    );
}

#[tokio::test]
async fn read_uses_default_whitespace_without_setting_unset_ifs() {
    let engine = esh::Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_stdin(b" one\t two \n".to_vec());

    let status = engine
        .eval_str("unset IFS\nread FIRST SECOND\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("FIRST").as_deref(), Some("one"));
    assert_eq!(ctx.get_var("SECOND").as_deref(), Some("two"));
    assert_eq!(ctx.get_var("IFS"), None);
}

#[tokio::test]
async fn read_assigns_partial_eof_data_and_returns_failure() {
    let engine = esh::Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"alpha beta".to_vec());

    let status = engine
        .eval_str("read FIRST REST\n", &mut ctx)
        .await
        .unwrap();
    assert_eq!(status, 1);
    assert_eq!(ctx.get_var("FIRST").as_deref(), Some("alpha"));
    assert_eq!(ctx.get_var("REST").as_deref(), Some("beta"));
}

#[tokio::test]
async fn read_preserves_trailing_backslash_in_partial_eof_input() {
    let engine = esh::Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_stdin(b"tail\\".to_vec());

    let status = engine.eval_str("read VALUE\n", &mut ctx).await.unwrap();
    assert_eq!(status, 1);
    assert_eq!(ctx.get_var("VALUE").as_deref(), Some("tail\\"));
}

#[tokio::test]
async fn read_keeps_buffered_lines_for_following_calls() {
    let engine = esh::Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_stdin(b"first\nsecond\n".to_vec());

    let status = engine
        .eval_str("read FIRST; read SECOND\n", &mut ctx)
        .await
        .unwrap();
    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("FIRST").as_deref(), Some("first"));
    assert_eq!(ctx.get_var("SECOND").as_deref(), Some("second"));
}

#[tokio::test]
async fn read_custom_delimiter_consumes_one_byte_and_keeps_following_input() {
    let plan = compile_script(
        "read -d ':ignored' LEFT; read RIGHT; printf '<%s><%s>\\n' \"$LEFT\" \"$RIGHT\"",
    )
    .expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"alpha:beta\n".to_vec());

    let status = plan.execute(&mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "<alpha><beta>\n");
}

#[tokio::test]
async fn read_empty_delimiter_uses_nul_and_keeps_following_input() {
    let plan = compile_script(
        "read -r -d '' FIRST; read -r SECOND; printf '<%s><%s>\\n' \"$FIRST\" \"$SECOND\"",
    )
    .expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"alpha\0beta\n".to_vec());

    let status = plan.execute(&mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "<alpha><beta>\n");
}

#[tokio::test]
async fn read_custom_delimiter_respects_default_and_raw_backslash_modes() {
    let engine = compile_script(
        "read -d : VALUE; read REST; printf 'default:<%s><%s>\\n' \"$VALUE\" \"$REST\"\nread -r -d : RAW; read TAIL; printf 'raw:<%s><%s>\\n' \"$RAW\" \"$TAIL\"",
    )
    .expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"alpha\\:beta:tail\nraw\\:value:rest:end\n".to_vec());

    let status = engine.execute(&mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "default:<alpha:beta><tail>\nraw:<raw\\><value:rest:end>\n"
    );
}

#[tokio::test]
async fn read_without_variable_operand_assigns_to_reply() {
    let engine = esh::Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_stdin(b"answer\n".to_vec());

    let status = engine.eval_str("read\n", &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("REPLY").as_deref(), Some("answer"));
}

#[tokio::test]
async fn read_does_not_assign_when_input_is_already_at_eof() {
    let engine = esh::Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_stdin(Vec::new());
    ctx.set_local_var("REPLY", "preserve me".to_string());

    let status = engine.eval_str("read REPLY\n", &mut ctx).await.unwrap();
    assert_eq!(status, 1);
    assert_eq!(ctx.get_var("REPLY").as_deref(), Some("preserve me"));
}

#[tokio::test]
async fn read_preserves_escaped_trailing_ifs_whitespace_before_unescaped_trailing_space() {
    let engine = esh::Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "IFS=' '\nread VALUE <<EOF\n  leading\\ and trailing\\  \nEOF\nprintf '<%s>\\n' \"$VALUE\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "<leading and trailing >\n"
    );
}

#[tokio::test]
async fn read_preserves_escaped_leading_and_trailing_ifs_whitespace() {
    let engine = esh::Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "IFS=' '\nread LEADING <<EOF\n  \\ leading\nEOF\nread TRAILING <<EOF\ntrailing\\ \nEOF\nprintf '<%s>|<%s>\\n' \"$LEADING\" \"$TRAILING\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "< leading>|<trailing >\n"
    );
}

#[tokio::test]
async fn read_preserves_non_whitespace_ifs_delimiters_in_the_last_variable() {
    let engine = esh::Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("IFS=:; read FIRST REST <<< ':alpha::omega'\n", &mut ctx)
        .await
        .unwrap();
    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("FIRST").as_deref(), Some(""));
    assert_eq!(ctx.get_var("REST").as_deref(), Some("alpha::omega"));
}

#[tokio::test]
async fn read_rejects_missing_option_arguments_and_unknown_long_options() {
    let engine = esh::Engine::new();
    for script in ["read -p\n", "read --unknown\n"] {
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();
        assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 2);
        assert!(ctx.get_captured_stderr().unwrap().contains("esh: read:"));
    }
}

#[tokio::test]
async fn read_rejects_invalid_variable_names_before_consuming_stdin() {
    let engine = esh::Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"keep this input\n".to_vec());

    let status = engine.eval_str("read bad-name\n", &mut ctx).await.unwrap();

    assert_eq!(status, 2);
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: read: invalid identifier: bad-name\n"
    );
    assert_eq!(
        ctx.take_stdin().as_deref(),
        Some(b"keep this input\n".as_slice())
    );
}

#[tokio::test]
async fn read_rejects_invalid_timeout_values_without_panicking_or_consuming_input() {
    for timeout in ["-1", "NaNs", "infs", "1e100s"] {
        let plan = compile_script(&format!("read -t {timeout} VALUE")).expect("compile");
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();
        ctx.set_stdin(b"preserve me\n".to_vec());

        assert_eq!(plan.execute(&mut ctx).await.unwrap(), 2, "{timeout}");
        assert_eq!(
            ctx.take_stdin().as_deref(),
            Some(b"preserve me\n".as_slice())
        );
        assert_eq!(
            ctx.get_captured_stderr().unwrap(),
            "esh: read: invalid timeout duration\n",
            "{timeout}"
        );
    }
}

#[tokio::test]
async fn read_rejects_invalid_targets_in_interactive_modes_before_prompting() {
    let engine = esh::Engine::new();
    for script in ["read -c bad-name\n", "read -s bad-name\n"] {
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();

        assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 2);
        assert_eq!(
            ctx.get_captured_stderr().unwrap(),
            "esh: read: invalid identifier: bad-name\n"
        );
    }
}

#[tokio::test]
async fn read_menu_rejects_invalid_target_without_losing_buffered_stdin() {
    let engine = esh::Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_stdin(b"first\nsecond\n".to_vec());

    assert_eq!(
        engine
            .eval_str("read --menu bad-name\n", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: read: invalid identifier: bad-name\n"
    );
    assert_eq!(
        ctx.take_stdin().as_deref(),
        Some(b"first\nsecond\n".as_slice())
    );
}
