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
async fn test_expansion_default_modifier() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        echo "port: ${PORT:-8080}"
        PORT=3000
        echo "port set: ${PORT:-8080}"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "port: 8080");
    assert_eq!(lines[1], "port set: 3000");
}

#[tokio::test]
async fn test_expansion_assign_modifier() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        echo "target: ${TARGET:=./build}"
        echo "target after assign: $TARGET"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "target: ./build");
    assert_eq!(lines[1], "target after assign: ./build");
}

#[tokio::test]
async fn test_expansion_length_modifier() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        MESSAGE="hello world"
        echo "len: ${#MESSAGE}"
        set -- "one" "two" "three"
        echo "args len: ${#@}"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "len: 11");
    assert_eq!(lines[1], "args len: 3");
}
