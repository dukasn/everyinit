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
use esh::planner::dispatch::resolve_builtin;
use esh::planner::ir::{CommandTarget, ExecutionStage};

#[test]
fn test_builtin_dispatch_resolution() {
    assert!(resolve_builtin("cd").is_some());
    assert!(resolve_builtin("mkdir").is_some());
    assert!(resolve_builtin("rm").is_some());
    assert!(resolve_builtin("cp").is_some());
    assert!(resolve_builtin("mv").is_some());
    assert!(resolve_builtin("touch").is_some());
    assert!(resolve_builtin("echo").is_some());
    assert!(resolve_builtin("exit").is_some());
    assert!(resolve_builtin("return").is_some());
    assert!(resolve_builtin("export").is_some());
    assert!(resolve_builtin("shift").is_some());
    assert!(resolve_builtin("set").is_some());
    assert!(resolve_builtin("trap").is_some());
    assert!(resolve_builtin("getopts").is_some());
    assert!(resolve_builtin("path").is_some());
    assert!(resolve_builtin("str").is_some());
    assert!(resolve_builtin("json").is_some());
    assert!(resolve_builtin("yaml").is_some());
    assert!(resolve_builtin("mktemp").is_some());
    assert!(resolve_builtin("read").is_some());
    assert!(resolve_builtin("prompt").is_none());
    assert!(resolve_builtin("http").is_some());

    assert!(resolve_builtin("nonexistent_utility_123").is_none());
}

#[test]
fn test_plan_lowering_builtins_vs_external() {
    let script = "mkdir -p ./build && cargo build";
    let plan = compile_script(script).expect("compilation failed");

    assert_eq!(plan.stages.len(), 1);

    match &plan.stages[0] {
        ExecutionStage::Sequential {
            left,
            right,
            require_success,
        } => {
            assert!(*require_success);

            match &**left {
                ExecutionStage::Pipeline(nodes) => {
                    assert_eq!(nodes.len(), 1);
                    assert_eq!(nodes[0].raw_name, "mkdir");
                    assert!(matches!(nodes[0].target, CommandTarget::Builtin(_)));
                }
                _ => panic!("Expected Pipeline for left"),
            }

            match &**right {
                ExecutionStage::Pipeline(nodes) => {
                    assert_eq!(nodes.len(), 1);
                    assert_eq!(nodes[0].raw_name, "cargo");
                    assert!(matches!(nodes[0].target, CommandTarget::External(_)));
                }
                _ => panic!("Expected Pipeline for right"),
            }
        }
        _ => panic!("Expected Sequential stage"),
    }
}
