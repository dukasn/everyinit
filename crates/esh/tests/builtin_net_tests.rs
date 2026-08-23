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
use tokio::net::TcpListener;

#[tokio::test]
async fn test_wait_port_success_and_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
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
