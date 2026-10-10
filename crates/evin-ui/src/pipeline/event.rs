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

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineLogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub enum UIEvent {
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
    PipelineStart {
        message: String,
    },
    Intro {
        title: String,
    },
    Summary {
        title: String,
        items: Vec<String>,
    },
    Step {
        message: String,
    },
    Remark {
        message: String,
    },
    Outro {
        message: String,
    },
    OutroCancel {
        message: String,
    },
    Error {
        message: String,
    },
    Restarting {
        display_name: String,
        duration: Duration,
    },
    WatchEvent {
        line: String,
    },
    WatchIdle {
        message: String,
    },
    ItemStarted {
        name: String,
        description: Option<String>,
        command: Option<String>,
    },
    TaskStepStarted {
        name: String,
        step_number: usize,
        step_count: usize,
        footer: Option<String>,
    },
    ItemFinished {
        name: String,
        failed: bool,
        unverified: bool,
        cancelled: bool,
        duration: Duration,
    },
    Log {
        name: String,
        level: PipelineLogLevel,
        message: String,
    },
    /// A localized, timestamped action activity line.
    Activity {
        name: String,
        level: PipelineLogLevel,
        message: String,
    },
    /// Verbatim output from a subprocess stdout or stderr stream.
    RawOutput {
        name: String,
        message: String,
    },
    StructuralDebug {
        name: String,
        message: String,
        snapshot_key: Option<String>,
    },
    Diagnostic {
        item_name: String,
        diagnostic: evin_diagnostic::Diagnostic,
        message: String,
    },
    PlannedOperation {
        message: String,
    },
    Progress {
        message: String,
    },
    PipelineState {
        success: bool,
    },
    Fenced {
        title: String,
        lines: Vec<String>,
        footer_label: Option<String>,
    },
    TaskDaemonStarted {
        name: String,
        startup_duration: Duration,
        status_message: String,
    },
    TaskDaemonStatus {
        name: String,
        status_message: String,
    },
    TaskDaemonStopped {
        name: String,
        duration: Duration,
        failed: bool,
    },
}
