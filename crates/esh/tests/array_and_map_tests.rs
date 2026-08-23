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
async fn test_indexed_array_creation_and_access() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        PACKAGES=("evin-core" "evin-git" "evin-sh")
        
        echo "first: ${PACKAGES[0]}"
        echo "second: ${PACKAGES[1]}"
        echo "third: ${PACKAGES[2]}"
        echo "all: ${PACKAGES[@]}"
        echo "count: ${#PACKAGES[@]}"
        echo "len0: ${#PACKAGES[0]}"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "first: evin-core");
    assert_eq!(lines[1], "second: evin-git");
    assert_eq!(lines[2], "third: evin-sh");
    assert_eq!(lines[3], "all: evin-core evin-git evin-sh");
    assert_eq!(lines[4], "count: 3");
    assert_eq!(lines[5], "len0: 9");
}

#[tokio::test]
async fn test_indexed_array_mutation() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        ARR=("alpha" "beta")
        ARR[1]="updated_beta"
        ARR[2]="gamma"
        
        echo "elem1: ${ARR[1]}"
        echo "elem2: ${ARR[2]}"
        echo "all: ${ARR[@]}"
        echo "count: ${#ARR[@]}"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "elem1: updated_beta");
    assert_eq!(lines[1], "elem2: gamma");
    assert_eq!(lines[2], "all: alpha updated_beta gamma");
    assert_eq!(lines[3], "count: 3");
}

#[tokio::test]
async fn test_associative_map_creation_and_access() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        declare -A TARGETS
        TARGETS[linux]="x86_64-unknown-linux-musl"
        TARGETS[macos]="aarch64-apple-darwin"
        TARGETS[windows]="x86_64-pc-windows-msvc"
        
        echo "linux: ${TARGETS[linux]}"
        echo "macos: ${TARGETS[macos]}"
        echo "windows: ${TARGETS[windows]}"
        OS_NAME="macos"
        echo "dynamic: ${TARGETS[$OS_NAME]}"
        echo "keys: ${!TARGETS[@]}"
        echo "count: ${#TARGETS[@]}"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "linux: x86_64-unknown-linux-musl");
    assert_eq!(lines[1], "macos: aarch64-apple-darwin");
    assert_eq!(lines[2], "windows: x86_64-pc-windows-msvc");
    assert_eq!(lines[3], "dynamic: aarch64-apple-darwin");
    assert_eq!(lines[4], "keys: linux macos windows");
    assert_eq!(lines[5], "count: 3");
}

#[tokio::test]
async fn test_parameter_modifiers_on_arrays_and_maps() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        PATHS=("/var/log/syslog" "/etc/config.json")
        declare -A HTTP_HEADERS
        HTTP_HEADERS[auth]="Bearer secret_token_12345"
        
        echo "filename: ${PATHS[0]##*/}"
        echo "token: ${HTTP_HEADERS[auth]##* }"
        echo "missing with fallback: ${HTTP_HEADERS[cookie]:-no_cookie_present}"
        echo "sub_slice: ${HTTP_HEADERS[auth]:7:6}"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "filename: syslog");
    assert_eq!(lines[1], "token: secret_token_12345");
    assert_eq!(lines[2], "missing with fallback: no_cookie_present");
    assert_eq!(lines[3], "sub_slice: secret");
}
