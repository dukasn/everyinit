// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

use async_trait::async_trait;
use evin_config::models::{EnvDeclaration, TaskObject};
use evin_config::parser::EverInitConfig;
use evin_diagnostic::Diagnostic;
use evin_pipeline::PipelineContext;
use evin_ui::discovery::{
    CatalogView, CountedTarget, ExecutionEntry, ExplanationView, FailedExplanationView, TargetCard,
    TargetKind, TargetMetadata, TaskGroup, WorkspaceIdentity,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::IsTerminal;
use std::sync::{Arc, Mutex};

use crate::engine::{CliContext, Phase, PipelineEmitter};

pub enum DiscoveryCommand {
    List,
    Explain(String),
}

pub struct DiscoveryOutput {
    pub rendered: String,
    pub success: bool,
}

pub struct RenderDiscoveryPhase {
    pub command: DiscoveryCommand,
    pub result: Arc<Mutex<Option<DiscoveryOutput>>>,
}

#[async_trait]
impl Phase for RenderDiscoveryPhase {
    fn locale_key(&self) -> String {
        "ui.phase.rendering_discovery".into()
    }

    async fn execute(
        &self,
        ctx: &mut CliContext,
        _emitter: &PipelineEmitter,
    ) -> Result<(), Diagnostic> {
        let config = ctx.config.as_ref().ok_or_else(|| {
            evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(
                "workspace configuration was not resolved before discovery rendering",
            ))
        })?;
        let output = match &self.command {
            DiscoveryCommand::List => DiscoveryOutput {
                rendered: render_list(config)?,
                success: true,
            },
            DiscoveryCommand::Explain(target) => {
                let (success, rendered) = render_explanation_for_target(config, target)?;
                DiscoveryOutput { rendered, success }
            }
        };
        *self.result.lock().unwrap() = Some(output);
        Ok(())
    }
}

pub fn render_list(config: &EverInitConfig) -> Result<String, Diagnostic> {
    Ok(evin_ui::discovery::render_catalog(
        &catalog_view(config)?,
        terminal_width(),
    ))
}

pub fn render_explanation_for_target(
    config: &EverInitConfig,
    target: &str,
) -> Result<(bool, String), Diagnostic> {
    match explanation_view(config, target) {
        Ok(view) => Ok((
            true,
            evin_ui::discovery::render_explanation(&view, terminal_width()),
        )),
        Err(error) => Ok((
            false,
            evin_ui::discovery::render_failed_explanation(
                &FailedExplanationView {
                    workspace: identity(config),
                    target: target.to_string(),
                    hint: error.hint,
                },
                terminal_width(),
            ),
        )),
    }
}

pub fn emit(rendered: String) {
    let tape = std::env::var_os("EVIN_TAPE").is_some();
    match output_destination(std::io::stdout().is_terminal(), tape) {
        OutputDestination::StyledStderr => eprint!("{rendered}"),
        OutputDestination::PlainStdout => {
            print!("{}", console::strip_ansi_codes(&rendered));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputDestination {
    StyledStderr,
    PlainStdout,
}

fn output_destination(stdout_is_terminal: bool, tape: bool) -> OutputDestination {
    if stdout_is_terminal || tape {
        OutputDestination::StyledStderr
    } else {
        OutputDestination::PlainStdout
    }
}

fn terminal_width() -> usize {
    console::Term::stderr()
        .size_checked()
        .map(|(_, width)| usize::from(width))
        .unwrap_or(evin_ui::constants::FALLBACK_WIDTH)
}

fn catalog_view(config: &EverInitConfig) -> Result<CatalogView, Diagnostic> {
    let mut root_tasks = Vec::new();
    let mut grouped_modules: BTreeMap<String, (String, Vec<TargetCard>)> = BTreeMap::new();

    let mut tasks: Vec<_> = config.tasks.iter().collect();
    tasks.sort_by(|left, right| left.name.cmp(&right.name));
    for task in tasks {
        let card = TargetCard {
            name: task.name.clone(),
            description: description(task),
        };
        if let Some((module, _)) = task.name.split_once(':') {
            let source = crate::commands::file_uri(
                task.source_file
                    .as_deref()
                    .unwrap_or(evin_config::paths::MODULE_MANIFEST),
            )?;
            grouped_modules
                .entry(module.to_string())
                .or_insert_with(|| (source, Vec::new()))
                .1
                .push(card);
        } else {
            root_tasks.push(card);
        }
    }

    let mut groups = vec![TaskGroup {
        source: crate::commands::file_uri(evin_config::paths::WORKSPACE_MANIFEST)?,
        module: None,
        tasks: root_tasks,
    }];
    let mut modules = Vec::new();
    for (name, (source, tasks)) in grouped_modules {
        modules.push(CountedTarget {
            name: name.clone(),
            task_count: tasks.len(),
        });
        groups.push(TaskGroup {
            source,
            module: Some(name),
            tasks,
        });
    }

    let stages: BTreeSet<_> = config.stages.iter().cloned().collect();
    let stages = stages
        .into_iter()
        .map(|name| CountedTarget {
            task_count: config
                .tasks
                .iter()
                .filter(|task| task.stage.as_deref() == Some(name.as_str()))
                .count(),
            name,
        })
        .collect();

    let hooks = hook_names(config)
        .into_iter()
        .map(|name| CountedTarget {
            task_count: config
                .tasks
                .iter()
                .filter(|task| {
                    task.on
                        .as_ref()
                        .and_then(|on| on.hooks.as_ref())
                        .is_some_and(|hooks| hooks.contains(&name))
                })
                .count(),
            name,
        })
        .collect();

    Ok(CatalogView {
        workspace: identity(config),
        groups,
        modules,
        stages,
        hooks,
        total_tasks: config.tasks.len(),
    })
}

fn explanation_view(config: &EverInitConfig, target: &str) -> Result<ExplanationView, Diagnostic> {
    let registry = crate::resolver::TargetRegistry::build(config.tasks.clone());
    let (context, mut roots) = registry.resolve(target)?;
    roots.sort();

    let tasks: HashMap<_, _> = config
        .tasks
        .iter()
        .map(|task| (task.name.as_str(), task))
        .collect();
    let mut visited = HashSet::new();
    let mut ordered = Vec::new();
    for root in &roots {
        visit(root, &tasks, &mut visited, &mut ordered);
    }

    let selected = tasks.get(target).copied();
    let kind = context_kind(context);
    let metadata = if let Some(task) = selected {
        TargetMetadata {
            name: target.to_string(),
            kind,
            description: Some(description(task)),
            stage: task.stage.clone(),
            cwd: Some(crate::commands::file_uri(
                task.cwd.as_deref().unwrap_or("."),
            )?),
            needs: sorted(task.needs.clone().unwrap_or_default()),
            environment: environment_names(task),
            steps: step_summaries(task),
            resolved_tasks: ordered.len(),
            modules: module_summary(&ordered),
        }
    } else {
        let stage_cwd = kind == TargetKind::Stage;
        TargetMetadata {
            name: target.to_string(),
            kind,
            description: None,
            stage: None,
            cwd: if stage_cwd {
                Some(crate::commands::file_uri(".")?)
            } else {
                None
            },
            needs: Vec::new(),
            environment: Vec::new(),
            steps: Vec::new(),
            resolved_tasks: ordered.len(),
            modules: module_summary(&ordered),
        }
    };

    let execution = ordered
        .iter()
        .filter_map(|name| tasks.get(name.as_str()))
        .map(|task| ExecutionEntry {
            name: task.name.clone(),
            description: description(task),
        })
        .collect();
    let step_count = ordered
        .iter()
        .filter_map(|name| tasks.get(name.as_str()))
        .map(|task| step_count(task))
        .sum();

    Ok(ExplanationView {
        workspace: identity(config),
        target: metadata,
        execution,
        step_count,
    })
}

fn identity(config: &EverInitConfig) -> WorkspaceIdentity {
    WorkspaceIdentity {
        name: config.workspace_name.clone(),
        version: config.workspace_version.clone(),
    }
}

fn module_summary(tasks: &[String]) -> Vec<String> {
    let mut counts = BTreeMap::<String, usize>::new();
    for task in tasks {
        let module = task
            .split_once(':')
            .map(|(module, _)| module)
            .unwrap_or("main");
        *counts.entry(module.to_string()).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(name, count)| format!("{name}({count})"))
        .collect()
}

fn hook_names(config: &EverInitConfig) -> BTreeSet<String> {
    let mut hooks: BTreeSet<_> = evin_policy::KNOWN_HOOKS
        .iter()
        .map(|hook| (*hook).to_string())
        .collect();
    for task in &config.tasks {
        if let Some(task_hooks) = task.on.as_ref().and_then(|on| on.hooks.as_ref()) {
            hooks.extend(task_hooks.iter().cloned());
        }
    }
    hooks
}

fn description(task: &TaskObject) -> String {
    task.description
        .clone()
        .unwrap_or_else(|| evin_locales::t!("discovery-no-description"))
}

fn environment_names(task: &TaskObject) -> Vec<String> {
    let names = match &task.env {
        Some(EnvDeclaration::List(names)) => names.clone(),
        Some(EnvDeclaration::Map(values)) => values.keys().cloned().collect(),
        None => Vec::new(),
    };
    sorted(names)
}

fn step_summaries(task: &TaskObject) -> Vec<String> {
    let mut summaries = Vec::new();
    if let Some(run) = &task.run {
        summaries.push(format!("run: {}", summarize(run)));
    }
    if let Some(steps) = &task.steps {
        for step in steps {
            if let Some(run) = &step.run {
                summaries.push(format!("run: {}", summarize(run)));
            } else if let Some(action) = &step.uses {
                summaries.push(format!("uses: {}", action.value));
            }
        }
    }
    summaries
}

fn step_count(task: &TaskObject) -> usize {
    usize::from(task.run.is_some()) + task.steps.as_ref().map_or(0, Vec::len)
}

fn sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

fn context_kind(context: PipelineContext) -> TargetKind {
    match context {
        PipelineContext::Task => TargetKind::Task,
        PipelineContext::Stage => TargetKind::Stage,
        PipelineContext::Hook => TargetKind::Hook,
        PipelineContext::Workspace => TargetKind::Workspace,
    }
}

fn summarize(script: &str) -> String {
    let first = script
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    let suffix = if script
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
        > 1
    {
        " …"
    } else {
        ""
    };
    format!("{}{suffix}", first.trim())
}

fn visit<'a>(
    name: &str,
    tasks: &HashMap<&'a str, &'a TaskObject>,
    visited: &mut HashSet<String>,
    ordered: &mut Vec<String>,
) {
    if !visited.insert(name.to_string()) {
        return;
    }
    let Some(task) = tasks.get(name) else {
        return;
    };
    for dependency in sorted(task.needs.clone().unwrap_or_default()) {
        visit(&dependency, tasks, visited, ordered);
    }
    ordered.push(name.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(tasks: Vec<TaskObject>) -> EverInitConfig {
        EverInitConfig {
            workspace_name: "demo".to_string(),
            workspace_version: "1.2.3".to_string(),
            stages: vec!["Test".to_string(), "Build".to_string()],
            tasks,
            expose: Vec::new(),
            workspace_env: HashMap::new(),
            workspace_values: HashMap::new(),
            modules: None,
            install_hint: None,
            hook_runner: Some(evin_config::workspace::HookRunner {
                command: "cargo".into(),
                args: vec!["evin-dev".into()],
            }),
            execution: None,
            gitrules: None,
            gitrules_source: evin_config::parser::GitRulesSource::Fallback,
            warnings: Vec::new(),
            modules_discovered: Vec::new(),
            module_versions: HashMap::new(),
        }
    }

    #[test]
    fn catalog_groups_module_tasks() {
        let view = catalog_view(&config(vec![
            TaskObject {
                name: "esh:test".into(),
                description: Some("ESH tests".into()),
                stage: Some("Test".into()),
                ..Default::default()
            },
            TaskObject {
                name: "build".into(),
                stage: Some("Build".into()),
                ..Default::default()
            },
        ]))
        .unwrap();
        assert_eq!(view.groups[0].tasks[0].name, "build");
        assert_eq!(view.groups[1].module.as_deref(), Some("esh"));
        assert_eq!(view.groups[1].tasks[0].name, "esh:test");
        assert_eq!(view.modules[0].task_count, 1);
    }

    #[test]
    fn catalog_summary_includes_sorted_stages_and_known_hooks() {
        let view = catalog_view(&config(vec![TaskObject {
            name: "test".into(),
            stage: Some("Test".into()),
            on: Some(evin_config::models::TriggerObject {
                hooks: Some(vec!["pre-commit".into()]),
            }),
            ..Default::default()
        }]))
        .unwrap();
        assert_eq!(view.stages[0].name, "Build");
        assert_eq!(view.stages[1].name, "Test");
        let pre_commit = view
            .hooks
            .iter()
            .find(|hook| hook.name == "pre-commit")
            .unwrap();
        assert_eq!(pre_commit.task_count, 1);
    }

    #[test]
    fn explanation_orders_dependencies_and_redacts_values_and_inputs() {
        let view = explanation_view(
            &config(vec![
                TaskObject {
                    name: "check".into(),
                    run: Some("cargo check".into()),
                    ..Default::default()
                },
                TaskObject {
                    name: "verify".into(),
                    needs: Some(vec!["check".into()]),
                    env: Some(EnvDeclaration::Map(HashMap::from([(
                        "TOKEN".into(),
                        "top-secret".into(),
                    )]))),
                    steps: Some(vec![evin_config::models::StepObject {
                        run: None,
                        uses: Some(evin_yaml::Spanned::new(
                            "evin/docs@v1".into(),
                            evin_diagnostic::Span::default(),
                        )),
                        with: Some(evin_yaml::SpannedValue::Mapping(
                            vec![(
                                evin_yaml::SpannedValue::String(
                                    "token".into(),
                                    evin_diagnostic::Span::default(),
                                ),
                                evin_yaml::SpannedValue::String(
                                    "action-secret".into(),
                                    evin_diagnostic::Span::default(),
                                ),
                            )],
                            evin_diagnostic::Span::default(),
                        )),
                    }]),
                    ..Default::default()
                },
            ]),
            "verify",
        )
        .unwrap();
        assert_eq!(view.execution[0].name, "check");
        assert_eq!(view.execution[1].name, "verify");
        assert_eq!(view.target.environment, ["TOKEN"]);
        let rendered = evin_ui::discovery::render_explanation(&view, 80);
        assert!(!rendered.contains("top-secret"));
        assert!(!rendered.contains("action-secret"));
    }

    #[test]
    fn discovery_output_routes_terminal_and_tape_to_the_ui_stream() {
        assert_eq!(
            output_destination(true, false),
            OutputDestination::StyledStderr
        );
        assert_eq!(
            output_destination(false, true),
            OutputDestination::StyledStderr
        );
        assert_eq!(
            output_destination(false, false),
            OutputDestination::PlainStdout
        );
    }
}
