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
async fn test_date_and_duration() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        date -u -d @1700000000 +%Y-%m-%d
        duration --human 134
        duration --human 500ms
        duration --human 3661
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "2023-11-14");
    assert_eq!(lines[1], "2m 14s");
    assert_eq!(lines[2], "500ms");
    assert_eq!(lines[3], "1h 1m 1s");
}

#[tokio::test]
async fn test_numfmt_and_stat() {
    let tmp = tempdir().unwrap();
    let file = tmp.path().join("payload.bin");
    fs::write(&file, vec![0u8; 2048]).unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let _ = ctx.set_cwd(tmp.path());

    let script = r#"
        echo "1048576" | numfmt --to=iec
        numfmt --from=iec "50M"
        stat -c %s payload.bin
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "1M");
    assert_eq!(lines[1], "52428800");
    assert_eq!(lines[2], "2048");
}

#[tokio::test]
async fn test_echo_formatting_table_csv() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        echo -n "Connecting..."
        echo " OK"
        echo -e "Line1\tLine2"
        echo -f "Version: v%s.%s.%s\n" "1" "4" "2"
        printf "[%s] Code: %d\n" "INFO" "200"
        echo "service port status" | csv
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("Connecting... OK"));
    assert!(output.contains("Line1\tLine2"));
    assert!(output.contains("Version: v1.4.2"));
    assert!(output.contains("[INFO] Code: 200"));
    assert!(output.contains("service,port,status"));
}

#[tokio::test]
async fn test_printf_repeats_its_format_for_remaining_arguments() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let code = engine
        .eval_str(
            r#"
                printf '%s\n' alpha beta gamma
                printf '%s:%s\n' one two three
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "alpha\nbeta\ngamma\none:two\nthree:\n"
    );
}

#[tokio::test]
async fn test_arithmetic_expansion() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        A=10
        B=5
        RES=$(( (A + B) * 2 - 4 / 2 ))
        echo "Result: $RES"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("Result: 28"));
}
