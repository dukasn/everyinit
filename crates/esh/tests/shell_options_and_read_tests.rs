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
        Some("Alice Developer /home/alice /bin/esh")
    );
}
