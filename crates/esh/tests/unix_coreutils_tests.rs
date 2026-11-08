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
async fn test_dirname_basename_realpath() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        dirname /usr/local/bin/esh
        basename /usr/local/bin/esh
        basename -s .tar.gz /dist/app-v1.0.0.tar.gz
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "/usr/local/bin");
    assert_eq!(lines[1], "esh");
    assert_eq!(lines[2], "app-v1.0.0");
}

#[tokio::test]
async fn test_cut_tr_wc() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        echo "alpha:beta:gamma:delta" | cut -d: -f2,4
        echo "hello world" | tr '[:lower:]' '[:upper:]'
        echo -n "word1 word2 word3" | wc -w
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "beta:delta");
    assert_eq!(lines[1], "HELLO WORLD");
    assert_eq!(lines[2].trim(), "3");
}

#[tokio::test]
async fn test_sha256sum_and_md5sum() {
    let tmp = tempdir().unwrap();
    let file = tmp.path().join("data.txt");
    fs::write(&file, b"hello world").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let _ = ctx.set_cwd(tmp.path());

    let script = r#"
        sha256sum data.txt | cut -d' ' -f1
        md5sum data.txt | cut -d' ' -f1
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines[0],
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
    assert_eq!(lines[1], "5eb63bbbe01eeed093cb22bb8f5acdc3");
}

#[tokio::test]
async fn test_sha256sum_check_manifest_reports_matches_and_mismatches() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("data.txt"), b"hello world").unwrap();
    fs::write(
        tmp.path().join("SHA256SUMS"),
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9  data.txt\n",
    )
    .unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("sha256sum -c SHA256SUMS", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "data.txt: OK\n");

    fs::write(
        tmp.path().join("SHA256SUMS"),
        "0000000000000000000000000000000000000000000000000000000000000000  data.txt\n",
    )
    .unwrap();
    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("sha256sum --check SHA256SUMS", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "data.txt: FAILED\n");
}

#[tokio::test]
async fn test_sort_and_uniq() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let code = engine
        .eval_str(
            r#"
                printf 'pear\napple\napple\nbanana\n' | sort -u
                printf 'apple\napple\nbanana\npear\npear\n' | uniq -c
                printf 'apple\napple\nbanana\npear\npear\n' | uniq -d
                printf 'apple\napple\nbanana\npear\npear\n' | uniq -u
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "apple\nbanana\npear\n      2 apple\n      1 banana\n      2 pear\napple\npear\nbanana\n"
    );
}
