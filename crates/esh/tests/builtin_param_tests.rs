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
async fn test_export_builtin() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        LOCAL_VAR="local_value"
        export LOCAL_VAR
        export DIRECT_VAR="direct_value"
        echo "L:$LOCAL_VAR D:$DIRECT_VAR"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    assert_eq!(ctx.env_vars().get("LOCAL_VAR").unwrap(), "local_value");
    assert_eq!(ctx.env_vars().get("DIRECT_VAR").unwrap(), "direct_value");

    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("L:local_value D:direct_value"));
}

#[tokio::test]
async fn test_set_and_shift_builtins() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        set -- "alpha" "beta" "gamma" "delta"
        echo "count: $# first: $1 all: $@"
        shift
        echo "after shift 1: count: $# first: $1"
        shift 2
        echo "after shift 2: count: $# first: $1"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines[0],
        "count: 4 first: alpha all: alpha beta gamma delta"
    );
    assert_eq!(lines[1], "after shift 1: count: 3 first: beta");
    assert_eq!(lines[2], "after shift 2: count: 1 first: delta");
}
