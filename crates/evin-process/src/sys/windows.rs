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

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::RawHandle;
use std::os::windows::process::ExitStatusExt;
use std::process::ExitStatus;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::net::windows::named_pipe::NamedPipeServer;
use windows_sys::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_PIPE_CONNECTED, GENERIC_READ,
    GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    PIPE_ACCESS_INBOUND, PIPE_ACCESS_OUTBOUND,
};
use windows_sys::Win32::System::Console::{
    GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
    DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess,
    GetCurrentProcessId, GetExitCodeProcess, InitializeProcThreadAttributeList,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES,
    STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
};

use crate::{EnvironmentPolicy, OutputPolicy, ProcessError, ProcessMode, ProcessSpec, StdinPolicy};

pub(crate) type ChildStdin = NamedPipeServer;
pub(crate) type ChildStdout = NamedPipeServer;
pub(crate) type ChildStderr = NamedPipeServer;

pub(crate) struct SpawnedProcess {
    pub(crate) child: Child,
    pub(crate) tree: ProcessTree,
    pub(crate) stdin: Option<ChildStdin>,
    pub(crate) stdout: Option<ChildStdout>,
    pub(crate) stderr: Option<ChildStderr>,
}

pub(crate) fn spawn(spec: &ProcessSpec) -> Result<SpawnedProcess, ProcessError> {
    spawn_inner(spec).map_err(|source| ProcessError::Spawn {
        program: spec.program.to_string_lossy().into_owned(),
        source,
    })
}

fn spawn_inner(spec: &ProcessSpec) -> io::Result<SpawnedProcess> {
    let mut stdio = PreparedStdio::new(spec)?;
    let inherited_handles = stdio.child_handles();
    let attributes = AttributeList::for_handles(&inherited_handles)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = inherited_handles[0];
    startup.StartupInfo.hStdOutput = inherited_handles[1];
    startup.StartupInfo.hStdError = inherited_handles[2];
    startup.lpAttributeList = attributes.pointer();

    let batch = is_batch_program(&spec.program);
    let application_program = if batch {
        OsString::from("cmd.exe")
    } else {
        spec.program.clone()
    };
    let application = wide_null(&application_program)?;
    let mut command_line = if batch {
        make_batch_command_line(&spec.program, &spec.args)?
    } else {
        make_command_line(&spec.program, &spec.args)?
    };
    let current_directory = wide_null(spec.cwd.as_os_str())?;
    let environment = make_environment(spec)?;
    let mut process_info = PROCESS_INFORMATION::default();
    let mut flags = CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT;
    if spec.mode == ProcessMode::SupervisedAutomation {
        flags |= CREATE_NO_WINDOW;
    }

    let tree = ProcessTree::new()?;
    // SAFETY: all pointers refer to live, correctly terminated buffers for the
    // duration of the call. STARTUPINFOEXW owns a valid attribute list whose
    // handle array also remains live. Only those three inheritable handles are
    // exposed to the suspended child.
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            flags,
            environment.as_ptr().cast(),
            current_directory.as_ptr(),
            &startup.StartupInfo,
            &mut process_info,
        )
    };
    if created == 0 {
        return Err(io::Error::last_os_error());
    }

    let process = OwnedHandle::new(process_info.hProcess)?;
    let thread = OwnedHandle::new(process_info.hThread)?;
    let mut child = Child {
        process,
        pid: process_info.dwProcessId,
        status: None,
    };
    if unsafe { AssignProcessToJobObject(tree.job, child.process.raw()) } == 0 {
        let error = io::Error::last_os_error();
        let _ = child.start_kill();
        return Err(error);
    }
    if unsafe { ResumeThread(thread.raw()) } == u32::MAX {
        let error = io::Error::last_os_error();
        tree.kill();
        return Err(error);
    }

    // The child has its own inherited copies. Closing these parent-side client
    // handles is what lets pipe EOF accurately follow process-tree lifetime.
    stdio.release_child_handles();
    Ok(SpawnedProcess {
        child,
        tree,
        stdin: stdio.stdin_parent.take(),
        stdout: stdio.stdout_parent.take(),
        stderr: stdio.stderr_parent.take(),
    })
}

pub(crate) struct Child {
    process: OwnedHandle,
    pid: u32,
    status: Option<ExitStatus>,
}

impl Child {
    pub(crate) fn id(&self) -> Option<u32> {
        Some(self.pid)
    }

    pub(crate) async fn wait(&mut self) -> io::Result<ExitStatus> {
        if let Some(status) = self.status {
            return Ok(status);
        }
        loop {
            match unsafe { WaitForSingleObject(self.process.raw(), 0) } {
                WAIT_OBJECT_0 => {
                    let mut code = 0;
                    if unsafe { GetExitCodeProcess(self.process.raw(), &mut code) } == 0 {
                        return Err(io::Error::last_os_error());
                    }
                    let status = ExitStatus::from_raw(code);
                    self.status = Some(status);
                    return Ok(status);
                }
                WAIT_TIMEOUT => tokio::time::sleep(Duration::from_millis(10)).await,
                _ => return Err(io::Error::last_os_error()),
            }
        }
    }

    pub(crate) fn start_kill(&mut self) -> io::Result<()> {
        if self.status.is_some() {
            return Ok(());
        }
        if unsafe { TerminateProcess(self.process.raw(), 1) } == 0 {
            let error = io::Error::last_os_error();
            if unsafe { WaitForSingleObject(self.process.raw(), 0) } != WAIT_OBJECT_0 {
                return Err(error);
            }
        }
        Ok(())
    }
}

pub(crate) struct ProcessTree {
    job: HANDLE,
    armed: bool,
}

// SAFETY: a Job Object HANDLE may be used and closed from any thread, and
// ProcessTree retains exclusive ownership of it.
unsafe impl Send for ProcessTree {}

impl ProcessTree {
    fn new() -> io::Result<Self> {
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            unsafe {
                CloseHandle(job);
            }
            return Err(io::Error::last_os_error());
        }
        Ok(Self { job, armed: true })
    }

    pub(crate) fn terminate(&self) {
        self.kill();
    }

    pub(crate) fn kill(&self) {
        if self.armed && !self.job.is_null() {
            unsafe {
                TerminateJobObject(self.job, 1);
            }
        }
    }

    pub(crate) fn disarm(&mut self) {
        if !self.armed || self.job.is_null() {
            return;
        }
        let limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        unsafe {
            SetInformationJobObject(
                self.job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
        }
        self.armed = false;
    }
}

impl Drop for ProcessTree {
    fn drop(&mut self) {
        if self.armed {
            self.kill();
        }
        if !self.job.is_null() {
            unsafe {
                CloseHandle(self.job);
            }
        }
    }
}

struct PreparedStdio {
    stdin_child: Option<OwnedHandle>,
    stdout_child: Option<OwnedHandle>,
    stderr_child: Option<OwnedHandle>,
    stdin_parent: Option<NamedPipeServer>,
    stdout_parent: Option<NamedPipeServer>,
    stderr_parent: Option<NamedPipeServer>,
}

impl PreparedStdio {
    fn new(spec: &ProcessSpec) -> io::Result<Self> {
        let (stdin_child, stdin_parent) = match spec.stdin {
            StdinPolicy::Inherit => (inherited_stdio(STD_INPUT_HANDLE, true)?, None),
            StdinPolicy::Null => (null_handle(true)?, None),
            StdinPolicy::Bytes(_) => {
                let pair = pipe_pair(false)?;
                (pair.child, Some(pair.parent))
            }
        };
        let (stdout_child, stdout_parent) = match spec.stdout {
            OutputPolicy::Inherit => (inherited_stdio(STD_OUTPUT_HANDLE, false)?, None),
            OutputPolicy::Capture => {
                let pair = pipe_pair(true)?;
                (pair.child, Some(pair.parent))
            }
        };
        let (stderr_child, stderr_parent) = match spec.stderr {
            OutputPolicy::Inherit => (inherited_stdio(STD_ERROR_HANDLE, false)?, None),
            OutputPolicy::Capture => {
                let pair = pipe_pair(true)?;
                (pair.child, Some(pair.parent))
            }
        };
        Ok(Self {
            stdin_child: Some(stdin_child),
            stdout_child: Some(stdout_child),
            stderr_child: Some(stderr_child),
            stdin_parent,
            stdout_parent,
            stderr_parent,
        })
    }

    fn child_handles(&self) -> [HANDLE; 3] {
        [
            self.stdin_child.as_ref().unwrap().raw(),
            self.stdout_child.as_ref().unwrap().raw(),
            self.stderr_child.as_ref().unwrap().raw(),
        ]
    }

    fn release_child_handles(&mut self) {
        self.stdin_child.take();
        self.stdout_child.take();
        self.stderr_child.take();
    }
}

struct PipePair {
    parent: NamedPipeServer,
    child: OwnedHandle,
}

fn pipe_pair(parent_reads: bool) -> io::Result<PipePair> {
    static NEXT_PIPE: AtomicU64 = AtomicU64::new(1);
    let name = format!(
        r"\\.\pipe\evin-process-{}-{}",
        unsafe { GetCurrentProcessId() },
        NEXT_PIPE.fetch_add(1, Ordering::Relaxed)
    );
    let name = wide_null(OsStr::new(&name))?;
    let access = if parent_reads {
        PIPE_ACCESS_INBOUND
    } else {
        PIPE_ACCESS_OUTBOUND
    } | FILE_FLAG_OVERLAPPED;
    let parent = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            access,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            64 * 1024,
            64 * 1024,
            0,
            std::ptr::null(),
        )
    };
    let parent = OwnedHandle::new(parent)?;
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 1,
    };
    let child = unsafe {
        CreateFileW(
            name.as_ptr(),
            if parent_reads {
                GENERIC_WRITE
            } else {
                GENERIC_READ
            },
            0,
            &security,
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    let child = OwnedHandle::new(child)?;
    let connected = unsafe { ConnectNamedPipe(parent.raw(), std::ptr::null_mut()) };
    if connected == 0 && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED {
        return Err(io::Error::last_os_error());
    }
    let raw_parent = parent.into_raw();
    // SAFETY: ownership of the unique overlapped server handle is transferred
    // exactly once to Tokio, which closes it when NamedPipeServer is dropped.
    let parent = unsafe { NamedPipeServer::from_raw_handle(raw_parent as RawHandle) }?;
    Ok(PipePair { parent, child })
}

fn inherited_stdio(identifier: u32, read: bool) -> io::Result<OwnedHandle> {
    let source = unsafe { GetStdHandle(identifier) };
    if source.is_null() || source == INVALID_HANDLE_VALUE {
        return null_handle(read);
    }
    let current = unsafe { GetCurrentProcess() };
    let mut duplicate = std::ptr::null_mut();
    let duplicated = unsafe {
        DuplicateHandle(
            current,
            source,
            current,
            &mut duplicate,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if duplicated == 0 {
        return Err(io::Error::last_os_error());
    }
    OwnedHandle::new(duplicate)
}

fn null_handle(read: bool) -> io::Result<OwnedHandle> {
    let name = wide_null(OsStr::new("NUL"))?;
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 1,
    };
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            if read { GENERIC_READ } else { GENERIC_WRITE },
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &security,
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    OwnedHandle::new(handle)
}

struct AttributeList {
    storage: Vec<u8>,
}

impl AttributeList {
    fn for_handles(handles: &[HANDLE]) -> io::Result<Self> {
        let mut bytes = 0;
        unsafe {
            InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut bytes);
        }
        let mut storage = vec![0_u8; bytes];
        let pointer = storage.as_mut_ptr().cast();
        if unsafe { InitializeProcThreadAttributeList(pointer, 1, 0, &mut bytes) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let updated = unsafe {
            UpdateProcThreadAttribute(
                pointer,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                std::mem::size_of_val(handles),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if updated == 0 {
            unsafe {
                DeleteProcThreadAttributeList(pointer);
            }
            return Err(io::Error::last_os_error());
        }
        Ok(Self { storage })
    }

    fn pointer(&self) -> *mut core::ffi::c_void {
        self.storage.as_ptr().cast_mut().cast()
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        if !self.storage.is_empty() {
            unsafe {
                DeleteProcThreadAttributeList(self.storage.as_mut_ptr().cast());
            }
        }
    }
}

struct OwnedHandle(HANDLE);

// SAFETY: Windows kernel HANDLE values are thread-safe. OwnedHandle closes its
// exclusively owned value exactly once.
unsafe impl Send for OwnedHandle {}
unsafe impl Sync for OwnedHandle {}

impl OwnedHandle {
    fn new(handle: HANDLE) -> io::Result<Self> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(handle))
        }
    }

    fn raw(&self) -> HANDLE {
        self.0
    }

    fn into_raw(mut self) -> HANDLE {
        let handle = self.0;
        self.0 = std::ptr::null_mut();
        handle
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

fn wide_null(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut encoded: Vec<u16> = value.encode_wide().collect();
    if encoded.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows process strings cannot contain NUL",
        ));
    }
    encoded.push(0);
    Ok(encoded)
}

fn make_command_line(program: &OsStr, args: &[OsString]) -> io::Result<Vec<u16>> {
    let mut command = Vec::new();
    append_quoted_arg(&mut command, &program.encode_wide().collect::<Vec<_>>())?;
    for arg in args {
        command.push(b' ' as u16);
        append_quoted_arg(&mut command, &arg.encode_wide().collect::<Vec<_>>())?;
    }
    command.push(0);
    Ok(command)
}

fn is_batch_program(program: &OsStr) -> bool {
    std::path::Path::new(program)
        .extension()
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("bat") || extension.eq_ignore_ascii_case("cmd")
        })
}

fn make_batch_command_line(program: &OsStr, args: &[OsString]) -> io::Result<Vec<u16>> {
    let script: Vec<u16> = program.encode_wide().collect();
    if script.contains(&0) || script.contains(&0x22) || script.last() == Some(&0x5c) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid Windows batch-file path",
        ));
    }
    let mut command: Vec<u16> = "cmd.exe /e:ON /v:OFF /d /c \"".encode_utf16().collect();
    command.push(0x22);
    command.extend(script);
    command.push(0x22);
    for arg in args {
        command.push(0x20);
        append_batch_arg(&mut command, arg)?;
    }
    command.push(0x22);
    command.push(0);
    Ok(command)
}

fn append_batch_arg(command: &mut Vec<u16>, arg: &OsStr) -> io::Result<()> {
    let arg: Vec<u16> = arg.encode_wide().collect();
    if arg.iter().any(|unit| matches!(*unit, 0 | 0x0a | 0x0d)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid Windows batch-file argument",
        ));
    }
    const UNQUOTED: &[u16] = &[
        0x23, 0x24, 0x2a, 0x2b, 0x2d, 0x2e, 0x2f, 0x3a, 0x3f, 0x40, 0x5c, 0x5f,
    ];
    let quote = arg.is_empty()
        || arg.last() == Some(&0x5c)
        || arg.iter().any(|unit| {
            (*unit <= 0x7f && !(*unit as u8).is_ascii_alphanumeric() && !UNQUOTED.contains(unit))
                || *unit < 0x20
        });
    if quote {
        command.push(0x22);
    }
    let mut backslashes = 0;
    for unit in arg {
        if unit == 0x5c {
            backslashes += 1;
        } else {
            if unit == 0x22 {
                command.extend(std::iter::repeat_n(0x5c, backslashes));
                command.push(0x22);
            } else if unit == 0x25 {
                command.extend([0x25, 0x25, 0x63, 0x64, 0x3a, 0x7e, 0x2c]);
            }
            backslashes = 0;
        }
        command.push(unit);
    }
    if quote {
        command.extend(std::iter::repeat_n(0x5c, backslashes));
        command.push(0x22);
    }
    Ok(())
}

fn append_quoted_arg(command: &mut Vec<u16>, arg: &[u16]) -> io::Result<()> {
    if arg.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows process arguments cannot contain NUL",
        ));
    }
    let quote = arg.is_empty() || arg.iter().any(|unit| matches!(*unit, 0x20 | 0x09 | 0x22));
    if !quote {
        command.extend_from_slice(arg);
        return Ok(());
    }
    command.push(0x22);
    let mut backslashes = 0;
    for &unit in arg {
        if unit == 0x5c {
            backslashes += 1;
        } else if unit == 0x22 {
            command.extend(std::iter::repeat_n(0x5c, backslashes * 2 + 1));
            command.push(unit);
            backslashes = 0;
        } else {
            command.extend(std::iter::repeat_n(0x5c, backslashes));
            command.push(unit);
            backslashes = 0;
        }
    }
    command.extend(std::iter::repeat_n(0x5c, backslashes * 2));
    command.push(0x22);
    Ok(())
}

fn make_environment(spec: &ProcessSpec) -> io::Result<Vec<u16>> {
    let mut values = BTreeMap::<Vec<u16>, (OsString, OsString)>::new();
    if spec.environment == EnvironmentPolicy::InheritAndOverride {
        for (key, value) in std::env::vars_os() {
            values.insert(fold_key(&key), (key, value));
        }
    }
    for (key, value) in &spec.env {
        values.insert(fold_key(key), (key.clone(), value.clone()));
    }

    let mut block = Vec::new();
    for (_, (key, value)) in values {
        let key: Vec<u16> = key.encode_wide().collect();
        let value: Vec<u16> = value.encode_wide().collect();
        if key.contains(&0) || value.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Windows environment entries cannot contain NUL",
            ));
        }
        block.extend(key);
        block.push(0x3d);
        block.extend(value);
        block.push(0);
    }
    block.push(0);
    if block.len() == 1 {
        block.push(0);
    }
    Ok(block)
}

fn fold_key(key: &OsStr) -> Vec<u16> {
    key.to_string_lossy()
        .to_uppercase()
        .encode_utf16()
        .collect()
}
