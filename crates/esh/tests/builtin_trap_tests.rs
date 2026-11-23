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

use esh::runtime::context::TrapSignal;
use esh::runtime::traits::ScopeManager;
use esh::{Engine, ExecutionContext};
use std::fs;
use std::time::Duration;
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_trap_err_handler() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        trap 'echo "caught failure in: $ESH_FAILED_CMD"' ERR
        cd /nonexistent_directory_987654
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 1);

    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("caught failure in: cd"));
}

#[tokio::test]
async fn prefix_assignment_is_restored_after_trap_registration() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_var("ESH_TEST_PREFIX_ASSIGNMENT_SENTINEL", "original");

    let status = engine
        .eval_str(
            "register_error() { ESH_TEST_PREFIX_ASSIGNMENT_SENTINEL=trap trap 'echo handled' ERR; }\nregister_error",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_var("ESH_TEST_PREFIX_ASSIGNMENT_SENTINEL")
            .as_deref(),
        Some("original")
    );
    assert!(ctx.get_trap(TrapSignal::Err).is_some());
}

#[tokio::test]
async fn exit_traps_registered_inside_functions_survive_function_return() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let script = r#"register_exit() {
    trap 'printf "function-exit-trap\n"' EXIT
}
register_exit
printf 'after-function\n'
"#;

    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "after-function\nfunction-exit-trap\n"
    );

    #[cfg(unix)]
    if let Ok(reference) = std::process::Command::new("bash")
        .args(["--posix", "-c", script])
        .output()
    {
        assert!(reference.status.success());
        assert_eq!(reference.stdout, b"after-function\nfunction-exit-trap\n");
    }
}

#[tokio::test]
async fn err_trap_runs_once_for_a_failed_command_and_pipeline_stage() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "trap 'echo handled' ERR\ncd /nonexistent_directory_987654\n",
            &mut ctx,
        )
        .await
        .unwrap();
    assert_eq!(status, 1);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "handled\n");

    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let status = engine
        .eval_str(
            "trap 'echo handled' ERR\necho input | cd /nonexistent_directory_987654\n",
            &mut ctx,
        )
        .await
        .unwrap();
    assert_eq!(status, 1);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "handled\n");
}

#[tokio::test]
async fn err_traps_in_parallel_branches_are_buffered_in_source_order() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                trap 'printf "err:%s\n" "$ESH_FAILED_CMD"' ERR
                first() { printf 'first-before\n'; false; }
                second() { printf 'second-before\n'; false; }
                first &| second
                printf 'join-status=%s\n' "$?"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "first-before\nerr:false\nerr:first\nsecond-before\nerr:false\nerr:second\njoin-status=1\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn isolated_parallel_branches_run_local_exit_traps_in_source_order() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                trap 'echo parent-exit' EXIT
                left() { trap 'echo left-exit' EXIT; echo left-work; }
                right() { trap 'echo right-exit' EXIT; echo right-work; }
                left &| right
                echo joined
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "left-work\nleft-exit\nright-work\nright-exit\njoined\nparent-exit\n"
    );
}

#[tokio::test]
async fn err_trap_runs_for_each_failure_inside_conditions_and_logical_lists() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "trap 'printf \"failed:%s\\n\" \"$ESH_FAILED_CMD\"' ERR\nif false; then :; fi\nfalse || :\ntrue && false\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 1);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "failed:false\nfailed:false\nfailed:false\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn err_trap_reports_inner_function_and_call_failure_locations() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"trap 'printf "%s:%s:%s\n" "$ESH_FAILED_CMD" "$ESH_FAILED_LINE" "$?"' ERR
fail_inside() {
  false
}
fail_inside
"#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 1);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "false:3:1\nfail_inside:5:1\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn err_trap_observes_failures_inside_command_substitution_and_parent_command() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "trap 'echo handled >&2' ERR\ntest \"$(test 1 -eq 2)\" = x\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 1);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "handled\nhandled\n");
}

#[tokio::test]
async fn failing_err_handler_does_not_recurse_or_replace_original_status() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "COUNT=0\ntrap 'COUNT=$((COUNT + 1)); test 1 -eq 2' ERR\ntest 1 -eq 2\nSAVED=$?\nprintf '%s:%s\\n' \"$COUNT\" \"$SAVED\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "1:1\n");
}

#[tokio::test]
async fn test_trap_exit_handler() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        trap "echo 'cleaning up before exit'" EXIT
        echo "executing workload"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("executing workload"));
    assert!(output.contains("cleaning up before exit"));
}

#[tokio::test]
async fn isolated_subshells_and_command_substitutions_do_not_run_parent_exit_trap() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                trap 'echo cleanup' EXIT
                (
                    echo subshell
                    exit 9
                    echo wrong-subshell-tail
                )
                VALUE=$(printf 'substitution'; exit 7)
                printf 'value=<%s> status=%s\n' "$VALUE" "$?"
                echo parent
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "subshell\nvalue=<substitution> status=7\nparent\ncleanup\n"
    );
}

#[tokio::test]
async fn failing_exit_handler_does_not_replace_script_status() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();

    assert_eq!(
        engine
            .eval_str("trap 'test 1 -eq 2' EXIT\ntest 1 -eq 2\n", &mut ctx)
            .await
            .unwrap(),
        1
    );

    assert_eq!(
        engine
            .eval_str("trap 'test 1 -eq 2' EXIT\ntest 1 -eq 1\n", &mut ctx)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn exit_trap_observes_the_status_that_triggered_it() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "trap 'printf \"exit-status=%s\\n\" \"$?\"' EXIT\ntest 1 -eq 2\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 1);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "exit-status=1\n");
}

#[tokio::test]
async fn prefix_assignment_to_exit_is_restored_before_exit_trap_runs() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "TEMP=base\ntrap 'printf \"<%s>\\n\" \"$TEMP\"' EXIT\nTEMP=prefix exit 0\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "<base>\n");
    assert_eq!(ctx.get_var("TEMP").as_deref(), Some("base"));
}

#[tokio::test]
async fn exit_trap_runs_for_explicit_exit_and_errexit_without_changing_status() {
    let engine = Engine::new();
    let mut explicit_exit = ExecutionContext::new();
    explicit_exit.enable_capture();

    assert_eq!(
        engine
            .eval_str(
                "trap 'printf \"cleanup:%s\\n\" \"$?\"' EXIT\nexit 7\n",
                &mut explicit_exit,
            )
            .await
            .unwrap(),
        7
    );
    assert_eq!(explicit_exit.get_captured_stdout().unwrap(), "cleanup:7\n");

    let mut errexit = ExecutionContext::new();
    errexit.enable_capture();
    assert_eq!(
        engine
            .eval_str(
                "trap 'echo cleanup' EXIT\nset -e\ntest 1 -eq 2\necho unreachable\n",
                &mut errexit
            )
            .await
            .unwrap(),
        1
    );
    assert_eq!(errexit.get_captured_stdout().unwrap(), "cleanup\n");
}

#[tokio::test]
async fn err_and_exit_traps_run_in_order_for_errexit_failure() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"trap 'printf "err:%s:%s\n" "$ESH_FAILED_CMD" "$?"' ERR
trap 'printf "exit:%s\n" "$?"' EXIT
set -e
false
echo unreachable
"#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 1);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "err:false:1\nexit:1\n");
    assert!(ctx.is_exiting());
}

#[tokio::test]
async fn exit_trap_runs_when_execution_returns_an_error_and_context_can_run_again() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let error = engine
        .eval_str("trap 'echo cleanup' EXIT\nVALUE=$((1 / 0))", &mut ctx)
        .await
        .expect_err("division by zero must fail evaluation");
    assert!(error.to_string().contains("Division by zero"));
    assert_eq!(ctx.get_captured_stdout().unwrap(), "cleanup\n");

    assert_eq!(engine.eval_str("echo next", &mut ctx).await.unwrap(), 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "cleanup\nnext\ncleanup\n"
    );
}

#[tokio::test]
async fn trap_registers_and_resets_multiple_conditions_and_distinguishes_ignore() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();

    engine
        .eval_str("trap '' INT ERR\n", &mut ctx)
        .await
        .unwrap();
    assert!(ctx.get_trap(TrapSignal::Interrupt).is_some());
    assert!(ctx.get_trap(TrapSignal::Err).is_some());

    engine.eval_str("trap - INT ERR\n", &mut ctx).await.unwrap();
    assert!(ctx.get_trap(TrapSignal::Interrupt).is_none());
    assert!(ctx.get_trap(TrapSignal::Err).is_none());
}

#[tokio::test]
async fn trap_zero_aliases_exit_for_registration_and_reset() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();

    engine.eval_str("trap '' 0\n", &mut ctx).await.unwrap();
    assert!(ctx.get_trap(TrapSignal::Exit).is_some());

    engine.eval_str("trap - 0\n", &mut ctx).await.unwrap();
    assert!(ctx.get_trap(TrapSignal::Exit).is_none());
}

#[tokio::test]
async fn trap_condition_without_action_lists_or_leaves_registration_unchanged() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine.eval_str("trap INT\n", &mut ctx).await.unwrap();
    assert_eq!(status, 0);
    assert!(ctx.get_trap(TrapSignal::Interrupt).is_none());
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
}

#[tokio::test]
async fn trap_registration_still_requires_at_least_one_condition() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str("trap 'echo never'\n", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert!(ctx.get_trap(TrapSignal::Exit).is_none());
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("usage: trap <action> <SIGNAL...>")
    );
}

#[tokio::test]
async fn trap_listing_round_trips_shell_quoted_actions() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    let status = engine
        .eval_str(
            "trap \"printf 'clean'\" EXIT\ntrap >traps.txt\ntrap - EXIT\n. traps.txt\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "clean");
    assert_eq!(
        fs::read_to_string(tmp.path().join("traps.txt")).unwrap(),
        "trap 'printf '\\''clean'\\''' EXIT\n"
    );
}

#[tokio::test]
async fn trap_usage_errors_are_status_two_and_keep_prior_registration() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "trap 'echo old-handler' EXIT\ntrap 'if' EXIT\nprintf 'parse:%s\\n' \"$?\"\ntrap '' UNKNOWN\nprintf 'signal:%s\\n' \"$?\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "parse:2\nsignal:2\nold-handler\n"
    );
    let stderr = ctx.get_captured_stderr().unwrap();
    assert!(stderr.contains("invalid handler script"));
    assert!(stderr.contains("unknown signal 'UNKNOWN'"));

    let mut errexit_ctx = ExecutionContext::new();
    errexit_ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str(
                "trap 'echo cleanup' EXIT\nset -e\ntrap 'if' EXIT\necho unreachable\n",
                &mut errexit_ctx,
            )
            .await
            .unwrap(),
        2
    );
    assert!(errexit_ctx.is_exiting());
    assert_eq!(errexit_ctx.get_captured_stdout().unwrap(), "cleanup\n");
}

#[tokio::test]
async fn interrupt_trap_runs_once_after_cancellation() {
    let engine = Engine::new();
    let token = CancellationToken::new();
    let mut ctx = ExecutionContext::new().with_token(token.clone());
    ctx.enable_capture();
    engine
        .eval_str(
            "trap 'echo interrupted' INT\ntrap 'echo cleanup' EXIT\n",
            &mut ctx,
        )
        .await
        .unwrap();
    ctx.enable_capture();

    token.cancel();
    assert_eq!(
        engine
            .eval_str("echo never-runs\n", &mut ctx)
            .await
            .unwrap(),
        130
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "interrupted\ncleanup\n");

    let token = CancellationToken::new();
    let cancel = token.clone();
    let mut ctx = ExecutionContext::new().with_token(token);
    ctx.enable_capture();
    engine
        .eval_str(
            "trap 'echo interrupted' INT\ntrap 'echo cleanup' EXIT\n",
            &mut ctx,
        )
        .await
        .unwrap();
    ctx.enable_capture();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(25)).await;
        cancel.cancel();
    });

    assert_eq!(engine.eval_str("sleep 30\n", &mut ctx).await.unwrap(), 130);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "interrupted\ncleanup\n");
}
