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
}
