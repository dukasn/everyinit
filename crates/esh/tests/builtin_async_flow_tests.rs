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
async fn test_sleep_and_wait() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        sleep 50ms
        wait
        echo "sleep done"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("sleep done"));
}

#[tokio::test]
async fn wait_reports_unknown_pids_and_rejects_unsupported_job_ids() {
    let engine = Engine::new();

    let mut no_jobs = ExecutionContext::new();
    assert_eq!(engine.eval_str("wait\n", &mut no_jobs).await.unwrap(), 0);

    let mut unknown_pid = ExecutionContext::new();
    assert_eq!(
        engine
            .eval_str("wait 12345\n", &mut unknown_pid)
            .await
            .unwrap(),
        127
    );

    let mut unsupported_job = ExecutionContext::new();
    unsupported_job.enable_capture();
    assert_eq!(
        engine
            .eval_str("wait %1\n", &mut unsupported_job)
            .await
            .unwrap(),
        2
    );
    assert!(
        unsupported_job
            .get_captured_stderr()
            .unwrap()
            .contains("unsupported process identifier '%1'")
    );
}

#[tokio::test]
async fn test_timeout_success_and_expiry() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    // Success within timeout
    let script_ok = r#"
        timeout 500ms echo "fast task completed"
    "#;
    let code_ok = engine.eval_str(script_ok, &mut ctx).await.unwrap();
    assert_eq!(code_ok, 0);
    assert!(
        ctx.get_captured_stdout()
            .unwrap()
            .contains("fast task completed")
    );

    // Expiry timeout
    let mut ctx2 = ExecutionContext::new();
    ctx2.enable_capture();
    let script_timeout = r#"
        timeout 50ms sleep 500ms
    "#;
    let code_timeout = engine.eval_str(script_timeout, &mut ctx2).await.unwrap();
    assert_eq!(code_timeout, 124);
}

#[tokio::test]
async fn invalid_float_durations_return_status_instead_of_panicking() {
    let engine = Engine::new();

    for duration in ["-1", "NaNs", "infs", "1e100s"] {
        for command in [
            format!("sleep {duration}"),
            format!("timeout {duration} true"),
        ] {
            let mut ctx = ExecutionContext::new();
            ctx.enable_capture();
            assert_eq!(
                engine.eval_str(&command, &mut ctx).await.unwrap(),
                1,
                "{command}"
            );
            assert_ne!(ctx.get_captured_stderr().unwrap(), "", "{command}");
        }
    }
}

#[tokio::test]
async fn sleep_rejects_extra_duration_operands() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str("sleep 0 1", &mut ctx).await.unwrap(), 2);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: sleep: expected exactly one duration argument\n"
    );
}

#[tokio::test]
async fn test_retry_attempts() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        retry -n 2 -d 20ms echo "retry succeeded"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    assert!(
        ctx.get_captured_stdout()
            .unwrap()
            .contains("retry succeeded")
    );

    let mut failed_ctx = ExecutionContext::new();
    failed_ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("retry -n 2 -d 0ms -b 2 -- false", &mut failed_ctx)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn retry_rejects_invalid_options_and_backoff_overflow_without_panicking() {
    let engine = Engine::new();
    for script in [
        "retry",
        "retry -n 2",
        "retry -z false",
        "retry -n nope false",
        "retry -n 0 false",
        "retry -n false",
        "retry -d NaNs false",
        "retry -d false",
        "retry -b NaN false",
        "retry -b inf false",
        "retry -b -1 false",
        "retry -b false",
        "retry -n 2 -d 1ms -b 1e308 false",
    ] {
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();
        assert_eq!(
            engine.eval_str(script, &mut ctx).await.unwrap(),
            2,
            "{script}"
        );
        assert_ne!(ctx.get_captured_stderr().unwrap(), "", "{script}");
    }
}
