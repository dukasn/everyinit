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

use std::collections::HashMap;
use std::ffi::OsString;

use evin_process::{
    EnvironmentPolicy, OutputChunk, OutputStream, ProcessHandle, ProcessMode, ProcessSpec,
    ProcessTermination,
};
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

pub trait RunnerStrategy {
    fn command(&self, script: &str) -> (OsString, Vec<OsString>);
}

/// Target-dependent process runner implementation.
/// Distinct implementations exist for Unix (`sh -c`) and Windows (`cmd.exe /c`) via conditional compilation.
#[cfg(unix)]
pub struct OsRunner;

#[cfg(unix)]
impl RunnerStrategy for OsRunner {
    fn command(&self, script: &str) -> (OsString, Vec<OsString>) {
        ("sh".into(), vec!["-c".into(), script.into()])
    }
}

#[cfg(windows)]
pub struct OsRunner;

#[cfg(windows)]
impl RunnerStrategy for OsRunner {
    fn command(&self, script: &str) -> (OsString, Vec<OsString>) {
        ("cmd.exe".into(), vec!["/c".into(), script.into()])
    }
}

pub struct EshRunner {
    engine: esh::Engine,
}

impl Default for EshRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl EshRunner {
    pub fn new() -> Self {
        Self {
            engine: esh::Engine::new(),
        }
    }

    pub fn engine(&self) -> &esh::Engine {
        &self.engine
    }
}

pub use evin_action_sdk::{LogMessage, RunnerContext};

pub struct ShellRunner<S = EshRunner> {
    strategy: S,
}

impl Default for ShellRunner<EshRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellRunner<EshRunner> {
    pub fn new() -> Self {
        Self {
            strategy: EshRunner::new(),
        }
    }

    pub fn new_with_engine(engine: esh::Engine) -> Self {
        Self {
            strategy: EshRunner { engine },
        }
    }

    pub async fn run(
        &self,
        ctx: &RunnerContext,
        script: &str,
        env: HashMap<String, String>,
        cancel_token: Option<&tokio_util::sync::CancellationToken>,
    ) -> Result<(), evin_diagnostic::IssueCode> {
        if ctx.mode == evin_action_sdk::PipelineMode::DryRun {
            ctx.log.plan(evin_action_sdk::activity::ShellPlan {
                script: script.to_string(),
            });
            ctx.mark_unverified(format!(
                "Shell script was skipped: {}",
                script.lines().next().unwrap_or(script)
            ));
            return Ok(());
        }

        let cwd_str = ctx.cwd.as_deref().unwrap_or(".");
        ctx.log
            .debug_event(evin_action_sdk::debug::SubprocessStart {
                command: script.to_string(),
                cwd: cwd_str.to_string(),
                shell: "esh".to_string(),
            });

        if !env.is_empty() {
            let mut keys: Vec<String> = env.keys().cloned().collect();
            keys.sort();
            ctx.log
                .debug_event(evin_action_sdk::debug::SubprocessEnvironment {
                    injected_vars: keys,
                });
        }

        let mut esh_ctx = if let Some(token) = cancel_token {
            esh::ExecutionContext::new().with_token(token.clone())
        } else {
            esh::ExecutionContext::new()
        };
        let state_file = tempfile::NamedTempFile::new().map_err(|error| {
            evin_diagnostic::IssueCode::new("issue-task-state-output-failed")
                .arg("error", error.to_string())
        })?;
        let state_path = state_file.into_temp_path();
        let mut env = env;
        env.insert(
            "EVIN_STATE".to_string(),
            state_path.to_string_lossy().into_owned(),
        );
        if let Some(cwd) = &ctx.cwd {
            let _ = esh_ctx.set_cwd(cwd);
        }
        for (k, v) in env {
            esh_ctx.set_env(k, v);
        }
        esh_ctx.enable_capture();

        let start_time = std::time::Instant::now();

        let status_res = self.strategy.engine.eval_str(script, &mut esh_ctx).await;

        let mut redaction_count = 0usize;

        let state_keys = capture_state_outputs(ctx, &state_path)?;
        if !state_keys.is_empty() {
            ctx.log
                .debug_event(evin_action_sdk::debug::SubprocessStateOutput { keys: state_keys });
        }

        if let Some(stdout_text) = esh_ctx.get_captured_stdout() {
            for line in stdout_text.lines() {
                let (safe_line, count) = ctx.secrets.redact_with_count(line);
                redaction_count += count;
                ctx.log
                    .raw_output(evin_action_sdk::OutputStream::Stdout, &safe_line);
            }
        }

        if let Some(stderr_text) = esh_ctx.get_captured_stderr() {
            for line in stderr_text.lines() {
                let (safe_line, count) = ctx.secrets.redact_with_count(line);
                redaction_count += count;
                ctx.log
                    .raw_output(evin_action_sdk::OutputStream::Stderr, &safe_line);
            }
        }

        let duration = start_time.elapsed();
        let exit_code = match &status_res {
            Ok(code) => *code,
            Err(e) => match e {
                esh::EshError::Exec(exec_err) => exec_err.exit_code(),
                esh::EshError::Parse(_) | esh::EshError::Lex(_) => 2,
            },
        };

        ctx.log
            .debug_event(evin_action_sdk::debug::SubprocessFinish {
                duration_ms: duration.as_millis(),
                exit_code: Some(exit_code as i64),
            });

        if redaction_count > 0 {
            ctx.log
                .debug_event(evin_action_sdk::debug::SubprocessRedaction { redaction_count });
        }

        if cancel_token.is_some_and(|token| token.is_cancelled()) || exit_code == 130 {
            return Err(evin_diagnostic::IssueCode::new(
                "issue-execution-interrupted",
            ));
        }

        if exit_code != 0 {
            return Err(evin_diagnostic::IssueCode::new("issue-task-exit-failed")
                .arg("task_name", &ctx.task_name)
                .arg("status", exit_code.to_string()));
        }

        Ok(())
    }
}

fn capture_state_outputs(
    ctx: &RunnerContext,
    path: &std::path::Path,
) -> Result<Vec<String>, evin_diagnostic::IssueCode> {
    let content = std::fs::read_to_string(path).map_err(|error| {
        evin_diagnostic::IssueCode::new("issue-task-state-output-failed")
            .arg("error", error.to_string())
    })?;
    let mut keys = std::collections::BTreeSet::new();
    for line in content.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.is_empty() {
            continue;
        }
        ctx.state.set_output(key, value);
        ctx.secrets.register_output_value_for_redaction(key, value);
        keys.insert(key.to_string());
    }
    Ok(keys.into_iter().collect())
}

impl ShellRunner<OsRunner> {
    pub fn new_os() -> Self {
        Self { strategy: OsRunner }
    }
}

impl<S: RunnerStrategy> ShellRunner<S> {
    pub fn new_with_strategy(strategy: S) -> Self {
        Self { strategy }
    }

    pub async fn run_os(
        &self,
        ctx: &RunnerContext,
        script: &str,
        env: HashMap<String, String>,
        cancel_token: Option<&tokio_util::sync::CancellationToken>,
    ) -> Result<(), evin_diagnostic::IssueCode> {
        if ctx.mode == evin_action_sdk::PipelineMode::DryRun {
            ctx.log.plan(evin_action_sdk::activity::ShellPlan {
                script: script.to_string(),
            });
            ctx.mark_unverified(format!(
                "Shell script was skipped: {}",
                script.lines().next().unwrap_or(script)
            ));
            return Ok(());
        }

        let cwd_str = ctx.cwd.as_deref().unwrap_or(".");
        let shell_binary = if cfg!(windows) { "cmd.exe" } else { "sh" };
        #[cfg(windows)]
        let env = {
            let mut env = env;
            preserve_platform_shell_environment(&mut env);
            env
        };
        ctx.log
            .debug_event(evin_action_sdk::debug::SubprocessStart {
                command: script.to_string(),
                cwd: cwd_str.to_string(),
                shell: shell_binary.to_string(),
            });

        if !env.is_empty() {
            let mut keys: Vec<String> = env.keys().cloned().collect();
            keys.sort();
            ctx.log
                .debug_event(evin_action_sdk::debug::SubprocessEnvironment {
                    injected_vars: keys,
                });
        }

        let start_time = std::time::Instant::now();
        let (program, args) = self.strategy.command(script);
        let spec = ProcessSpec::new(
            program,
            ctx.cwd.as_deref().unwrap_or("."),
            ProcessMode::SupervisedAutomation,
        )
        .args(args)
        .environment(EnvironmentPolicy::Replace, env);
        let (output_sender, output_receiver) = tokio::sync::mpsc::unbounded_channel();
        let handle = ProcessHandle::spawn(spec, Some(output_sender)).map_err(|error| {
            evin_diagnostic::IssueCode::new("issue-process-spawn-failed")
                .arg("error", error.to_string())
        })?;

        let redaction_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let output_task = tokio::spawn(forward_process_output(
            output_receiver,
            ctx.log.clone(),
            ctx.secrets.clone(),
            redaction_count.clone(),
        ));
        let cancellation = cancel_token.cloned().unwrap_or_else(CancellationToken::new);
        let result = handle.wait(cancellation).await.map_err(|error| {
            evin_diagnostic::IssueCode::new("issue-process-wait-failed")
                .arg("error", error.to_string())
        })?;
        let _ = output_task.await;

        let status = match result.termination {
            ProcessTermination::Cancelled => {
                return Err(evin_diagnostic::IssueCode::new(
                    "issue-execution-interrupted",
                ));
            }
            ProcessTermination::Exited(status) => status,
        };

        let duration = start_time.elapsed();
        ctx.log
            .debug_event(evin_action_sdk::debug::SubprocessFinish {
                duration_ms: duration.as_millis(),
                exit_code: status.code().map(i64::from),
            });

        let red_total = redaction_count.load(std::sync::atomic::Ordering::Relaxed);
        if red_total > 0 {
            ctx.log
                .debug_event(evin_action_sdk::debug::SubprocessRedaction {
                    redaction_count: red_total,
                });
        }

        if !status.success() {
            return Err(evin_diagnostic::IssueCode::new("issue-task-exit-failed")
                .arg("task_name", &ctx.task_name)
                .arg(
                    "status",
                    status
                        .code()
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| status.to_string()),
                ));
        }

        Ok(())
    }
}

#[cfg(windows)]
fn preserve_platform_shell_environment(env: &mut HashMap<String, String>) {
    for name in [
        "SystemRoot",
        "WINDIR",
        "COMSPEC",
        "TEMP",
        "TMP",
        "USERPROFILE",
    ] {
        if env.keys().any(|key| key.eq_ignore_ascii_case(name)) {
            continue;
        }
        if let Some((_, value)) =
            std::env::vars_os().find(|(key, _)| key.to_string_lossy().eq_ignore_ascii_case(name))
        {
            env.insert(name.to_string(), value.to_string_lossy().into_owned());
        }
    }
}

async fn forward_process_output(
    mut receiver: tokio::sync::mpsc::UnboundedReceiver<OutputChunk>,
    log: evin_action_sdk::Logger,
    secrets: std::sync::Arc<evin_action_sdk::secrets::SecretsProvider>,
    redaction_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    while let Some(chunk) = receiver.recv().await {
        let (buffer, stream) = match chunk.stream {
            OutputStream::Stdout => (&mut stdout, evin_action_sdk::OutputStream::Stdout),
            OutputStream::Stderr => (&mut stderr, evin_action_sdk::OutputStream::Stderr),
        };
        buffer.extend_from_slice(&chunk.bytes);
        emit_complete_lines(buffer, stream, &log, &secrets, &redaction_count);
    }
    emit_final_line(
        &mut stdout,
        evin_action_sdk::OutputStream::Stdout,
        &log,
        &secrets,
        &redaction_count,
    );
    emit_final_line(
        &mut stderr,
        evin_action_sdk::OutputStream::Stderr,
        &log,
        &secrets,
        &redaction_count,
    );
}

fn emit_complete_lines(
    buffer: &mut Vec<u8>,
    stream: evin_action_sdk::OutputStream,
    log: &evin_action_sdk::Logger,
    secrets: &evin_action_sdk::secrets::SecretsProvider,
    redaction_count: &std::sync::atomic::AtomicUsize,
) {
    while let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
        let mut bytes: Vec<_> = buffer.drain(..=newline).collect();
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        emit_line(&bytes, stream, log, secrets, redaction_count);
    }
}

fn emit_final_line(
    buffer: &mut Vec<u8>,
    stream: evin_action_sdk::OutputStream,
    log: &evin_action_sdk::Logger,
    secrets: &evin_action_sdk::secrets::SecretsProvider,
    redaction_count: &std::sync::atomic::AtomicUsize,
) {
    if !buffer.is_empty() {
        emit_line(buffer, stream, log, secrets, redaction_count);
        buffer.clear();
    }
}

fn emit_line(
    bytes: &[u8],
    stream: evin_action_sdk::OutputStream,
    log: &evin_action_sdk::Logger,
    secrets: &evin_action_sdk::secrets::SecretsProvider,
    redaction_count: &std::sync::atomic::AtomicUsize,
) {
    let line = String::from_utf8_lossy(bytes);
    let (safe_line, count) = secrets.redact_with_count(&line);
    if count > 0 {
        redaction_count.fetch_add(count, std::sync::atomic::Ordering::Relaxed);
    }
    log.raw_output(stream, &safe_line);
}
