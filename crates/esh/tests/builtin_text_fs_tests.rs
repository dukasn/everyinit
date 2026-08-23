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
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_pwd_cat_head_tail() {
    let tmp = tempdir().unwrap();
    let file_path = tmp.path().join("lines.txt");
    fs::write(
        &file_path,
        "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\n",
    )
    .unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let _ = ctx.set_cwd(tmp.path());

    let script = r#"
        pwd
        cat lines.txt | head -n 3
        cat lines.txt | tail -n 2
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("line1"));
    assert!(output.contains("line2"));
    assert!(output.contains("line3"));
    assert!(output.contains("line11"));
    assert!(output.contains("line12"));
}

#[tokio::test]
async fn test_grep_and_glob() {
    let tmp = tempdir().unwrap();
    let sub = tmp.path().join("src");
    fs::create_dir_all(&sub).unwrap();
    fs::write(
        sub.join("main.rs"),
        "fn main() {\n    println!(\"Hello Evin!\");\n}\n",
    )
    .unwrap();
    fs::write(
        sub.join("lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let _ = ctx.set_cwd(tmp.path());

    let script = r#"
        grep -i "hello" src/main.rs
        glob "src/*.rs"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("println!(\"Hello Evin!\");"));
    assert!(output.contains("src/main.rs") || output.contains("src/lib.rs"));
}
