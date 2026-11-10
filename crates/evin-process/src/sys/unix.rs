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

use std::io;
use std::process::Stdio;
use tokio::process::Command;

use crate::{EnvironmentPolicy, OutputPolicy, ProcessError, ProcessMode, ProcessSpec, StdinPolicy};

pub(crate) type Child = tokio::process::Child;
pub(crate) type ChildStdin = tokio::process::ChildStdin;
pub(crate) type ChildStdout = tokio::process::ChildStdout;
pub(crate) type ChildStderr = tokio::process::ChildStderr;

pub(crate) struct SpawnedProcess {
    pub(crate) child: Child,
    pub(crate) tree: ProcessTree,
    pub(crate) stdin: Option<ChildStdin>,
    pub(crate) stdout: Option<ChildStdout>,
    pub(crate) stderr: Option<ChildStderr>,
}

pub(crate) fn spawn(spec: &ProcessSpec) -> Result<SpawnedProcess, ProcessError> {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args);
    command.current_dir(&spec.cwd);
    command.kill_on_drop(true);
    if spec.environment == EnvironmentPolicy::Replace {
        command.env_clear();
    }
    command.envs(&spec.env);
    match &spec.stdin {
        StdinPolicy::Inherit => command.stdin(Stdio::inherit()),
        StdinPolicy::Null => command.stdin(Stdio::null()),
        StdinPolicy::Bytes(_) => command.stdin(Stdio::piped()),
    };
    match spec.stdout {
        OutputPolicy::Inherit => command.stdout(Stdio::inherit()),
        OutputPolicy::Capture => command.stdout(Stdio::piped()),
    };
    match spec.stderr {
        OutputPolicy::Inherit => command.stderr(Stdio::inherit()),
        OutputPolicy::Capture => command.stderr(Stdio::piped()),
    };

    let spawn_context =
        configure_command(&mut command, spec.mode).map_err(ProcessError::Supervision)?;
    let mut child = spawn_command(&mut command).map_err(|source| ProcessError::Spawn {
        program: spec.program.to_string_lossy().into_owned(),
        source,
    })?;
    let tree = match ProcessTree::attach(&child, spec.mode, spawn_context) {
        Ok(tree) => tree,
        Err(error) => {
            let _ = child.start_kill();
            return Err(ProcessError::Supervision(error));
        }
    };
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    Ok(SpawnedProcess {
        child,
        tree,
        stdin,
        stdout,
        stderr,
    })
}

fn spawn_command(command: &mut Command) -> io::Result<Child> {
    let mut attempts = 0_u32;
    loop {
        match command.spawn() {
            Ok(child) => return Ok(child),
            // On Unix, pre_exec forces fork+exec. If another thread forks
            // while a freshly written executable descriptor is being closed,
            // the kernel can briefly return ETXTBSY until the sibling child
            // completes execve.
            Err(error) if error.raw_os_error() == Some(libc::ETXTBSY) && attempts < 25 => {
                attempts += 1;
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(error) => return Err(error),
        }
    }
}

pub(crate) fn configure_command(
    command: &mut Command,
    mode: ProcessMode,
) -> io::Result<SpawnContext> {
    let terminal = TerminalGuard::new(mode == ProcessMode::InteractiveForeground);
    match mode {
        ProcessMode::SupervisedAutomation => {
            // SAFETY: pre_exec runs after fork and before exec. setsid is
            // async-signal-safe and does not access memory shared with other
            // threads. It makes the child both session and process-group
            // leader, detaching automation from the caller's terminal.
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() == -1 {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        ProcessMode::InteractiveForeground => {
            command.process_group(0);
            if terminal.enabled {
                // SAFETY: every libc operation here is async-signal-safe. The
                // signal mask prevents the background child from being
                // stopped while it transfers foreground ownership.
                unsafe {
                    command.pre_exec(|| {
                        let child_pgrp = libc::getpid();
                        let mut mask: libc::sigset_t = std::mem::zeroed();
                        let mut old_mask: libc::sigset_t = std::mem::zeroed();
                        libc::sigemptyset(&mut mask);
                        libc::sigaddset(&mut mask, libc::SIGTTOU);
                        libc::pthread_sigmask(libc::SIG_BLOCK, &mask, &mut old_mask);
                        let result = libc::tcsetpgrp(libc::STDIN_FILENO, child_pgrp);
                        let error = io::Error::last_os_error();
                        libc::pthread_sigmask(libc::SIG_SETMASK, &old_mask, std::ptr::null_mut());
                        if result == -1 {
                            return Err(error);
                        }
                        Ok(())
                    });
                }
            }
        }
    }
    Ok(SpawnContext { terminal })
}

pub(crate) struct SpawnContext {
    terminal: TerminalGuard,
}

pub(crate) struct ProcessTree {
    pid: Option<u32>,
    _terminal: TerminalGuard,
}

impl ProcessTree {
    pub(crate) fn attach(
        child: &Child,
        _mode: ProcessMode,
        spawn_context: SpawnContext,
    ) -> io::Result<Self> {
        let pid = child
            .id()
            .ok_or_else(|| io::Error::other("spawned child has no process id"))?;
        Ok(Self {
            pid: Some(pid),
            _terminal: spawn_context.terminal,
        })
    }

    pub(crate) fn terminate(&self) {
        self.signal(libc::SIGTERM);
    }

    pub(crate) fn kill(&self) {
        self.signal(libc::SIGKILL);
    }

    pub(crate) fn disarm(&mut self) {
        self.pid = None;
    }

    fn signal(&self, signal: libc::c_int) {
        if let Some(pid) = self.pid {
            // SAFETY: a negative PID addresses the process group created by
            // setpgid/setsid. ESRCH simply means the tree is already gone.
            unsafe {
                libc::kill(-(pid as libc::pid_t), signal);
            }
        }
    }
}

impl Drop for ProcessTree {
    fn drop(&mut self) {
        self.kill();
    }
}

struct TerminalGuard {
    enabled: bool,
    parent_pgrp: libc::pid_t,
}

impl TerminalGuard {
    fn new(interactive: bool) -> Self {
        let enabled = interactive && unsafe { libc::isatty(libc::STDIN_FILENO) == 1 };
        Self {
            enabled,
            parent_pgrp: if enabled {
                unsafe { libc::getpgrp() }
            } else {
                0
            },
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if !self.enabled {
            return;
        }
        // SAFETY: the guard owns only the terminal handoff. Blocking SIGTTOU
        // lets a background parent reclaim its controlling terminal.
        unsafe {
            let mut mask: libc::sigset_t = std::mem::zeroed();
            let mut old_mask: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut mask);
            libc::sigaddset(&mut mask, libc::SIGTTOU);
            libc::pthread_sigmask(libc::SIG_BLOCK, &mask, &mut old_mask);
            libc::tcsetpgrp(libc::STDIN_FILENO, self.parent_pgrp);
            libc::pthread_sigmask(libc::SIG_SETMASK, &old_mask, std::ptr::null_mut());
        }
    }
}
