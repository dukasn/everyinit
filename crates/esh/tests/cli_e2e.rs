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

use std::fs;
use std::io::Write;
#[cfg(not(windows))]
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::tempdir;

#[cfg(unix)]
fn run_esh_with_pty(
    script: &str,
    input: &[u8],
    input_delay: std::time::Duration,
    ready_marker: Option<&str>,
) -> (std::process::ExitStatus, Vec<u8>) {
    use std::ffi::CString;
    use std::os::unix::process::ExitStatusExt;

    let executable = CString::new(env!("CARGO_BIN_EXE_esh")).unwrap();
    let option = CString::new("-c").unwrap();
    let script = CString::new(script).unwrap();
    let mut argv = vec![
        executable.as_ptr(),
        option.as_ptr(),
        script.as_ptr(),
        std::ptr::null(),
    ];
    let mut master = -1;
    // SAFETY: forkpty creates a master/slave PTY pair and makes the child a
    // session leader with the slave as its controlling terminal.
    let pid = unsafe {
        libc::forkpty(
            &mut master,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert!(
        pid >= 0,
        "forkpty failed: {}",
        std::io::Error::last_os_error()
    );
    if pid == 0 {
        // SAFETY: these pointers refer to CStrings allocated before fork and
        // remain valid until exec replaces this process image.
        unsafe {
            libc::execv(executable.as_ptr(), argv.as_mut_ptr());
            libc::_exit(127);
        }
    }

    let mut output = Vec::new();
    if let Some(marker) = ready_marker {
        let mut buffer = [0u8; 4096];
        while !String::from_utf8_lossy(&output).contains(marker) {
            let mut descriptor = libc::pollfd {
                fd: master,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: descriptor is an initialized pollfd for the duration of
            // the readiness check.
            let ready = unsafe { libc::poll(&mut descriptor, 1, 5_000) };
            if ready <= 0 {
                // SAFETY: pid is the child created above.
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                    libc::waitpid(pid, std::ptr::null_mut(), 0);
                }
                panic!("esh did not display PTY readiness marker {marker:?}");
            }
            // SAFETY: buffer is writable and master is an open PTY descriptor.
            let count = unsafe { libc::read(master, buffer.as_mut_ptr().cast(), buffer.len()) };
            assert!(count > 0, "PTY closed before readiness marker {marker:?}");
            output.extend_from_slice(&buffer[..count as usize]);
        }
    }
    std::thread::sleep(input_delay);
    // SAFETY: input is a valid readable byte slice and master is the PTY input.
    let written = unsafe { libc::write(master, input.as_ptr().cast(), input.len()) };
    assert_eq!(written, input.len() as isize);

    let mut buffer = [0u8; 4096];
    loop {
        let mut descriptor = libc::pollfd {
            fd: master,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: descriptor points to one initialized pollfd for the duration
        // of the call.
        let ready = unsafe { libc::poll(&mut descriptor, 1, 5_000) };
        if ready == 0 {
            // SAFETY: pid is the child created above.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
                libc::waitpid(pid, std::ptr::null_mut(), 0);
            }
            panic!(
                "esh did not finish its PTY interaction within five seconds; output: {}",
                String::from_utf8_lossy(&output)
            );
        }
        assert!(
            ready > 0,
            "PTY poll failed: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: buffer is writable and master is an open PTY descriptor.
        let count = unsafe { libc::read(master, buffer.as_mut_ptr().cast(), buffer.len()) };
        if count > 0 {
            output.extend_from_slice(&buffer[..count as usize]);
        } else {
            break;
        }
    }
    // SAFETY: master has been consumed and is no longer needed.
    unsafe { libc::close(master) };
    let mut wait_status = 0;
    // SAFETY: pid is the child created above and wait_status is writable.
    assert_eq!(unsafe { libc::waitpid(pid, &mut wait_status, 0) }, pid);
    (std::process::ExitStatus::from_raw(wait_status), output)
}

#[cfg(unix)]
struct DescendantCleanup(Option<std::path::PathBuf>);

#[cfg(unix)]
impl Drop for DescendantCleanup {
    fn drop(&mut self) {
        let Some(pid_file) = &self.0 else {
            return;
        };
        let Some(pid) = fs::read_to_string(pid_file)
            .ok()
            .and_then(|contents| contents.trim().parse::<libc::pid_t>().ok())
        else {
            return;
        };
        // SAFETY: this cleanup guard only signals the descendant PID written
        // by the test process tree it created.
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
    }
}

fn esh() -> Command {
    Command::new(env!("CARGO_BIN_EXE_esh"))
}

fn output_text(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn inline_mode_uses_the_requested_working_directory() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().join("workspace with spaces");
    fs::create_dir(&workspace).unwrap();

    let output = esh()
        .current_dir(&workspace)
        .args(["-c", "mkdir -p nested; printf portable > nested/result.txt"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", output_text(&output));
    assert_eq!(
        fs::read_to_string(workspace.join("nested/result.txt")).unwrap(),
        "portable"
    );
}

#[cfg(unix)]
#[test]
fn mkdir_explicit_mode_overrides_restrictive_process_umask() {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::CommandExt;

    let temp = tempdir().unwrap();
    let mut command = esh();
    command.current_dir(temp.path()).args([
        "-c",
        "mkdir -m 777 octal; mkdir -m u=rwx symbolic; mkdir -pm 777 nested/deep/final",
    ]);

    // SAFETY: the pre-exec hook only sets the child process's umask before
    // exec; it does not access parent-process state or allocate resources.
    unsafe {
        command.pre_exec(|| {
            libc::umask(0o077);
            Ok(())
        });
    }

    let output = command.output().unwrap();
    assert!(output.status.success(), "{}", output_text(&output));
    for name in ["octal", "symbolic", "nested/deep/final"] {
        assert_eq!(
            fs::metadata(temp.path().join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o777,
            "explicit mkdir mode for {name} should override umask"
        );
    }
    for name in ["nested", "nested/deep"] {
        assert_eq!(
            fs::metadata(temp.path().join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o700,
            "mkdir -p intermediate directory {name} should retain umask-filtered permissions"
        );
    }
}

#[test]
fn inline_mode_sets_command_name_and_positional_arguments() {
    let output = esh()
        .args([
            "-c",
            "printf '%s|%s|%s|%s\\n' \"$0\" \"$#\" \"$1\" \"$2\"",
            "inline-name",
            "first value",
            "second",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", output_text(&output));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "inline-name|2|first value|second\n"
    );

    let output = esh()
        .args(["-c", "printf '%s|%s\\n' \"$0\" \"$#\""])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", output_text(&output));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "esh|0\n");
}

#[cfg(unix)]
#[test]
fn read_consumes_real_terminal_input_and_applies_backslash_modes() {
    let (status, output) = run_esh_with_pty(
        r#"read -p "Name: " FIRST; read -r RAW; printf "FIRST=<%s> RAW=<%s>\n" "$FIRST" "$RAW""#,
        b"first\\ value\nliteral\\ value\n",
        std::time::Duration::ZERO,
        None,
    );

    assert!(
        status.success(),
        "PTY output: {}",
        String::from_utf8_lossy(&output)
    );
    let output = String::from_utf8_lossy(&output);
    assert!(output.contains("Name: "), "missing prompt: {output}");
    assert!(
        output.contains("FIRST=<first value> RAW=<literal\\ value>"),
        "unexpected terminal read result: {output}"
    );
}

#[cfg(unix)]
#[test]
fn terminal_ctrl_c_cancels_esh_and_kills_sigint_ignoring_descendants() {
    use std::time::Duration;

    let temp = tempdir().unwrap();
    let pid_file = temp.path().join("server-descendant.pid");
    let mut cleanup = DescendantCleanup(Some(pid_file.clone()));
    let script = format!(
        r#"trap 'echo interrupt-trap' INT
trap 'echo exit-trap' EXIT
sh -c 'trap "trap - INT; kill -INT \$\$" INT; ( trap "" INT; sleep 30 & echo $! > {}; echo server-ready; wait ) & wait'
echo after-interrupt
"#,
        pid_file.display()
    );

    let (status, output) = run_esh_with_pty(&script, b"\x03", Duration::ZERO, Some("server-ready"));
    let output = String::from_utf8_lossy(&output).replace("\r\n", "\n");
    assert_eq!(
        status.code(),
        Some(130),
        "ESH should report interruption; PTY output: {}",
        output
    );
    assert!(
        !output.contains("after-interrupt"),
        "execution continued after terminal Ctrl+C: {}",
        output
    );
    assert_eq!(output.matches("interrupt-trap\n").count(), 1, "{output}");
    assert_eq!(output.matches("exit-trap\n").count(), 1, "{output}");

    let descendant_pid = fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse::<libc::pid_t>()
        .unwrap();
    for _ in 0..100 {
        // SAFETY: signal zero only checks whether the recorded process still
        // exists; the PID belongs to the descendant started by this test.
        if unsafe { libc::kill(descendant_pid, 0) } == -1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // SAFETY: signal zero only checks whether the test-owned descendant PID
    // still exists.
    assert_eq!(
        unsafe { libc::kill(descendant_pid, 0) },
        -1,
        "descendant {descendant_pid} survived terminal Ctrl+C; PTY output: {}",
        output
    );
    cleanup.0 = None;
}

#[cfg(unix)]
#[test]
fn terminal_ctrl_c_kills_node_server_started_through_npm() {
    use std::time::Duration;

    if Command::new("npm").arg("--version").output().is_err()
        || Command::new("node").arg("--version").output().is_err()
    {
        eprintln!("skipping npm process-tree test because Node.js or npm is unavailable");
        return;
    }

    let temp = tempdir().unwrap();
    let pid_file = temp.path().join("server.pid");
    let mut cleanup = DescendantCleanup(Some(pid_file.clone()));
    fs::write(
        temp.path().join("package.json"),
        r#"{"name":"esh-ctrl-c-fixture","version":"1.0.0","scripts":{"start":"node server.cjs"}}"#,
    )
    .unwrap();
    fs::write(
        temp.path().join("server.cjs"),
        r#"const fs = require('node:fs');
fs.writeFileSync('server.pid', String(process.pid));
console.log('server-ready');
setInterval(() => {}, 1000);
"#,
    )
    .unwrap();

    let script = format!(
        "trap 'echo interrupt-trap' INT\ntrap 'echo exit-trap' EXIT\ncd '{}'\nnpm run start\necho after-interrupt\n",
        temp.path().display()
    );
    let (status, output) = run_esh_with_pty(&script, b"\x03", Duration::ZERO, Some("server-ready"));
    let output = String::from_utf8_lossy(&output).replace("\r\n", "\n");

    assert_eq!(
        status.code(),
        Some(130),
        "ESH should report interruption; PTY output: {output}"
    );
    assert!(!output.contains("after-interrupt"), "{output}");
    assert_eq!(output.matches("interrupt-trap\n").count(), 1, "{output}");
    assert_eq!(output.matches("exit-trap\n").count(), 1, "{output}");

    let server_pid = fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse::<libc::pid_t>()
        .unwrap();
    for _ in 0..100 {
        // SAFETY: signal zero only checks whether the Node process created by
        // this test still exists.
        if unsafe { libc::kill(server_pid, 0) } == -1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // SAFETY: this PID was written by the test-owned Node server process.
    assert_eq!(
        unsafe { libc::kill(server_pid, 0) },
        -1,
        "Node server process {server_pid} survived terminal Ctrl+C; PTY output: {output}"
    );
    cleanup.0 = None;
}

#[cfg(unix)]
#[test]
fn explicit_external_status_130_does_not_cancel_following_script_commands() {
    use std::time::Duration;

    let script = "sh -c 'exit 130'; printf 'after:%s\\n' \"$?\"";
    let (status, output) = run_esh_with_pty(script, b"", Duration::ZERO, None);
    let output = String::from_utf8_lossy(&output).replace("\r\n", "\n");

    assert_eq!(status.code(), Some(0), "PTY output: {output}");
    assert!(output.contains("after:130\n"), "PTY output: {output}");
}

#[cfg(unix)]
#[test]
fn read_reports_terminal_eof_and_preserves_partial_input() {
    let script = r#"read VALUE; printf "STATUS=<%s> VALUE=<%s>\n" "$?" "$VALUE""#;

    let (empty_status, empty_output) =
        run_esh_with_pty(script, b"\x04", std::time::Duration::ZERO, None);
    assert!(empty_status.success());
    assert!(String::from_utf8_lossy(&empty_output).contains("STATUS=<1> VALUE=<>"));

    let (partial_status, partial_output) =
        run_esh_with_pty(script, b"partial\x04", std::time::Duration::ZERO, None);
    assert!(partial_status.success());
    assert!(
        String::from_utf8_lossy(&partial_output).contains("STATUS=<1> VALUE=<partial>"),
        "partial terminal EOF was not preserved: {}",
        String::from_utf8_lossy(&partial_output)
    );
}

#[cfg(unix)]
#[test]
fn terminal_read_preserves_bytes_after_a_custom_delimiter() {
    let (status, output) = run_esh_with_pty(
        r#"read -d : FIRST; read REST; printf "FIRST=<%s> REST=<%s>\n" "$FIRST" "$REST""#,
        b"left:right\n",
        std::time::Duration::ZERO,
        None,
    );

    assert!(status.success());
    assert!(
        String::from_utf8_lossy(&output).contains("FIRST=<left> REST=<right>"),
        "terminal read discarded bytes after the delimiter: {}",
        String::from_utf8_lossy(&output)
    );
}

#[cfg(unix)]
#[test]
fn secret_read_hides_terminal_input_but_assigns_the_value() {
    let (status, output) = run_esh_with_pty(
        r#"read -s -p "Secret: " SECRET; printf "STORED_LENGTH=<%s>\n" "${#SECRET}""#,
        b"topsecret\r",
        std::time::Duration::ZERO,
        Some("Secret:"),
    );

    assert!(
        status.success(),
        "PTY output: {}",
        String::from_utf8_lossy(&output)
    );
    let output = String::from_utf8_lossy(&output);
    assert!(
        output.contains("STORED_LENGTH=<9>"),
        "secret was not read: {output}"
    );
    assert!(
        !output.contains("topsecret"),
        "secret appeared in terminal output: {output}"
    );
}

#[cfg(unix)]
#[test]
fn menu_read_accepts_terminal_navigation_and_assigns_the_choice() {
    let (status, output) = run_esh_with_pty(
        r#"read --menu 'one="One" two="Two"' CHOICE; printf "CHOICE=<%s>\n" "$CHOICE""#,
        b"\x1b[B\r",
        std::time::Duration::ZERO,
        Some("Select choice"),
    );

    assert!(
        status.success(),
        "PTY output: {}",
        String::from_utf8_lossy(&output)
    );
    let output = String::from_utf8_lossy(&output);
    assert!(
        output.contains("CHOICE=<two>"),
        "menu selection failed: {output}"
    );
}

#[cfg(unix)]
#[test]
fn select_reads_a_choice_from_a_terminal() {
    let (status, output) = run_esh_with_pty(
        "select CHOICE in one two; do printf 'CHOICE=<%s>\\n' \"$CHOICE\"; break; done",
        b"2\n",
        std::time::Duration::ZERO,
        None,
    );

    assert!(
        status.success(),
        "PTY output: {}",
        String::from_utf8_lossy(&output)
    );
    let output = String::from_utf8_lossy(&output);
    assert!(
        output.contains("1) one\r\n2) two\r\n#? "),
        "menu missing: {output}"
    );
    assert!(
        output.contains("CHOICE=<two>"),
        "selection missing: {output}"
    );
}

#[cfg(unix)]
#[test]
fn timed_terminal_read_does_not_consume_the_next_readers_input() {
    let (status, output) = run_esh_with_pty(
        r#"read -t 50ms FIRST; printf "STATUS=<%s> FIRST=<%s>\n" "$?" "$FIRST"; read SECOND; printf "SECOND=<%s>\n" "$SECOND""#,
        b"later\n",
        std::time::Duration::from_millis(150),
        None,
    );

    assert!(
        status.success(),
        "PTY output: {}",
        String::from_utf8_lossy(&output)
    );
    let output = String::from_utf8_lossy(&output);
    assert!(
        output.contains("STATUS=<1> FIRST=<>") && output.contains("SECOND=<later>"),
        "timed read lost or altered terminal input: {output}"
    );
}

#[test]
fn timed_pipe_read_assigns_partial_input_and_leaves_later_input_for_next_read() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_esh"))
        .args([
            "-c",
            r#"read -t 50ms FIRST; printf "STATUS=<%s> FIRST=<%s>\n" "$?" "$FIRST"; read SECOND; printf "SECOND=<%s>\n" "$SECOND""#,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start ESH with piped stdin");
    let mut stdin = child.stdin.take().expect("piped stdin");
    let writer = std::thread::spawn(move || {
        stdin.write_all(b"partial").expect("write partial input");
        std::thread::sleep(std::time::Duration::from_millis(150));
        stdin.write_all(b"later\n").expect("write next input");
    });

    let output = child.wait_with_output().expect("wait for ESH");
    writer.join().expect("stdin writer thread");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "STATUS=<1> FIRST=<partial>\nSECOND=<later>\n"
    );
}

#[test]
fn script_mode_preserves_zero_positionals_and_shift() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().join("workspace with spaces");
    fs::create_dir(&workspace).unwrap();
    let script = workspace.join("arguments.esh");
    fs::write(
        &script,
        "printf '%s|%s|%s\\n' \"$0\" \"$1\" \"$2\"\nshift\nprintf '%s|%s\\n' \"$1\" \"$2\"\n",
    )
    .unwrap();

    let output = esh()
        .arg(&script)
        .args(["first value", "second value", "third value"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", output_text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        [
            format!("{}|first value|second value", script.to_string_lossy()),
            "second value|third value".to_string(),
        ]
    );
}

#[test]
fn dry_run_has_no_side_effects() {
    let temp = tempdir().unwrap();
    let script = temp.path().join("dry run.esh");
    let artifact = temp.path().join("should-not-exist.txt");
    fs::write(&script, "printf changed > should-not-exist.txt\n").unwrap();

    let output = esh()
        .current_dir(temp.path())
        .args(["--dry-run"])
        .arg(&script)
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", output_text(&output));
    assert!(!artifact.exists());
    assert!(!output.stdout.is_empty());
}

#[test]
fn exit_builtin_sets_the_process_status() {
    let status = esh().args(["-c", "exit 23"]).status().unwrap();
    assert_eq!(status.code(), Some(23));
}

#[test]
fn required_parameter_expansion_stops_inline_script_with_failure() {
    let output = esh()
        .args([
            "-c",
            "printf before; printf '%s' \"${ESH_REQUIRED:?required value}\"; printf after",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "before");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "esh: Execution error: Custom error: required value\n"
    );
}

#[test]
fn path_output_uses_script_separators() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().join("workspace with spaces");
    fs::create_dir(&workspace).unwrap();

    let output = esh()
        .current_dir(&workspace)
        .args([
            "-c",
            "mkdir -p nested/dir; touch nested/dir/file.txt; pwd; path -j nested dir file.txt; realpath nested/dir; glob 'nested/**/*.txt'; mktemp -d",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", output_text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().nth(1), Some("nested/dir/file.txt"));
    assert!(stdout.contains("nested/dir/file.txt"));
    assert_no_native_separators(&stdout);
}

#[cfg(windows)]
fn assert_no_native_separators(output: &str) {
    assert!(
        !output.contains('\\'),
        "path output was not canonical: {output}"
    );
    assert!(
        output
            .lines()
            .next()
            .is_some_and(|line| line.contains(":/"))
    );
}

#[cfg(not(windows))]
fn assert_no_native_separators(output: &str) {
    assert!(
        output
            .lines()
            .next()
            .is_some_and(|line| Path::new(line).is_absolute())
    );
}
