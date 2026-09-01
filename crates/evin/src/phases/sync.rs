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
use evin_git::manager::HookChange;

pub struct ReconcileReleaseLockPhase;

#[async_trait]
impl Phase for ReconcileReleaseLockPhase {
    fn locale_key(&self) -> String {
        "ui.phase.reconciling_release_lock".into()
    }

    async fn execute(
        &self,
        ctx: &mut CliContext,
        emitter: &PipelineEmitter,
    ) -> Result<(), Diagnostic> {
        let dry_run = ctx.mode == evin_action_sdk::PipelineMode::DryRun;
        if let crate::sync::release_lock::ReconcileOutcome::Planned(yaml) =
            crate::sync::release_lock::reconcile(dry_run).await?
        {
            emitter.emit(PipelineEvent::FencedOutput {
                title: ".evin/release-lock.yaml".into(),
                lines: yaml.lines().map(str::to_owned).collect(),
                footer_label: Some("planned".into()),
            });
        }
        Ok(())
    }
}

pub struct SyncNativeActionsPhase;

#[async_trait]
impl Phase for SyncNativeActionsPhase {
    fn locale_key(&self) -> String {
        "ui.phase.synchronizing_native_actions".into()
    }

    async fn execute(
        &self,
        ctx: &mut CliContext,
        emitter: &PipelineEmitter,
    ) -> Result<(), Diagnostic> {
        let config = ctx.config.as_ref().unwrap();
        let res = crate::sync::actions::sync_actions(
            &config.tasks,
            &crate::commands::builtin_actions(),
            ctx.mode,
        )
        .await?;
        ctx.synced_actions = res;

        if ctx.verbose {
            for action in &ctx.synced_actions {
                let title = format!(
                    "{}{}",
                    evin_ui::render::SYMBOL_FENCED_ACTION_PREFIX,
                    action.label
                );
                let change_str = match action.change {
                    evin_action_sdk::SyncChange::Create => "created",
                    evin_action_sdk::SyncChange::Update => "updated",
                    evin_action_sdk::SyncChange::Unchanged => "unchanged",
                    evin_action_sdk::SyncChange::Stale => "stale",
                };
                let lines = vec![format!("label: {}", action.label)];
                emitter.emit(PipelineEvent::FencedOutput {
                    title,
                    lines,
                    footer_label: Some(change_str.to_string()),
                });
            }
        }

        Ok(())
    }
}

pub struct SyncGitHooksPhase;

#[async_trait]
impl Phase for SyncGitHooksPhase {
    fn locale_key(&self) -> String {
        "ui.phase.synchronizing_git_hooks".into()
    }

    async fn execute(
        &self,
        ctx: &mut CliContext,
        emitter: &PipelineEmitter,
    ) -> Result<(), Diagnostic> {
        // 1. Aggregate Hooks
        let config = match &ctx.config {
            Some(c) => c,
            None => {
                return Err(evin_locales::diagnostic(
                    evin_diagnostic::IssueCode::new("internal-error")
                        .arg("message", "Configuration not loaded"),
                ));
            }
        };

        let mut hook_summaries =
            crate::sync::hooks::aggregate_hooks(&config.tasks).map_err(|binding| {
                evin_locales::diagnostic(
                    evin_diagnostic::IssueCode::new("invalid-git-hook")
                        .arg("task_name", &binding.task_name)
                        .arg("hook", &binding.hook_name),
                )
            })?;

        let standard_hooks = vec![
            evin_git::types::GitHook::PreCommit,
            evin_git::types::GitHook::CommitMsg,
            evin_git::types::GitHook::PrePush,
        ];
        for hook in standard_hooks {
            let hook_name = hook.as_str().to_string();
            if !hook_summaries.iter().any(|s| s.name == hook_name) {
                hook_summaries.push(crate::sync::hooks::HookSummary {
                    hook,
                    name: hook_name,
                    task_count: 0,
                });
            }
        }

        let has_native_actions = config.tasks.iter().any(|task| {
            task.steps
                .as_deref()
                .unwrap_or_default()
                .iter()
                .any(|step| step.uses.is_some())
        });

        if hook_summaries.is_empty() && !has_native_actions {
            emitter.emit(PipelineEvent::DebugLog(
                None,
                evin_action_sdk::debug::ExecutionSkipped {
                    reason: "no_triggers_matched".into(),
                }
                .into_record(),
            ));
            ctx.hook_summaries = hook_summaries;
            return Ok(());
        }

        ctx.hook_summaries = hook_summaries;

        // 2. Evaluate Git Environment & Plan Hooks
        let env = evin_git::env::native::NativeGitEnv::new()
            .await
            .map_err(|error| {
                evin_locales::diagnostic(
                    evin_diagnostic::IssueCode::new("issue-sync-execution-failed")
                        .arg("error", error.to_string()),
                )
            })?;
        let manager = evin_git::manager::HookManager::new(env);
        let dynamic_payload = crate::sync::payload::build_hook_payload(
            config.hook_runner.as_ref(),
            config.install_hint.as_deref(),
        );

        let plans = crate::sync::hooks::plan_hooks(&manager, &ctx.hook_summaries, &dynamic_payload)
            .await
            .map_err(|(hook_name, error)| {
                evin_locales::diagnostic(
                    evin_diagnostic::IssueCode::new("hook-install-failed")
                        .arg("hook", hook_name)
                        .arg("error", error.to_string()),
                )
            })?;

        ctx.hook_plans = plans;

        // 3. Install Git Hooks
        if ctx.mode != evin_action_sdk::PipelineMode::DryRun
            && !ctx.hook_plans.is_empty()
            && let Err((hook_name, error)) =
                crate::sync::hooks::apply_hook_plans(&manager, &ctx.hook_plans).await
        {
            return Err(evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("hook-install-failed")
                    .arg("hook", hook_name)
                    .arg("error", error.to_string()),
            ));
        }

        if ctx.verbose {
            for plan in &ctx.hook_plans {
                let title = format!(
                    "{}.git/hooks/{}",
                    evin_ui::render::SYMBOL_FENCED_HOOK_PREFIX,
                    plan.hook.as_str()
                );
                let lines: Vec<String> = plan.content.lines().map(String::from).collect();
                let change_str = match plan.change {
                    evin_git::manager::HookChange::Create => "installed",
                    evin_git::manager::HookChange::Update => "updated",
                    evin_git::manager::HookChange::Unchanged => "unchanged",
                };
                if !lines.is_empty() {
                    emitter.emit(PipelineEvent::FencedOutput {
                        title,
                        lines,
                        footer_label: Some(change_str.to_string()),
                    });
                }
            }
        }

        // 4. Generate Summary Details
        let details = ctx
            .hook_summaries
            .iter()
            .zip(&ctx.hook_plans)
            .map(|(summary, plan)| {
                let styled_name = evin_ui::utils::highlight(&summary.name);
                let bound_tasks: Vec<String> = config
                    .tasks
                    .iter()
                    .filter(|t| {
                        t.on.as_ref()
                            .and_then(|on| on.hooks.as_ref())
                            .map(|hooks| hooks.contains(&summary.name))
                            .unwrap_or(false)
                            || t.stage
                                .as_ref()
                                .map(|s| s.eq_ignore_ascii_case(&summary.name))
                                .unwrap_or(false)
                    })
                    .map(|t| t.name.clone())
                    .collect();

                let task_suffix = if ctx.verbose && !bound_tasks.is_empty() {
                    format!(": {}", bound_tasks.join(", "))
                } else {
                    String::new()
                };

                if ctx.mode == evin_action_sdk::PipelineMode::DryRun {
                    let change = match plan.change {
                        HookChange::Create => evin_locales::t!("hook-change-create"),
                        HookChange::Update => evin_locales::t!("hook-change-update"),
                        HookChange::Unchanged => evin_locales::t!("hook-change-unchanged"),
                    };
                    format!(
                        "{}{}",
                        evin_locales::t!(
                            "hook-preview",
                            hook = &styled_name,
                            count = summary.task_count,
                            change = change
                        ),
                        task_suffix
                    )
                } else {
                    format!(
                        "{}{}",
                        evin_locales::t!(
                            "hook-task-count",
                            hook = &styled_name,
                            count = summary.task_count
                        ),
                        task_suffix
                    )
                }
            })
            .collect::<Vec<_>>();

        let mut details = details;
        details.extend(ctx.synced_actions.iter().map(|resource| {
            let change = match resource.change {
                evin_action_sdk::SyncChange::Create => evin_locales::t!("hook-change-create"),
                evin_action_sdk::SyncChange::Update => evin_locales::t!("hook-change-update"),
                evin_action_sdk::SyncChange::Unchanged => evin_locales::t!("hook-change-unchanged"),
                evin_action_sdk::SyncChange::Stale => evin_locales::t!("sync-change-stale"),
            };
            evin_locales::t!("schema-preview", schema = &resource.label, change = change)
        }));

        ctx.skip_default_summary = true;
        ctx.sync_summary_details = Some(details);

        Ok(())
    }
}
