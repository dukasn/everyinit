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

use crate::engine::{CliContext, Phase, PipelineEmitter};
use crate::events::PipelineEvent;
use async_trait::async_trait;
use evin_action_sdk::debug::StructuredDebugEvent;
use evin_diagnostic::Diagnostic;
use evin_pipeline::dag::DagResolver;
use evin_pipeline::engine::PipelineEngine;
use std::sync::Arc;

pub struct ResolveConfigPhase;

fn validate_resolved_action_configs(resolver: &DagResolver) -> Result<(), Diagnostic> {
    evin_pipeline::preflight::validate_task_contracts(
        resolver.registry().values(),
        &crate::commands::builtin_actions(),
    )
    .into_iter()
    .next()
    .map_or(Ok(()), |finding| Err(finding.diagnostic))
}

#[async_trait]
impl Phase for ResolveConfigPhase {
    fn locale_key(&self) -> String {
        "ui.phase.resolving_configuration".into()
    }

    async fn execute(
        &self,
        ctx: &mut CliContext,
        emitter: &PipelineEmitter,
    ) -> Result<(), Diagnostic> {
        if ctx.debug {
            super::emit_resolving_configuration_debug_logs(emitter).await;
        }
        let config = evin_config::load_workspace_tasks().await?;

        let workspace_yaml_path = std::path::PathBuf::from(evin_config::paths::WORKSPACE_MANIFEST);
        let bytes = if let Ok(metadata) = std::fs::metadata(&workspace_yaml_path) {
            metadata.len() as usize
        } else {
            0
        };

        emitter.emit(PipelineEvent::DebugLog(
            None,
            evin_action_sdk::debug::ConfigLoaded {
                target: evin_config::paths::WORKSPACE_MANIFEST.into(),
                name: config.workspace_name.clone(),
                bytes,
                status: "ok".into(),
            }
            .into_record(),
        ));

        if let Some(gitrules) = &config.gitrules {
            let loaded_rules = [
                gitrules.commits.is_some(),
                gitrules.branches.is_some(),
                gitrules.releases.is_some(),
                gitrules.authorship.is_some(),
                gitrules.history.is_some(),
                gitrules.tags.is_some(),
                gitrules.pull_requests.is_some(),
                gitrules.files.is_some(),
                gitrules.lfs.is_some(),
                gitrules.submodules.is_some(),
                gitrules.remotes.is_some(),
            ]
            .into_iter()
            .filter(|&x| x)
            .count();
            let target = match &config.gitrules_source {
                evin_config::parser::GitRulesSource::Local(path) => path.clone(),
                evin_config::parser::GitRulesSource::Fallback => "fallback".into(),
            };
            emitter.emit(PipelineEvent::DebugLog(
                None,
                evin_action_sdk::debug::GitrulesLoaded {
                    target,
                    loaded_rules,
                }
                .into_record(),
            ));
        }

        for warning in &config.warnings {
            if warning.contains("local rules path") && warning.contains("does not exist") {
                let target = warning
                    .replace("local rules path '", "")
                    .replace("' does not exist", "");
                emitter.emit(PipelineEvent::DebugLog(
                    None,
                    evin_action_sdk::debug::GitrulesLoaded {
                        target,
                        loaded_rules: 0,
                    }
                    .into_record(),
                ));
            }
        }

        for (name, target, tasks) in &config.modules_discovered {
            emitter.emit(PipelineEvent::DebugLog(
                None,
                evin_action_sdk::debug::ModuleLoaded {
                    name: name.clone(),
                    target: target.clone(),
                    tasks: *tasks,
                }
                .into_record(),
            ));
        }

        let workspace_root =
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

        let env_schema_path = workspace_root.join(evin_config::paths::ENVIRONMENT_MANIFEST);
        let env_schema = if env_schema_path.exists() {
            match <evin_config::environment::EnvironmentYaml as evin_yaml::YamlDocument>::from_file(
                &env_schema_path,
            ) {
                Ok(schema) => Some(schema),
                Err(e) => return Err(e),
            }
        } else {
            None
        };

        let global_env = evin_pipeline::env::load_global_env(
            &workspace_root,
            &config.workspace_env,
            env_schema.as_ref(),
        )?;

        if env_schema_path.exists() || !global_env.is_empty() {
            emitter.emit(PipelineEvent::DebugLog(
                None,
                evin_action_sdk::debug::EnvironmentResolved {
                    schema: if env_schema_path.exists() {
                        let rel_path = env_schema_path
                            .strip_prefix(&workspace_root)
                            .unwrap_or(&env_schema_path);
                        Some(rel_path.display().to_string())
                    } else {
                        None
                    },
                    vars: global_env.len(),
                }
                .into_record(),
            ));
        }

        let arc_env_schema = env_schema.map(std::sync::Arc::new);
        ctx.global_env = Some(global_env);
        ctx.env_schema = arc_env_schema;

        let mut resolver = DagResolver::new();
        for task in config.tasks.clone() {
            resolver.add_task(task);
        }

        emitter.emit(PipelineEvent::DebugLog(
            None,
            evin_action_sdk::debug::DagGenerated {
                nodes: resolver.registry().len(),
                edges: resolver
                    .registry()
                    .values()
                    .filter_map(|t| t.needs.as_ref())
                    .map(|n| n.len())
                    .sum(),
                cycles: 0,
            }
            .into_record(),
        ));

        let is_sync_command = ctx.targets.len() == 1 && ctx.targets[0] == "sync";
        if !is_sync_command {
            let registry = crate::resolver::TargetRegistry::build(config.tasks.clone());
            let mut target_names = Vec::new();
            let mut any_hook = false;

            for t in &ctx.targets {
                match registry.resolve(t) {
                    Ok((context_type, names)) => {
                        emitter.emit(PipelineEvent::DebugLog(
                            None,
                            evin_action_sdk::debug::TargetResolved {
                                input: t.clone(),
                                matched: names.len(),
                            }
                            .into_record(),
                        ));
                        target_names.extend(names);
                        if context_type == evin_pipeline::PipelineContext::Hook {
                            any_hook = true;
                        }
                    }
                    Err(e) => return Err(e),
                }
            }

            let original_targets = ctx.targets.clone();
            ctx.targets = target_names;
            ctx.is_hook = any_hook;

            if let Err(e) = resolver.resolve_targets(&ctx.targets) {
                return Err(evin_locales::diagnostic(e.into()));
            }

            validate_resolved_action_configs(&resolver)?;

            let invocation = evin_pipeline::preflight::resolve_invocation_flags(
                resolver.registry().values(),
                &crate::commands::builtin_actions(),
                &ctx.run_payload,
            )?;
            ctx.run_payload = invocation.hook_payload;
            ctx.invocation_flags = invocation.action_flags;

            if any_hook && let Some(rules) = &config.gitrules {
                let engine = evin_policy::PolicyEngine::new();
                if let Some(target) = original_targets.first()
                    && let Err(violations) = engine
                        .enforce(target, rules, &workspace_root, &ctx.run_payload)
                        .await
                {
                    let diagnostics = evin_policy::reporter::violations_to_diagnostics(&violations);
                    ctx.phase_failure_diagnostics = diagnostics.clone();
                    return Err(diagnostics.into_iter().next().unwrap_or_else(|| {
                        Diagnostic::new("", evin_diagnostic::IssueCode::custom(""))
                    }));
                }
            }
        }

        ctx.dag = Some(resolver);
        ctx.config = Some(config);

        Ok(())
    }
}

pub struct ExecuteTasksPhase;

#[async_trait]
impl Phase for ExecuteTasksPhase {
    fn locale_key(&self) -> String {
        "ui.phase.executing_tasks".into()
    }

    async fn execute(
        &self,
        ctx: &mut CliContext,
        emitter: &PipelineEmitter,
    ) -> Result<(), Diagnostic> {
        let config = match ctx.config.take() {
            Some(c) => c,
            None => {
                return Err(evin_locales::diagnostic(
                    evin_diagnostic::IssueCode::new("internal-error")
                        .arg("message", "Configuration not loaded"),
                ));
            }
        };

        let resolver = ctx.dag.take().unwrap();
        let global_env = ctx.global_env.take().unwrap_or_default();

        let mut engine = PipelineEngine::new(
            false,
            ctx.mode,
            config.workspace_env,
            ctx.run_payload.clone(),
            Some(config.workspace_name.clone()),
        );
        engine.config.workspace_values = config.workspace_values;
        engine.config.invocation_flags = ctx.invocation_flags.clone();
        engine.config.global_env = Arc::new(global_env);
        engine.config.env_schema = ctx.env_schema.clone();
        engine.config.gitrules = config.gitrules;
        engine.config.forced_execution = ctx.forced_execution;
        engine.config.workspace_execution = config.execution;

        for (id, action) in crate::commands::builtin_actions() {
            engine.register_action_arc(id, action);
        }

        let (log_tx, mut log_rx) = tokio::sync::mpsc::unbounded_channel();
        let emitter_clone = emitter.clone();

        let translator_handle = tokio::spawn(async move {
            while let Some(msg) = log_rx.recv().await {
                match msg {
                    evin_action_sdk::LogMessage::Activity { task, record } => {
                        emitter_clone.emit(PipelineEvent::Activity { task, record });
                    }
                    evin_action_sdk::LogMessage::RawOutput { task, stream, line } => {
                        emitter_clone.emit(PipelineEvent::RawOutput { task, stream, line });
                    }
                    evin_action_sdk::LogMessage::PlannedOperation { message, .. } => {
                        emitter_clone.emit(PipelineEvent::PlannedOperation(message));
                    }
                    evin_action_sdk::LogMessage::Progress { message, .. } => {
                        emitter_clone.emit(PipelineEvent::Progress(message));
                    }
                    evin_action_sdk::LogMessage::Diagnostic { task, diagnostic } => {
                        emitter_clone.emit(PipelineEvent::Diagnostic { task, diagnostic });
                    }
                    evin_action_sdk::LogMessage::DebugRecord { task, record } => {
                        emitter_clone.emit(PipelineEvent::DebugLog(Some(task), record));
                    }
                    evin_action_sdk::LogMessage::TaskStarted {
                        task_name,
                        description,
                        command,
                    } => {
                        emitter_clone.emit(PipelineEvent::TaskOutputStarted {
                            name: task_name,
                            description,
                            command,
                        });
                    }
                    evin_action_sdk::LogMessage::TaskStepStarted {
                        task_name,
                        step_number,
                        step_count,
                        footer,
                    } => {
                        emitter_clone.emit(PipelineEvent::TaskStepStarted {
                            name: task_name,
                            step_number,
                            step_count,
                            footer,
                        });
                    }
                    evin_action_sdk::LogMessage::DependencyUpdate {
                        task_name,
                        in_degree,
                    } => {
                        emitter_clone.emit(PipelineEvent::DebugLog(
                            Some(task_name.clone()),
                            evin_action_sdk::debug::DependencyUpdate {
                                task_id: task_name,
                                in_degree,
                            }
                            .into_record(),
                        ));
                    }
                    evin_action_sdk::LogMessage::TaskFinished(name, outcome, duration) => {
                        let failed = outcome == evin_action_sdk::TaskOutcome::Failed;
                        let unverified = outcome == evin_action_sdk::TaskOutcome::Unverified;
                        let cancelled = outcome == evin_action_sdk::TaskOutcome::Cancelled;
                        emitter_clone.emit(PipelineEvent::TaskOutputEnded {
                            name,
                            duration,
                            failed,
                            unverified,
                            cancelled,
                        });
                    }
                    evin_action_sdk::LogMessage::PipelineState(_) => {}
                    evin_action_sdk::LogMessage::DaemonStarted {
                        task,
                        startup_duration,
                        status_message,
                    } => {
                        emitter_clone.emit(PipelineEvent::TaskDaemonStarted {
                            name: task,
                            startup_duration,
                            status_message,
                        });
                    }
                    evin_action_sdk::LogMessage::DaemonStatus {
                        task,
                        status_message,
                    } => {
                        emitter_clone.emit(PipelineEvent::TaskDaemonStatus {
                            name: task,
                            status_message,
                        });
                    }
                    evin_action_sdk::LogMessage::DaemonStopped {
                        task,
                        duration,
                        failed,
                    } => {
                        emitter_clone.emit(PipelineEvent::TaskDaemonStopped {
                            name: task,
                            duration,
                            failed,
                        });
                    }
                }
            }
        });

        let report = engine
            .execute_with_cancel(resolver, log_tx, ctx.cancel_token.clone())
            .await;
        let _ = translator_handle.await;

        emitter.emit(PipelineEvent::TelemetryReport {
            telemetry: report.telemetry,
            workspace_root: std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from(".")),
        });

        if report.cancelled {
            return Ok(());
        }

        if let Some(err) = report.error {
            return Err(err);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evin_config::models::{StepObject, TaskObject};

    fn docs_task(name: &str, needs: Option<Vec<String>>, with: &str) -> TaskObject {
        TaskObject {
            name: name.into(),
            needs,
            source_file: Some("docs/.evin/module.flow".into()),
            steps: Some(vec![StepObject {
                run: None,
                uses: Some(evin_yaml::Spanned::new(
                    "evin/docs@v1".into(),
                    evin_yaml::Span::default(),
                )),
                with: Some(evin_yaml::parse_yaml(with).unwrap()),
            }]),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_resolve_config_phase_sync_command() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let emitter = PipelineEmitter::new(tx);
        let mut ctx = CliContext {
            targets: vec!["sync".to_string()],
            ..Default::default()
        };

        let phase = ResolveConfigPhase;
        let res = phase.execute(&mut ctx, &emitter).await;
        assert!(res.is_ok());
        assert!(ctx.config.is_some());
    }

    #[test]
    fn selected_graph_preflight_rejects_an_invalid_dependency() {
        let mut resolver = DagResolver::new();
        resolver.add_task(docs_task(
            "docs:dependency",
            None,
            "site_url: https://example.com\ntile: Docs\n",
        ));
        resolver.add_task(TaskObject {
            name: "docs:build".into(),
            needs: Some(vec!["docs:dependency".into()]),
            run: Some("true".into()),
            ..Default::default()
        });
        resolver.resolve_targets(&["docs:build".into()]).unwrap();

        let error = validate_resolved_action_configs(&resolver).unwrap_err();

        assert_eq!(error.code.key, "issue-action-config-invalid");
    }

    #[test]
    fn selected_graph_preflight_ignores_unrelated_invalid_tasks() {
        let mut resolver = DagResolver::new();
        resolver.add_task(docs_task(
            "docs:broken",
            None,
            "site_url: https://example.com\ntile: Docs\n",
        ));
        resolver.add_task(TaskObject {
            name: "build".into(),
            run: Some("true".into()),
            ..Default::default()
        });
        resolver.resolve_targets(&["build".into()]).unwrap();

        assert!(validate_resolved_action_configs(&resolver).is_ok());
    }
}
