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

use esh::{Engine, ExecutionContext, ScopeManager};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_cancellation_pre_cancelled() {
    let engine = Engine::new();
    let token = CancellationToken::new();
    token.cancel(); // Cancel upfront

    let mut ctx = ExecutionContext::new().with_token(token);
    let status = engine
        .eval_str("echo 'unreachable 1'; echo 'unreachable 2';", &mut ctx)
        .await
        .expect("eval failed");

    assert_eq!(status, 130);
    assert_eq!(ctx.get_var("?").unwrap(), "130");
    assert_eq!(ctx.get_var("ESH_FAILED_CMD").unwrap(), "interrupted");
}

#[tokio::test]
async fn test_cancellation_mid_execution() {
    let engine = Engine::new();
    let token = CancellationToken::new();

    let mut ctx = ExecutionContext::new().with_token(token.clone());
    ctx.enable_capture();

    // Cancel token right before parallel join evaluation
    let script = r#"
        echo "Step 1"
        echo "Step 2"
    "#;

    token.cancel();

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 130);
}

#[tokio::test]
async fn cancellation_during_control_flow_tests_and_expansions_returns_interrupted_status() {
    let engine = Engine::new();

    for script in [
        "if sleep 30; then printf 'wrong-then\\n'; else printf 'wrong-else\\n'; fi",
        "if sleep 30; then printf 'wrong-then\\n'; fi",
        "if false; then printf 'wrong-if\\n'; elif sleep 30; then printf 'wrong-elif\\n'; fi",
        "for item in $(sleep 30); do printf 'wrong-for-body\\n'; done",
        "select item in $(sleep 30); do printf 'wrong-select-body\\n'; done",
        "case $(sleep 30) in value) printf 'wrong-case-body\\n';; esac",
        "while sleep 30; do printf 'wrong-body\\n'; done",
        "until sleep 30; do printf 'wrong-body\\n'; done",
    ] {
        let script = script.replace("sleep 30", long_running_descendant_command());
        let token = CancellationToken::new();
        let cancel = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            cancel.cancel();
        });

        let mut ctx = ExecutionContext::new().with_token(token);
        ctx.enable_capture();
        let status =
            tokio::time::timeout(Duration::from_secs(3), engine.eval_str(&script, &mut ctx))
                .await
                .expect("condition cancellation timed out")
                .expect("evaluation failed");

        assert_eq!(status, 130, "script: {script}");
        assert_eq!(ctx.get_captured_stdout().unwrap(), "", "script: {script}");
        assert!(!ctx.in_loop(), "loop scope leaked for: {script}");
    }
}

#[tokio::test]
async fn cancellation_inside_control_flow_bodies_returns_interrupted_status_and_unwinds_loops() {
    let engine = Engine::new();

    for script in [
        "if true; then sleep 30; fi",
        "for item in one; do if true; then sleep 30; fi; done",
        "while true; do sleep 30; done",
        "until false; do sleep 30; done",
        "case value in value) sleep 30;; esac",
    ] {
        let script = script.replace("sleep 30", long_running_descendant_command());
        let token = CancellationToken::new();
        let cancel = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            cancel.cancel();
        });

        let mut ctx = ExecutionContext::new().with_token(token);
        ctx.enable_capture();
        let status =
            tokio::time::timeout(Duration::from_secs(3), engine.eval_str(&script, &mut ctx))
                .await
                .expect("control-flow body cancellation timed out")
                .expect("evaluation failed");

        assert_eq!(status, 130, "script: {script}");
        assert!(!ctx.in_loop(), "loop scope leaked for: {script}");
    }
}

#[tokio::test]
async fn cancellation_inside_select_body_returns_interrupted_status_and_unwinds_loop() {
    let engine = Engine::new();
    let token = CancellationToken::new();
    let cancel = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        cancel.cancel();
    });

    let mut ctx = ExecutionContext::new().with_token(token);
    ctx.enable_capture();
    ctx.set_stdin(b"1\n".to_vec());
    let status = tokio::time::timeout(
        Duration::from_secs(3),
        engine.eval_str(
            &format!(
                "select choice in one; do {}; done",
                long_running_descendant_command()
            ),
            &mut ctx,
        ),
    )
    .await
    .expect("select body cancellation timed out")
    .expect("evaluation failed");

    assert_eq!(status, 130);
    assert!(
        !ctx.in_loop(),
        "select loop scope leaked after cancellation"
    );
}

#[tokio::test]
async fn cancellation_inside_function_unwinds_function_and_loop_scopes() {
    let engine = engine();
    let token = CancellationToken::new();
    let cancel = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        cancel.cancel();
    });

    let mut ctx = ExecutionContext::new().with_token(token);
    ctx.enable_capture();
    let status = tokio::time::timeout(
        Duration::from_secs(3),
        engine.eval_str(
            &format!(
                "wait_for_server() {{ {}; printf 'wrong-function-tail\\n'; }}\nfor attempt in one two; do wait_for_server; printf 'wrong-loop-tail\\n'; done\nprintf 'wrong-script-tail\\n'",
                long_running_descendant_command()
            ),
            &mut ctx,
        ),
    )
    .await
    .expect("function cancellation timed out")
    .expect("evaluation failed");

    assert_eq!(status, 130);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert!(
        !ctx.in_function(),
        "function scope leaked after cancellation"
    );
    assert!(!ctx.in_loop(), "loop scope leaked after cancellation");
}

#[tokio::test]
async fn cancellation_inside_sourced_file_restores_caller_positionals() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("interruptible.esh"),
        format!(
            "SOURCE_POSITIONALS=\"$#:$1\"\n{}\nprintf 'wrong-source-tail\\n'\n",
            long_running_descendant_command()
        ),
    )
    .unwrap();

    let engine = engine();
    let token = CancellationToken::new();
    let cancel = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        cancel.cancel();
    });

    let mut ctx = ExecutionContext::new().with_token(token);
    ctx.enable_capture();
    ctx.set_cwd(temp.path()).unwrap();
    let status = tokio::time::timeout(
        Duration::from_secs(3),
        engine.eval_str(
            "set -- caller-arg\n. ./interruptible.esh source-arg\nprintf 'wrong-caller-tail\\n'\n",
            &mut ctx,
        ),
    )
    .await
    .expect("sourced-file cancellation timed out")
    .expect("evaluation failed");

    assert_eq!(status, 130);
    assert_eq!(
        ctx.get_var("SOURCE_POSITIONALS").as_deref(),
        Some("1:source-arg")
    );
    assert_eq!(ctx.positional_args_with_zero(), ["esh", "caller-arg"]);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
}

#[tokio::test]
async fn test_captured_external_command_cancels_without_waiting_for_pipe_descendants() {
    let engine = Engine::new();
    let token = CancellationToken::new();
    let cancel = token.clone();
    let mut ctx = ExecutionContext::new().with_token(token);
    ctx.enable_capture();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(25)).await;
        cancel.cancel();
    });

    let status = tokio::time::timeout(
        Duration::from_secs(3),
        engine.eval_str(long_running_descendant_command(), &mut ctx),
    )
    .await
    .expect("external command cancellation timed out")
    .expect("eval failed");

    assert_eq!(status, 130);
}

#[cfg(unix)]
fn long_running_descendant_command() -> &'static str {
    "sh -c 'sleep 30'"
}

#[cfg(windows)]
fn long_running_descendant_command() -> &'static str {
    // `ping.exe` is part of Windows and runs long enough for cancellation to
    // exercise Job Object tree cleanup while retaining cmd.exe's output pipes.
    r#"cmd.exe /d /s /c "ping.exe -n 31 127.0.0.1""#
}

#[cfg(unix)]
#[tokio::test]
async fn test_captured_external_command_finishes_when_a_descendant_keeps_its_pipes_open() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = tokio::time::timeout(
        Duration::from_secs(2),
        engine.eval_str("sh -c 'sleep 30 &'", &mut ctx),
    )
    .await
    .expect("captured command waited for a background descendant")
    .expect("eval failed");

    assert_eq!(status, 0);
}

#[cfg(unix)]
#[tokio::test]
async fn test_captured_external_command_is_not_stopped_by_terminal_job_control() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = tokio::time::timeout(
        Duration::from_secs(2),
        engine.eval_str("sh -c 'kill -TTIN $$; printf resumed'", &mut ctx),
    )
    .await
    .expect("captured command was stopped by terminal job control")
    .expect("eval failed");

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().as_deref(), Some("resumed"));
}

#[cfg(unix)]
#[tokio::test]
async fn test_cancellation_terminates_the_external_process_group() {
    let temp = tempfile::tempdir().unwrap();
    let pid_file = temp.path().join("descendant.pid");
    let token = CancellationToken::new();
    let cancel = token.clone();
    let pid_for_cancel = pid_file.clone();
    let mut ctx = ExecutionContext::new().with_token(token);
    ctx.enable_capture();

    tokio::spawn(async move {
        for _ in 0..100 {
            if pid_for_cancel.exists() {
                cancel.cancel();
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        cancel.cancel();
    });

    let script = format!("sh -c 'sleep 30 & echo $! > {}; wait'", pid_file.display());
    let status = tokio::time::timeout(Duration::from_secs(2), engine().eval_str(&script, &mut ctx))
        .await
        .expect("process-tree cancellation timed out")
        .expect("eval failed");
    assert_eq!(status, 130);

    let pid: i32 = std::fs::read_to_string(pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    for _ in 0..50 {
        if unsafe { libc::kill(pid, 0) } == -1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("descendant process {pid} survived cancellation");
}

#[cfg(unix)]
#[tokio::test]
async fn terminal_sigint_cancels_external_command_and_runs_shell_traps() {
    let temp = tempfile::tempdir().unwrap();
    let engine = engine();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();

    let run = tokio::spawn(async move {
        engine
            .eval_str(
                "trap 'echo INT > int-fired' INT; trap 'echo EXIT > exit-fired' EXIT; sh -c 'echo $$ > root.pid; exec sleep 30'",
                &mut ctx,
            )
            .await
    });

    let root_pid = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let root_pid = std::fs::read_to_string(temp.path().join("root.pid"))
                .ok()
                .and_then(|value| value.trim().parse::<i32>().ok());
            if let Some(root_pid) = root_pid {
                break root_pid;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("external process tree was not created");

    // SAFETY: ESH runs an interactive external command in its own process
    // group; signal that group as a terminal would on Ctrl+C.
    assert_eq!(
        unsafe { libc::kill(-(root_pid as libc::pid_t), libc::SIGINT) },
        0
    );

    assert_eq!(
        tokio::time::timeout(Duration::from_secs(4), run)
            .await
            .expect("ESH did not finish after terminal SIGINT")
            .unwrap()
            .unwrap(),
        130
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join("int-fired")).unwrap(),
        "INT\n"
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join("exit-fired")).unwrap(),
        "EXIT\n"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn cancellation_terminates_external_process_tree_and_runs_int_then_exit_traps() {
    let temp = tempfile::tempdir().unwrap();
    let pid_file = temp.path().join("trapped-descendant.pid");
    let token = CancellationToken::new();
    let cancel = token.clone();
    let pid_for_cancel = pid_file.clone();
    let mut ctx = ExecutionContext::new().with_token(token);
    ctx.set_cwd(temp.path()).unwrap();
    ctx.enable_capture();

    tokio::spawn(async move {
        for _ in 0..200 {
            if pid_for_cancel.exists() {
                cancel.cancel();
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        cancel.cancel();
    });

    let script = "trap 'echo INT' INT; trap 'echo EXIT' EXIT; sh -c 'sleep 30 & echo $! > trapped-descendant.pid; wait'";
    let status = tokio::time::timeout(Duration::from_secs(4), engine().eval_str(script, &mut ctx))
        .await
        .expect("cancellation with traps timed out")
        .expect("eval failed");

    assert_eq!(status, 130);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "INT\nEXIT\n");
    let pid: i32 = std::fs::read_to_string(pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    for _ in 0..50 {
        if unsafe { libc::kill(pid, 0) } == -1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("descendant process {pid} survived trapped cancellation");
}

#[cfg(unix)]
#[tokio::test]
async fn test_parallel_join_cancellation_terminates_each_external_process_group() {
    let temp = tempfile::tempdir().unwrap();
    let pid_files = [temp.path().join("left.pid"), temp.path().join("right.pid")];
    let token = CancellationToken::new();
    let cancel = token.clone();
    let watched_files = pid_files.clone();
    let mut ctx = ExecutionContext::new().with_token(token);
    ctx.set_cwd(temp.path()).unwrap();
    ctx.enable_capture();

    tokio::spawn(async move {
        for _ in 0..200 {
            if watched_files.iter().all(|path| path.exists()) {
                cancel.cancel();
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        cancel.cancel();
    });

    let script = "sh -c 'sleep 30 & echo $! > left.pid; wait' &| sh -c 'sleep 30 & echo $! > right.pid; wait'";
    let status = tokio::time::timeout(Duration::from_secs(4), engine().eval_str(script, &mut ctx))
        .await
        .expect("parallel process-tree cancellation timed out")
        .expect("eval failed");
    assert_eq!(status, 130);

    let pids = pid_files
        .iter()
        .map(|path| {
            std::fs::read_to_string(path)
                .unwrap()
                .trim()
                .parse::<i32>()
                .unwrap()
        })
        .collect::<Vec<_>>();
    for pid in &pids {
        for _ in 0..50 {
            if unsafe { libc::kill(*pid, 0) } == -1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            unsafe { libc::kill(*pid, 0) },
            -1,
            "child process {pid} survived"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn native_workflow_state_survives_parallel_cancellation_and_exit_cleanup() {
    let engine = Engine::new();
    let token = CancellationToken::new();
    let cancel = token.clone();
    let mut ctx = ExecutionContext::new().with_token(token);
    ctx.enable_capture();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
    });

    let script = r#"
        TARGETS=("linux" "macos")
        declare -A CONFIG
        CONFIG[mode]="safe"
        trap 'echo "interrupt"' INT
        trap 'printf "cleanup:%s:%s\n" "${TARGETS[0]}" "${CONFIG[mode]}"' EXIT
        worker() {
          TARGETS[0]="branch"
          CONFIG[mode]="branch"
          sleep 30
        }
        worker &| printf 'parallel:%s:%s\n' "${TARGETS[0]}" "${CONFIG[mode]}"
    "#;
    let status = tokio::time::timeout(Duration::from_secs(5), engine.eval_str(script, &mut ctx))
        .await
        .expect("workflow cancellation must finish")
        .unwrap();

    assert_eq!(status, 130);
    let stdout = ctx.get_captured_stdout().unwrap();
    assert!(stdout.contains("parallel:linux:safe\n"), "{stdout}");
    assert!(stdout.contains("cleanup:linux:safe\n"), "{stdout}");
    assert!(
        !stdout.contains("branch"),
        "parallel branch state leaked: {stdout}"
    );
    assert_eq!(
        stdout.matches("interrupt\n").count(),
        1,
        "the INT trap should run once across the parent and isolated branches: {stdout}"
    );
}

#[tokio::test]
async fn native_workflow_failure_runs_cleanup_without_parallel_state_leaks() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let script = r#"
        set -e
        TARGETS=("linux" "macos")
        declare -A CONFIG
        CONFIG[mode]="safe"
        trap 'printf "cleanup:%s:%s\n" "${TARGETS[0]}" "${CONFIG[mode]}"' EXIT
        worker() {
          TARGETS[0]="branch"
          CONFIG[mode]="branch"
          return 7
        }
        worker &| printf 'join:%s:%s\n' "${TARGETS[0]}" "${CONFIG[mode]}"
    "#;

    let status = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(status, 7);
    let stdout = ctx.get_captured_stdout().unwrap();
    assert!(stdout.contains("join:linux:safe\n"), "{stdout}");
    assert!(stdout.contains("cleanup:linux:safe\n"), "{stdout}");
    assert!(
        !stdout.contains("branch"),
        "parallel branch state leaked: {stdout}"
    );
}

fn engine() -> Engine {
    Engine::new()
}
