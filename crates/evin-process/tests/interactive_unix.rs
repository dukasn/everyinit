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

#![cfg(unix)]

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::unix::AsyncFd;
use tokio::process::{Child, Command};

struct OwnedFd(RawFd);

impl AsRawFd for OwnedFd {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

impl Drop for OwnedFd {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.0);
        }
    }
}

struct Harness {
    master: AsyncFd<OwnedFd>,
    child: Child,
    output: Vec<u8>,
}

impl Harness {
    async fn launch(scenario: &str) -> Self {
        let mut master = -1;
        let mut slave = -1;
        let opened = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(opened, 0, "openpty failed: {}", io::Error::last_os_error());

        let flags = unsafe { libc::fcntl(master, libc::F_GETFL) };
        assert_ne!(flags, -1);
        assert_ne!(
            unsafe { libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK) },
            -1
        );

        let mut command = Command::new(fixture_binary());
        command.args(["interactive-harness", scenario]);
        command.stdin(duplicate_stdio(slave));
        command.stdout(duplicate_stdio(slave));
        command.stderr(duplicate_stdio(slave));
        command.kill_on_drop(true);

        // SAFETY: the child performs only async-signal-safe operations. It
        // creates a session, claims the PTY slave as its controlling terminal,
        // and starts as that terminal's foreground process group.
        unsafe {
            command.pre_exec(move || {
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                if libc::ioctl(slave, libc::TIOCSCTTY as _, 0) == -1 {
                    return Err(io::Error::last_os_error());
                }
                if libc::tcsetpgrp(slave, libc::getpgrp()) == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }

        let child = command.spawn().unwrap();
        unsafe {
            libc::close(slave);
        }
        let master = AsyncFd::new(OwnedFd(master)).unwrap();
        Self {
            master,
            child,
            output: Vec::new(),
        }
    }

    async fn read_until(&mut self, expected: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !String::from_utf8_lossy(&self.output).contains(expected) {
                let mut ready = self.master.readable().await.unwrap();
                let mut buffer = [0_u8; 1024];
                let read = unsafe {
                    libc::read(
                        self.master.get_ref().as_raw_fd(),
                        buffer.as_mut_ptr().cast(),
                        buffer.len(),
                    )
                };
                if read > 0 {
                    self.output.extend_from_slice(&buffer[..read as usize]);
                } else if read == -1 {
                    let error = io::Error::last_os_error();
                    if error.kind() == io::ErrorKind::WouldBlock {
                        ready.clear_ready();
                    } else {
                        panic!("PTY read failed before '{expected}': {error}");
                    }
                }
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "PTY output did not contain '{expected}': {}",
                String::from_utf8_lossy(&self.output)
            )
        });
    }

    fn write_ctrl_c(&self) {
        let byte = [3_u8];
        let written = unsafe {
            libc::write(
                self.master.get_ref().as_raw_fd(),
                byte.as_ptr().cast(),
                byte.len(),
            )
        };
        assert_eq!(written, 1, "failed to write Ctrl+C to PTY");
    }

    async fn finish(mut self) -> String {
        self.read_until("parent-foreground=true").await;
        let status = tokio::time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .expect("interactive harness did not exit")
            .unwrap();
        assert!(status.success());
        String::from_utf8_lossy(&self.output).into_owned()
    }
}

fn duplicate_stdio(fd: RawFd) -> Stdio {
    let duplicate = unsafe { libc::dup(fd) };
    assert_ne!(duplicate, -1, "dup failed: {}", io::Error::last_os_error());
    unsafe { Stdio::from_raw_fd(duplicate) }
}

fn fixture_binary() -> std::ffi::OsString {
    std::env::var_os("CARGO_BIN_EXE_process_fixture")
        .expect("Cargo did not provide the process fixture binary")
}

#[tokio::test]
async fn child_owns_foreground_and_parent_reclaims_after_success() {
    let output = Harness::launch("success").await.finish().await;
    assert!(output.contains("child-foreground=true"), "{output}");
}

#[tokio::test]
async fn parent_reclaims_terminal_after_exec_failure() {
    let output = Harness::launch("spawn-failure").await.finish().await;
    assert!(output.contains("spawn-error=true"), "{output}");
}

#[tokio::test]
async fn parent_reclaims_terminal_after_cancellation() {
    let output = Harness::launch("cancel").await.finish().await;
    assert!(output.contains("harness-cancelled=true"), "{output}");
}

#[tokio::test]
async fn ctrl_c_is_routed_to_the_foreground_child() {
    let mut harness = Harness::launch("ctrl-c").await;
    harness.read_until("child-ready").await;
    harness.write_ctrl_c();
    let output = harness.finish().await;
    assert!(output.contains("child-interrupted"), "{output}");
    assert!(output.contains("harness-cancelled=false"), "{output}");
}
