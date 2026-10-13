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

use super::event::UIEvent;
use super::state::{PendingLog, PipelineState};
use crate::render::{
    Layout, Logger, Mode, OutputBackend, SPINNER_FRAMES, SPINNER_TEXT_MIN_LIFETIME_MS, Spinner,
    StderrBackend, UI_OUTPUT_MIN_DELAY_MS, format_duration,
};
use std::collections::VecDeque;
use std::sync::Arc;

pub struct DaemonStatusInfo {
    pub task_name: String,
    pub name: String,
    pub display_name: String,
    pub command: Option<String>,
    pub status_message: String,
    pub started_at: std::time::Instant,
    rendered_lines: usize,
    last_snapshot: Vec<String>,
}

pub struct TerminalPresenter {
    pub spin: Spinner,
    pub logger: Logger,
    pub backend: Arc<dyn OutputBackend>,
    last_spinner_update: Option<std::time::Instant>,
    pub active_daemon_status: Option<DaemonStatusInfo>,
    pub spinner_frame: usize,
}

impl TerminalPresenter {
    pub fn new(backend: Arc<dyn OutputBackend>) -> Self {
        let layout = Layout::current();
        Self {
            spin: Spinner::with_backend(layout, backend.clone()),
            logger: Logger::new(layout, backend.clone()),
            backend,
            last_spinner_update: None,
            active_daemon_status: None,
            spinner_frame: 0,
        }
    }

    pub async fn handle_immediate_events(&mut self, state: &PipelineState, event: &UIEvent) {
        match event {
            UIEvent::PipelineStart { message } => {
                self.spin.start(message);
            }
            UIEvent::ItemStarted { .. } => {
                self.update_spinner_msg(state, true);
                return;
            }
            UIEvent::Progress { message } => {
                self.spin.set_message(message);
            }
            UIEvent::Diagnostic { item_name, .. } => {
                if !state.tracker.is_running(item_name) {
                    self.spin.clear().await;
                }
            }
            UIEvent::PipelineState { success } => {
                if *success {
                    self.spin.stop().await;
                } else {
                    self.spin.clear().await;
                }
            }
            UIEvent::WatchIdle { message } => {
                if !self.spin.is_running() {
                    self.spin.start(message);
                } else {
                    self.spin
                        .set_message_with_timer(message, std::time::Instant::now());
                    self.spin.unclear();
                }
                return;
            }
            _ => {}
        }

        self.update_spinner_msg(state, false);
    }

    pub async fn tick(&mut self, state: &mut PipelineState, flushing: bool) {
        let mut popped = false;
        while let Some(line) = state.pending_output.front() {
            let _ = line;
            self.spin.clear().await;
            self.clear_live_daemon_view().await;
            let line = state.pending_output.pop_front().unwrap();
            popped = true;
            match &line.log {
                PendingLog::Header {
                    version,
                    mode,
                    os,
                    arch,
                    shell,
                } => {
                    self.logger.header(version, mode, os, arch, shell).await;
                }
                PendingLog::WorkspaceHeader { version, workspace } => {
                    self.logger.workspace_header(version, workspace).await;
                }
                PendingLog::Footer { command, flags } => {
                    self.logger.footer(command, flags).await;
                }
                PendingLog::Info(msg) => self.logger.info(msg).await,
                PendingLog::Success(msg) => self.logger.success(msg).await,
                PendingLog::Error(msg) => self.logger.error(msg).await,
                PendingLog::Warn(msg) => self.logger.warn(msg).await,
                PendingLog::Debug(msg) => self.logger.debug(msg).await,
                PendingLog::Fenced(title, lines, is_error, command) => {
                    self.logger
                        .fenced(title, lines, *is_error, command.as_deref())
                        .await
                }
                PendingLog::Intro(msg) => self.logger.intro(msg).await,
                PendingLog::Summary(title, items) => self.logger.summary(title, items).await,
                PendingLog::Step(msg) => self.logger.step(msg).await,
                PendingLog::Remark(msg) => self.logger.remark(msg).await,
                PendingLog::Outro(msg) => self.logger.outro(msg).await,
                PendingLog::OutroCancel(msg) => self.logger.outro_cancel(msg).await,
                PendingLog::Restarting {
                    display_name,
                    duration,
                } => {
                    self.logger
                        .reloading(display_name, &format_duration(duration.as_secs_f64()))
                        .await
                }
                PendingLog::DaemonStarted {
                    task_name,
                    display_name,
                    startup_duration,
                    status_message,
                } => {
                    // Transfer exclusive ownership of the live terminal row
                    // from the ordinary task spinner to the daemon panel.
                    self.spin.stop().await;
                    if self.backend.is_terminal() {
                        let _ = console::Term::stderr().hide_cursor();
                    }
                    self.logger
                        .daemon_started(
                            display_name,
                            &format_duration(startup_duration.as_secs_f64()),
                        )
                        .await;
                    self.active_daemon_status = Some(DaemonStatusInfo {
                        task_name: task_name.clone(),
                        name: task_name.clone(),
                        display_name: display_name.clone(),
                        command: state
                            .tracker
                            .get(task_name)
                            .and_then(|task| task.command.clone()),
                        status_message: status_message.clone(),
                        started_at: std::time::Instant::now(),
                        rendered_lines: 0,
                        last_snapshot: Vec::new(),
                    });
                }
                PendingLog::DaemonStatus {
                    name: _,
                    status_message,
                } => {
                    if let Some(ref mut ds) = self.active_daemon_status {
                        ds.status_message = status_message.clone();
                    }
                }
                PendingLog::DaemonStopped {
                    name,
                    duration,
                    failed,
                } => {
                    self.active_daemon_status = None;
                    if self.backend.is_terminal() {
                        let _ = console::Term::stderr().show_cursor();
                    }
                    self.logger
                        .daemon_stopped(name, &format_duration(duration.as_secs_f64()), *failed)
                        .await;
                }
            }
        }

        if popped && (state.tracker.running_count() > 0 || self.spin.is_running()) {
            self.spin.unclear();
        }

        if self.active_daemon_status.is_some() {
            self.render_live_daemon_panel(state).await;
        } else if !flushing {
            self.update_spinner_msg(state, false);
        } else {
            self.spin.clear().await;
        }
    }

    /// The daemon view consists of a fixed-height fence followed by one
    /// transient status line. Clear the status first, then walk back over the
    /// fence so the next render starts at its top border.
    async fn clear_live_daemon_view(&mut self) {
        let Some(panel) = self.active_daemon_status.as_mut() else {
            return;
        };
        if self.backend.is_terminal() {
            self.backend.clear_line().await;
            if panel.rendered_lines > 0 {
                self.backend
                    .clear_previous_lines(panel.rendered_lines)
                    .await;
            }
        }
        panel.rendered_lines = 0;
        panel.last_snapshot.clear();
    }

    async fn render_live_daemon_panel(&mut self, state: &PipelineState) {
        let Some(panel) = self.active_daemon_status.as_mut() else {
            return;
        };
        let Some(task) = state.tracker.get(&panel.task_name) else {
            return;
        };

        let start = task.daemon_output.len().saturating_sub(8);
        let lines = task.daemon_output[start..].to_vec();
        let snapshot = lines.clone();

        if !self.backend.is_terminal() {
            return;
        }
        if panel.last_snapshot != snapshot {
            self.backend.clear_line().await;
            if panel.rendered_lines > 0 {
                self.backend
                    .clear_previous_lines(panel.rendered_lines)
                    .await;
            }
            let rendered = self.logger.fenced_lines(
                &format!("(task) ↩ {}", panel.name),
                &lines,
                task.daemon_had_error,
                panel.command.as_deref(),
            );
            for line in &rendered {
                self.backend.write_line(line).await;
            }
            panel.rendered_lines = rendered.len();
            panel.last_snapshot = snapshot;
        } else {
            self.backend.clear_line().await;
        }
        let elapsed = format_duration(panel.started_at.elapsed().as_secs_f64());
        let frame_symbol = SPINNER_FRAMES[self.spinner_frame % SPINNER_FRAMES.len()];
        self.spinner_frame = (self.spinner_frame + 1) % SPINNER_FRAMES.len();
        self.logger
            .daemon_status_line(frame_symbol, &panel.display_name, &elapsed)
            .await;
        self.backend.flush().await;
    }

    fn update_spinner_msg(&mut self, state: &PipelineState, force: bool) {
        if state.finished {
            return;
        }

        let now = std::time::Instant::now();
        if !force
            && let Some(last_update) = self.last_spinner_update
            && (now.duration_since(last_update).as_millis() as u64) < SPINNER_TEXT_MIN_LIFETIME_MS
        {
            return;
        }

        let count = state.tracker.running_count();
        if count > 0
            && let Some(latest_task) = state.tracker.get_latest_running_task()
        {
            let msg = latest_task.display_name.clone();

            self.spin
                .set_message_with_timer(&msg, latest_task.start_time);
            self.last_spinner_update = Some(now);
        }
    }
}

pub fn build_pipeline_summary_items(
    mode: Mode,
    is_hook: bool,
    targets: &[String],
    elapsed: f64,
    peak_memory_kb: Option<f64>,
) -> Vec<String> {
    let mut items = Vec::new();
    let dim = console::Style::new().color256(244);
    let bold = console::Style::new().bold();

    if mode == Mode::DryRun {
        // Already printed by UI logic elsewhere for dry-run
    } else if is_hook && targets.is_empty() {
        let time = format_duration(elapsed);
        items.push(format!(
            "{}: {}",
            dim.apply_to("Action"),
            bold.apply_to("Policy verification")
        ));
        items.push(format!(
            "{}: {}",
            dim.apply_to("Duration"),
            bold.apply_to(time)
        ));
    } else {
        let t_label = if targets.len() > 1 {
            "Targets"
        } else {
            "Target"
        };
        let time = format_duration(elapsed);
        items.push(format!(
            "{}: {}",
            dim.apply_to(t_label),
            bold.apply_to(targets.join(", "))
        ));
        items.push(format!(
            "{}: {}",
            dim.apply_to("Duration"),
            bold.apply_to(time)
        ));
    }

    if let Some(mem_kb) = peak_memory_kb
        && matches!(mode, Mode::Debug)
    {
        let mem_val = format!("{:.2} MB", mem_kb / 1024.0);
        items.push(format!(
            "{}: {}",
            dim.apply_to("Peak Memory"),
            bold.apply_to(mem_val)
        ));
    }

    items
}

pub struct PipelineUI {
    state: PipelineState,
    presenter: TerminalPresenter,
    event_queue: VecDeque<UIEvent>,
    last_event_tick: Option<std::time::Instant>,
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::pipeline::event::PipelineLogLevel;
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Debug)]
    struct RecordingBackend {
        terminal: bool,
        lines: Mutex<Vec<String>>,
        cleared_previous: Mutex<usize>,
    }

    impl RecordingBackend {
        fn new(terminal: bool) -> Self {
            Self {
                terminal,
                lines: Mutex::new(Vec::new()),
                cleared_previous: Mutex::new(0),
            }
        }
    }

    #[async_trait]
    impl OutputBackend for RecordingBackend {
        async fn write_line(&self, text: &str) {
            self.lines.lock().unwrap().push(text.to_string());
        }

        async fn write_str(&self, text: &str) {
            self.lines.lock().unwrap().push(text.to_string());
        }

        async fn clear_line(&self) {}

        async fn clear_previous_lines(&self, lines: usize) {
            *self.cleared_previous.lock().unwrap() += lines;
        }

        fn is_terminal(&self) -> bool {
            self.terminal
        }

        async fn flush(&self) {}
    }

    async fn emit(ui: &mut PipelineUI, event: UIEvent) {
        ui.handle_event(event).await;
        ui.tick(true).await;
    }

    #[tokio::test]
    async fn non_terminal_daemon_emits_one_final_fence_without_redraw() {
        let backend = Arc::new(RecordingBackend::new(false));
        let mut ui = PipelineUI::with_backend(Mode::Normal, false, false, true, backend.clone());
        emit(
            &mut ui,
            UIEvent::ItemStarted {
                name: "docs:serve".to_string(),
                description: None,
                command: Some("evin/docs@v1".to_string()),
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::TaskDaemonStarted {
                name: "docs:serve".to_string(),
                startup_duration: std::time::Duration::from_millis(1),
                status_message: "Watching documentation changes".to_string(),
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::Log {
                name: "docs:serve".to_string(),
                level: PipelineLogLevel::Info,
                message: "Scanning documentation files...".to_string(),
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::TaskDaemonStopped {
                name: "docs:serve".to_string(),
                duration: std::time::Duration::from_secs(1),
                failed: false,
            },
        )
        .await;

        let lines = backend.lines.lock().unwrap();
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.contains("╭─(task) ↩ docs:serve"))
                .count(),
            1
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Scanning documentation files"))
        );
        assert_eq!(*backend.cleared_previous.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn terminal_daemon_redraws_its_live_fence_below_an_animated_lifecycle_line() {
        let backend = Arc::new(RecordingBackend::new(true));
        let mut ui = PipelineUI::with_backend(Mode::Normal, false, false, true, backend.clone());
        emit(
            &mut ui,
            UIEvent::ItemStarted {
                name: "docs:serve".to_string(),
                description: Some("Serve live-reloading documentation dev server".to_string()),
                command: Some("evin/docs@v1".to_string()),
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::TaskDaemonStarted {
                name: "docs:serve".to_string(),
                startup_duration: std::time::Duration::from_millis(1),
                status_message: "Watching documentation changes".to_string(),
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::Log {
                name: "docs:serve".to_string(),
                level: PipelineLogLevel::Info,
                message: "Scanning documentation files...".to_string(),
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::TaskDaemonStatus {
                name: "docs:serve".to_string(),
                status_message: "Rebuilding documentation".to_string(),
            },
        )
        .await;

        assert!(*backend.cleared_previous.lock().unwrap() > 0);
        let lines = backend.lines.lock().unwrap();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("│") && line.contains("Scanning documentation files"))
        );
        assert!(lines.iter().any(|line| {
            let plain = console::strip_ansi_codes(line);
            plain.contains("daemon - Serve live-reloading documentation dev server")
                && !plain.contains("│")
        }));
    }

    #[tokio::test]
    async fn daemon_fence_uses_the_builtin_action_label() {
        let backend = Arc::new(RecordingBackend::new(false));
        let mut ui = PipelineUI::with_backend(Mode::Normal, false, false, true, backend.clone());
        emit(
            &mut ui,
            UIEvent::ItemStarted {
                name: "docs:serve".to_string(),
                description: None,
                command: Some("evin/docs@v1".to_string()),
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::TaskDaemonStarted {
                name: "docs:serve".to_string(),
                startup_duration: std::time::Duration::from_millis(1),
                status_message: "Watching documentation changes".to_string(),
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::Log {
                name: "docs:serve".to_string(),
                level: PipelineLogLevel::Info,
                message: "Documentation site generated".to_string(),
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::TaskDaemonStopped {
                name: "docs:serve".to_string(),
                duration: std::time::Duration::from_secs(1),
                failed: false,
            },
        )
        .await;

        assert!(
            backend
                .lines
                .lock()
                .unwrap()
                .iter()
                .any(|line| line.contains("╰─(action) ↩ evin/docs@v1"))
        );
    }

    #[tokio::test]
    async fn restart_freezes_daemon_watch_events_and_preserves_history() {
        let backend = Arc::new(RecordingBackend::new(false));
        let mut ui = PipelineUI::with_backend(Mode::Normal, false, false, true, backend.clone());
        emit(
            &mut ui,
            UIEvent::ItemStarted {
                name: "docs:serve".to_string(),
                description: Some("Serve documentation".to_string()),
                command: Some("evin/docs@v1".to_string()),
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::TaskDaemonStarted {
                name: "docs:serve".to_string(),
                startup_duration: std::time::Duration::from_millis(1),
                status_message: "Watching".to_string(),
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::WatchEvent {
                line: "14:34:11  change   crates/evin/src/main.rs".to_string(),
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::WatchEvent {
                line: "14:34:11  restart  source change detected".to_string(),
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::TaskDaemonStopped {
                name: "docs:serve".to_string(),
                duration: std::time::Duration::from_secs(1),
                failed: false,
            },
        )
        .await;

        ui.reset_session().await;
        emit(
            &mut ui,
            UIEvent::Restarting {
                display_name: "Serve documentation".to_string(),
                duration: std::time::Duration::from_millis(1200),
            },
        )
        .await;

        let lines = backend.lines.lock().unwrap();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("change   crates/evin/src/main.rs"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("restart  source change detected"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("reload - Serve documentation") && line.contains("1.2s"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("╰─(action) ↩ evin/docs@v1"))
        );
    }

    #[tokio::test]
    async fn structural_debug_bursts_do_not_delay_lifecycle_completion() {
        let backend = Arc::new(RecordingBackend::new(false));
        let mut ui = PipelineUI::with_backend(Mode::Debug, false, true, false, backend.clone());
        emit(
            &mut ui,
            UIEvent::ItemStarted {
                name: "Resolving configuration".to_string(),
                description: None,
                command: None,
            },
        )
        .await;

        // Reproduce an active output throttle window. Structural telemetry
        // and the completion must still drain together in the next UI tick.
        ui.last_event_tick = Some(std::time::Instant::now());
        for index in 0..16 {
            ui.handle_event(UIEvent::StructuralDebug {
                name: "Resolving configuration".to_string(),
                message: format!("[E_1{index:02}] count={index}"),
                snapshot_key: None,
            })
            .await;
        }
        ui.handle_event(UIEvent::ItemFinished {
            name: "Resolving configuration".to_string(),
            failed: false,
            unverified: false,
            cancelled: false,
            duration: std::time::Duration::from_millis(12),
        })
        .await;

        ui.tick(false).await;

        assert!(ui.event_queue.is_empty());
        assert!(!ui.state.tracker.is_running("Resolving configuration"));
        let lines = backend.lines.lock().unwrap();
        assert!(
            lines.iter().any(|line| {
                line.contains("Resolving configuration") && line.contains("12.0ms")
            })
        );
    }

    #[tokio::test]
    async fn phase_failure_diagnostic_is_fenced_under_its_phase() {
        let backend = Arc::new(RecordingBackend::new(false));
        let mut ui = PipelineUI::with_backend(Mode::Normal, false, false, false, backend.clone());
        emit(
            &mut ui,
            UIEvent::Intro {
                title: "Workspace › Catalog".into(),
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::ItemStarted {
                name: "Resolving configuration".into(),
                description: None,
                command: None,
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::PhaseDiagnostic {
                phase_name: "Resolving configuration".into(),
                diagnostic: evin_diagnostic::Diagnostic::new(
                    "Parse Error",
                    evin_diagnostic::IssueCode::custom("issue-yaml-parse-failed"),
                ),
                message: "Parse Error".into(),
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::ItemFinished {
                name: "Resolving configuration".into(),
                failed: true,
                unverified: false,
                cancelled: false,
                duration: std::time::Duration::from_millis(2),
            },
        )
        .await;
        emit(&mut ui, UIEvent::PipelineState { success: false }).await;
        emit(
            &mut ui,
            UIEvent::OutroCancel {
                message: "Execution failed".into(),
            },
        )
        .await;

        let lines = backend.lines.lock().unwrap();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Workspace › Catalog"))
        );
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.contains("╭─(task) ↩ Resolving configuration"))
                .count(),
            1
        );
        assert!(lines.iter().any(|line| line.contains("Parse Error")));
        assert!(!lines.iter().any(|line| line.contains("╭─(error) ↩")));
        assert!(lines.iter().any(|line| line.contains("Execution failed")));
    }

    #[tokio::test]
    async fn watch_abort_renders_error_step_and_preserves_gutter_to_reload() {
        let backend = Arc::new(RecordingBackend::new(false));
        let mut ui = PipelineUI::with_backend(Mode::Normal, false, false, true, backend.clone());

        emit(
            &mut ui,
            UIEvent::Error {
                message: "Execution failed; watching for the next change".to_string(),
            },
        )
        .await;
        emit(&mut ui, UIEvent::PipelineState { success: false }).await;
        emit(
            &mut ui,
            UIEvent::WatchIdle {
                message: "watching for file changes...".to_string(),
            },
        )
        .await;

        // Raw watch chatter outside active daemon must be suppressed
        emit(
            &mut ui,
            UIEvent::WatchEvent {
                line: "19:39:37  change   crates/esh/src/runtime/builtins/flow.rs".to_string(),
            },
        )
        .await;
        emit(
            &mut ui,
            UIEvent::WatchEvent {
                line: "19:39:37  restart  source change detected".to_string(),
            },
        )
        .await;

        ui.reset_session().await;
        emit(
            &mut ui,
            UIEvent::Restarting {
                display_name: "Serve live-reloading documentation dev server".to_string(),
                duration: std::time::Duration::from_millis(5),
            },
        )
        .await;

        let lines = backend.lines.lock().unwrap();
        // Error line must be present with error symbol and followed by a connecting gutter bar
        assert!(
            lines
                .iter()
                .any(|line| { line.contains("Execution failed; watching for the next change") })
        );
        // Outro symbol '╰' must NOT be used for intermediate watch failure
        assert!(
            !lines
                .iter()
                .any(|line| { line.contains("╰") && line.contains("Execution aborted") })
        );
        // Gutter spine '│' must connect to the reload line
        assert!(lines.iter().any(|line| line == "│"));
        // Raw watch events must be suppressed
        assert!(!lines.iter().any(|line| line.contains("19:39:37  change")));
        assert!(!lines.iter().any(|line| line.contains("19:39:37  restart")));
        // Reload step must render
        assert!(lines.iter().any(|line| {
            line.contains("reload - Serve live-reloading documentation dev server")
        }));
    }

    #[tokio::test]
    async fn supervisor_cancellation_closes_an_open_failed_watch_session_once() {
        let backend = Arc::new(RecordingBackend::new(false));
        let mut ui = PipelineUI::with_backend(Mode::Normal, false, false, true, backend.clone());

        emit(
            &mut ui,
            UIEvent::Error {
                message: "Execution failed; watching for the next change".to_string(),
            },
        )
        .await;
        emit(&mut ui, UIEvent::PipelineState { success: false }).await;
        emit(
            &mut ui,
            UIEvent::WatchIdle {
                message: "watching for file changes...".to_string(),
            },
        )
        .await;

        ui.finalize_cancelled("Execution aborted".to_string()).await;

        let lines = backend.lines.lock().unwrap();
        let closing = lines
            .iter()
            .filter(|line| {
                let plain = console::strip_ansi_codes(line);
                plain.contains('╰') && plain.contains("Execution aborted")
            })
            .count();
        assert_eq!(closing, 1);
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.contains("Execution failed; watching for the next change"))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn cancellation_finalization_flushes_queued_task_starts() {
        let backend = Arc::new(RecordingBackend::new(false));
        let mut ui = PipelineUI::with_backend(Mode::Normal, false, false, true, backend);

        ui.handle_event(UIEvent::ItemStarted {
            name: "test".to_string(),
            description: None,
            command: Some("cargo test".to_string()),
        })
        .await;

        ui.finalize_cancelled("Execution aborted".to_string()).await;

        assert!(ui.is_idle());
        assert_eq!(ui.state.tracker.running_count(), 0);
    }
}

impl PipelineUI {
    pub fn new(mode: Mode, silent_success: bool, debug_enabled: bool, verbose: bool) -> Self {
        Self::with_backend(
            mode,
            silent_success,
            debug_enabled,
            verbose,
            Arc::new(StderrBackend),
        )
    }

    pub fn with_backend(
        mode: Mode,
        silent_success: bool,
        debug_enabled: bool,
        verbose: bool,
        backend: Arc<dyn OutputBackend>,
    ) -> Self {
        Self {
            state: PipelineState::new(mode, silent_success, debug_enabled, verbose),
            presenter: TerminalPresenter::new(backend),
            event_queue: VecDeque::new(),
            last_event_tick: None,
        }
    }

    pub fn is_idle(&self) -> bool {
        self.event_queue.is_empty()
            && self.state.pending_output.is_empty()
            && (self.state.finished || self.state.tracker.running_count() == 0)
    }

    pub async fn reset_session(&mut self) {
        // A cleared spinner is still an active animation task. Stop it before
        // the next watch session starts so redraw loops cannot accumulate.
        self.presenter.spin.stop().await;
        self.presenter.clear_live_daemon_view().await;
        self.presenter.active_daemon_status = None;
        self.state = PipelineState::new(
            self.state.mode,
            self.state.silent_success,
            self.state.debug_enabled,
            self.state.verbose,
        );
        self.event_queue.clear();
        self.last_event_tick = None;
    }

    /// Finalize the complete invocation after supervisor cancellation. This is
    /// deliberately distinct from an intermediate failed watch session, which
    /// keeps its gutter open while waiting for another change.
    pub async fn finalize_cancelled(&mut self, message: String) {
        // A cancellation may arrive before the next UI tick has applied a
        // queued ItemStarted event. Apply queued lifecycle events first so a
        // stale start cannot reopen a task after PipelineState clears it.
        self.tick(true).await;

        let daemons: Vec<_> = self.state.daemons.iter().cloned().collect();
        for name in daemons {
            let duration = self
                .state
                .tracker
                .get(&name)
                .map(|task| task.start_time.elapsed())
                .unwrap_or_default();
            self.state.ingest(UIEvent::TaskDaemonStopped {
                name,
                duration,
                failed: false,
            });
        }
        self.state.ingest(UIEvent::PipelineState { success: false });
        self.presenter.tick(&mut self.state, true).await;
        self.presenter.spin.stop().await;
        self.presenter.clear_live_daemon_view().await;
        self.state.ingest(UIEvent::OutroCancel { message });
        self.presenter.tick(&mut self.state, true).await;
    }

    pub async fn handle_event(&mut self, event: UIEvent) -> bool {
        let is_running = match &event {
            UIEvent::Diagnostic { item_name, .. } => self.state.tracker.is_running(item_name),
            UIEvent::PhaseDiagnostic { phase_name, .. } => {
                self.state.tracker.is_running(phase_name)
            }
            _ => true,
        };

        self.event_queue.push_back(event);
        !is_running
    }

    pub async fn tick(&mut self, flushing: bool) {
        let now = std::time::Instant::now();

        if flushing {
            while let Some(event) = self.event_queue.pop_front() {
                self.state.ingest(event.clone());
                self.presenter
                    .handle_immediate_events(&self.state, &event)
                    .await;
            }
        } else {
            while let Some(event) = self.event_queue.front() {
                let is_fast = matches!(
                    event,
                    UIEvent::PipelineStart { .. }
                        | UIEvent::PipelineState { .. }
                        | UIEvent::ItemStarted { .. }
                        | UIEvent::ItemFinished { .. }
                        | UIEvent::Intro { .. }
                        | UIEvent::Log { .. }
                        | UIEvent::Activity { .. }
                        | UIEvent::RawOutput { .. }
                        // A task can publish many structural records at once.
                        // They aggregate into one final block and must never
                        // hold a lifecycle completion behind output pacing.
                        | UIEvent::StructuralDebug { .. }
                        | UIEvent::Progress { .. }
                        | UIEvent::TaskDaemonStarted { .. }
                        | UIEvent::TaskDaemonStatus { .. }
                        | UIEvent::TaskDaemonStopped { .. }
                        | UIEvent::Error { .. }
                        | UIEvent::WatchIdle { .. }
                );

                let can_process = if let Some(last) = self.last_event_tick {
                    is_fast || now.duration_since(last).as_millis() as u64 >= UI_OUTPUT_MIN_DELAY_MS
                } else {
                    true
                };

                if can_process {
                    let event = self.event_queue.pop_front().unwrap();
                    self.state.ingest(event.clone());
                    self.presenter
                        .handle_immediate_events(&self.state, &event)
                        .await;
                    if !is_fast {
                        self.last_event_tick = Some(now);
                    }
                } else {
                    break;
                }
            }
        }

        self.presenter.tick(&mut self.state, flushing).await;
    }
}
