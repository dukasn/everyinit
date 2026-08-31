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

use super::event::{PipelineLogLevel, UIEvent};
use crate::render::{Mode, SYMBOL_FENCED_ERROR_PREFIX, SYMBOL_FENCED_TASK_PREFIX, format_duration};

use std::collections::{BTreeSet, HashMap, VecDeque};

// --- Pending Output Types ---
pub enum PendingLog {
    Header {
        version: String,
        mode: String,
        os: String,
        arch: String,
        shell: String,
    },
    Footer {
        command: String,
        flags: Vec<String>,
    },
    Info(String),
    Success(String),
    Error(String),
    Warn(String),
    Debug(Vec<String>),
    Fenced(String, Vec<String>, bool, Option<String>),
    Intro(String),
    Summary(String, Vec<String>),
    Step(String),
    Remark(String),
    Outro(String),
    OutroCancel(String),
    Restarting {
        display_name: String,
        duration: std::time::Duration,
    },
    DaemonStarted {
        task_name: String,
        display_name: String,
        startup_duration: std::time::Duration,
        status_message: String,
    },
    DaemonStatus {
        name: String,
        status_message: String,
    },
    DaemonStopped {
        name: String,
        duration: std::time::Duration,
        failed: bool,
    },
}

pub struct RenderedLine {
    pub log: PendingLog,
    pub enqueued_at: std::time::Instant,
}

// --- Task Tracker & State ---
pub struct TaskState {
    pub display_name: String,
    pub command: Option<String>,
    pub start_time: std::time::Instant,
    pub buffer: Vec<String>,
    /// Warning and error activity that must remain visible without --verbose.
    pub important_activity: Vec<String>,
    pub activity_had_error: bool,
    /// Complete output produced after the task transitions into a daemon.
    /// The presenter renders a short tail live and the UI keeps this complete
    /// transcript for the final, non-interactive-safe fence.
    pub daemon_output: Vec<String>,
    pub daemon_had_error: bool,
    pub debug_buffer: Vec<String>,
    pub debug_snapshot_indexes: HashMap<String, usize>,
    pub fenced_buffer: Vec<(String, Vec<String>, bool, Option<String>)>,
    pub diagnosed: bool,
    pub was_parallel: bool,
    pub is_daemon: bool,
}

impl TaskState {
    pub fn new(display_name: String, command: Option<String>) -> Self {
        Self {
            display_name,
            command,
            start_time: std::time::Instant::now(),
            buffer: Vec::new(),
            important_activity: Vec::new(),
            activity_had_error: false,
            daemon_output: Vec::new(),
            daemon_had_error: false,
            debug_buffer: Vec::new(),
            debug_snapshot_indexes: HashMap::new(),
            fenced_buffer: Vec::new(),
            diagnosed: false,
            was_parallel: false,
            is_daemon: false,
        }
    }
}

#[derive(Default)]
pub struct TaskTracker {
    pub tasks: HashMap<String, TaskState>,
    pub running_tasks: BTreeSet<String>,
}

impl TaskTracker {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            running_tasks: BTreeSet::new(),
        }
    }

    pub fn add(&mut self, name: String, display_name: String, command: Option<String>) {
        let mut state = TaskState::new(display_name, command);
        if let Some(existing) = self.tasks.get(&name) {
            state.buffer = existing.buffer.clone();
            state.important_activity = existing.important_activity.clone();
            state.activity_had_error = existing.activity_had_error;
            state.daemon_output = existing.daemon_output.clone();
            state.daemon_had_error = existing.daemon_had_error;
            state.debug_buffer = existing.debug_buffer.clone();
            state.debug_snapshot_indexes = existing.debug_snapshot_indexes.clone();
            state.diagnosed = existing.diagnosed;
            state.was_parallel = existing.was_parallel;
        }

        if !self.running_tasks.is_empty() {
            state.was_parallel = true;
            for r in &self.running_tasks {
                if let Some(t) = self.tasks.get_mut(r) {
                    t.was_parallel = true;
                }
            }
        }
        self.tasks.insert(name.clone(), state);
        self.running_tasks.insert(name);
    }

    pub fn remove(&mut self, name: &str) -> Option<TaskState> {
        self.running_tasks.remove(name);
        self.tasks.remove(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut TaskState> {
        self.tasks.get_mut(name)
    }

    pub fn get(&self, name: &str) -> Option<&TaskState> {
        self.tasks.get(name)
    }

    pub fn is_running(&self, name: &str) -> bool {
        self.running_tasks.contains(name)
    }

    pub fn mark_diagnosed(&mut self, name: &str) {
        if let Some(task) = self.get_mut(name) {
            task.diagnosed = true;
        } else {
            let mut state = TaskState::new(name.to_string(), None);
            state.diagnosed = true;
            self.tasks.insert(name.to_string(), state);
        }
    }

    pub fn get_latest_running_task(&self) -> Option<&TaskState> {
        let mut latest_task = None;
        for name in &self.running_tasks {
            if let Some(task) = self.get(name) {
                match latest_task {
                    None => latest_task = Some(task),
                    Some(latest) => {
                        if task.start_time > latest.start_time {
                            latest_task = Some(task);
                        }
                    }
                }
            }
        }
        latest_task
    }

    pub fn get_latest_running_task_mut(&mut self) -> Option<&mut TaskState> {
        let mut latest_name: Option<String> = None;
        let mut latest_time: Option<std::time::Instant> = None;

        for name in &self.running_tasks {
            if let Some(task) = self.tasks.get(name)
                && (latest_time.is_none() || task.start_time > latest_time.unwrap())
            {
                latest_time = Some(task.start_time);
                latest_name = Some(name.clone());
            }
        }

        if let Some(name) = latest_name {
            self.tasks.get_mut(&name)
        } else {
            None
        }
    }

    pub fn running_count(&self) -> usize {
        self.running_tasks.len()
    }
}

fn format_task_result(display_name: &str, duration_str: &str, was_parallel: bool) -> String {
    let symbol = if was_parallel {
        crate::render::SYMBOL_PARALLEL
    } else {
        crate::render::SYMBOL_SEQUENTIAL
    };
    format!(
        "{}  {}  {}",
        display_name,
        crate::render::dim(symbol),
        crate::render::dim(duration_str)
    )
}

fn is_timestamped_event(message: &str) -> bool {
    let bytes = message.as_bytes();
    bytes.len() >= 10
        && bytes[2] == b':'
        && bytes[5] == b':'
        && bytes[8] == b' '
        && bytes[9] == b' '
        && [0, 1, 3, 4, 6, 7]
            .into_iter()
            .all(|index| bytes[index].is_ascii_digit())
}

// --- Pipeline State Machine ---
pub struct PipelineState {
    pub mode: Mode,
    pub silent_success: bool,
    pub debug_enabled: bool,
    pub tracker: TaskTracker,
    pub daemons: BTreeSet<String>,
    pub pending_output: VecDeque<RenderedLine>,
    pub pipeline_start_time: std::time::Instant,
    pub global_progress: Option<String>,
    pub finished: bool,
    pub success: Option<bool>,
    pub verbose: bool,
}

impl PipelineState {
    pub fn new(mode: Mode, silent_success: bool, debug_enabled: bool, verbose: bool) -> Self {
        Self {
            mode,
            silent_success,
            debug_enabled,
            tracker: TaskTracker::new(),
            daemons: BTreeSet::new(),
            pending_output: VecDeque::new(),
            pipeline_start_time: std::time::Instant::now(),
            global_progress: None,
            finished: false,
            success: None,
            verbose,
        }
    }

    fn enqueue(&mut self, log: PendingLog) {
        self.pending_output.push_back(RenderedLine {
            log,
            enqueued_at: std::time::Instant::now(),
        });
    }

    pub fn ingest(&mut self, event: UIEvent) {
        match event {
            UIEvent::PipelineStart { message } => {
                self.global_progress = Some(message);
            }
            UIEvent::Restarting {
                display_name,
                duration,
            } => {
                self.enqueue(PendingLog::Restarting {
                    display_name,
                    duration,
                });
            }
            UIEvent::WatchEvent { line } => {
                let daemon_names: Vec<_> = self.daemons.iter().cloned().collect();
                if !daemon_names.is_empty() {
                    for name in daemon_names {
                        if let Some(task) = self.tracker.get_mut(&name) {
                            task.daemon_output.push(line.clone());
                        }
                    }
                }
            }
            UIEvent::WatchIdle { .. } => {}
            UIEvent::ItemStarted {
                name,
                description,
                command,
            } => {
                let display_name = description.unwrap_or_else(|| name.clone());
                self.tracker.add(name, display_name, command);
            }
            UIEvent::ItemFinished {
                name,
                failed,
                unverified,
                cancelled,
                duration,
            } => {
                let is_daemon = self.daemons.remove(&name)
                    || self.tracker.tasks.get(&name).is_some_and(|t| t.is_daemon);

                if is_daemon {
                    if let Some(task) = self.tracker.remove(&name)
                        && self.debug_enabled
                        && !task.debug_buffer.is_empty()
                    {
                        self.enqueue(PendingLog::Debug(task.debug_buffer));
                    }
                    return;
                }

                if cancelled {
                    if let Some(task) = self.tracker.remove(&name)
                        && self.debug_enabled
                        && !task.debug_buffer.is_empty()
                    {
                        self.enqueue(PendingLog::Debug(task.debug_buffer));
                    }
                    return;
                }

                let (
                    display_name,
                    mut lines,
                    debug_lines,
                    fenced_items,
                    important_activity,
                    activity_had_error,
                    _already_diagnosed,
                    was_parallel,
                    command,
                ) = if let Some(task) = self.tracker.remove(&name) {
                    (
                        task.display_name,
                        task.buffer,
                        task.debug_buffer,
                        task.fenced_buffer,
                        task.important_activity,
                        task.activity_had_error,
                        task.diagnosed,
                        task.was_parallel,
                        task.command,
                    )
                } else {
                    (
                        name.to_string(),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        false,
                        false,
                        false,
                        None,
                    )
                };

                if failed {
                    let duration_str = format_duration(duration.as_secs_f64());
                    let text = format_task_result(&display_name, &duration_str, was_parallel);
                    self.enqueue(PendingLog::Error(text));
                    if !debug_lines.is_empty() {
                        self.enqueue(PendingLog::Debug(debug_lines));
                    }
                    for (f_title, f_lines, _, _) in fenced_items {
                        if !lines.is_empty() {
                            lines.push(String::new());
                        }
                        lines.push(f_title);
                        lines.extend(f_lines);
                    }
                    if !lines.is_empty() {
                        let title = format!("{}{}", SYMBOL_FENCED_TASK_PREFIX, name);
                        self.enqueue(PendingLog::Fenced(title, lines, true, command));
                    }
                } else if unverified {
                    let text = format!("{} was skipped or unverified", display_name);
                    self.enqueue(PendingLog::Warn(text));
                    if !debug_lines.is_empty() {
                        self.enqueue(PendingLog::Debug(debug_lines));
                    }
                    for (f_title, f_lines, f_err, f_label) in fenced_items {
                        self.enqueue(PendingLog::Fenced(f_title, f_lines, f_err, f_label));
                    }
                } else {
                    let duration_str = format_duration(duration.as_secs_f64());
                    let text = format_task_result(&display_name, &duration_str, was_parallel);
                    self.enqueue(PendingLog::Success(text));
                    if !debug_lines.is_empty() {
                        self.enqueue(PendingLog::Debug(debug_lines));
                    }
                    for (f_title, f_lines, f_err, f_label) in fenced_items {
                        self.enqueue(PendingLog::Fenced(f_title, f_lines, f_err, f_label));
                    }
                    if self.verbose && !lines.is_empty() {
                        let title = format!("{}{}", SYMBOL_FENCED_TASK_PREFIX, name);
                        self.enqueue(PendingLog::Fenced(title, lines, false, command));
                    } else if !important_activity.is_empty() {
                        let title = format!("{}{}", SYMBOL_FENCED_TASK_PREFIX, name);
                        self.enqueue(PendingLog::Fenced(
                            title,
                            important_activity,
                            activity_had_error,
                            command,
                        ));
                    }
                }
            }
            UIEvent::Activity {
                name,
                level,
                message,
            } => {
                let clean_line = message.trim_end().to_string();
                let is_daemon = self.tracker.tasks.get(&name).is_some_and(|t| t.is_daemon);
                let is_running_task = self.tracker.is_running(&name) && !is_daemon;

                if is_daemon {
                    if matches!(level, PipelineLogLevel::Info) && !self.verbose {
                        return;
                    }
                    if let Some(task) = self.tracker.get_mut(&name) {
                        if matches!(level, PipelineLogLevel::Error) {
                            task.daemon_had_error = true;
                        }
                        task.daemon_output.push(clean_line);
                    }
                    return;
                }

                if let Some(task) = self.tracker.get_mut(&name) {
                    task.buffer.push(clean_line.clone());
                    if matches!(level, PipelineLogLevel::Warn | PipelineLogLevel::Error) {
                        task.important_activity.push(clean_line);
                    }
                    if matches!(level, PipelineLogLevel::Error) {
                        task.activity_had_error = true;
                    }
                } else if !is_running_task {
                    match level {
                        PipelineLogLevel::Info | PipelineLogLevel::Debug => {
                            if self.verbose {
                                self.enqueue(PendingLog::Info(clean_line));
                            }
                        }
                        PipelineLogLevel::Warn => self.enqueue(PendingLog::Warn(clean_line)),
                        PipelineLogLevel::Error => self.enqueue(PendingLog::Error(clean_line)),
                    }
                }
            }
            UIEvent::RawOutput { name, message } => {
                let clean_line = message.trim_end().to_string();
                let is_daemon = self.tracker.tasks.get(&name).is_some_and(|t| t.is_daemon);
                if let Some(task) = self.tracker.get_mut(&name) {
                    if is_daemon {
                        if self.verbose {
                            task.daemon_output.push(clean_line);
                        } else {
                            task.buffer.push(clean_line);
                        }
                    } else {
                        task.buffer.push(clean_line);
                    }
                } else if self.verbose {
                    self.enqueue(PendingLog::Info(clean_line));
                }
            }
            UIEvent::Log {
                name,
                level,
                message,
            } => {
                let clean_line = message.trim_end().to_string();
                let is_daemon = self.tracker.tasks.get(&name).is_some_and(|t| t.is_daemon);
                let is_running_task = self.tracker.is_running(&name) && !is_daemon;

                if matches!(level, PipelineLogLevel::Debug) {
                    if self.debug_enabled {
                        if is_daemon {
                            if let Some(task) = self.tracker.get_mut(&name) {
                                task.debug_buffer.push(clean_line);
                            }
                        } else if let Some(task) = self.tracker.get_mut(&name) {
                            task.debug_buffer.push(clean_line);
                        } else if !is_running_task {
                            let mut state = TaskState::new(name.clone(), None);
                            state.debug_buffer.push(clean_line);
                            self.tracker.tasks.insert(name.clone(), state);
                        }
                    }
                    return;
                }

                if is_daemon {
                    if matches!(level, PipelineLogLevel::Info) && !self.verbose {
                        return;
                    }
                    if let Some(task) = self.tracker.get_mut(&name) {
                        let line = match level {
                            PipelineLogLevel::Info => clean_line,
                            PipelineLogLevel::Warn => format!("Warning  {clean_line}"),
                            PipelineLogLevel::Error => {
                                task.daemon_had_error = true;
                                if is_timestamped_event(&clean_line) {
                                    clean_line
                                } else {
                                    format!("Error  {clean_line}")
                                }
                            }
                            PipelineLogLevel::Debug => unreachable!(),
                        };
                        task.daemon_output.push(line);
                    }
                    return;
                }

                let should_buffer = is_running_task || self.silent_success;

                if should_buffer {
                    if let Some(task) = self.tracker.get_mut(&name) {
                        task.buffer.push(clean_line);
                    } else if !is_running_task {
                        let mut state = TaskState::new(name.clone(), None);
                        state.buffer.push(clean_line);
                        self.tracker.tasks.insert(name.clone(), state);
                    }
                } else {
                    match level {
                        PipelineLogLevel::Debug => unreachable!(),
                        PipelineLogLevel::Info => {
                            self.enqueue(PendingLog::Info(clean_line));
                        }
                        PipelineLogLevel::Warn => {
                            self.enqueue(PendingLog::Warn(clean_line));
                        }
                        PipelineLogLevel::Error => {
                            self.enqueue(PendingLog::Error(format!("Error  {}", clean_line)));
                        }
                    }
                }
            }
            UIEvent::StructuralDebug {
                name,
                message,
                snapshot_key,
            } => {
                if !self.debug_enabled {
                    return;
                }
                let task = self
                    .tracker
                    .tasks
                    .entry(name.clone())
                    .or_insert_with(|| TaskState::new(name, None));
                let clean_line = message.trim_end().to_string();
                if let Some(key) = snapshot_key {
                    if let Some(index) = task.debug_snapshot_indexes.get(&key).copied() {
                        task.debug_buffer[index] = clean_line;
                    } else {
                        let index = task.debug_buffer.len();
                        task.debug_buffer.push(clean_line);
                        task.debug_snapshot_indexes.insert(key, index);
                    }
                } else {
                    task.debug_buffer.push(clean_line);
                }
            }
            UIEvent::Diagnostic {
                item_name,
                diagnostic,
                message,
            } => {
                let mut lines = Vec::new();
                for line in message.lines() {
                    if !line.is_empty() {
                        lines.push(line.to_string());
                    }
                }
                if let Some(hint_str) = &diagnostic.hint {
                    lines.push(String::new());
                    for line in hint_str.lines() {
                        lines.push(line.to_string());
                    }
                }
                if diagnostic.location.is_some()
                    && let Some(snippet) = crate::diagnostics::snippet::SnippetRenderer::new()
                        .render(&diagnostic, &message)
                {
                    lines.push(String::new());
                    lines.extend(snippet);
                }
                let title = diagnostic.title;
                let fenced_title = if title.starts_with("(error)") {
                    title
                } else {
                    format!("{}{}", SYMBOL_FENCED_ERROR_PREFIX, title)
                };
                let is_named_task = self
                    .tracker
                    .tasks
                    .get(&item_name)
                    .is_some_and(|task| task.is_daemon)
                    || self.tracker.is_running(&item_name);
                let target_task = if is_named_task {
                    self.tracker.get_mut(&item_name)
                } else {
                    self.tracker.get_latest_running_task_mut()
                };

                if let Some(task) = target_task {
                    if task.is_daemon {
                        task.daemon_had_error = true;
                        task.daemon_output.push(fenced_title);
                        task.daemon_output.extend(lines);
                    } else {
                        task.fenced_buffer.push((
                            fenced_title,
                            lines,
                            true,
                            Some("aborted".into()),
                        ));
                    }
                    task.diagnosed = true;
                } else {
                    self.enqueue(PendingLog::Fenced(
                        fenced_title,
                        lines,
                        true,
                        Some("aborted".into()),
                    ));
                    self.tracker.mark_diagnosed(&item_name);
                }
            }
            UIEvent::PlannedOperation { message } => {
                let text = self.mode.accent_style().apply_to(&message).to_string();
                self.enqueue(PendingLog::Info(text));
            }
            UIEvent::Progress { message } => {
                self.global_progress = Some(message);
            }
            UIEvent::PipelineState { success } => {
                if !success {
                    let orphaned: Vec<String> =
                        self.tracker.running_tasks.iter().cloned().collect();
                    for name in orphaned {
                        if let Some(task) = self.tracker.remove(&name) {
                            let duration_str =
                                format_duration(task.start_time.elapsed().as_secs_f64());
                            let text = format_task_result(
                                &task.display_name,
                                &duration_str,
                                task.was_parallel,
                            );
                            self.enqueue(PendingLog::Error(text));
                            if !task.debug_buffer.is_empty() {
                                self.enqueue(PendingLog::Debug(task.debug_buffer));
                            }
                            if !task.buffer.is_empty() {
                                let title = format!("{}{}", SYMBOL_FENCED_TASK_PREFIX, name);
                                self.enqueue(PendingLog::Fenced(
                                    title,
                                    task.buffer,
                                    true,
                                    task.command,
                                ));
                            } else {
                                for (f_title, f_lines, f_err, f_label) in task.fenced_buffer {
                                    self.enqueue(PendingLog::Fenced(
                                        f_title, f_lines, f_err, f_label,
                                    ));
                                }
                            }
                        }
                    }
                }
                self.finished = true;
                self.success = Some(success);
            }
            UIEvent::TaskDaemonStarted {
                name,
                startup_duration,
                status_message,
            } => {
                self.daemons.insert(name.clone());
                self.tracker.running_tasks.remove(&name);
                let display_name = if let Some(task) = self.tracker.tasks.get_mut(&name) {
                    task.is_daemon = true;
                    if self.verbose {
                        task.daemon_output.append(&mut task.buffer);
                    } else {
                        task.daemon_output
                            .extend(task.important_activity.iter().cloned());
                    }
                    task.display_name.clone()
                } else {
                    name.clone()
                };

                self.enqueue(PendingLog::DaemonStarted {
                    task_name: name,
                    display_name,
                    startup_duration,
                    status_message,
                });
            }
            UIEvent::TaskDaemonStatus {
                name,
                status_message,
            } => {
                self.enqueue(PendingLog::DaemonStatus {
                    name,
                    status_message,
                });
            }
            UIEvent::TaskDaemonStopped {
                name,
                duration,
                failed,
            } => {
                let (display_name, mut output, raw_output, debug_lines, has_error, command) = self
                    .tracker
                    .tasks
                    .get_mut(&name)
                    .map(|task| {
                        (
                            task.display_name.clone(),
                            task.daemon_output.clone(),
                            task.buffer.clone(),
                            std::mem::take(&mut task.debug_buffer),
                            task.daemon_had_error,
                            task.command.clone(),
                        )
                    })
                    .unwrap_or_else(|| {
                        (
                            name.clone(),
                            Vec::new(),
                            Vec::new(),
                            Vec::new(),
                            false,
                            None,
                        )
                    });

                if failed && !raw_output.is_empty() {
                    output.extend(raw_output);
                }

                if self.debug_enabled && !debug_lines.is_empty() {
                    self.enqueue(PendingLog::Debug(debug_lines));
                }

                if let Some(task) = self.tracker.tasks.get_mut(&name) {
                    task.debug_snapshot_indexes.clear();
                }

                if !output.is_empty() {
                    let title = format!("{}{}", SYMBOL_FENCED_TASK_PREFIX, name);
                    self.enqueue(PendingLog::Fenced(
                        title,
                        output,
                        failed || has_error,
                        command,
                    ));
                }
                self.enqueue(PendingLog::DaemonStopped {
                    name: display_name,
                    duration,
                    failed,
                });
            }
            UIEvent::Header {
                version,
                mode,
                os,
                arch,
                shell,
            } => {
                self.enqueue(PendingLog::Header {
                    version,
                    mode,
                    os,
                    arch,
                    shell,
                });
            }
            UIEvent::Footer { command, flags } => {
                self.enqueue(PendingLog::Footer { command, flags });
            }
            UIEvent::Intro { title } => {
                self.enqueue(PendingLog::Intro(title));
            }
            UIEvent::Summary { title, items } => {
                self.enqueue(PendingLog::Summary(title, items));
            }
            UIEvent::Step { message } => {
                self.enqueue(PendingLog::Step(message));
            }
            UIEvent::Remark { message } => {
                self.enqueue(PendingLog::Remark(message));
            }
            UIEvent::Outro { message } => {
                self.enqueue(PendingLog::Outro(message));
            }
            UIEvent::OutroCancel { message } => {
                self.enqueue(PendingLog::OutroCancel(message));
            }
            UIEvent::Error { message } => {
                self.enqueue(PendingLog::Error(message));
            }
            UIEvent::Fenced {
                title,
                lines,
                footer_label,
            } => {
                if let Some(task) = self.tracker.get_latest_running_task_mut() {
                    task.fenced_buffer.push((title, lines, false, footer_label));
                } else {
                    self.enqueue(PendingLog::Fenced(title, lines, false, footer_label));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evin_diagnostic::{Diagnostic, IssueCode};

    #[test]
    fn test_pipeline_state_clears_running_tasks_on_failure() {
        let mut state = PipelineState::new(Mode::Normal, false, false, false);
        state.ingest(UIEvent::ItemStarted {
            name: "task_a".to_string(),
            description: Some("Task A".to_string()),
            command: None,
        });
        state.ingest(UIEvent::ItemStarted {
            name: "task_b".to_string(),
            description: Some("Task B".to_string()),
            command: None,
        });

        assert_eq!(state.tracker.running_count(), 2);

        // Task A finishes with error
        state.ingest(UIEvent::ItemFinished {
            name: "task_a".to_string(),
            failed: true,
            unverified: false,
            cancelled: false,
            duration: std::time::Duration::from_millis(100),
        });

        assert_eq!(state.tracker.running_count(), 1);
        assert!(state.tracker.is_running("task_b"));

        // Pipeline fails - task_b was aborted without ItemFinished
        state.ingest(UIEvent::PipelineState { success: false });

        assert_eq!(state.tracker.running_count(), 0);
        assert!(!state.tracker.is_running("task_b"));
        assert!(state.finished);
        assert_eq!(state.success, Some(false));
    }

    #[test]
    fn pipeline_diagnostics_preserve_source_snippets_inside_the_fence() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline/state.rs");
        let source = std::fs::read_to_string(path).unwrap();
        let offset = source
            .find("pipeline_diagnostics_preserve_source_snippets")
            .unwrap();
        let diagnostic = Diagnostic::new(
            "Configuration Error",
            IssueCode::new("issue-action-config-invalid"),
        )
        .with_file_span(path, offset, 8, Some("invalid option".into()));
        let mut state = PipelineState::new(Mode::Normal, false, false, false);
        state.ingest(UIEvent::ItemStarted {
            name: "Resolving configuration".into(),
            description: None,
            command: None,
        });

        state.ingest(UIEvent::Diagnostic {
            item_name: "Pipeline".into(),
            diagnostic,
            message: "Unknown field `tile`.".into(),
        });

        let lines = &state
            .tracker
            .get("Resolving configuration")
            .unwrap()
            .fenced_buffer[0]
            .1;
        assert!(lines.iter().any(|line| line.contains(path)));
        assert!(lines.iter().any(|line| line.contains("invalid option")));
    }

    #[test]
    fn test_daemon_task_captures_logs_and_suppresses_duplicate_finish_line() {
        let mut state = PipelineState::new(Mode::Debug, false, true, true);

        // Start serve-docs task
        state.ingest(UIEvent::ItemStarted {
            name: "serve-docs".to_string(),
            description: Some("Serve Docs".to_string()),
            command: None,
        });

        // Pre-daemon debug log & info log
        state.ingest(UIEvent::Log {
            name: "serve-docs".to_string(),
            level: PipelineLogLevel::Debug,
            message: "[E_202] debug pre-daemon".to_string(),
        });
        state.ingest(UIEvent::Log {
            name: "serve-docs".to_string(),
            level: PipelineLogLevel::Info,
            message: "Building site assets...".to_string(),
        });

        // Transition into Daemon Started
        state.ingest(UIEvent::TaskDaemonStarted {
            name: "serve-docs".to_string(),
            startup_duration: std::time::Duration::from_millis(50),
            status_message: "Running on http://127.0.0.1:3000".to_string(),
        });

        // Pre-daemon output becomes part of the daemon transcript and debug
        // output waits for the daemon's single final debug block.
        let logs: Vec<_> = state.pending_output.iter().map(|l| &l.log).collect();
        assert!(matches!(logs[0], PendingLog::DaemonStarted { .. }));
        assert_eq!(logs.len(), 1);
        assert_eq!(
            state.tracker.get("serve-docs").unwrap().daemon_output,
            ["Building site assets..."]
        );

        // Post-start daemon logs remain in the daemon transcript. They must
        // never be emitted as top-level info/error lines.
        state.ingest(UIEvent::Log {
            name: "serve-docs".to_string(),
            level: PipelineLogLevel::Info,
            message: "Scanning documentation files...".to_string(),
        });
        state.ingest(UIEvent::Log {
            name: "serve-docs".to_string(),
            level: PipelineLogLevel::Error,
            message: "Documentation rebuild failed".to_string(),
        });
        state.ingest(UIEvent::Log {
            name: "serve-docs".to_string(),
            level: PipelineLogLevel::Debug,
            message: "[E_701] debug during daemon".to_string(),
        });
        state.ingest(UIEvent::StructuralDebug {
            name: "serve-docs".to_string(),
            message: "[E_551] scans=1 pages=10".to_string(),
            snapshot_key: Some("docs.scan".to_string()),
        });
        state.ingest(UIEvent::StructuralDebug {
            name: "serve-docs".to_string(),
            message: "[E_551] scans=2 pages=11".to_string(),
            snapshot_key: Some("docs.scan".to_string()),
        });
        let task = state.tracker.get("serve-docs").unwrap();
        assert!(
            task.daemon_output
                == [
                    "Building site assets...",
                    "Scanning documentation files...",
                    "Error  Documentation rebuild failed",
                ]
        );
        assert!(task.daemon_had_error);
        assert!(
            !state
                .pending_output
                .iter()
                .any(|line| matches!(line.log, PendingLog::Info(_) | PendingLog::Error(_)))
        );

        // Emit DaemonStopped and ItemFinished
        state.ingest(UIEvent::TaskDaemonStopped {
            name: "serve-docs".to_string(),
            duration: std::time::Duration::from_secs(5),
            failed: false,
        });
        state.ingest(UIEvent::ItemFinished {
            name: "serve-docs".to_string(),
            failed: false,
            unverified: false,
            cancelled: false,
            duration: std::time::Duration::from_secs(5),
        });

        // Verify no duplicate PendingLog::Success was enqueued by ItemFinished
        let final_logs: Vec<_> = state.pending_output.iter().map(|l| &l.log).collect();
        assert!(final_logs.iter().any(|log| matches!(
            log,
            PendingLog::Fenced(title, lines, true, _) if title.contains("serve-docs")
                && lines.last().is_some_and(|line| line.contains("Documentation rebuild failed"))
        )));
        assert!(final_logs.iter().any(|log| matches!(
            log,
            PendingLog::Debug(lines)
                if lines == &[
                    "[E_202] debug pre-daemon",
                    "[E_701] debug during daemon",
                    "[E_551] scans=2 pages=11",
                ]
        )));
        let success_count = final_logs
            .iter()
            .filter(|l| matches!(l, PendingLog::Success(_)))
            .count();
        assert_eq!(
            success_count, 0,
            "ItemFinished should not enqueue PendingLog::Success for daemon task"
        );
    }

    #[test]
    fn structural_debug_snapshots_replace_in_place_and_remain_task_local() {
        let mut state = PipelineState::new(Mode::Debug, false, true, true);
        for task in ["docs", "api"] {
            state.ingest(UIEvent::ItemStarted {
                name: task.to_string(),
                description: None,
                command: None,
            });
        }
        state.ingest(UIEvent::StructuralDebug {
            name: "docs".to_string(),
            message: "[E_109] overrides=2".to_string(),
            snapshot_key: None,
        });
        state.ingest(UIEvent::StructuralDebug {
            name: "docs".to_string(),
            message: "[E_551] scans=1 pages=10".to_string(),
            snapshot_key: Some("docs.scan".to_string()),
        });
        state.ingest(UIEvent::StructuralDebug {
            name: "docs".to_string(),
            message: "[E_552] audits=1 links=4".to_string(),
            snapshot_key: Some("docs.links".to_string()),
        });
        state.ingest(UIEvent::StructuralDebug {
            name: "docs".to_string(),
            message: "[E_551] scans=3 pages=12".to_string(),
            snapshot_key: Some("docs.scan".to_string()),
        });
        state.ingest(UIEvent::StructuralDebug {
            name: "api".to_string(),
            message: "[E_551] scans=1 pages=2".to_string(),
            snapshot_key: Some("docs.scan".to_string()),
        });

        assert_eq!(
            state.tracker.get("docs").unwrap().debug_buffer,
            [
                "[E_109] overrides=2",
                "[E_551] scans=3 pages=12",
                "[E_552] audits=1 links=4",
            ]
        );
        assert_eq!(
            state.tracker.get("api").unwrap().debug_buffer,
            ["[E_551] scans=1 pages=2"]
        );

        state.ingest(UIEvent::ItemFinished {
            name: "docs".to_string(),
            failed: false,
            unverified: false,
            cancelled: false,
            duration: std::time::Duration::from_millis(1),
        });
        assert!(state.pending_output.iter().any(|line| matches!(
            &line.log,
            PendingLog::Debug(lines) if lines == &[
                "[E_109] overrides=2",
                "[E_551] scans=3 pages=12",
                "[E_552] audits=1 links=4",
            ]
        )));
    }

    #[test]
    fn daemon_preserves_timestamped_error_events_without_a_generic_prefix() {
        let mut state = PipelineState::new(Mode::Normal, false, false, true);
        state.ingest(UIEvent::ItemStarted {
            name: "docs:serve".to_string(),
            description: None,
            command: None,
        });
        state.ingest(UIEvent::TaskDaemonStarted {
            name: "docs:serve".to_string(),
            startup_duration: std::time::Duration::from_millis(1),
            status_message: "Watching documentation changes".to_string(),
        });
        state.ingest(UIEvent::Activity {
            name: "docs:serve".to_string(),
            level: PipelineLogLevel::Error,
            message: "14:33:02  rebuild  failed".to_string(),
        });

        let task = state.tracker.get("docs:serve").unwrap();
        assert_eq!(task.daemon_output, ["14:33:02  rebuild  failed"]);
        assert!(task.daemon_had_error);
    }

    #[test]
    fn daemon_hides_routine_activity_without_verbose_output() {
        let mut state = PipelineState::new(Mode::Normal, false, false, false);
        state.ingest(UIEvent::ItemStarted {
            name: "docs:serve".to_string(),
            description: None,
            command: None,
        });
        for message in [
            "22:44:20  scan    Scanning documentation files...",
            "22:44:20  audit   Auditing internal links and anchor IDs...",
            "22:44:20  ready   Documentation site generated",
        ] {
            state.ingest(UIEvent::Activity {
                name: "docs:serve".to_string(),
                level: PipelineLogLevel::Info,
                message: message.to_string(),
            });
        }
        state.ingest(UIEvent::TaskDaemonStarted {
            name: "docs:serve".to_string(),
            startup_duration: std::time::Duration::from_millis(1),
            status_message: "Watching documentation changes".to_string(),
        });
        state.ingest(UIEvent::Activity {
            name: "docs:serve".to_string(),
            level: PipelineLogLevel::Info,
            message: "22:44:21  scan    Scanning documentation files...".to_string(),
        });
        state.ingest(UIEvent::Activity {
            name: "docs:serve".to_string(),
            level: PipelineLogLevel::Warn,
            message: "22:44:21  warning A recoverable rebuild warning".to_string(),
        });

        assert_eq!(
            state.tracker.get("docs:serve").unwrap().daemon_output,
            ["22:44:21  warning A recoverable rebuild warning"]
        );
    }

    #[test]
    fn daemon_lifecycle_uses_the_task_description_as_its_display_name() {
        let mut state = PipelineState::new(Mode::Normal, false, false, true);
        state.ingest(UIEvent::ItemStarted {
            name: "docs:serve".to_string(),
            description: Some("Serve live-reloading documentation dev server".to_string()),
            command: Some("evin/docs@v1".to_string()),
        });
        state.ingest(UIEvent::TaskDaemonStarted {
            name: "docs:serve".to_string(),
            startup_duration: std::time::Duration::from_millis(1),
            status_message: "Watching documentation changes".to_string(),
        });

        assert!(matches!(
            &state.pending_output[0].log,
            PendingLog::DaemonStarted { task_name, display_name, .. }
                if task_name == "docs:serve"
                    && display_name == "Serve live-reloading documentation dev server"
        ));
    }

    fn completed_activity_state(verbose: bool, debug: bool) -> PipelineState {
        let mut state = PipelineState::new(Mode::Normal, false, debug, verbose);
        state.ingest(UIEvent::ItemStarted {
            name: "build".into(),
            description: None,
            command: Some("./scripts/build".into()),
        });
        state.ingest(UIEvent::Activity {
            name: "build".into(),
            level: PipelineLogLevel::Info,
            message: "12:00:00  build   compiling".into(),
        });
        state.ingest(UIEvent::RawOutput {
            name: "build".into(),
            message: "compiler output".into(),
        });
        state.ingest(UIEvent::Activity {
            name: "build".into(),
            level: PipelineLogLevel::Warn,
            message: "12:00:01  quality Warning: generated fallback".into(),
        });
        state.ingest(UIEvent::StructuralDebug {
            name: "build".into(),
            message: "[E_554] renders=1".into(),
            snapshot_key: Some("docs.output".into()),
        });
        state.ingest(UIEvent::ItemFinished {
            name: "build".into(),
            failed: false,
            unverified: false,
            cancelled: false,
            duration: std::time::Duration::from_millis(1),
        });
        state
    }

    #[test]
    fn activity_raw_output_and_debug_follow_the_four_mode_matrix() {
        for (verbose, debug) in [(false, false), (true, false), (false, true), (true, true)] {
            let state = completed_activity_state(verbose, debug);
            let fences = state
                .pending_output
                .iter()
                .filter_map(|line| match &line.log {
                    PendingLog::Fenced(_, lines, _, _) => Some(lines),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(fences.len(), 1, "verbose={verbose} debug={debug}");
            assert_eq!(
                fences[0].iter().any(|line| line.contains("compiling")),
                verbose
            );
            assert_eq!(
                fences[0]
                    .iter()
                    .any(|line| line.contains("compiler output")),
                verbose
            );
            assert!(
                fences[0]
                    .iter()
                    .any(|line| line.contains("generated fallback"))
            );
            assert_eq!(
                state
                    .pending_output
                    .iter()
                    .any(|line| matches!(line.log, PendingLog::Debug(_))),
                debug
            );
        }
    }
}
