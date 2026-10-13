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

use evin_action_sdk::OutputStream;
use evin_action_sdk::activity::{ActivityLevel, ActivityRecord, LocalizedText};
use evin_action_sdk::debug::{self, DebugAggregation, DebugRecord};
use evin_diagnostic::Diagnostic;
use evin_pipeline::telemetry::{TraceEvent, TraceFormat};
use evin_ui::event::UIEvent;
use evin_ui::mode::Mode;
use evin_ui::pipeline::{PipelineLogLevel, PipelineUI};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum PipelineEvent {
    Header {
        version: String,
        mode: String,
        os: String,
        arch: String,
        shell: String,
    },
    WorkspaceHeader {
        version: String,
        workspace: String,
    },
    Footer {
        command: String,
        flags: Vec<String>,
    },
    Intro {
        text: String,
        mode: Mode,
    },
    PhaseStarted(String),
    PhaseCompleted(String, Duration),
    PhaseFailed(String, Duration, Vec<Diagnostic>),
    DebugLog(Option<String>, DebugRecord),
    TaskOutputStarted {
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
    TaskOutputEnded {
        name: String,
        duration: Duration,
        failed: bool,
        unverified: bool,
        cancelled: bool,
    },
    Activity {
        task: String,
        record: ActivityRecord,
    },
    RawOutput {
        task: String,
        stream: OutputStream,
        line: String,
    },
    PlannedOperation(LocalizedText),
    Progress(LocalizedText),
    Diagnostic {
        task: String,
        diagnostic: Diagnostic,
    },
    TelemetryReport {
        telemetry: Vec<TraceEvent>,
        workspace_root: PathBuf,
    },
    PipelineSummary {
        total_duration: Duration,
        peak_memory_kb: f64,
        context: evin_pipeline::PipelineContext,
        targets: Vec<String>,
        outcome: PipelineSummaryOutcome,
    },
    SyncSummary {
        details: Vec<String>,
    },
    PipelineCancelled,
    /// The current watch session ended intentionally so the reporter can
    /// flush it without terminating the long-lived supervisor UI.
    PipelineSessionInterrupted,
    /// A watch session failed, but the supervisor remains available for a
    /// later source change.
    WatchSessionFailed,
    WatchEvent {
        line: String,
    },
    Restarting {
        display_name: String,
        duration: Duration,
    },
    SupervisorTerminated {
        forced: bool,
    },
    FencedOutput {
        title: String,
        lines: Vec<String>,
        footer_label: Option<String>,
    },
    TaskDaemonStarted {
        name: String,
        startup_duration: Duration,
        status_message: LocalizedText,
    },
    TaskDaemonStatus {
        name: String,
        status_message: LocalizedText,
    },
    TaskDaemonStopped {
        name: String,
        duration: Duration,
        failed: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineSummaryOutcome {
    Success,
    Cancelled,
}

fn render_localized(text: &LocalizedText) -> String {
    text.redact_rendered(&evin_locales::render_message(text.key, &text.args))
}

fn render_activity(record: &ActivityRecord) -> String {
    let at: chrono::DateTime<chrono::Local> = record.at.into();
    let message = render_localized(&record.text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let message = match record.level {
        ActivityLevel::Info => message,
        ActivityLevel::Warn => format!("{}: {message}", evin_locales::t!("activity-warning-label")),
        ActivityLevel::Error => format!("{}: {message}", evin_locales::t!("activity-error-label")),
    };
    format!(
        "{}  {:<7} {}",
        at.format("%H:%M:%S"),
        record.category.as_str(),
        message
    )
}

pub async fn run_ui_reporter(
    mut rx: mpsc::UnboundedReceiver<PipelineEvent>,
    mode: Mode,
    verbose: bool,
    watch_mode: bool,
) -> bool {
    let debug_enabled = matches!(mode, Mode::Debug);
    let mut ui = PipelineUI::new(mode, false, debug_enabled, verbose);

    // Spawn the spinner background task immediately so animation is live from the start
    ui.handle_event(UIEvent::PipelineStart {
        message: String::new(),
    })
    .await;

    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(50));
    let mut active_task = String::new();
    let mut phase_failed = false;
    let mut shutting_down = false;
    let mut exit_code = true;

    loop {
        if shutting_down && ui.is_idle() {
            while let Ok(event) = rx.try_recv() {
                if let PipelineEvent::Footer { command, flags } = event {
                    ui.handle_event(UIEvent::Footer { command, flags }).await;
                }
            }
            ui.tick(true).await;
            return exit_code;
        }

        tokio::select! {
            _ = ticker.tick() => {
                ui.tick(false).await;
            }
            event = rx.recv(), if !shutting_down => {
                match event {
                    Some(event) => {
                        match event {
                            PipelineEvent::Header { version, mode, os, arch, shell } => {
                                ui.handle_event(UIEvent::Header { version, mode, os, arch, shell }).await;
                            }
                            PipelineEvent::WorkspaceHeader { version, workspace } => {
                                ui.handle_event(UIEvent::WorkspaceHeader { version, workspace }).await;
                            }
                            PipelineEvent::Footer { command, flags } => {
                                ui.handle_event(UIEvent::Footer { command, flags }).await;
                            }
                            PipelineEvent::Intro { text, mode: intro_mode } => {
                                ui.handle_event(UIEvent::Intro {
                                    title: intro_mode.title_style().apply_to(intro_mode.format_title(&text)).to_string(),
                                }).await;
                            }
                            PipelineEvent::PhaseStarted(locale_key) => {
                                if locale_key == "ui.phase.executing_tasks" {
                                    continue;
                                }
                                let display_name = evin_ui::pipeline::presenter::resolve_phase_display_name(&locale_key);
                                active_task = display_name.clone();
                                ui.handle_event(UIEvent::ItemStarted {
                                    name: display_name,
                                    description: None,
                                    command: None,
                                }).await;
                            }
                            PipelineEvent::PhaseCompleted(locale_key, duration) => {
                                if locale_key == "ui.phase.executing_tasks" {
                                    continue;
                                }
                                let display_name = evin_ui::pipeline::presenter::resolve_phase_display_name(&locale_key);
                                ui.handle_event(UIEvent::ItemFinished {
                                    name: display_name,
                                    failed: false,
                                    unverified: false,
                                    cancelled: false,
                                    duration,
                                }).await;
                            }
                            PipelineEvent::PhaseFailed(locale_key, duration, diagnostics) => {
                                phase_failed = true;
                                if locale_key == "ui.phase.executing_tasks" {
                                    for diagnostic in diagnostics {
                                        let rendered = evin_locales::render_issue(&diagnostic.code);
                                        ui.handle_event(UIEvent::Diagnostic {
                                            item_name: "Pipeline".into(),
                                            diagnostic,
                                            message: rendered,
                                        }).await;
                                    }
                                    ui.handle_event(UIEvent::PipelineState { success: false }).await;
                                    continue;
                                }
                                let display_name = evin_ui::pipeline::presenter::resolve_phase_display_name(&locale_key);
                                for diagnostic in diagnostics {
                                    let rendered = evin_locales::render_issue(&diagnostic.code);
                                    ui.handle_event(UIEvent::PhaseDiagnostic {
                                        phase_name: display_name.clone(),
                                        diagnostic,
                                        message: rendered,
                                    }).await;
                                }
                                ui.handle_event(UIEvent::ItemFinished {
                                    name: display_name,
                                    failed: true,
                                    unverified: false,
                                    cancelled: false,
                                    duration,
                                }).await;
                                ui.handle_event(UIEvent::PipelineState { success: false }).await;
                            }
                            PipelineEvent::DebugLog(task, record) => {
                                // Structural telemetry is intentionally absent outside
                                // debug mode. Dropping it before the UI queue also keeps
                                // invisible records from delaying the visible phase end.
                                if !debug_enabled {
                                    continue;
                                }
                                let msg = debug::format_record(&record);
                                let target_task = task.unwrap_or_else(|| active_task.clone());
                                let snapshot_key = match record.aggregation {
                                    DebugAggregation::Append => None,
                                    DebugAggregation::Snapshot { key } => Some(key.to_string()),
                                };
                                ui.handle_event(UIEvent::StructuralDebug {
                                    name: target_task,
                                    message: msg,
                                    snapshot_key,
                                }).await;
                            }
                            PipelineEvent::TaskOutputStarted { name, description, command } => {
                                active_task = name.clone();
                                ui.handle_event(UIEvent::ItemStarted {
                                    name,
                                    description,
                                    command,
                                }).await;
                            }
                            PipelineEvent::TaskStepStarted { name, step_number, step_count, footer } => {
                                ui.handle_event(UIEvent::TaskStepStarted {
                                    name,
                                    step_number,
                                    step_count,
                                    footer,
                                }).await;
                            }
                            PipelineEvent::Activity { task, record } => {
                                let pipeline_level = match record.level {
                                    ActivityLevel::Info => PipelineLogLevel::Info,
                                    ActivityLevel::Warn => PipelineLogLevel::Warn,
                                    ActivityLevel::Error => PipelineLogLevel::Error,
                                };
                                let message = render_activity(&record);
                                ui.handle_event(UIEvent::Activity {
                                    name: task,
                                    level: pipeline_level,
                                    message,
                                }).await;
                            }
                            PipelineEvent::RawOutput { task, stream: _, line } => {
                                ui.handle_event(UIEvent::RawOutput { name: task, message: line }).await;
                            }
                            PipelineEvent::TaskOutputEnded { name, duration, failed, unverified, cancelled } => {
                                ui.handle_event(UIEvent::ItemFinished {
                                    name,
                                    failed,
                                    unverified,
                                    cancelled,
                                    duration,
                                }).await;
                            }
                            PipelineEvent::PlannedOperation(message) => {
                                ui.handle_event(UIEvent::PlannedOperation { message: render_localized(&message) }).await;
                            }
                            PipelineEvent::Progress(message) => {
                                ui.handle_event(UIEvent::Progress { message: render_localized(&message) }).await;
                            }
                            PipelineEvent::Diagnostic { task, diagnostic } => {
                                let rendered = evin_locales::render_issue(&diagnostic.code);
                                ui.handle_event(UIEvent::Diagnostic {
                                    item_name: task,
                                    diagnostic,
                                    message: rendered.clone(),
                                }).await;
                            }
                            PipelineEvent::TelemetryReport { telemetry, workspace_root } => {
                                if mode != Mode::DryRun {
                                    write_telemetry_report(telemetry, workspace_root);
                                }
                            }
                            PipelineEvent::PipelineSummary { total_duration, peak_memory_kb, context, targets, outcome } => {
                                let elapsed = total_duration.as_secs_f64();

                                let title = match mode {
                                    Mode::DryRun => evin_locales::t!("dry-run-summary-title"),
                                    Mode::Normal | Mode::Debug => evin_locales::t!("summary-title"),
                                };

                                let peak_memory_kb_opt = if matches!(mode, Mode::Debug) {
                                    Some(peak_memory_kb)
                                } else {
                                    None
                                };
                                let is_hook = context == evin_pipeline::PipelineContext::Hook;
                                let items = evin_ui::pipeline::presenter::build_pipeline_summary_items(
                                    mode, is_hook, &targets, elapsed, peak_memory_kb_opt
                                );

                                ui.handle_event(UIEvent::Summary {
                                    title,
                                    items,
                                }).await;

                                if outcome == PipelineSummaryOutcome::Success {
                                    let completion = match mode {
                                        Mode::DryRun => evin_locales::t!("dry-run-completed-successfully"),
                                        Mode::Normal | Mode::Debug => evin_locales::t!("execution-finished"),
                                    };

                                    let msg = mode.accent_style().apply_to(completion).to_string();
                                    ui.handle_event(UIEvent::Outro { message: msg }).await;
                                    ui.handle_event(UIEvent::PipelineState { success: true }).await;

                                    shutting_down = !watch_mode;
                                    exit_code = true;
                                }
                            }
                            PipelineEvent::SyncSummary { details } => {
                                let title = if mode == Mode::DryRun {
                                    evin_locales::t!("git-hooks-preview")
                                } else {
                                    evin_locales::t!("summary-title")
                                };
                                ui.handle_event(UIEvent::Summary {
                                    title,
                                    items: details,
                                }).await;

                                let completion = if mode == Mode::DryRun {
                                    evin_locales::t!("dry-run-completed-successfully")
                                } else {
                                    evin_locales::t!("execution-finished")
                                };

                                let msg = mode.accent_style().apply_to(completion).to_string();
                                ui.handle_event(UIEvent::Outro { message: msg }).await;
                                ui.handle_event(UIEvent::PipelineState { success: true }).await;

                                shutting_down = !watch_mode;
                                exit_code = true;
                            }
                            PipelineEvent::PipelineCancelled => {
                                let message = if phase_failed {
                                    evin_locales::t!("pipeline-failed")
                                } else {
                                    evin_locales::t!("pipeline-rejected")
                                };
                                ui.finalize_cancelled(message).await;
                                shutting_down = true;
                                exit_code = false;
                            }
                            PipelineEvent::PipelineSessionInterrupted => {
                                ui.tick(true).await;
                            }
                            PipelineEvent::WatchSessionFailed => {
                                phase_failed = false;
                                ui.handle_event(UIEvent::Error {
                                    message: evin_locales::t!("watch-session-failed"),
                                }).await;
                                ui.handle_event(UIEvent::PipelineState { success: false }).await;
                                ui.handle_event(UIEvent::WatchIdle {
                                    message: evin_locales::t!("watching-for-changes"),
                                }).await;
                            }
                            PipelineEvent::WatchEvent { line } => {
                                ui.handle_event(UIEvent::WatchEvent { line }).await;
                            }
                            PipelineEvent::Restarting { display_name, duration } => {
                                phase_failed = false;
                                // Commit every pending line from the old session before
                                // resetting transient task/spinner state. Scrollback stays intact.
                                ui.tick(true).await;
                                ui.reset_session().await;
                                ui.handle_event(UIEvent::Restarting { display_name, duration }).await;
                            }
                            PipelineEvent::SupervisorTerminated { forced: _ } => {
                                ui.finalize_cancelled(evin_locales::t!("pipeline-rejected")).await;
                                shutting_down = true;
                                exit_code = false;
                            }
                            PipelineEvent::FencedOutput { title, lines, footer_label } => {
                                ui.handle_event(UIEvent::Fenced { title, lines, footer_label }).await;
                            }
                            PipelineEvent::TaskDaemonStarted { name, startup_duration, status_message } => {
                                ui.handle_event(UIEvent::TaskDaemonStarted {
                                    name,
                                    startup_duration,
                                    status_message: render_localized(&status_message),
                                }).await;
                            }
                            PipelineEvent::TaskDaemonStatus { name, status_message } => {
                                ui.handle_event(UIEvent::TaskDaemonStatus {
                                    name,
                                    status_message: render_localized(&status_message),
                                }).await;
                            }
                            PipelineEvent::TaskDaemonStopped { name, duration, failed } => {
                                ui.handle_event(UIEvent::TaskDaemonStopped {
                                    name,
                                    duration,
                                    failed,
                                }).await;
                            }
                        }
                    }
                    None => {
                        shutting_down = true;
                        exit_code = false;
                    }
                }
            }
        }
    }
}

fn write_telemetry_report(telemetry: Vec<TraceEvent>, workspace_root: PathBuf) {
    let trace_format = TraceFormat {
        trace_events: telemetry,
        display_time_unit: "ms".to_string(),
    };
    let evin_dir = workspace_root.join(".evin");
    let trace_file = evin_dir.join("trace.json");
    let _ = std::fs::create_dir_all(&evin_dir);
    if let Ok(json) = serde_json::to_string_pretty(&trace_format) {
        let _ = std::fs::write(&trace_file, json);
    }
}
