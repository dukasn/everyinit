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

use crate::error::ExecError;
use crate::planner::ir::ExecutionPlan;
use crate::runtime::traits::ScopeManager;
use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrapSignal {
    Err,
    Exit,
    Interrupt,
}

impl TrapSignal {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "ERR" | "ERROR" => Some(Self::Err),
            "EXIT" => Some(Self::Exit),
            "INT" | "SIGINT" | "INTERRUPT" => Some(Self::Interrupt),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellValue {
    String(String),
    Array(Vec<String>),
    Map(FxHashMap<String, String>),
}

impl ShellValue {
    pub fn to_string_lossy(&self) -> String {
        match self {
            ShellValue::String(s) => s.clone(),
            ShellValue::Array(arr) => arr.first().cloned().unwrap_or_default(),
            ShellValue::Map(_) => String::new(),
        }
    }

    pub fn to_unrolled_string(&self) -> String {
        match self {
            ShellValue::String(s) => s.clone(),
            ShellValue::Array(arr) => arr.join(" "),
            ShellValue::Map(m) => {
                let mut vals: Vec<&String> = m.values().collect();
                vals.sort();
                vals.into_iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        }
    }

    pub fn len(&self) -> usize {
        match self {
            ShellValue::String(s) => s.len(),
            ShellValue::Array(arr) => arr.len(),
            ShellValue::Map(m) => m.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            ShellValue::String(s) => s.is_empty(),
            ShellValue::Array(arr) => arr.is_empty(),
            ShellValue::Map(m) => m.is_empty(),
        }
    }

    pub fn get_subscript(&self, sub: &str) -> Option<String> {
        match self {
            ShellValue::String(s) => {
                if sub == "0" || sub == "@" || sub == "*" {
                    Some(s.clone())
                } else {
                    None
                }
            }
            ShellValue::Array(arr) => {
                if sub == "@" || sub == "*" {
                    Some(arr.join(" "))
                } else if let Ok(idx) = sub.parse::<usize>() {
                    arr.get(idx).cloned()
                } else {
                    None
                }
            }
            ShellValue::Map(m) => {
                if sub == "@" || sub == "*" {
                    let mut vals: Vec<_> = m.values().cloned().collect();
                    vals.sort();
                    Some(vals.join(" "))
                } else {
                    m.get(sub).cloned()
                }
            }
        }
    }

    pub fn get_keys(&self) -> Vec<String> {
        match self {
            ShellValue::String(_) => vec!["0".to_string()],
            ShellValue::Array(arr) => (0..arr.len()).map(|i| i.to_string()).collect(),
            ShellValue::Map(m) => {
                let mut keys: Vec<String> = m.keys().cloned().collect();
                keys.sort();
                keys
            }
        }
    }
}

impl From<String> for ShellValue {
    fn from(s: String) -> Self {
        ShellValue::String(s)
    }
}

impl From<&str> for ShellValue {
    fn from(s: &str) -> Self {
        ShellValue::String(s.to_string())
    }
}

impl From<Vec<String>> for ShellValue {
    fn from(v: Vec<String>) -> Self {
        ShellValue::Array(v)
    }
}

impl From<FxHashMap<String, String>> for ShellValue {
    fn from(m: FxHashMap<String, String>) -> Self {
        ShellValue::Map(m)
    }
}

#[derive(Clone, Default)]
pub struct ScopeFrame {
    pub locals: FxHashMap<String, ShellValue>,
    pub traps: FxHashMap<TrapSignal, ExecutionPlan>,
}

#[derive(Clone)]
pub struct ExecutionContext {
    scopes: Vec<ScopeFrame>,
    env_vars: FxHashMap<String, String>,
    cwd: PathBuf,
    positional_args: Vec<String>,
    last_exit: i32,
    fail_fast: bool,
    token: CancellationToken,
    failed_cmd: Option<String>,
    failed_line: u32,
    failed_msg: Option<String>,
    captured_stdout: Option<Arc<Mutex<Vec<u8>>>>,
    captured_stderr: Option<Arc<Mutex<Vec<u8>>>>,
    stdin_buffer: Option<Vec<u8>>,
    functions: FxHashMap<String, ExecutionPlan>,
    in_trap: bool,
    exec_depth: usize,
    exit_requested: Option<i32>,
    return_requested: Option<i32>,
    loop_depth: usize,
    break_requested: Option<usize>,
    continue_requested: Option<usize>,
    options: ShellOptions,
    options_shielded: bool,
    interactive_events: Option<std::collections::VecDeque<crossterm::event::Event>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellOptions {
    pub errexit: bool,   // -e / -o errexit
    pub nounset: bool,   // -u / -o nounset
    pub xtrace: bool,    // -x / -o xtrace
    pub pipefail: bool,  // -o pipefail
    pub noglob: bool,    // -f / -o noglob
    pub noclobber: bool, // -C / -o noclobber
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionContext {
    pub fn new() -> Self {
        let mut env_map = FxHashMap::default();
        for (k, v) in env::vars() {
            env_map.insert(k, v);
        }

        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        Self {
            scopes: vec![ScopeFrame::default()],
            env_vars: env_map,
            cwd,
            positional_args: vec!["esh".to_string()],
            last_exit: 0,
            fail_fast: false,
            token: CancellationToken::new(),
            failed_cmd: None,
            failed_line: 0,
            failed_msg: None,
            captured_stdout: None,
            captured_stderr: None,
            stdin_buffer: None,
            functions: FxHashMap::default(),
            in_trap: false,
            exec_depth: 0,
            exit_requested: None,
            return_requested: None,
            loop_depth: 0,
            break_requested: None,
            continue_requested: None,
            options: ShellOptions::default(),
            options_shielded: false,
            interactive_events: None,
        }
    }

    pub fn set_interactive_events(&mut self, events: Vec<crossterm::event::Event>) {
        self.interactive_events = Some(std::collections::VecDeque::from(events));
    }

    pub fn set_interactive_chars(&mut self, text: &str) {
        let mut events = Vec::new();
        for c in text.chars() {
            if c == '\n' || c == '\r' {
                events.push(crossterm::event::Event::Key(crossterm::event::KeyEvent {
                    code: crossterm::event::KeyCode::Enter,
                    modifiers: crossterm::event::KeyModifiers::NONE,
                    kind: crossterm::event::KeyEventKind::Press,
                    state: crossterm::event::KeyEventState::NONE,
                }));
            } else {
                events.push(crossterm::event::Event::Key(crossterm::event::KeyEvent {
                    code: crossterm::event::KeyCode::Char(c),
                    modifiers: crossterm::event::KeyModifiers::NONE,
                    kind: crossterm::event::KeyEventKind::Press,
                    state: crossterm::event::KeyEventState::NONE,
                }));
            }
        }
        self.set_interactive_events(events);
    }

    pub fn take_interactive_session(&mut self) -> Box<dyn evin_ui::prompt::PromptSession> {
        if let Some(events) = self.interactive_events.take() {
            Box::new(evin_ui::prompt::MockPromptSession::new(events.into()))
        } else if let Ok(tty) = evin_ui::prompt::TtySession::try_new() {
            Box::new(tty)
        } else {
            Box::new(evin_ui::prompt::MockPromptSession::new(Vec::new()))
        }
    }

    pub fn adopt_lastpipe_state(&mut self, source: ExecutionContext) {
        self.scopes = source.scopes;
        self.env_vars = source.env_vars;
        self.interactive_events = source.interactive_events;
    }

    pub fn options(&self) -> &ShellOptions {
        &self.options
    }

    pub fn options_mut(&mut self) -> &mut ShellOptions {
        &mut self.options
    }

    pub fn is_errexit_shielded(&self) -> bool {
        self.options_shielded
    }

    pub fn set_errexit_shielded(&mut self, shielded: bool) {
        self.options_shielded = shielded;
    }

    pub fn request_exit(&mut self, code: i32) {
        self.exit_requested = Some(code);
        self.set_last_exit(code);
    }

    pub fn is_exiting(&self) -> bool {
        self.exit_requested.is_some()
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.exit_requested
    }

    pub fn request_return(&mut self, code: i32) {
        self.return_requested = Some(code);
        self.set_last_exit(code);
    }

    pub fn is_returning(&self) -> bool {
        self.return_requested.is_some()
    }

    pub fn take_return(&mut self) -> Option<i32> {
        self.return_requested.take()
    }

    pub fn enter_loop(&mut self) {
        self.loop_depth += 1;
    }

    pub fn exit_loop(&mut self) {
        self.loop_depth = self.loop_depth.saturating_sub(1);
    }

    pub fn in_loop(&self) -> bool {
        self.loop_depth > 0
    }

    pub fn trigger_break(&mut self, depth: usize) {
        self.break_requested = Some(depth);
    }

    pub fn trigger_continue(&mut self, depth: usize) {
        self.continue_requested = Some(depth);
    }

    pub fn break_requested(&self) -> Option<usize> {
        self.break_requested
    }

    pub fn continue_requested(&self) -> Option<usize> {
        self.continue_requested
    }

    pub fn clear_break(&mut self) {
        self.break_requested = None;
    }

    pub fn clear_continue(&mut self) {
        self.continue_requested = None;
    }

    pub fn decrement_break(&mut self) {
        if let Some(levels) = self.break_requested {
            if levels <= 1 {
                self.break_requested = None;
            } else {
                self.break_requested = Some(levels - 1);
            }
        }
    }

    pub fn decrement_continue(&mut self) {
        if let Some(levels) = self.continue_requested {
            if levels <= 1 {
                self.continue_requested = None;
            } else {
                self.continue_requested = Some(levels - 1);
            }
        }
    }

    pub fn register_function(&mut self, name: String, plan: ExecutionPlan) {
        self.functions.insert(name, plan);
    }

    pub fn get_function(&self, name: &str) -> Option<ExecutionPlan> {
        self.functions.get(name).cloned()
    }

    pub fn enter_exec(&mut self) {
        self.exec_depth += 1;
    }

    pub fn leave_exec(&mut self) {
        self.exec_depth = self.exec_depth.saturating_sub(1);
    }

    pub fn is_root_exec(&self) -> bool {
        self.exec_depth == 0
    }

    pub fn is_in_trap(&self) -> bool {
        self.in_trap
    }

    pub fn set_in_trap(&mut self, in_trap: bool) {
        self.in_trap = in_trap;
    }

    pub fn with_token(mut self, token: CancellationToken) -> Self {
        self.token = token;
        self
    }

    pub fn clone_isolated(&self) -> Self {
        let mut child = self.clone();
        if self.captured_stdout.is_some() {
            child.captured_stdout = Some(Arc::new(Mutex::new(Vec::new())));
        }
        if self.captured_stderr.is_some() {
            child.captured_stderr = Some(Arc::new(Mutex::new(Vec::new())));
        }
        // Subshells and command substitutions should not trigger parent exit traps
        child.clear_trap(TrapSignal::Exit);
        child
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn set_cwd(&mut self, path: impl AsRef<Path>) -> Result<(), ExecError> {
        let p = path.as_ref();
        let target = if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.cwd.join(p)
        };

        if target.is_dir() {
            self.cwd = target.canonicalize().unwrap_or(target);
            Ok(())
        } else {
            Err(ExecError::NotFound(format!(
                "Directory does not exist: {}",
                p.display()
            )))
        }
    }

    pub fn set_fail_fast(&mut self, enabled: bool) {
        self.fail_fast = enabled;
    }

    pub fn fail_fast_enabled(&self) -> bool {
        self.fail_fast
    }

    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    pub fn cancel(&self) {
        self.token.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    pub fn last_exit(&self) -> i32 {
        self.last_exit
    }

    pub fn set_last_exit(&mut self, code: i32) {
        self.last_exit = code;
        self.set_local("?", code.to_string());
    }

    pub fn set_positional_args(&mut self, args: Vec<String>) {
        if args.is_empty() {
            self.positional_args = vec!["esh".to_string()];
        } else {
            self.positional_args = args;
        }
    }

    pub fn set_positional_params(&mut self, params: &[String]) {
        let prog_name = self
            .positional_args
            .first()
            .cloned()
            .unwrap_or_else(|| "esh".to_string());
        let mut new_vec = Vec::with_capacity(1 + params.len());
        new_vec.push(prog_name);
        new_vec.extend_from_slice(params);
        self.positional_args = new_vec;
    }

    pub fn positional_args(&self) -> &[String] {
        if self.positional_args.len() > 1 {
            &self.positional_args[1..]
        } else {
            &[]
        }
    }

    pub fn shift_positional_args(&mut self, n: usize) -> usize {
        if self.positional_args.len() <= 1 {
            return 0;
        }
        let available = self.positional_args.len() - 1;
        let actual_n = std::cmp::min(n, available);
        self.positional_args.drain(1..=actual_n);
        actual_n
    }

    pub fn set_trap(&mut self, signal: TrapSignal, plan: ExecutionPlan) {
        if let Some(frame) = self.scopes.last_mut() {
            frame.traps.insert(signal, plan);
        }
    }

    pub fn get_trap(&self, signal: TrapSignal) -> Option<ExecutionPlan> {
        for frame in self.scopes.iter().rev() {
            if let Some(plan) = frame.traps.get(&signal) {
                return Some(plan.clone());
            }
        }
        None
    }

    pub fn clear_trap(&mut self, signal: TrapSignal) {
        if let Some(frame) = self.scopes.last_mut() {
            frame.traps.remove(&signal);
        }
    }

    pub fn set_env(&mut self, key: impl Into<String>, val: impl Into<String>) {
        let k = key.into();
        let v = val.into();
        self.env_vars.insert(k.clone(), v.clone());
        self.set_local(&k, v);
    }

    pub fn env_vars(&self) -> &FxHashMap<String, String> {
        &self.env_vars
    }

    pub fn enable_capture(&mut self) {
        self.captured_stdout = Some(Arc::new(Mutex::new(Vec::new())));
        self.captured_stderr = Some(Arc::new(Mutex::new(Vec::new())));
    }

    pub(crate) fn is_capturing(&self) -> bool {
        self.captured_stdout.is_some() || self.captured_stderr.is_some()
    }

    pub fn get_captured_stdout(&self) -> Option<String> {
        self.captured_stdout
            .as_ref()
            .map(|m| String::from_utf8_lossy(&m.lock()).to_string())
    }

    pub fn get_captured_stdout_bytes(&self) -> Option<Vec<u8>> {
        self.captured_stdout.as_ref().map(|m| m.lock().clone())
    }

    pub fn get_captured_stderr(&self) -> Option<String> {
        self.captured_stderr
            .as_ref()
            .map(|m| String::from_utf8_lossy(&m.lock()).to_string())
    }

    pub fn get_captured_stderr_bytes(&self) -> Option<Vec<u8>> {
        self.captured_stderr.as_ref().map(|m| m.lock().clone())
    }

    pub fn write_stdout(&mut self, data: &[u8]) {
        if let Some(buf) = &self.captured_stdout {
            buf.lock().extend_from_slice(data);
        } else {
            use std::io::Write;
            let _ = std::io::stdout().write_all(data);
            let _ = std::io::stdout().flush();
        }
    }

    pub fn write_stderr(&mut self, data: &[u8]) {
        if let Some(buf) = &self.captured_stderr {
            buf.lock().extend_from_slice(data);
        } else {
            use std::io::Write;
            let _ = std::io::stderr().write_all(data);
            let _ = std::io::stderr().flush();
        }
    }

    pub fn set_stdin(&mut self, data: Vec<u8>) {
        self.stdin_buffer = Some(data);
    }

    pub fn take_stdin(&mut self) -> Option<Vec<u8>> {
        self.stdin_buffer.take()
    }

    pub fn record_failure(&mut self, cmd: &str, line: u32, code: i32, msg: Option<String>) {
        self.set_last_exit(code);
        self.failed_cmd = Some(cmd.to_string());
        self.failed_line = line;
        self.failed_msg = msg.clone();
        self.set_local("ESH_FAILED_CMD", cmd.to_string());
        self.set_local("ESH_FAILED_LINE", line.to_string());
        if let Some(m) = msg {
            self.set_local("ESH_FAILED_MSG", m);
        }
    }

    pub fn set_local_val(&mut self, key: &str, val: ShellValue) {
        if let Some(current) = self.scopes.last_mut() {
            current.locals.insert(key.to_string(), val);
        }
    }

    pub fn set_scoped_val(&mut self, key: &str, val: ShellValue) {
        for frame in self.scopes.iter_mut().rev() {
            if frame.locals.contains_key(key) {
                frame.locals.insert(key.to_string(), val);
                return;
            }
        }
        self.set_local_val(key, val);
    }

    pub fn set_local_var(&mut self, key: &str, val: String) {
        self.set_local_val(key, ShellValue::String(val));
    }

    pub fn set_scoped_var(&mut self, key: &str, val: String) {
        self.set_scoped_val(key, ShellValue::String(val));
    }

    pub fn set_array_elem(&mut self, key: &str, index: usize, val: String) {
        for frame in self.scopes.iter_mut().rev() {
            if let Some(existing) = frame.locals.get_mut(key) {
                match existing {
                    ShellValue::Array(arr) => {
                        if index >= arr.len() {
                            arr.resize(index + 1, String::new());
                        }
                        arr[index] = val;
                        return;
                    }
                    ShellValue::String(s) => {
                        let mut arr = vec![s.clone()];
                        if index >= arr.len() {
                            arr.resize(index + 1, String::new());
                        }
                        arr[index] = val;
                        *existing = ShellValue::Array(arr);
                        return;
                    }
                    _ => {}
                }
            }
        }
        let mut arr = Vec::new();
        if index >= arr.len() {
            arr.resize(index + 1, String::new());
        }
        arr[index] = val;
        self.set_local_val(key, ShellValue::Array(arr));
    }

    pub fn set_map_elem(&mut self, key: &str, sub_key: String, val: String) {
        for frame in self.scopes.iter_mut().rev() {
            if let Some(existing) = frame.locals.get_mut(key)
                && let ShellValue::Map(m) = existing
            {
                m.insert(sub_key, val);
                return;
            }
        }
        let mut m = FxHashMap::default();
        m.insert(sub_key, val);
        self.set_local_val(key, ShellValue::Map(m));
    }

    pub fn set_var(&mut self, key: &str, val: &str) {
        self.set_scoped_var(key, val.to_string());
    }

    pub fn get_shell_val(&self, key: &str) -> Option<&ShellValue> {
        for frame in self.scopes.iter().rev() {
            if let Some(val) = frame.locals.get(key) {
                return Some(val);
            }
        }
        None
    }
}

impl ScopeManager for ExecutionContext {
    fn push_scope(&mut self) {
        self.scopes.push(ScopeFrame::default());
    }

    fn pop_scope(&mut self) -> Result<(), ExecError> {
        if self.scopes.len() <= 1 {
            return Err(ExecError::ScopeUnderflow(
                "Cannot pop root scope".to_string(),
            ));
        }
        self.scopes.pop();
        Ok(())
    }

    fn set_local(&mut self, key: &str, val: String) {
        self.set_local_val(key, ShellValue::String(val));
    }

    fn get_var(&self, key: &str) -> Option<String> {
        // Special variables
        if key == "?" {
            return Some(self.last_exit.to_string());
        }
        if key == "#" {
            let count = self.positional_args.len().saturating_sub(1);
            return Some(count.to_string());
        }
        if key == "@" || key == "*" {
            if self.positional_args.len() > 1 {
                return Some(self.positional_args[1..].join(" "));
            } else {
                return Some(String::new());
            }
        }
        if key == "0" {
            return self
                .positional_args
                .first()
                .cloned()
                .or(Some("esh".to_string()));
        }
        if key == "ESH_FAILED_CMD"
            && let Some(c) = &self.failed_cmd
        {
            return Some(c.clone());
        }
        if key == "ESH_FAILED_LINE" {
            return Some(self.failed_line.to_string());
        }
        if key == "ESH_FAILED_MSG"
            && let Some(m) = &self.failed_msg
        {
            return Some(m.clone());
        }
        if let Ok(idx) = key.parse::<usize>()
            && idx < self.positional_args.len()
        {
            return Some(self.positional_args[idx].clone());
        }

        // Search lexical scopes from top to bottom
        for frame in self.scopes.iter().rev() {
            if let Some(val) = frame.locals.get(key) {
                return Some(val.to_string_lossy());
            }
        }

        // Fallback to environment variables
        self.env_vars.get(key).cloned()
    }
}
