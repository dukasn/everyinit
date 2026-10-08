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

use std::ffi::OsString;
use std::time::{Duration, Instant};

mod common;
use common::fixture_binary;
use evin_process::{
    EnvironmentPolicy, OutputChunk, OutputPolicy, ProcessHandle, ProcessMode, ProcessSpec,
    ProcessTermination, StdinPolicy,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

fn fixture_spec(mode: &str, args: &[&str]) -> ProcessSpec {
    let mut all_args = vec![OsString::from(mode)];
    all_args.extend(args.iter().map(OsString::from));
    ProcessSpec::new(
        fixture_binary(),
        std::env::current_dir().unwrap(),
        ProcessMode::SupervisedAutomation,
    )
    .args(all_args)
}

#[tokio::test]
async fn captures_exact_output_and_streams_chunks() {
    let (sender, mut receiver) = mpsc::unbounded_channel::<OutputChunk>();
    let result = ProcessHandle::spawn(fixture_spec("emit", &[]), Some(sender))
        .unwrap()
        .wait(CancellationToken::new())
        .await
        .unwrap();

    assert!(matches!(result.termination, ProcessTermination::Exited(status) if status.success()));
    assert_eq!(result.stdout.as_deref(), Some(b"fixture-stdout".as_slice()));
    assert_eq!(result.stderr.as_deref(), Some(b"fixture-stderr".as_slice()));

    let mut streamed = Vec::new();
    while let Ok(chunk) = receiver.try_recv() {
        streamed.extend_from_slice(&chunk.bytes);
    }
    assert!(streamed.windows(14).any(|bytes| bytes == b"fixture-stdout"));
    assert!(streamed.windows(14).any(|bytes| bytes == b"fixture-stderr"));
}

#[tokio::test]
async fn writes_buffered_stdin_and_replaces_environment() {
    let mut spec = fixture_spec("echo-stdin", &[]);
    spec.stdin = StdinPolicy::Bytes(b"buffered input".to_vec());
    let result = ProcessHandle::spawn(spec, None)
        .unwrap()
        .wait(CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.stdout.as_deref(), Some(b"buffered input".as_slice()));

    let spec = fixture_spec("print-env", &["EVIN_PROCESS_TEST"]).environment(
        EnvironmentPolicy::Replace,
        [("EVIN_PROCESS_TEST", "isolated")],
    );
    let result = ProcessHandle::spawn(spec, None)
        .unwrap()
        .wait(CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.stdout.as_deref(), Some(b"isolated".as_slice()));
}

#[tokio::test]
async fn inherits_environment_and_nulls_stdin() {
    let inherited = ProcessHandle::spawn(fixture_spec("print-env", &["PATH"]), None)
        .unwrap()
        .wait(CancellationToken::new())
        .await
        .unwrap();
    assert!(!inherited.stdout.as_deref().unwrap_or_default().is_empty());

    let empty_stdin = ProcessHandle::spawn(fixture_spec("echo-stdin", &[]), None)
        .unwrap()
        .wait(CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(empty_stdin.stdout.as_deref(), Some(b"".as_slice()));
}

#[tokio::test]
async fn preserves_nonzero_exit_status() {
    let result = ProcessHandle::spawn(fixture_spec("exit", &["23"]), None)
        .unwrap()
        .wait(CancellationToken::new())
        .await
        .unwrap();
    assert!(
        matches!(result.termination, ProcessTermination::Exited(status) if status.code() == Some(23))
    );
}

#[tokio::test]
async fn pre_cancelled_token_terminates_the_process() {
    let token = CancellationToken::new();
    token.cancel();
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        ProcessHandle::spawn(fixture_spec("sleep", &["30000"]), None)
            .unwrap()
            .wait(token),
    )
    .await
    .expect("pre-cancelled process did not terminate")
    .unwrap();
    assert!(matches!(result.termination, ProcessTermination::Cancelled));
}

#[tokio::test]
async fn bounds_drain_when_a_descendant_holds_pipes_open() {
    let mut spec = fixture_spec("hold-pipes", &["30000"]);
    spec.drain_grace = Duration::from_millis(100);
    spec.termination_grace = Duration::from_millis(100);
    let started = Instant::now();
    let result = ProcessHandle::spawn(spec, None)
        .unwrap()
        .wait(CancellationToken::new())
        .await
        .unwrap();
    assert!(matches!(result.termination, ProcessTermination::Exited(status) if status.success()));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[cfg(unix)]
#[tokio::test]
async fn detached_automation_is_not_stopped_by_sigttin() {
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        ProcessHandle::spawn(fixture_spec("stop-ttin", &[]), None)
            .unwrap()
            .wait(CancellationToken::new()),
    )
    .await
    .expect("automation process was stopped by terminal job control")
    .unwrap();
    assert_eq!(result.stdout.as_deref(), Some(b"resumed".as_slice()));
}

#[tokio::test]
async fn cancellation_reaches_a_depth_four_process_tree() {
    let temp = tempfile::tempdir().unwrap();
    let pid_file = temp.path().join("tree.pids");
    let mut spec = fixture_spec("spawn-tree", &["4", pid_file.to_str().unwrap(), "30000"]);
    spec.termination_grace = Duration::from_millis(250);
    let token = CancellationToken::new();
    let handle = ProcessHandle::spawn(spec, None).unwrap();
    let waiter = tokio::spawn(handle.wait(token.clone()));

    let pids = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let pids = std::fs::read_to_string(&pid_file)
                .unwrap_or_default()
                .lines()
                .filter_map(|line| line.parse::<u32>().ok())
                .collect::<Vec<_>>();
            if pids.len() == 5 {
                break pids;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fixture process tree was not created");

    token.cancel();
    let result = tokio::time::timeout(Duration::from_secs(3), waiter)
        .await
        .expect("tree cancellation timed out")
        .unwrap()
        .unwrap();
    assert!(matches!(result.termination, ProcessTermination::Cancelled));

    for pid in pids {
        for _ in 0..100 {
            if !process_exists(pid) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!process_exists(pid), "process {pid} survived");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn cancellation_allows_a_sigterm_trap_to_finish() {
    let token = CancellationToken::new();
    let cancel = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
    });
    let result = ProcessHandle::spawn(fixture_spec("trap-term", &[]), None)
        .unwrap()
        .wait(token)
        .await
        .unwrap();
    assert!(matches!(result.termination, ProcessTermination::Cancelled));
    assert_eq!(
        result.stdout.as_deref(),
        Some(b"terminated-gracefully".as_slice())
    );
}

#[cfg(unix)]
#[tokio::test]
async fn cancellation_forces_sigkill_after_the_grace_period() {
    let mut spec = fixture_spec("ignore-term", &[]);
    spec.termination_grace = Duration::from_millis(75);
    let token = CancellationToken::new();
    let trigger = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(25)).await;
        trigger.cancel();
    });
    let started = Instant::now();
    let result = ProcessHandle::spawn(spec, None)
        .unwrap()
        .wait(token)
        .await
        .unwrap();
    assert!(matches!(result.termination, ProcessTermination::Cancelled));
    assert!(started.elapsed() >= Duration::from_millis(75));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn dropping_a_handle_forces_child_cleanup() {
    let handle = ProcessHandle::spawn(fixture_spec("sleep", &["30000"]), None).unwrap();
    let pid = handle.id().unwrap();
    drop(handle);
    for _ in 0..100 {
        if !process_exists(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("dropped child {pid} was not reaped");
}

#[tokio::test]
async fn dropping_the_wait_future_still_reaps_the_child() {
    let handle = ProcessHandle::spawn(fixture_spec("sleep", &["30000"]), None).unwrap();
    let pid = handle.id().unwrap();
    let waiter = tokio::spawn(handle.wait(CancellationToken::new()));
    tokio::time::sleep(Duration::from_millis(20)).await;
    waiter.abort();
    let _ = waiter.await;

    for _ in 0..100 {
        if !process_exists(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("child {pid} survived cancellation of its wait future");
}

#[tokio::test]
async fn dropping_a_handle_forces_descendant_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let pid_file = temp.path().join("drop-tree.pids");
    let handle = ProcessHandle::spawn(
        fixture_spec("spawn-tree", &["1", pid_file.to_str().unwrap(), "30000"]),
        None,
    )
    .unwrap();
    let pids = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let pids = std::fs::read_to_string(&pid_file)
                .unwrap_or_default()
                .lines()
                .filter_map(|line| line.parse::<u32>().ok())
                .collect::<Vec<_>>();
            if pids.len() == 2 {
                break pids;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fixture descendant was not created");

    drop(handle);
    for pid in pids {
        for _ in 0..100 {
            if !process_exists(pid) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!process_exists(pid), "dropped process {pid} survived");
    }
}

#[cfg(windows)]
#[tokio::test]
async fn windows_interactive_mode_accepts_inherited_standard_handles() {
    let spec = ProcessSpec::new(
        fixture_binary(),
        std::env::current_dir().unwrap(),
        ProcessMode::InteractiveForeground,
    )
    .args(["exit", "0"]);
    let result = ProcessHandle::spawn(spec, None)
        .unwrap()
        .wait(CancellationToken::new())
        .await
        .unwrap();
    assert!(matches!(result.termination, ProcessTermination::Exited(status) if status.success()));
}

#[cfg(windows)]
#[tokio::test]
async fn windows_spawns_executable_from_system_path() {
    let spec = ProcessSpec::new(
        "cmd.exe",
        std::env::current_dir().unwrap(),
        ProcessMode::SupervisedAutomation,
    )
    .args(["/d", "/c", "echo", "path-resolved"]);
    let result = ProcessHandle::spawn(spec, None)
        .unwrap()
        .wait(CancellationToken::new())
        .await
        .unwrap();
    assert!(matches!(result.termination, ProcessTermination::Exited(status) if status.success()));
    assert_eq!(
        result
            .stdout
            .as_deref()
            .map(|b| b.strip_suffix(b"\r\n").unwrap_or(b)),
        Some(b"path-resolved".as_slice())
    );
}

#[test]
fn rejects_non_inherited_interactive_stdio() {
    let mut spec = ProcessSpec::new(
        fixture_binary(),
        std::env::current_dir().unwrap(),
        ProcessMode::InteractiveForeground,
    );
    spec.stdout = OutputPolicy::Capture;
    assert!(ProcessHandle::spawn(spec, None).is_err());
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(windows)]
fn process_exists(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if process.is_null() {
        return false;
    }
    let result = unsafe { WaitForSingleObject(process, 0) };
    unsafe {
        CloseHandle(process);
    }
    result == WAIT_TIMEOUT
}
