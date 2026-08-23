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
async fn test_str_trim_upper_lower() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        str -t '   hello world   '
        str -u 'evin shell'
        str -l 'EVIN RUNTIME'
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "hello world");
    assert_eq!(lines[1], "EVIN SHELL");
    assert_eq!(lines[2], "evin runtime");
}

#[tokio::test]
async fn test_str_replace_and_split() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        str -r 'world' 'evin' 'hello world'
        str -s ',' 'apple,banana,cherry'
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "hello evin");
    assert_eq!(lines[1], "apple");
    assert_eq!(lines[2], "banana");
    assert_eq!(lines[3], "cherry");
}

#[tokio::test]
async fn test_str_predicates_and_len() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        str -c 'run' 'task-runner' && echo "contains match"
        str --starts-with 'v1' 'v1.0.0' && echo "starts_with match"
        str --ends-with '.esh' 'deploy.esh' && echo "ends_with match"
        str --len '12345'
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("contains match"));
    assert!(output.contains("starts_with match"));
    assert!(output.contains("ends_with match"));
    assert!(output.contains("5"));
}
