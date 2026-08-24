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

//! Cross-platform ownership of child-process lifecycles for Evin and ESH.
//!
//! Application crates remain responsible for deciding when cancellation is
//! requested. This crate owns how a child is isolated, how its streams are
//! drained, and how its complete process tree is terminated.

mod sys;

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::ExitStatus;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub const DEFAULT_TERMINATION_GRACE: Duration = Duration::from_millis(1500);
pub const DEFAULT_DRAIN_GRACE: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessMode {
    SupervisedAutomation,
    InteractiveForeground,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentPolicy {
    InheritAndOverride,
    Replace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StdinPolicy {
    Inherit,
    Null,
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputPolicy {
    Inherit,
    Capture,
}

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub env: HashMap<OsString, OsString>,
    pub environment: EnvironmentPolicy,
    pub mode: ProcessMode,
    pub stdin: StdinPolicy,
    pub stdout: OutputPolicy,
    pub stderr: OutputPolicy,
    pub termination_grace: Duration,
    pub drain_grace: Duration,
}

impl ProcessSpec {
    pub fn new(program: impl Into<OsString>, cwd: impl Into<PathBuf>, mode: ProcessMode) -> Self {
        let (stdin, stdout, stderr) = match mode {
            ProcessMode::SupervisedAutomation => (
                StdinPolicy::Null,
                OutputPolicy::Capture,
                OutputPolicy::Capture,
            ),
            ProcessMode::InteractiveForeground => (
                StdinPolicy::Inherit,
                OutputPolicy::Inherit,
                OutputPolicy::Inherit,
            ),
        };
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: cwd.into(),
            env: HashMap::new(),
            environment: EnvironmentPolicy::InheritAndOverride,
            mode,
            stdin,
            stdout,
            stderr,
            termination_grace: DEFAULT_TERMINATION_GRACE,
            drain_grace: DEFAULT_DRAIN_GRACE,
        }
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn environment<I, K, V>(mut self, policy: EnvironmentPolicy, env: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
    {
        self.environment = policy;
        self.env = env
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
        self
    }

    fn validate(&self) -> Result<(), ProcessError> {
        if self.mode == ProcessMode::InteractiveForeground
            && (!matches!(self.stdin, StdinPolicy::Inherit)
                || self.stdout != OutputPolicy::Inherit
                || self.stderr != OutputPolicy::Inherit)
        {
            return Err(ProcessError::InvalidSpec(
                "interactive foreground processes must inherit stdin, stdout, and stderr"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputChunk {
    pub stream: OutputStream,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub enum ProcessTermination {
    Exited(ExitStatus),
    Cancelled,
}

#[derive(Debug)]
pub struct ProcessResult {
    pub termination: ProcessTermination,
    pub stdout: Option<Vec<u8>>,
    pub stderr: Option<Vec<u8>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("invalid process specification: {0}")]
    InvalidSpec(String),
    #[error("failed to spawn '{program}': {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to configure process supervision: {0}")]
    Supervision(#[source] std::io::Error),
    #[error("failed while waiting for child process: {0}")]
    Wait(#[source] std::io::Error),
    #[error("failed to read captured {stream}: {source}")]
    Output {
        stream: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("captured {0} reader task failed: {1}")]
    OutputTask(&'static str, tokio::task::JoinError),
}

type ReaderResult = Result<Vec<u8>, std::io::Error>;

pub struct ProcessHandle {
    child: Option<sys::Child>,
    tree: sys::ProcessTree,
    stdin_writer: Option<JoinHandle<()>>,
    stdout_reader: Option<JoinHandle<ReaderResult>>,
    stderr_reader: Option<JoinHandle<ReaderResult>>,
    termination_grace: Duration,
    drain_grace: Duration,
}

impl ProcessHandle {
    pub fn spawn(
        spec: ProcessSpec,
        output: Option<mpsc::UnboundedSender<OutputChunk>>,
    ) -> Result<Self, ProcessError> {
        spec.validate()?;
        let sys::SpawnedProcess {
            child,
            tree,
            stdin,
            stdout,
            stderr,
        } = sys::spawn(&spec)?;

        let stdin_writer = match spec.stdin {
            StdinPolicy::Bytes(bytes) => stdin.map(|mut stdin| {
                tokio::spawn(async move {
                    let _ = stdin.write_all(&bytes).await;
                    let _ = stdin.shutdown().await;
                })
            }),
            StdinPolicy::Inherit | StdinPolicy::Null => None,
        };
        let stdout_reader =
            stdout.map(|reader| spawn_reader(reader, OutputStream::Stdout, output.clone()));
        let stderr_reader = stderr.map(|reader| spawn_reader(reader, OutputStream::Stderr, output));

        Ok(Self {
            child: Some(child),
            tree,
            stdin_writer,
            stdout_reader,
            stderr_reader,
            termination_grace: spec.termination_grace,
            drain_grace: spec.drain_grace,
        })
    }

    pub fn id(&self) -> Option<u32> {
        self.child.as_ref().and_then(sys::Child::id)
    }

    pub async fn wait(
        mut self,
        cancellation: CancellationToken,
    ) -> Result<ProcessResult, ProcessError> {
        let (termination, cancelled) = {
            let child = self
                .child
                .as_mut()
                .expect("process handle can only be awaited once");
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => {
                    self.tree.terminate();
                    let graceful = tokio::time::timeout(
                        self.termination_grace,
                        child.wait(),
                    ).await;
                    if !matches!(graceful, Ok(Ok(_))) {
                        self.tree.kill();
                        let _ = child.wait().await;
                    }
                    (ProcessTermination::Cancelled, true)
                }
                result = child.wait() => {
                    let status = result.map_err(ProcessError::Wait)?;
                    (ProcessTermination::Exited(status), false)
                }
            }
        };
        let _reaped_child = self.child.take();

        if let Some(writer) = self.stdin_writer.take() {
            let _ = writer.await;
        }

        let mut drain = Box::pin(drain_readers(
            self.stdout_reader.take(),
            self.stderr_reader.take(),
        ));
        let (stdout, stderr) = match tokio::time::timeout(self.drain_grace, &mut drain).await {
            Ok(result) => result?,
            Err(_) => {
                self.tree.terminate();
                match tokio::time::timeout(self.drain_grace, &mut drain).await {
                    Ok(result) => result?,
                    Err(_) => {
                        self.tree.kill();
                        drain.await?
                    }
                }
            }
        };

        if !cancelled {
            self.tree.disarm();
        }
        Ok(ProcessResult {
            termination,
            stdout,
            stderr,
        })
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        self.tree.kill();
        let _ = child.start_kill();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = child.wait().await;
            });
        }
    }
}

fn spawn_reader<R>(
    mut reader: R,
    stream: OutputStream,
    output: Option<mpsc::UnboundedSender<OutputChunk>>,
) -> JoinHandle<ReaderResult>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut collected = Vec::new();
        let mut chunk = vec![0_u8; 8192];
        loop {
            let read = reader.read(&mut chunk).await?;
            if read == 0 {
                break;
            }
            let bytes = chunk[..read].to_vec();
            collected.extend_from_slice(&bytes);
            if let Some(sender) = &output {
                let _ = sender.send(OutputChunk { stream, bytes });
            }
        }
        Ok(collected)
    })
}

async fn drain_readers(
    stdout: Option<JoinHandle<ReaderResult>>,
    stderr: Option<JoinHandle<ReaderResult>>,
) -> Result<(Option<Vec<u8>>, Option<Vec<u8>>), ProcessError> {
    let stdout = join_reader(stdout, "stdout");
    let stderr = join_reader(stderr, "stderr");
    let (stdout, stderr) = tokio::join!(stdout, stderr);
    Ok((stdout?, stderr?))
}

async fn join_reader(
    reader: Option<JoinHandle<ReaderResult>>,
    stream: &'static str,
) -> Result<Option<Vec<u8>>, ProcessError> {
    match reader {
        Some(reader) => match reader.await {
            Ok(Ok(bytes)) => Ok(Some(bytes)),
            Ok(Err(source)) => Err(ProcessError::Output { stream, source }),
            Err(error) => Err(ProcessError::OutputTask(stream, error)),
        },
        None => Ok(None),
    }
}

pub fn display_program(program: &OsStr) -> String {
    program.to_string_lossy().into_owned()
}
