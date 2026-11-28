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
use evin_pipeline::telemetry::{TraceEvent, TraceFormat, TraceMetadata, TraceScope, TraceSummary};
use evin_ui::event::UIEvent;
use evin_ui::mode::Mode;
use evin_ui::pipeline::{PipelineLogLevel, PipelineUI};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
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
        context: Option<TelemetryContext>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryContext {
    pub command: String,
    pub targets: Vec<String>,
    pub context: String,
    pub hook: Option<String>,
    pub mode: String,
    pub watch_session: Option<u64>,
    pub started_at: String,
    pub duration_us: u64,
    pub outcome: String,
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
                            PipelineEvent::TelemetryReport { telemetry, workspace_root, context } => {
                                if mode != Mode::DryRun {
                                    write_telemetry_report(telemetry, workspace_root, context);
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

fn atomic_write_json(file_path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let unique_id = format!(
        "{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let tmp_path = file_path.with_extension(format!("tmp.{unique_id}"));
    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, file_path)
}

fn compute_max_concurrency(tasks: &[&TraceEvent]) -> usize {
    let mut endpoints: Vec<(u64, i32)> = Vec::new();
    for t in tasks {
        if let Some(start) = t.ts {
            let end = start + t.dur.unwrap_or(0);
            endpoints.push((start, 1));
            endpoints.push((end, -1));
        }
    }
    endpoints.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let mut current = 0i32;
    let mut max_c = 0i32;
    for (_, delta) in endpoints {
        current += delta;
        if current > max_c {
            max_c = current;
        }
    }
    max_c.max(0) as usize
}

fn compute_critical_path(tasks: &[&TraceEvent]) -> u64 {
    let mut task_durations: HashMap<String, u64> = HashMap::new();
    let mut task_deps: HashMap<String, Vec<String>> = HashMap::new();

    for task in tasks {
        let dur = task.dur.unwrap_or(0);
        task_durations.insert(task.name.clone(), dur);
        let deps = task
            .args
            .get("needs")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        task_deps.insert(task.name.clone(), deps);
    }

    fn longest_path(
        node: &str,
        task_durations: &HashMap<String, u64>,
        task_deps: &HashMap<String, Vec<String>>,
        memo: &mut HashMap<String, u64>,
        visiting: &mut HashSet<String>,
    ) -> u64 {
        if let Some(&val) = memo.get(node) {
            return val;
        }
        if visiting.contains(node) {
            return 0;
        }
        visiting.insert(node.to_string());

        let my_dur = task_durations.get(node).copied().unwrap_or(0);
        let max_dep = task_deps
            .get(node)
            .map(|deps| {
                deps.iter()
                    .filter(|d| task_durations.contains_key(d.as_str()))
                    .map(|d| longest_path(d, task_durations, task_deps, memo, visiting))
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);

        visiting.remove(node);
        let total = my_dur + max_dep;
        memo.insert(node.to_string(), total);
        total
    }

    let mut max_crit = 0;
    let mut memo: HashMap<String, u64> = HashMap::new();
    let mut visiting = HashSet::new();
    for task_name in task_durations.keys() {
        let path = longest_path(
            task_name,
            &task_durations,
            &task_deps,
            &mut memo,
            &mut visiting,
        );
        if path > max_crit {
            max_crit = path;
        }
    }

    max_crit
}

fn compute_summary(tasks: &[&TraceEvent]) -> TraceSummary {
    let mut failed = 0;
    let mut unverified = 0;
    let mut cancelled = 0;

    for t in tasks {
        let status = t
            .args
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        match status {
            "failed" => failed += 1,
            "unverified" => unverified += 1,
            "cancelled" => cancelled += 1,
            _ => {}
        }
    }

    let max_concurrency = compute_max_concurrency(tasks);
    let critical_path_us = compute_critical_path(tasks);

    TraceSummary {
        tasks: tasks.len(),
        failed,
        unverified,
        cancelled,
        max_concurrency,
        critical_path_us,
    }
}

fn write_telemetry_report(
    telemetry: Vec<TraceEvent>,
    workspace_root: PathBuf,
    context: Option<TelemetryContext>,
) {
    let run_id = format!(
        "{:x}_{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros(),
        std::process::id()
    );

    let (
        command,
        targets,
        ctx_type,
        hook,
        mode_str,
        watch_session,
        started_at,
        duration_us,
        outcome_str,
    ) = if let Some(ctx) = context {
        (
            ctx.command,
            ctx.targets,
            ctx.context,
            ctx.hook,
            ctx.mode,
            ctx.watch_session,
            ctx.started_at,
            ctx.duration_us,
            ctx.outcome,
        )
    } else {
        (
            "run".to_string(),
            vec![],
            "run".to_string(),
            None,
            "normal".to_string(),
            None,
            chrono::Utc::now().to_rfc3339(),
            0,
            if telemetry.is_empty() {
                "empty".to_string()
            } else {
                "success".to_string()
            },
        )
    };

    if telemetry.is_empty() {
        let root_meta = TraceMetadata {
            schema: "evin.trace/v1".to_string(),
            run_id,
            scope: TraceScope::Workspace,
            workspace_trace: None,
            evin_version: env!("CARGO_PKG_VERSION").to_string(),
            command,
            targets,
            context: ctx_type,
            hook,
            mode: mode_str,
            watch_session,
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            started_at,
            duration_us,
            outcome: "empty".to_string(),
            summary: TraceSummary::default(),
            modules: vec![],
        };
        let root_doc = TraceFormat {
            other_data: Some(root_meta),
            trace_events: vec![],
            display_time_unit: "ms".to_string(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&root_doc) {
            let root_trace_path = workspace_root.join(".evin").join("trace.json");
            let _ = atomic_write_json(&root_trace_path, &json);
        }
        return;
    }

    let mut task_to_module: HashMap<String, Option<(String, String)>> = HashMap::new();
    let mut touched_modules: BTreeMap<String, String> = BTreeMap::new();

    for event in &telemetry {
        if event.cat == "task" {
            let source_opt = event.args.get("source").and_then(|v| v.as_str());
            if let Some(source) = source_opt {
                let source_path = Path::new(source);
                if let Some(mod_root) = evin_config::paths::find_module_root(source_path) {
                    let root_str = mod_root.display().to_string().replace('\\', "/");
                    let mod_name = event
                        .args
                        .get("module")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .unwrap_or_else(|| {
                            event
                                .name
                                .split_once(':')
                                .map(|(m, _)| m.to_string())
                                .unwrap_or_else(|| {
                                    mod_root
                                        .file_name()
                                        .map(|f| f.to_string_lossy().to_string())
                                        .unwrap_or_default()
                                })
                        });
                    touched_modules.insert(mod_name.clone(), root_str.clone());
                    task_to_module.insert(event.name.clone(), Some((mod_name, root_str)));
                    continue;
                }
            }
            task_to_module.insert(event.name.clone(), None);
        }
    }

    let mut written_module_traces: Vec<String> = Vec::new();

    for (mod_name, mod_root) in &touched_modules {
        let mut mod_events: Vec<TraceEvent> = Vec::new();
        let mut mod_tasks: Vec<&TraceEvent> = Vec::new();

        for event in &telemetry {
            if event.cat == "task" {
                if let Some(Some((m, _))) = task_to_module.get(&event.name)
                    && m == mod_name
                {
                    mod_events.push(event.clone());
                    mod_tasks.push(event);
                }
            } else if event.cat == "step" {
                let task_opt = event.args.get("task").and_then(|v| v.as_str());
                if let Some(task_name) = task_opt
                    && let Some(Some((m, _))) = task_to_module.get(task_name)
                    && m == mod_name
                {
                    mod_events.push(event.clone());
                }
            } else if event.cat == "needs" {
                if let Some((from_task, to_task)) = event.name.split_once(" \u{2192} ") {
                    let from_in_mod = task_to_module
                        .get(from_task)
                        .and_then(|opt| opt.as_ref())
                        .map(|(m, _)| m == mod_name)
                        .unwrap_or(false);
                    let to_in_mod = task_to_module
                        .get(to_task)
                        .and_then(|opt| opt.as_ref())
                        .map(|(m, _)| m == mod_name)
                        .unwrap_or(false);
                    if from_in_mod || to_in_mod {
                        mod_events.push(event.clone());
                    }
                }
            } else {
                let task_opt = event.args.get("task").and_then(|v| v.as_str());
                if let Some(task_name) = task_opt
                    && let Some(Some((m, _))) = task_to_module.get(task_name)
                    && m == mod_name
                {
                    mod_events.push(event.clone());
                }
            }
        }

        let mod_summary = compute_summary(&mod_tasks);
        let mod_duration_us = if mod_tasks.is_empty() {
            0
        } else {
            let min_ts = mod_tasks.iter().filter_map(|t| t.ts).min().unwrap_or(0);
            let max_end = mod_tasks
                .iter()
                .filter_map(|t| t.ts.map(|s| s + t.dur.unwrap_or(0)))
                .max()
                .unwrap_or(0);
            max_end.saturating_sub(min_ts)
        };
        let mod_outcome = if mod_summary.failed > 0 {
            "failed".to_string()
        } else if mod_summary.cancelled > 0 {
            "cancelled".to_string()
        } else if mod_summary.tasks == 0 {
            "empty".to_string()
        } else {
            "success".to_string()
        };

        let depth = Path::new(mod_root).components().count() + 1;
        let workspace_rel = format!("{}/.evin/trace.json", vec![".."; depth].join("/"));

        let mut final_mod_events = Vec::new();
        let mut mod_meta_args = BTreeMap::new();
        mod_meta_args.insert(
            "name".to_string(),
            serde_json::Value::String(mod_name.clone()),
        );
        final_mod_events.push(TraceEvent {
            name: "process_name".to_string(),
            cat: "__metadata".to_string(),
            ph: "M".to_string(),
            ts: None,
            dur: None,
            pid: mod_tasks.first().map(|t| t.pid).unwrap_or(1),
            tid: 0,
            id: None,
            bp: None,
            s: None,
            args: mod_meta_args,
        });

        let used_tids: HashSet<u64> = mod_tasks.iter().map(|t| t.tid).collect();
        for tid in used_tids {
            let mut thread_args = BTreeMap::new();
            thread_args.insert(
                "name".to_string(),
                serde_json::Value::String(format!("Slot {tid}")),
            );
            final_mod_events.push(TraceEvent {
                name: "thread_name".to_string(),
                cat: "__metadata".to_string(),
                ph: "M".to_string(),
                ts: None,
                dur: None,
                pid: mod_tasks.first().map(|t| t.pid).unwrap_or(1),
                tid,
                id: None,
                bp: None,
                s: None,
                args: thread_args,
            });
        }

        final_mod_events.extend(mod_events);

        let mod_doc = TraceFormat {
            other_data: Some(TraceMetadata {
                schema: "evin.trace/v1".to_string(),
                run_id: run_id.clone(),
                scope: TraceScope::Module {
                    module: mod_name.clone(),
                    root: mod_root.clone(),
                },
                workspace_trace: Some(workspace_rel),
                evin_version: env!("CARGO_PKG_VERSION").to_string(),
                command: command.clone(),
                targets: targets.clone(),
                context: ctx_type.clone(),
                hook: hook.clone(),
                mode: mode_str.clone(),
                watch_session,
                os: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
                started_at: started_at.clone(),
                duration_us: mod_duration_us,
                outcome: mod_outcome,
                summary: mod_summary,
                modules: vec![],
            }),
            trace_events: final_mod_events,
            display_time_unit: "ms".to_string(),
        };

        let mod_trace_rel = format!("{}/.evin/trace.json", mod_root.replace('\\', "/"));
        let mod_trace_path = workspace_root.join(&mod_trace_rel);
        if let Ok(json) = serde_json::to_string_pretty(&mod_doc) {
            let _ = atomic_write_json(&mod_trace_path, &json);
            written_module_traces.push(mod_trace_rel);
        }
    }

    let all_tasks: Vec<&TraceEvent> = telemetry.iter().filter(|t| t.cat == "task").collect();
    let root_summary = compute_summary(&all_tasks);
    let root_duration_us = if duration_us > 0 {
        duration_us
    } else if all_tasks.is_empty() {
        0
    } else {
        let min_ts = all_tasks.iter().filter_map(|t| t.ts).min().unwrap_or(0);
        let max_end = all_tasks
            .iter()
            .filter_map(|t| t.ts.map(|s| s + t.dur.unwrap_or(0)))
            .max()
            .unwrap_or(0);
        max_end.saturating_sub(min_ts)
    };

    let mut root_events = Vec::new();
    let mut ws_meta_args = BTreeMap::new();
    ws_meta_args.insert(
        "name".to_string(),
        serde_json::Value::String("workspace".to_string()),
    );
    root_events.push(TraceEvent {
        name: "process_name".to_string(),
        cat: "__metadata".to_string(),
        ph: "M".to_string(),
        ts: None,
        dur: None,
        pid: 1,
        tid: 0,
        id: None,
        bp: None,
        s: None,
        args: ws_meta_args,
    });

    let all_tids: HashSet<u64> = all_tasks.iter().map(|t| t.tid).collect();
    for tid in all_tids {
        let mut thread_args = BTreeMap::new();
        thread_args.insert(
            "name".to_string(),
            serde_json::Value::String(format!("Slot {tid}")),
        );
        root_events.push(TraceEvent {
            name: "thread_name".to_string(),
            cat: "__metadata".to_string(),
            ph: "M".to_string(),
            ts: None,
            dur: None,
            pid: 1,
            tid,
            id: None,
            bp: None,
            s: None,
            args: thread_args,
        });
    }

    root_events.extend(telemetry);

    let root_doc = TraceFormat {
        other_data: Some(TraceMetadata {
            schema: "evin.trace/v1".to_string(),
            run_id,
            scope: TraceScope::Workspace,
            workspace_trace: None,
            evin_version: env!("CARGO_PKG_VERSION").to_string(),
            command,
            targets,
            context: ctx_type,
            hook,
            mode: mode_str,
            watch_session,
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            started_at,
            duration_us: root_duration_us,
            outcome: outcome_str,
            summary: root_summary,
            modules: written_module_traces,
        }),
        trace_events: root_events,
        display_time_unit: "ms".to_string(),
    };

    let root_trace_path = workspace_root.join(".evin").join("trace.json");
    if let Ok(json) = serde_json::to_string_pretty(&root_doc) {
        let _ = atomic_write_json(&root_trace_path, &json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_trace_generation_and_correlation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();

        let mut args_evin = BTreeMap::new();
        args_evin.insert("status".to_string(), serde_json::json!("success"));
        args_evin.insert(
            "source".to_string(),
            serde_json::json!("crates/evin/.evin/module.flow"),
        );
        args_evin.insert("module".to_string(), serde_json::json!("evin"));
        args_evin.insert("needs".to_string(), serde_json::json!([]));

        let mut args_docs = BTreeMap::new();
        args_docs.insert("status".to_string(), serde_json::json!("success"));
        args_docs.insert(
            "source".to_string(),
            serde_json::json!("docs/.evin/module.flow"),
        );
        args_docs.insert("module".to_string(), serde_json::json!("docs"));
        args_docs.insert("needs".to_string(), serde_json::json!(["evin:build"]));

        let telemetry = vec![
            TraceEvent {
                name: "evin:build".to_string(),
                cat: "task".to_string(),
                ph: "X".to_string(),
                ts: Some(1_000_000),
                dur: Some(100_000),
                pid: 2,
                tid: 1,
                id: None,
                bp: None,
                s: None,
                args: args_evin,
            },
            TraceEvent {
                name: "docs:build".to_string(),
                cat: "task".to_string(),
                ph: "X".to_string(),
                ts: Some(1_100_000),
                dur: Some(200_000),
                pid: 3,
                tid: 2,
                id: None,
                bp: None,
                s: None,
                args: args_docs,
            },
        ];

        write_telemetry_report(telemetry, root.clone(), None);

        // Check workspace root trace
        let root_trace = root.join(".evin").join("trace.json");
        assert!(root_trace.exists());
        let root_content = std::fs::read_to_string(&root_trace).expect("read root trace");
        let root_parsed: TraceFormat =
            serde_json::from_str(&root_content).expect("parse root trace");
        let root_meta = root_parsed.other_data.expect("root other_data");
        assert_eq!(root_meta.scope, TraceScope::Workspace);
        assert_eq!(root_meta.summary.tasks, 2);
        assert!(
            root_meta
                .modules
                .contains(&"crates/evin/.evin/trace.json".to_string())
        );
        assert!(
            root_meta
                .modules
                .contains(&"docs/.evin/trace.json".to_string())
        );

        // Check module trace for evin
        let evin_trace = root.join("crates/evin/.evin/trace.json");
        assert!(evin_trace.exists());
        let evin_content = std::fs::read_to_string(&evin_trace).expect("read evin trace");
        let evin_parsed: TraceFormat =
            serde_json::from_str(&evin_content).expect("parse evin trace");
        let evin_meta = evin_parsed.other_data.expect("evin other_data");
        assert_eq!(
            evin_meta.scope,
            TraceScope::Module {
                module: "evin".to_string(),
                root: "crates/evin".to_string(),
            }
        );
        assert_eq!(evin_meta.run_id, root_meta.run_id);
        assert_eq!(
            evin_meta.workspace_trace.as_deref(),
            Some("../../../.evin/trace.json")
        );
        assert_eq!(evin_meta.summary.tasks, 1);

        // Check module trace for docs
        let docs_trace = root.join("docs/.evin/trace.json");
        assert!(docs_trace.exists());
        let docs_content = std::fs::read_to_string(&docs_trace).expect("read docs trace");
        let docs_parsed: TraceFormat =
            serde_json::from_str(&docs_content).expect("parse docs trace");
        let docs_meta = docs_parsed.other_data.expect("docs other_data");
        assert_eq!(
            docs_meta.scope,
            TraceScope::Module {
                module: "docs".to_string(),
                root: "docs".to_string(),
            }
        );
        assert_eq!(docs_meta.run_id, root_meta.run_id);
        assert_eq!(
            docs_meta.workspace_trace.as_deref(),
            Some("../../.evin/trace.json")
        );
        assert_eq!(docs_meta.summary.tasks, 1);
    }

    #[test]
    fn empty_run_preserves_module_traces() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();

        let evin_trace = root.join("crates/evin/.evin/trace.json");
        std::fs::create_dir_all(evin_trace.parent().unwrap()).unwrap();
        std::fs::write(&evin_trace, "{\"preserved\":true}").unwrap();

        write_telemetry_report(vec![], root.clone(), None);

        // Module trace should be untouched
        let evin_content = std::fs::read_to_string(&evin_trace).unwrap();
        assert_eq!(evin_content, "{\"preserved\":true}");

        // Root trace should reflect empty run
        let root_trace = root.join(".evin").join("trace.json");
        assert!(root_trace.exists());
        let root_content = std::fs::read_to_string(&root_trace).unwrap();
        let root_parsed: TraceFormat = serde_json::from_str(&root_content).unwrap();
        let root_meta = root_parsed.other_data.unwrap();
        assert_eq!(root_meta.outcome, "empty");
        assert_eq!(root_meta.summary.tasks, 0);
        assert!(root_parsed.trace_events.is_empty());
    }
}
