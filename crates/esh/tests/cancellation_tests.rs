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
        Duration::from_secs(1),
        engine.eval_str("sh -c 'sleep 30'", &mut ctx),
    )
    .await
    .expect("external command cancellation timed out")
    .expect("eval failed");

    assert_eq!(status, 130);
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
fn engine() -> Engine {
    Engine::new()
}
