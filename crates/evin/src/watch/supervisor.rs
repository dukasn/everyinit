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

use super::polling::PollingWatcher;
use super::shutdown::{
    CleanupOutcome, RestartCleanup, SignalCoordinator, finish_restart_session, finish_session,
};
use super::spec::{WatchSpec, resolve_watch_spec};
use crate::engine::{CliContext, PipelineCompletion, PipelineEngine};
use crate::events::PipelineEvent;
use chrono::Local;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct RunSession {
    pub target: String,
    pub args: Vec<String>,
    pub mode: evin_action_sdk::PipelineMode,
    pub verbose: bool,
    pub debug: bool,
    pub forced_execution: Option<evin_config::workspace::ExecutionMode>,
}

impl RunSession {
    fn context(
        &self,
        cancel_token: CancellationToken,
        watch_session: bool,
        summarize_cancelled_session: Arc<AtomicBool>,
    ) -> CliContext {
        CliContext {
            mode: self.mode,
            targets: vec![self.target.clone()],
            run_payload: self.args.clone(),
            forced_execution: self.forced_execution,
            verbose: self.verbose,
            debug: self.debug,
            cancel_token,
            watch_session,
            summarize_cancelled_session,
            ..Default::default()
        }
    }
}

/// Runs complete, freshly prepared pipeline sessions until a termination
/// signal. The reporter is intentionally owned by the caller and therefore
/// survives every session boundary.
pub async fn supervise(
    sender: mpsc::UnboundedSender<PipelineEvent>,
    request: RunSession,
    initial_spec: WatchSpec,
) -> bool {
    let mut watcher = PollingWatcher::new(initial_spec);
    let mut signals = SignalCoordinator::install();

    loop {
        let cancel_token = CancellationToken::new();
        let summarize_cancelled_session = Arc::new(AtomicBool::new(false));
        let context = request.context(
            cancel_token.clone(),
            true,
            summarize_cancelled_session.clone(),
        );
        let engine = PipelineEngine::new(sender.clone());
        let mut session =
            tokio::spawn(engine.execute_pipeline(crate::commands::run_phases(), context));

        let change = tokio::select! {
            result = &mut session => {
                let _completion = result.unwrap_or(PipelineCompletion::Failed);
                tokio::select! {
                    change = watcher.next_change() => change,
                    _ = signals.next() => {
                        let _ = sender.send(PipelineEvent::SupervisorTerminated { forced: false });
                        return false;
                    }
                }
            }
            change = watcher.next_change() => change,
            _ = signals.next() => {
                summarize_cancelled_session.store(true, Ordering::Relaxed);
                cancel_token.cancel();
                let cleanup = finish_session(&mut session, &mut signals).await;
                let _ = sender.send(PipelineEvent::SupervisorTerminated {
                    forced: cleanup != CleanupOutcome::Graceful,
                });
                return false;
            }
        };

        let reload_started = std::time::Instant::now();
        let timestamp = Local::now().format("%H:%M:%S");
        let _ = sender.send(PipelineEvent::WatchEvent {
            line: format!("{timestamp}  change   {}", change.summary()),
        });
        let _ = sender.send(PipelineEvent::WatchEvent {
            line: format!("{timestamp}  restart  source change detected"),
        });

        if !session.is_finished() {
            cancel_token.cancel();
            if let RestartCleanup::Terminate { forced } =
                finish_restart_session(&mut session, &summarize_cancelled_session, &mut signals)
                    .await
            {
                let _ = sender.send(PipelineEvent::SupervisorTerminated { forced });
                return false;
            }
        }

        // Re-resolve from disk. Invalid configuration is rendered by the next
        // session; the last valid matcher remains installed so fixing that file
        // can always trigger recovery.
        watcher.apply_resolved_spec(resolve_watch_spec(&request.target).await);
        let _ = sender.send(PipelineEvent::Restarting {
            display_name: watcher.spec.display_name.clone(),
            duration: reload_started.elapsed(),
        });
    }
}

pub async fn run_once_with_signals(
    sender: mpsc::UnboundedSender<PipelineEvent>,
    request: RunSession,
) -> PipelineCompletion {
    let mut signals = SignalCoordinator::install();
    let cancel_token = CancellationToken::new();
    let context = request.context(
        cancel_token.clone(),
        false,
        Arc::new(AtomicBool::new(false)),
    );
    let engine = PipelineEngine::new(sender.clone());
    let mut session = tokio::spawn(engine.execute_pipeline(crate::commands::run_phases(), context));
    tokio::select! {
        result = &mut session => result.unwrap_or(PipelineCompletion::Failed),
        _ = signals.next() => {
            cancel_token.cancel();
            finish_session(&mut session, &mut signals).await;
            // If forced cleanup aborted the engine before it could publish its
            // terminal event, this guarantees the one-shot reporter can exit.
            let _ = sender.send(PipelineEvent::PipelineCancelled);
            PipelineCompletion::Cancelled
        }
    }
}
