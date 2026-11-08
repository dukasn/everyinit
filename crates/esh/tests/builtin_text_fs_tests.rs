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
async fn head_reads_a_binary_prefix() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("binary.bin"),
        [0x28_u8, 0xb5, 0x2f, 0xfd, 4, 5],
    )
    .unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    let code = engine
        .eval_str("head -c 4 binary.bin", &mut ctx)
        .await
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        ctx.get_captured_stdout_bytes().unwrap(),
        vec![0x28, 0xb5, 0x2f, 0xfd]
    );

    let mut pipeline_ctx = ExecutionContext::new();
    pipeline_ctx.enable_capture();
    pipeline_ctx.set_cwd(tmp.path()).unwrap();
    let code = engine
        .eval_str("head -c 4 binary.bin | base64", &mut pipeline_ctx)
        .await
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(pipeline_ctx.get_captured_stdout().unwrap(), "KLUv/Q==\n");
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

#[tokio::test]
async fn test_fixed_grep_combined_flags_and_recursive_glob() {
    let tmp = tempdir().unwrap();
    let nested = tmp.path().join("tree/one/two");
    fs::create_dir_all(&nested).unwrap();
    fs::write(tmp.path().join("literal.txt"), "a.b\naxb\n").unwrap();
    fs::write(nested.join("leaf.txt"), "leaf\n").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let _ = ctx.set_cwd(tmp.path());

    let code = engine
        .eval_str(
            r#"
                grep -Fn "a.b" literal.txt
                grep -Fq "a.b" literal.txt
                glob "tree/**"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("1:a.b"));
    assert!(output.contains("tree/one/two/leaf.txt"));
}

#[cfg(unix)]
#[tokio::test]
async fn test_executable_predicate_checks_permission_bits() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempdir().unwrap();
    let executable = tmp.path().join("tool");
    let plain = tmp.path().join("plain");
    fs::write(&executable, "#!/bin/sh\n").unwrap();
    fs::write(&plain, "text\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&plain, fs::Permissions::from_mode(0o644)).unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    let _ = ctx.set_cwd(tmp.path());

    assert_eq!(engine.eval_str("test -x tool", &mut ctx).await.unwrap(), 0);
    assert_eq!(engine.eval_str("test -x plain", &mut ctx).await.unwrap(), 1);
}
