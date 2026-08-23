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

use crate::events::{PipelineEvent, PipelineSummaryOutcome};
use async_trait::async_trait;
use evin_action_sdk::debug::StructuredDebugEvent;
use evin_config::EverInitConfig;
use evin_diagnostic::Diagnostic;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct PipelineEmitter {
    sender: broadcast::Sender<PipelineEvent>,
}

impl PipelineEmitter {
    pub fn new(sender: broadcast::Sender<PipelineEvent>) -> Self {
        Self { sender }
    }

    pub fn emit(&self, event: PipelineEvent) {
        let _ = self.sender.send(event); // Ignore receiver dropped error
    }
}

pub struct CliContext {
    pub config: Option<EverInitConfig>,
    pub mode: evin_action_sdk::PipelineMode,
    pub targets: Vec<String>,
    pub run_payload: Vec<String>,
    pub global_env: Option<std::collections::HashMap<String, String>>,
    pub env_schema: Option<std::sync::Arc<evin_config::environment::EnvironmentYaml>>,
    pub is_hook: bool,
    pub hook_summaries: Vec<crate::sync::hooks::HookSummary>,
    pub hook_plans: Vec<evin_git::manager::HookPlan>,
    pub synced_actions: Vec<evin_action_sdk::SyncResource>,
    pub skip_default_summary: bool,
    pub sync_summary_details: Option<Vec<String>>,
    pub verbose: bool,
    pub dag: Option<evin_pipeline::dag::DagResolver>,
    pub forced_execution: Option<evin_config::workspace::ExecutionMode>,
    pub cancel_token: tokio_util::sync::CancellationToken,
    pub watch_session: bool,
    /// Set by the supervisor only when cancellation terminates the overall
    /// invocation. Source-triggered restarts deliberately leave this false.
    pub summarize_cancelled_session: Arc<AtomicBool>,
}

impl Default for CliContext {
    fn default() -> Self {
        Self {
            config: None,
            mode: evin_action_sdk::PipelineMode::Normal,
            targets: vec![],
            run_payload: vec![],
            global_env: None,
            env_schema: None,
            is_hook: false,
            hook_summaries: vec![],
            hook_plans: vec![],
            synced_actions: vec![],
            skip_default_summary: false,
            sync_summary_details: None,
            verbose: false,
            dag: None,
            forced_execution: None,
            cancel_token: tokio_util::sync::CancellationToken::new(),
            watch_session: false,
            summarize_cancelled_session: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[async_trait]
pub trait Phase: Send + Sync {
    /// Returns the raw locale key representing this phase
    fn locale_key(&self) -> String;
    async fn execute(
        &self,
        ctx: &mut CliContext,
        emitter: &PipelineEmitter,
    ) -> Result<(), Diagnostic>;
}

pub struct PipelineEngine {
    emitter: PipelineEmitter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineCompletion {
    Success,
    Failed,
    Cancelled,
}

impl PipelineEngine {
    pub fn new(sender: broadcast::Sender<PipelineEvent>) -> Self {
        Self {
            emitter: PipelineEmitter::new(sender),
        }
    }

    pub async fn execute_pipeline(
        self,
        phases: Vec<Box<dyn Phase>>,
        mut ctx: CliContext,
    ) -> PipelineCompletion {
        let pipeline_start = Instant::now();
        crate::ALLOCATOR.reset_peak_usage();

        for phase in phases {
            let locale_key = phase.locale_key();
            self.emitter
                .emit(PipelineEvent::PhaseStarted(locale_key.clone()));

            let start_mem = crate::ALLOCATOR.current_usage();
            let start = Instant::now();
            let result = phase.execute(&mut ctx, &self.emitter).await;
            let duration = start.elapsed();
            let end_mem = crate::ALLOCATOR.current_usage();

            let phase_name = locale_key.trim_start_matches("ui.phase.").replace('_', "-");
            let heap_alloc_kb = (end_mem as f64 / 1024.0).round() as u64;
            let task_delta_kb =
                ((end_mem as i64 - start_mem as i64) as f64 / 1024.0).round() as i64;

            self.emitter.emit(PipelineEvent::DebugLog(
                None,
                evin_action_sdk::debug::MemorySnapshot {
                    phase: phase_name,
                    heap_alloc_kb,
                    task_delta_kb,
                }
                .into_record(),
            ));

            if let Err(diag) = result {
                self.emitter
                    .emit(PipelineEvent::PhaseFailed(locale_key, duration));
                self.emitter.emit(PipelineEvent::ExecutionError(diag));
                if ctx.watch_session {
                    self.emitter.emit(PipelineEvent::WatchSessionFailed);
                } else {
                    self.emitter.emit(PipelineEvent::PipelineCancelled);
                }
                return PipelineCompletion::Failed;
            }

            self.emitter
                .emit(PipelineEvent::PhaseCompleted(locale_key, duration));

            if ctx.cancel_token.is_cancelled() {
                if ctx.watch_session {
                    if ctx
                        .summarize_cancelled_session
                        .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        self.emit_summary(
                            &mut ctx,
                            pipeline_start.elapsed(),
                            PipelineSummaryOutcome::Cancelled,
                        );
                    }
                    self.emitter.emit(PipelineEvent::PipelineSessionInterrupted);
                } else {
                    self.emitter.emit(PipelineEvent::PipelineCancelled);
                }
                return PipelineCompletion::Cancelled;
            }
        }

        self.emit_summary(
            &mut ctx,
            pipeline_start.elapsed(),
            PipelineSummaryOutcome::Success,
        );
        PipelineCompletion::Success
    }

    fn emit_summary(
        &self,
        ctx: &mut CliContext,
        total_duration: std::time::Duration,
        outcome: PipelineSummaryOutcome,
    ) {
        let peak_memory_kb = crate::ALLOCATOR.peak_usage() as f64 / 1024.0;
        let context = if ctx.is_hook {
            evin_pipeline::PipelineContext::Hook
        } else if !ctx.targets.is_empty() {
            if let Some(first_target) = ctx.targets.first() {
                if first_target
                    .chars()
                    .next()
                    .unwrap_or('a')
                    .is_ascii_uppercase()
                {
                    evin_pipeline::PipelineContext::Stage
                } else {
                    evin_pipeline::PipelineContext::Task
                }
            } else {
                evin_pipeline::PipelineContext::Task
            }
        } else {
            evin_pipeline::PipelineContext::Workspace
        };

        if let Some(details) = ctx.sync_summary_details.take() {
            self.emitter.emit(PipelineEvent::SyncSummary { details });
        } else if !ctx.skip_default_summary {
            self.emitter.emit(PipelineEvent::PipelineSummary {
                total_duration,
                peak_memory_kb,
                context,
                targets: ctx.targets.clone(),
                outcome,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CancelPhase;

    #[async_trait]
    impl Phase for CancelPhase {
        fn locale_key(&self) -> String {
            "ui.phase.executing_tasks".to_string()
        }

        async fn execute(
            &self,
            ctx: &mut CliContext,
            _emitter: &PipelineEmitter,
        ) -> Result<(), Diagnostic> {
            ctx.cancel_token.cancel();
            Ok(())
        }
    }

    #[tokio::test]
    async fn cancelled_watch_session_emits_summary_before_interrupting() {
        let (sender, mut receiver) = broadcast::channel(16);
        let engine = PipelineEngine::new(sender);
        let ctx = CliContext {
            targets: vec!["docs:serve".to_string()],
            watch_session: true,
            summarize_cancelled_session: Arc::new(AtomicBool::new(true)),
            ..Default::default()
        };

        let completion = engine
            .execute_pipeline(vec![Box::new(CancelPhase)], ctx)
            .await;
        assert_eq!(completion, PipelineCompletion::Cancelled);

        let events: Vec<_> = std::iter::from_fn(|| receiver.try_recv().ok()).collect();
        let summary = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    PipelineEvent::PipelineSummary {
                        outcome: PipelineSummaryOutcome::Cancelled,
                        ..
                    }
                )
            })
            .expect("watch cancellation should emit a summary");
        let interrupted = events
            .iter()
            .position(|event| matches!(event, PipelineEvent::PipelineSessionInterrupted))
            .expect("watch cancellation should emit an interruption marker");
        assert!(summary < interrupted);
    }

    #[tokio::test]
    async fn restarted_watch_session_skips_completion_summary() {
        let (sender, mut receiver) = broadcast::channel(16);
        let engine = PipelineEngine::new(sender);
        let ctx = CliContext {
            targets: vec!["docs:serve".to_string()],
            watch_session: true,
            ..Default::default()
        };

        let completion = engine
            .execute_pipeline(vec![Box::new(CancelPhase)], ctx)
            .await;
        assert_eq!(completion, PipelineCompletion::Cancelled);

        let events: Vec<_> = std::iter::from_fn(|| receiver.try_recv().ok()).collect();
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, PipelineEvent::PipelineSummary { .. }))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, PipelineEvent::PipelineSessionInterrupted))
        );
    }
}
