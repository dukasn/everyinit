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
