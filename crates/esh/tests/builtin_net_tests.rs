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
use tokio::net::TcpListener;

#[tokio::test]
async fn http_uses_curl_style_request_options() {
    let tmp = tempdir().unwrap();
    let source = tmp.path().join("response.txt");
    fs::write(&source, "local response").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    // ESH treats backslashes in bare words as escapes. Use URL separators and
    // quote the value so Windows drive paths reach the builtin intact.
    let file_path = source.to_string_lossy().replace('\\', "/");
    let url = format!("file://{file_path}");
    let code = engine
        .eval_str(&format!("http -X GET '{url}'"), &mut ctx)
        .await
        .unwrap();

    assert_eq!(code, 0, "{}", ctx.get_captured_stderr().unwrap());
    assert_eq!(ctx.get_captured_stdout().unwrap(), "local response");

    let mut legacy_ctx = ExecutionContext::new();
    legacy_ctx.enable_capture();
    let code = engine
        .eval_str(&format!("http get {url}"), &mut legacy_ctx)
        .await
        .unwrap();
    assert_eq!(code, 2);
    assert!(
        legacy_ctx
            .get_captured_stderr()
            .unwrap()
            .contains("unexpected operand")
    );
}

#[tokio::test]
async fn test_wait_port_success_and_timeout() {
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        // Some constrained runners prohibit opening even loopback sockets.
        // Keep the integration coverage where sockets are available without
        // making the entire suite fail in those environments.
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("skipping wait_port integration test: {error}");
            return;
        }
        Err(error) => panic!("failed to bind loopback listener: {error}"),
    };
    let port = listener.local_addr().unwrap().port();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = format!("wait_port 127.0.0.1:{} --timeout 1s", port);
    let code = engine.eval_str(&script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);

    // Timeout on nonexistent port
    let mut ctx2 = ExecutionContext::new();
    ctx2.enable_capture();
    let script_fail = "wait_port 127.0.0.1:59999 --timeout 50ms";
    let code_fail = engine.eval_str(script_fail, &mut ctx2).await.unwrap();
    assert_eq!(code_fail, 1);
}
