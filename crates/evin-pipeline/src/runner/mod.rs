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
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

const PROCESS_TERMINATION_GRACE: std::time::Duration = std::time::Duration::from_millis(1500);

#[cfg(test)]
mod tests;

pub trait RunnerStrategy {
    fn build_command(&self, script: &str) -> Command;
}

/// Target-dependent process runner implementation.
/// Distinct implementations exist for Unix (`sh -c`) and Windows (`cmd.exe /c`) via conditional compilation.
#[cfg(unix)]
pub struct OsRunner;

#[cfg(unix)]
impl RunnerStrategy for OsRunner {
    fn build_command(&self, script: &str) -> Command {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(script);
        cmd
    }
}

#[cfg(windows)]
pub struct OsRunner;

#[cfg(windows)]
impl RunnerStrategy for OsRunner {
    fn build_command(&self, script: &str) -> Command {
        let mut cmd = Command::new("cmd.exe");
        cmd.arg("/c").arg(script);
        cmd
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

        if let Some(stdout_text) = esh_ctx.get_captured_stdout() {
            for line in stdout_text.lines() {
                let safe_line = ctx.secrets.redact(line);
                if safe_line != line {
                    redaction_count += 1;
                }
                ctx.log
                    .raw_output(evin_action_sdk::OutputStream::Stdout, &safe_line);
            }
        }

        if let Some(stderr_text) = esh_ctx.get_captured_stderr() {
            for line in stderr_text.lines() {
                let safe_line = ctx.secrets.redact(line);
                if safe_line != line {
                    redaction_count += 1;
                }
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
                .arg("status", format!("exit status: {}", exit_code)));
        }

        Ok(())
    }
}

impl ShellRunner<OsRunner> {
    pub fn new_os() -> Self {
        Self { strategy: OsRunner }
    }
}

pub fn kill_process_group(pid: u32) {
    #[cfg(unix)]
    {
        let pgid = pid as i32;
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status();
    }
}

pub fn terminate_process_group(pid: u32) {
    #[cfg(unix)]
    {
        let pgid = pid as i32;
        unsafe {
            libc::kill(-pgid, libc::SIGTERM);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T"])
            .status();
    }
}

struct ProcessGroupGuard {
    pid: Option<u32>,
}

impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            kill_process_group(pid);
        }
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

        let mut cmd = self.strategy.build_command(script);

        // Apply environment and isolation
        cmd.env_clear();
        cmd.envs(&env);

        let cwd_str = ctx.cwd.as_deref().unwrap_or(".");
        let shell_binary = if cfg!(windows) { "cmd.exe" } else { "sh" };
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

        if let Some(cwd) = &ctx.cwd {
            cmd.current_dir(cwd);
        }

        let start_time = std::time::Instant::now();

        // The Zombie Reaper: Tie child process lifecycle to the Command future
        cmd.kill_on_drop(true);
        #[cfg(unix)]
        cmd.process_group(0);

        // Async I/O Multiplexing
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            evin_diagnostic::IssueCode::new("issue-process-spawn-failed")
                .arg("error", e.to_string())
        })?;

        let pid = child.id();
        let mut guard = ProcessGroupGuard { pid };

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let log_out = ctx.log.clone();
        let log_err = ctx.log.clone();

        let redaction_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let secrets_out = ctx.secrets.clone();
        let red_out = redaction_count.clone();
        let stdout_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let safe_line = secrets_out.redact(&line);
                if safe_line != line {
                    red_out.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                log_out.raw_output(evin_action_sdk::OutputStream::Stdout, &safe_line);
            }
        });

        let secrets_err = ctx.secrets.clone();
        let red_err = redaction_count.clone();
        let stderr_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let safe_line = secrets_err.redact(&line);
                if safe_line != line {
                    red_err.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                log_err.raw_output(evin_action_sdk::OutputStream::Stderr, &safe_line);
            }
        });

        let (status_res, cancelled) = if let Some(token) = cancel_token {
            tokio::select! {
                res = child.wait() => (res.map(Some).map_err(|e| {
                    evin_diagnostic::IssueCode::new("issue-process-wait-failed").arg("error", e.to_string())
                }), false),
                _ = token.cancelled() => {
                    if let Some(p) = guard.pid {
                        terminate_process_group(p);
                    }
                    let graceful = tokio::time::timeout(PROCESS_TERMINATION_GRACE, child.wait()).await;
                    if !matches!(graceful, Ok(Ok(_))) {
                        if let Some(p) = guard.pid {
                            kill_process_group(p);
                        }
                        let _ = child.wait().await;
                    }
                    (Ok(None), true)
                }
            }
        } else {
            (
                child.wait().await.map(Some).map_err(|e| {
                    evin_diagnostic::IssueCode::new("issue-process-wait-failed")
                        .arg("error", e.to_string())
                }),
                false,
            )
        };

        // Wait for output streams to finish flushing
        let _ = stdout_handle.await;
        let _ = stderr_handle.await;
        guard.pid = None;

        if cancelled {
            return Err(evin_diagnostic::IssueCode::new(
                "issue-execution-interrupted",
            ));
        }

        let status = status_res?.expect("completed process must have an exit status");

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
                .arg("status", status.to_string()));
        }

        Ok(())
    }
}
