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
async fn test_pipeline_negation() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        ! [ -f "/non/existent/file.txt" ] && echo "negation passed"
        ! false && echo "false was negated"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("negation passed"));
    assert!(output.contains("false was negated"));
}

#[tokio::test]
async fn test_local_scoping_and_functions() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        GLOBAL_VAR="original"
        
        my_func() {
            local LOCAL_VAR="secret"
            GLOBAL_VAR="mutated"
            echo "Inside func: local=$LOCAL_VAR, global=$GLOBAL_VAR, arg1=$1"
        }
        
        my_func "first_arg"
        echo "Outside func: local=$LOCAL_VAR, global=$GLOBAL_VAR"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("Inside func: local=secret, global=mutated, arg1=first_arg"));
    assert!(output.contains("Outside func: local=, global=mutated"));
}

#[tokio::test]
async fn test_advanced_parameter_expansions() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        FILE="/var/log/app/service.prod.log"
        BASENAME="${FILE##*/}"
        DIRNAME="${FILE%/*}"
        NO_EXT="${BASENAME%.*}"
        
        COMMIT="a1b2c3d4e5f6g7h8"
        SHORT_COMMIT="${COMMIT:0:7}"
        
        TEXT="apple-banana-apple"
        REPLACED_FIRST="${TEXT/apple/orange}"
        REPLACED_ALL="${TEXT//apple/orange}"
        
        echo "basename=$BASENAME"
        echo "dirname=$DIRNAME"
        echo "no_ext=$NO_EXT"
        echo "short_commit=$SHORT_COMMIT"
        echo "replaced_first=$REPLACED_FIRST"
        echo "replaced_all=$REPLACED_ALL"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("basename=service.prod.log"));
    assert!(output.contains("dirname=/var/log/app"));
    assert!(output.contains("no_ext=service.prod"));
    assert!(output.contains("short_commit=a1b2c3d"));
    assert!(output.contains("replaced_first=orange-banana-apple"));
    assert!(output.contains("replaced_all=orange-banana-orange"));
}

#[tokio::test]
async fn test_read_builtin_and_herestrings() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        read -r FIRST SECOND REST <<< "alpha beta gamma"
        echo "FIRST=$FIRST, SECOND=$SECOND, REST=$REST"
        
        read -r SINGLE <<< "single_line_payload"
        echo "SINGLE=$SINGLE"

        read -r <<< "standalone_line"
        echo "REPLY=$REPLY"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("FIRST=alpha, SECOND=beta, REST=gamma"));
    assert!(output.contains("SINGLE=single_line_payload"));
    assert!(output.contains("REPLY=standalone_line"));
}
