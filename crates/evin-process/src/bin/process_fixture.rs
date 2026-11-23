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

use std::fs::OpenOptions;
use std::io::{BufRead, Read, Write};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

#[cfg(unix)]
static TERMINATED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(unix)]
static INTERRUPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("emit") => {
            print!("fixture-stdout");
            eprint!("fixture-stderr");
        }
        Some("emit-reversed") => {
            std::io::stderr().write_all(b"err").unwrap();
            std::io::stdout().write_all(b"out").unwrap();
        }
        Some("emit-overwrite") => {
            std::io::stderr().write_all(b"e").unwrap();
            std::io::stdout().write_all(b"123456").unwrap();
        }
        Some("burst") => {
            let count = parse_usize(&args, 1);
            for index in 0..count {
                println!("output-{index}");
            }
        }
        Some("echo-stdin") => {
            let mut input = Vec::new();
            std::io::stdin().read_to_end(&mut input).unwrap();
            std::io::stdout().write_all(&input).unwrap();
        }
        Some("emit-until-disconnected") => {
            let mut stdout = std::io::stdout().lock();
            loop {
                if stdout.write_all(b"line\n").is_err() || stdout.flush().is_err() {
                    break;
                }
            }
        }
        Some("read-one-line") => {
            let mut line = Vec::new();
            std::io::stdin()
                .lock()
                .read_until(b'\n', &mut line)
                .unwrap();
            std::io::stdout().write_all(&line).unwrap();
        }
        Some("print-env") => {
            if let Some(name) = args.get(1) {
                print!(
                    "{}",
                    std::env::var_os(name).unwrap_or_default().to_string_lossy()
                );
            }
        }
        Some("exit") => std::process::exit(parse_usize(&args, 1) as i32),
        Some("sleep") => std::thread::sleep(Duration::from_millis(parse_usize(&args, 1) as u64)),
        Some("hold-pipes") => {
            let duration = parse_usize(&args, 1);
            spawn_pipe_holder(duration);
        }
        Some("spawn-tree") => {
            let depth = parse_usize(&args, 1);
            let pid_file = args.get(2).expect("pid file");
            let duration = parse_usize(&args, 3);
            append_pid(Path::new(pid_file));
            if depth > 0 {
                let mut child = Command::new(std::env::current_exe().unwrap())
                    .args([
                        "spawn-tree",
                        &(depth - 1).to_string(),
                        pid_file,
                        &duration.to_string(),
                    ])
                    .spawn()
                    .unwrap();
                let _ = child.wait();
            } else {
                std::thread::sleep(Duration::from_millis(duration as u64));
            }
        }
        #[cfg(unix)]
        Some("interrupt-tree") => {
            let pid_file = args.get(1).expect("pid file");
            append_pid(Path::new(pid_file));
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["ignore-int-child", pid_file])
                .spawn()
                .unwrap();
            // Waiting leaves the parent blocked until the test sends SIGINT.
            // The parent keeps the default action and exits; the child ignores
            // SIGINT so the supervisor must clean up the remaining group.
            let _ = child.wait();
        }
        #[cfg(unix)]
        Some("ignore-int-child") => {
            let pid_file = args.get(1).expect("pid file");
            append_pid(Path::new(pid_file));
            // SAFETY: this fixture deliberately ignores SIGINT to model a
            // server process left behind by its npm parent.
            unsafe {
                libc::signal(libc::SIGINT, libc::SIG_IGN);
            }
            loop {
                std::thread::sleep(Duration::from_secs(30));
            }
        }
        #[cfg(unix)]
        Some("stop-ttin") => {
            // SAFETY: raising a signal in this single-threaded fixture is the
            // behavior under test.
            unsafe {
                libc::raise(libc::SIGTTIN);
            }
            print!("resumed");
        }
        #[cfg(unix)]
        Some("trap-term") => {
            extern "C" fn handle_term(_: libc::c_int) {
                TERMINATED.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            // SAFETY: this fixture installs a signal handler that only writes
            // to an atomic flag, then waits on the main thread.
            unsafe {
                libc::signal(
                    libc::SIGTERM,
                    handle_term as *const () as libc::sighandler_t,
                );
            }
            while !TERMINATED.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
            print!("terminated-gracefully");
        }
        #[cfg(unix)]
        Some("ignore-term") => {
            // SAFETY: deliberately ignoring SIGTERM is the fixture behavior
            // used to verify the supervisor's SIGKILL fallback.
            unsafe {
                libc::signal(libc::SIGTERM, libc::SIG_IGN);
            }
            std::thread::sleep(Duration::from_secs(30));
        }
        #[cfg(unix)]
        Some("interactive-report") => {
            let foreground = unsafe { libc::tcgetpgrp(libc::STDIN_FILENO) };
            let process_group = unsafe { libc::getpgrp() };
            println!("child-foreground={}", foreground == process_group);
        }
        #[cfg(unix)]
        Some("interactive-wait-int") => {
            extern "C" fn handle_int(_: libc::c_int) {
                INTERRUPTED.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            // SAFETY: the handler only writes an atomic flag.
            unsafe {
                libc::signal(libc::SIGINT, handle_int as *const () as libc::sighandler_t);
            }
            println!("child-ready");
            std::io::stdout().flush().unwrap();
            while !INTERRUPTED.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
            println!("child-interrupted");
        }
        #[cfg(unix)]
        Some("interactive-harness") => {
            run_interactive_harness(args.get(1).map(String::as_str).unwrap_or("success"));
        }
        _ => {
            eprintln!("unknown process fixture mode");
            std::process::exit(2);
        }
    }
}

#[cfg(unix)]
fn run_interactive_harness(scenario: &str) {
    use evin_process::{ProcessHandle, ProcessMode, ProcessSpec, ProcessTermination};
    use tokio_util::sync::CancellationToken;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async move {
        if scenario == "spawn-failure" {
            let spec = ProcessSpec::new(
                "/definitely/not/an/evin-process-fixture",
                std::env::current_dir().unwrap(),
                ProcessMode::InteractiveForeground,
            );
            println!("spawn-error={}", ProcessHandle::spawn(spec, None).is_err());
            print_parent_foreground();
            return;
        }

        let child_mode = match scenario {
            "success" => "interactive-report",
            "cancel" => "sleep",
            "ctrl-c" => "interactive-wait-int",
            other => panic!("unknown interactive harness scenario: {other}"),
        };
        let args = if scenario == "cancel" {
            vec![child_mode, "30000"]
        } else {
            vec![child_mode]
        };
        let spec = ProcessSpec::new(
            std::env::current_exe().unwrap(),
            std::env::current_dir().unwrap(),
            ProcessMode::InteractiveForeground,
        )
        .args(args);
        let cancellation = CancellationToken::new();
        if scenario == "cancel" {
            let trigger = cancellation.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                trigger.cancel();
            });
        }
        let result = ProcessHandle::spawn(spec, None)
            .unwrap()
            .wait(cancellation)
            .await
            .unwrap();
        println!(
            "harness-cancelled={}",
            matches!(result.termination, ProcessTermination::Cancelled)
        );
        print_parent_foreground();
    });
}

#[cfg(unix)]
fn print_parent_foreground() {
    let foreground = unsafe { libc::tcgetpgrp(libc::STDIN_FILENO) };
    let process_group = unsafe { libc::getpgrp() };
    println!("parent-foreground={}", foreground == process_group);
    std::io::stdout().flush().unwrap();
}

fn parse_usize(args: &[String], index: usize) -> usize {
    args.get(index)
        .expect("fixture argument")
        .parse()
        .expect("numeric fixture argument")
}

fn append_pid(path: &Path) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    writeln!(file, "{}", std::process::id()).unwrap();
    file.flush().unwrap();
}

#[allow(clippy::zombie_processes)]
fn spawn_pipe_holder(duration: usize) {
    // Intentionally do not wait: the fixture's purpose is to exit while its
    // descendant retains the inherited stdout/stderr pipes. The supervisor
    // under test must terminate that descendant and bound pipe drainage.
    Command::new(std::env::current_exe().unwrap())
        .args(["sleep", &duration.to_string()])
        .spawn()
        .unwrap();
}
