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
use tempfile::tempdir;

#[tokio::test]
async fn test_runtime_echo_and_capture() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("echo 'Hello' 'World'", &mut ctx)
        .await
        .expect("eval failed");

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "Hello World\n");
}

#[tokio::test]
async fn test_external_command_stdout_and_stderr_are_captured() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let esh = env!("CARGO_BIN_EXE_esh");
    let script = format!("'{esh}' --version; '{esh}' help definitely-not-a-builtin || true");
    let status = engine
        .eval_str(&script, &mut ctx)
        .await
        .expect("eval failed");

    assert_eq!(status, 0);
    assert!(ctx.get_captured_stdout().unwrap().starts_with("esh "));
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("no help documentation found")
    );
}

#[tokio::test]
async fn test_runtime_filesystem_builtins() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();

    let script = r#"
        mkdir -p sub/nested
        touch sub/nested/test.txt
        cp sub/nested/test.txt sub/copy.txt
        mv sub/copy.txt sub/renamed.txt
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    assert!(tmp.path().join("sub/nested/test.txt").exists());
    assert!(tmp.path().join("sub/renamed.txt").exists());
    assert!(!tmp.path().join("sub/copy.txt").exists());

    // Clean up with rm -rf
    let rm_script = "rm -rf sub";
    let status_rm = engine
        .eval_str(rm_script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status_rm, 0);
    assert!(!tmp.path().join("sub").exists());
}

#[tokio::test]
async fn test_runtime_variables_and_expansion() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_local("USER_NAME", "EvinDev".to_string());

    let script = r#"
        echo "User is ${USER_NAME}"
        GREETING=Hello echo "$GREETING $USER_NAME"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("User is EvinDev"));
    assert!(output.contains("Hello EvinDev"));
}

#[tokio::test]
async fn test_runtime_command_substitution() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = "echo \"Current status: $(echo active)\"";
    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "Current status: active\n"
    );
}

#[tokio::test]
async fn test_runtime_logical_chains() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        cd /nonexistent_dir_path_98765 || echo "Fallback triggered"
        echo "Step 1" && echo "Step 2"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("Fallback triggered"));
    assert!(output.contains("Step 1"));
    assert!(output.contains("Step 2"));
}

#[tokio::test]
async fn test_runtime_fail_fast_block_telemetry() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        {
            echo "Entering try block"
            cd /nonexistent_dir_path_98765
            echo "Unreachable line"
        } || {
            echo "Caught failure at $ESH_FAILED_CMD (exit code: $?)"
        }
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("Entering try block"));
    assert!(!output.contains("Unreachable line"));
    assert!(output.contains("Caught failure at cd (exit code: 1)"));
}

#[tokio::test]
async fn test_runtime_parallel_joins() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = "echo branch1 &| echo branch2 &| echo branch3";
    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("branch1"));
    assert!(output.contains("branch2"));
    assert!(output.contains("branch3"));
}

#[tokio::test]
async fn test_runtime_pipeline_stream() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = "echo -n 'hello piped world' | echo 'done'";
    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "done\n");
}
