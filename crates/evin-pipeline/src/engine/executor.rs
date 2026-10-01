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

use crate::dag::DagResolver;
use crate::env::build_task_env;
use crate::runner::{LogMessage, ShellRunner};
use crate::telemetry::TraceEvent;
use evin_action_sdk::TaskOutcome;
use evin_config::models::TaskObject;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use super::core::PipelineEngine;
use super::types::{EngineContext, PipelineReport, TaskResult};

impl PipelineEngine {
    #[allow(clippy::result_large_err)]
    pub fn validate(&self, resolver: &mut DagResolver) -> Result<(), evin_diagnostic::Diagnostic> {
        // Build graph state (trigger validation and in_degree generation)
        if let Err(e) = resolver.build_graph_public() {
            return Err(evin_locales::diagnostic(e.into()));
        }
        crate::preflight::validate_task_contracts(resolver.registry().values(), &self.state.actions)
            .into_iter()
            .next()
            .map_or(Ok(()), |finding| Err(finding.diagnostic))
    }

    pub async fn execute(
        &self,
        resolver: DagResolver,
        log_tx: mpsc::UnboundedSender<LogMessage>,
    ) -> PipelineReport {
        self.execute_with_cancel(resolver, log_tx, tokio_util::sync::CancellationToken::new())
            .await
    }

    pub async fn execute_with_cancel(
        &self,
        mut resolver: DagResolver,
        log_tx: mpsc::UnboundedSender<LogMessage>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> PipelineReport {
        if let Ok(mut state) = self.state.global_state.lock() {
            state.clear();
        }
        if let Ok(mut outcomes) = self.state.task_outcomes.lock() {
            outcomes.clear();
        }
        self.state.dry_run.reset();

        // Rebuild the graph one last time from the potentially pruned registry
        if let Err(e) = resolver.build_graph_public() {
            return PipelineReport {
                error: Some(evin_locales::diagnostic(e.into())),
                ..Default::default()
            };
        }

        let (registry, adjacency_list, mut in_degree) = resolver.into_pipeline_state();

        let mut join_set = JoinSet::new();

        let mut secrets_provider = evin_action_sdk::secrets::SecretsProvider::new(
            &std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        );
        if let Some(schema) = &self.config.env_schema {
            for (key, def) in &schema.environments {
                if def.var_type == evin_config::environment::EnvVarType::Secret
                    && let Some(val) = self.config.global_env.get(key)
                {
                    secrets_provider.register_secret(key, val);
                }
            }
        }

        let engine_ctx = Arc::new(EngineContext {
            log_tx: log_tx.clone(),
            state_outputs: self.state.global_state.clone(),
            task_outcomes: self.state.task_outcomes.clone(),
            runner: Arc::new(ShellRunner::new()),
            workspace_root: std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from(".")),
            secrets: Arc::new(secrets_provider),
            cancel_token: cancel_token.clone(),
        });

        let effective_execution = self
            .config
            .forced_execution
            .or(self.config.workspace_execution)
            .unwrap_or(evin_config::workspace::ExecutionMode::Linear);

        let mut ready_queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
        let mut running_non_parallel_count: usize = 0;
        let mut task_is_parallel: HashMap<String, bool> = HashMap::new();

        // Enqueue initial orphans
        for (node, &degree) in &in_degree {
            if degree == 0 {
                let _ = engine_ctx.log_tx.send(LogMessage::DependencyUpdate {
                    task_name: node.clone(),
                    in_degree: 0,
                });
                ready_queue.push_back(node.clone());
            }
        }

        let dispatch_ready =
            |ready_queue: &mut std::collections::VecDeque<String>,
             running_non_parallel_count: &mut usize,
             join_set: &mut JoinSet<TaskResult>,
             task_is_parallel: &mut HashMap<String, bool>| {
                let mut remaining = std::collections::VecDeque::new();
                while let Some(node) = ready_queue.pop_front() {
                    let task = registry.get(&node).unwrap();
                    let is_parallel = effective_execution
                        == evin_config::workspace::ExecutionMode::Parallel
                        || task.parallel == Some(true);

                    if is_parallel {
                        task_is_parallel.insert(node.clone(), true);
                        self.spawn_task(node, task.clone(), join_set, engine_ctx.clone());
                    } else if *running_non_parallel_count == 0 {
                        *running_non_parallel_count += 1;
                        task_is_parallel.insert(node.clone(), false);
                        self.spawn_task(node, task.clone(), join_set, engine_ctx.clone());
                    } else {
                        remaining.push_back(node);
                    }
                }
                *ready_queue = remaining;
            };

        dispatch_ready(
            &mut ready_queue,
            &mut running_non_parallel_count,
            &mut join_set,
            &mut task_is_parallel,
        );

        let mut execution_failed = false;
        let mut execution_cancelled = false;
        let mut error_msg: Option<evin_diagnostic::Diagnostic> = None;
        let mut trace_events = Vec::new();

        loop {
            tokio::select! {
                res = join_set.join_next() => {
                    match res {
                        Some(Ok(task_result)) => {
                            let was_parallel = task_is_parallel.get(&task_result.task_name).copied().unwrap_or(false);
                            if !was_parallel {
                                running_non_parallel_count = running_non_parallel_count.saturating_sub(1);
                            }

                            // Collect trace event
                            let mut args = HashMap::new();
                            args.insert("status".to_string(), "success".to_string());
                            trace_events.push(TraceEvent {
                                name: task_result.task_name.clone(),
                                cat: "task".to_string(),
                                ph: "X".to_string(),
                                ts: task_result.start_ts,
                                dur: task_result.dur_us,
                                pid: 1,
                                tid: task_result.thread_id,
                                args,
                            });
                            if task_result.outcome == TaskOutcome::Cancelled {
                                execution_cancelled = true;
                                join_set.abort_all();
                                while join_set.join_next().await.is_some() {}
                                break;
                            }
                            if task_result.result.is_err() {
                                execution_failed = true;
                                error_msg = Some(evin_diagnostic::Diagnostic::new("Execution Failed", evin_diagnostic::IssueCode::custom("")));
                                cancel_token.cancel();
                                join_set.abort_all();
                                while join_set.join_next().await.is_some() {}
                                break;
                            }

                            if let Ok(mut outcomes) = engine_ctx.task_outcomes.lock() {
                                outcomes.insert(task_result.task_name.clone(), task_result.outcome);
                            }

                            // Success! Unlock neighbors
                            let task_name = task_result.task_name;
                            if let Some(neighbors) = adjacency_list.get(&task_name) {
                                for neighbor in neighbors {
                                    if let Some(degree) = in_degree.get_mut(neighbor) {
                                        *degree -= 1;
                                        let _ = engine_ctx.log_tx.send(LogMessage::DependencyUpdate {
                                            task_name: neighbor.clone(),
                                            in_degree: *degree,
                                        });
                                        if *degree == 0 {
                                            ready_queue.push_back(neighbor.clone());
                                        }
                                    }
                                }
                            }

                            dispatch_ready(
                                &mut ready_queue,
                                &mut running_non_parallel_count,
                                &mut join_set,
                                &mut task_is_parallel,
                            );
                        }
                        Some(Err(join_err)) => {
                            execution_failed = true;
                            error_msg = Some(evin_locales::diagnostic(
                                evin_diagnostic::IssueCode::new("issue-task-panicked")
                                    .arg("error", join_err.to_string()),
                            ));
                            cancel_token.cancel();
                            join_set.abort_all();
                            while join_set.join_next().await.is_some() {}
                            break;
                        }
                        None => {
                            break; // All tasks finished
                        }
                    }
                }
                _ = cancel_token.cancelled() => {
                    execution_cancelled = true;
                    cancel_token.cancel();
                    let grace_period = tokio::time::timeout(
                        std::time::Duration::from_secs(2),
                        async {
                            while join_set.join_next().await.is_some() {}
                        }
                    ).await;

                    if grace_period.is_err() {
                        join_set.abort_all();
                        while join_set.join_next().await.is_some() {}
                    }
                    break;
                }
            }
        }

        let _ = log_tx.send(LogMessage::PipelineState(!execution_failed));

        // Drop log_tx to signal UI task to close
        drop(log_tx);

        let outcomes = self
            .state
            .task_outcomes
            .lock()
            .map(|outcomes| outcomes.clone())
            .unwrap_or_default();

        let mut report = PipelineReport {
            telemetry: trace_events,
            cancelled: execution_cancelled,
            ..Default::default()
        };

        for (task, outcome) in outcomes {
            match outcome {
                TaskOutcome::Planned => report.planned += 1,
                TaskOutcome::Unverified => {
                    report.unverified += 1;
                    report
                        .uncertainty
                        .insert(task.clone(), self.state.dry_run.uncertainty_for(&task));
                }
                TaskOutcome::Failed => report.failed += 1,
                TaskOutcome::Cancelled => {}
            }
        }

        if execution_failed {
            report.error = error_msg;
        }

        report
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_task(
        &self,
        task_name: String,
        task: TaskObject,
        join_set: &mut JoinSet<TaskResult>,
        engine_ctx: Arc<EngineContext>,
    ) {
        let is_ci_mode = self.config.is_ci_mode;
        let mode = self.config.mode;
        let dry_run = self.state.dry_run.clone();

        let pure = task.pure.unwrap_or(false);

        let workspace_values = self.config.workspace_values.clone();
        let module_env = task.module_env.clone().unwrap_or_default();
        let module_values = task.module_values.clone().unwrap_or_default();
        let task_values = task.values.clone().unwrap_or_default();

        let task_stage = task.stage.as_deref().unwrap_or("");
        let merged_env = build_task_env(
            pure,
            &self.config.global_env,
            &module_env,
            task.env.as_ref(),
            task.presets.as_deref(),
            &crate::env::OsEnvironmentProvider,
            self.config.env_schema.as_deref(),
            task_stage,
        );
        let mut merged_values = workspace_values;
        merged_values.extend(module_values);
        merged_values.extend(task_values);

        let run_script = task.run.clone();
        let steps = task.steps.clone();
        let hook_payload = self.config.run_payload.clone();
        let invocation_flags = self.config.invocation_flags.clone();
        let actions = self.state.actions.clone();
        let needs = task.needs.clone().unwrap_or_default();
        let gitrules = self.config.gitrules.clone();
        let workspace_name = self.config.workspace_name.clone();

        join_set.spawn(async move {
            let start_sys_time = SystemTime::now();
            let start_ts = start_sys_time
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_micros() as u64)
                .unwrap_or(0);
            let start_instant = Instant::now();
            static NEXT_TID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            let thread_id = NEXT_TID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            let _ = engine_ctx.log_tx.send(LogMessage::TaskStarted {
                task_name: task_name.clone(),
                description: task.description.clone(),
                command: task
                    .run
                    .clone()
                    .map(|script| format!("(cmd) {}", engine_ctx.secrets.redact(&script)))
                    .or_else(|| {
                        if let Some(steps) = &task.steps {
                            if steps.len() == 1 {
                                steps[0]
                                    .run
                                    .as_ref()
                                    .map(|script| {
                                        format!("(cmd) {}", engine_ctx.secrets.redact(script))
                                    })
                                    .or_else(|| {
                                        steps[0].uses.as_ref().map(|uses| uses.value.clone())
                                    })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }),
            });

            let mut inputs = HashMap::new();
            let mut unverified_inputs = std::collections::HashSet::new();
            {
                let state_lock = engine_ctx.state_outputs.lock().unwrap();
                let outcomes = engine_ctx.task_outcomes.lock().unwrap();
                for dep in &needs {
                    inputs.insert(
                        dep.clone(),
                        state_lock.get(dep).cloned().unwrap_or_default(),
                    );
                    if outcomes.get(dep) == Some(&TaskOutcome::Unverified) {
                        unverified_inputs.insert(dep.clone());
                    }
                }
            }

            let git_provider = Arc::new(evin_action_sdk::git::GitProvider::new_with_dry_run(
                engine_ctx.workspace_root.clone(),
                mode,
                evin_action_sdk::Logger::new(
                    task_name.clone(),
                    engine_ctx.log_tx.clone(),
                    engine_ctx.secrets.clone(),
                ),
                dry_run.clone(),
            ));

            let mut ctx = evin_action_sdk::RunnerContextBuilder::new(
                task_name.clone(),
                engine_ctx.log_tx.clone(),
                engine_ctx.workspace_root.clone(),
                git_provider,
                engine_ctx.secrets.clone(),
            )
            .workspace_name(workspace_name)
            .is_ci_mode(is_ci_mode)
            .mode(mode)
            .hook_payload(hook_payload)
            .cwd(task.cwd.clone())
            .env(merged_env.clone())
            .values(merged_values)
            .inputs(inputs)
            .unverified_inputs(unverified_inputs.clone())
            .dry_run(dry_run)
            .cancel_token(Some(engine_ctx.cancel_token.clone()))
            .build();

            // Pass the globally resolved gitrules down to the action context
            if let Some(r) = gitrules {
                ctx.gitrules = r;
            }

            for dep in &unverified_inputs {
                ctx.mark_unverified(format!("Depends on unverified task '{}'", dep));
            }

            let final_result = if let Some(script) = run_script {
                engine_ctx
                    .runner
                    .run(&ctx, &script, merged_env, Some(&engine_ctx.cancel_token))
                    .await
            } else if let Some(step_list) = steps {
                let mut res = Ok(());
                for step in step_list {
                    if let Some(script) = step.run {
                        let step_res = engine_ctx
                            .runner
                            .run(
                                &ctx,
                                &script,
                                merged_env.clone(),
                                Some(&engine_ctx.cancel_token),
                            )
                            .await;
                        if step_res.is_err() {
                            res = step_res;
                            break;
                        }
                    } else if let Some(uses) = &step.uses {
                        if let Some(action) = actions.get(uses.as_str()) {
                            let flags = invocation_flags
                                .get(uses.as_str())
                                .cloned()
                                .unwrap_or_default();
                            let step_res = action
                                .execute_with_invocation(&ctx, step.with.clone(), &flags)
                                .await;
                            if step_res.is_err() {
                                res = step_res.map_err(|e| e.code);
                                break;
                            }
                        } else {
                            res = Err(evin_diagnostic::IssueCode::new(
                                "issue-unknown-native-action",
                            )
                            .arg("action", uses.as_str()));
                            break;
                        }
                    }
                }
                res
            } else {
                Ok(())
            };

            let was_cancelled = engine_ctx.cancel_token.is_cancelled();
            let outcome = if was_cancelled {
                TaskOutcome::Cancelled
            } else if final_result.is_err() {
                TaskOutcome::Failed
            } else if mode == evin_action_sdk::PipelineMode::DryRun && !ctx.uncertainty().is_empty()
            {
                TaskOutcome::Unverified
            } else {
                TaskOutcome::Planned
            };

            #[allow(clippy::collapsible_if)]
            if final_result.is_ok() {
                if let Ok(local_outputs) = ctx.state.outputs.lock() {
                    if !pure {
                        let mut state_lock = engine_ctx.state_outputs.lock().unwrap();
                        state_lock.insert(task_name.clone(), local_outputs.clone());
                    }
                }
            }

            if let Err(error) = &final_result
                && !was_cancelled
            {
                let _ = engine_ctx.log_tx.send(LogMessage::Diagnostic {
                    task: task_name.clone(),
                    diagnostic: evin_locales::diagnostic(error.clone()),
                });
            }

            let dur_us = start_instant.elapsed().as_micros() as u64;
            let _ = engine_ctx.log_tx.send(LogMessage::TaskFinished(
                task_name.clone(),
                outcome,
                std::time::Duration::from_micros(dur_us),
            ));

            TaskResult {
                task_name,
                result: final_result,
                outcome,
                start_ts,
                dur_us,
                thread_id,
            }
        });
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;

    #[tokio::test]
    async fn external_cancellation_is_intentional_and_not_a_failure() {
        let mut resolver = DagResolver::new();
        resolver.add_task(TaskObject {
            name: "long-running".to_string(),
            run: Some(if cfg!(windows) {
                "timeout 30".to_string()
            } else {
                "sleep 30".to_string()
            }),
            ..Default::default()
        });
        let engine = PipelineEngine::new(
            false,
            evin_action_sdk::PipelineMode::Normal,
            HashMap::new(),
            vec![],
            None,
        );
        let token = tokio_util::sync::CancellationToken::new();
        let cancel = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            cancel.cancel();
        });
        let (log_tx, mut log_rx) = tokio::sync::mpsc::unbounded_channel();

        let report = engine.execute_with_cancel(resolver, log_tx, token).await;

        assert!(report.cancelled);
        assert!(report.error.is_none());
        let messages: Vec<_> = std::iter::from_fn(|| log_rx.try_recv().ok()).collect();
        assert!(
            !messages
                .iter()
                .any(|message| matches!(message, LogMessage::Diagnostic { .. }))
        );
        assert!(messages.iter().any(|message| matches!(
            message,
            LogMessage::TaskFinished(name, TaskOutcome::Cancelled, _) if name == "long-running"
        )));
    }
}
