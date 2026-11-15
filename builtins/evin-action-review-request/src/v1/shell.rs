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

use evin_action_sdk::{OutputStream, RunnerContext};
use std::collections::HashMap;

/// Runs a script through ESH and returns its stdout.
pub(super) async fn execute_command(
    ctx: &RunnerContext,
    script: &str,
    env: &HashMap<String, String>,
    show_stdout: bool,
) -> Result<String, String> {
    let mut esh_ctx = match &ctx.cancel_token {
        Some(token) => esh::ExecutionContext::new().with_token(token.clone()),
        None => esh::ExecutionContext::new(),
    };
    let _ = esh_ctx.set_cwd(ctx.fs.task_cwd());
    for (key, value) in ctx.env.iter().chain(env.iter()) {
        esh_ctx.set_env(key.clone(), value.clone());
    }
    esh_ctx.enable_capture();

    let status = esh::Engine::new().eval_str(script, &mut esh_ctx).await;
    let stdout = esh_ctx.get_captured_stdout().unwrap_or_default();
    let stderr = esh_ctx.get_captured_stderr().unwrap_or_default();
    if show_stdout {
        ctx.log.process_output(OutputStream::Stdout, &stdout);
    }
    ctx.log.process_output(OutputStream::Stderr, &stderr);

    let code = match status {
        Ok(code) => code,
        Err(esh::EshError::Exec(e)) => e.exit_code(),
        Err(_) => 2,
    };
    if code != 0 {
        return Err(format!(
            "command failed with exit status {code}: {}",
            script.lines().next().unwrap_or(script)
        ));
    }
    Ok(stdout)
}
