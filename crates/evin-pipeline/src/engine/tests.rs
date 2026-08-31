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

use super::*;
use crate::dag::DagResolver;
use crate::runner::LogMessage;
use evin_config::models::TaskObject;

use std::collections::HashMap;

struct NoopAction;

#[async_trait::async_trait]
impl evin_action_sdk::Action for NoopAction {
    fn validate_config(
        &self,
        _with: Option<&evin_yaml::SpannedValue>,
    ) -> Result<(), evin_yaml::ConfigError> {
        Ok(())
    }

    async fn execute(
        &self,
        _ctx: &evin_action_sdk::RunnerContext,
        _with: Option<evin_yaml::SpannedValue>,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        Ok(())
    }
}

struct ValuesAction;

#[async_trait::async_trait]
impl evin_action_sdk::Action for ValuesAction {
    fn validate_config(
        &self,
        _with: Option<&evin_yaml::SpannedValue>,
    ) -> Result<(), evin_yaml::ConfigError> {
        Ok(())
    }

    async fn execute(
        &self,
        ctx: &evin_action_sdk::RunnerContext,
        _with: Option<evin_yaml::SpannedValue>,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        assert_eq!(
            ctx.values.get("workspace_only").map(String::as_str),
            Some("yes")
        );
        assert_eq!(
            ctx.values.get("module_only").map(String::as_str),
            Some("yes")
        );
        assert_eq!(ctx.values.get("override").map(String::as_str), Some("task"));
        Ok(())
    }
}

struct GitRulesAction;

#[async_trait::async_trait]
impl evin_action_sdk::Action for GitRulesAction {
    fn validate_config(
        &self,
        _with: Option<&evin_yaml::SpannedValue>,
    ) -> Result<(), evin_yaml::ConfigError> {
        Ok(())
    }

    async fn execute(
        &self,
        ctx: &evin_action_sdk::RunnerContext,
        _with: Option<evin_yaml::SpannedValue>,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        let format = ctx
            .gitrules()
            .commits
            .as_ref()
            .unwrap()
            .message_format
            .as_ref()
            .unwrap()
            .iter()
            .next()
            .unwrap();
        assert_eq!(format, "{type}: {description}");
        Ok(())
    }
}

fn new_task(name: &str, needs: Option<Vec<&str>>, script: &str) -> TaskObject {
    TaskObject {
        name: name.to_string(),
        needs: needs.map(|v| v.into_iter().map(|s| s.to_string()).collect()),
        run: Some(script.to_string()),
        ..Default::default()
    }
}

#[tokio::test]
async fn test_executor_success() {
    let mut resolver = DagResolver::new();

    // A linear graph with simple shell commands
    #[cfg(unix)]
    let script = "exit 0";
    #[cfg(windows)]
    let script = "exit 0";

    resolver.add_task(new_task("A", None, script));
    resolver.add_task(new_task("B", Some(vec!["A"]), script));
    resolver.add_task(new_task("C", Some(vec!["B"]), script));

    let engine = PipelineEngine::new(
        false,
        evin_action_sdk::PipelineMode::Normal,
        HashMap::new(),
        vec![],
        None,
    );
    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let res = engine.execute(resolver, log_tx).await;

    assert!(res.error.is_none());
}

#[tokio::test]
async fn test_executor_fail_fast() {
    let mut resolver = DagResolver::new();

    // Task A fails immediately
    #[cfg(unix)]
    let script_fail = "exit 1";
    #[cfg(windows)]
    let script_fail = "exit 1";

    // Task B sleeps and should be aborted
    #[cfg(unix)]
    let script_sleep = "sleep 2";
    #[cfg(windows)]
    let script_sleep = "timeout 2";

    resolver.add_task(new_task("A_fail", None, script_fail));
    resolver.add_task(new_task("B_sleep", None, script_sleep));

    let engine = PipelineEngine::new(
        false,
        evin_action_sdk::PipelineMode::Normal,
        HashMap::new(),
        vec![],
        None,
    );

    let start = std::time::Instant::now();
    let (log_tx, mut log_rx) = tokio::sync::mpsc::unbounded_channel();
    let res = engine.execute(resolver, log_tx).await;
    let duration = start.elapsed();

    // The execution should fail immediately (<< 2 seconds)
    assert!(res.error.is_some());
    assert!(duration.as_secs() < 1);

    assert!(
        std::iter::from_fn(|| log_rx.try_recv().ok()).any(|message| {
            matches!(
                message,
                LogMessage::TaskFinished(task_name, outcome, _)
                    if task_name == "A_fail"
                        && outcome == evin_action_sdk::TaskOutcome::Failed
            )
        })
    );
}

#[tokio::test]
async fn test_executor_steps() {
    let mut resolver = DagResolver::new();

    let mut task_with_steps = new_task("StepsTask", None, "");
    task_with_steps.run = None;
    task_with_steps.steps = Some(vec![
        evin_config::models::StepObject {
            run: Some("echo step1".to_string()),
            uses: None,
            with: None,
        },
        evin_config::models::StepObject {
            run: Some("echo step2".to_string()),
            uses: None,
            with: None,
        },
    ]);

    resolver.add_task(task_with_steps);

    let engine = PipelineEngine::new(
        false,
        evin_action_sdk::PipelineMode::Normal,
        HashMap::new(),
        vec![],
        None,
    );
    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let res = engine.execute(resolver, log_tx).await;

    assert!(res.error.is_none());
}

#[tokio::test]
async fn test_executor_emits_final_task_error() {
    let mut resolver = DagResolver::new();

    #[cfg(unix)]
    let script = "exit 1";
    #[cfg(windows)]
    let script = "exit 1";

    resolver.add_task(new_task("failure", None, script));

    let engine = PipelineEngine::new(
        false,
        evin_action_sdk::PipelineMode::Normal,
        HashMap::new(),
        vec![],
        None,
    );
    let (log_tx, mut log_rx) = tokio::sync::mpsc::unbounded_channel();
    let result = engine.execute(resolver, log_tx).await;

    assert!(result.error.is_some());
    assert!(
        std::iter::from_fn(|| log_rx.try_recv().ok()).any(|message| {
            matches!(
                message,
                LogMessage::Diagnostic { task: task_name, diagnostic }
                    if task_name == "failure" && diagnostic.title == "Task Failed" && matches!(diagnostic.code, evin_diagnostic::IssueCode::TaskExitFailed { .. })
            )
        })
    );
}

#[tokio::test]
async fn dry_run_propagates_unverified_shell_outcomes() {
    let mut resolver = DagResolver::new();
    resolver.add_task(new_task("build", None, "create artifact"));

    let mut package = new_task("package", Some(vec!["build"]), "");
    package.run = None;
    package.steps = Some(vec![evin_config::models::StepObject {
        run: None,
        uses: Some(evin_yaml::Spanned::new(
            "noop".to_string(),
            evin_yaml::Span::default(),
        )),
        with: None,
    }]);
    resolver.add_task(package);

    let mut engine = PipelineEngine::new(
        false,
        evin_action_sdk::PipelineMode::DryRun,
        HashMap::new(),
        vec![],
        None,
    );
    engine.register_action("noop", Box::new(NoopAction));

    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let report = engine.execute(resolver, log_tx).await;

    assert_eq!(report.planned, 0);
    assert_eq!(report.unverified, 2);
    assert!(report.uncertainty.contains_key("build"));
    assert!(report.uncertainty.contains_key("package"));
}

#[tokio::test]
async fn dry_run_native_only_task_is_fully_planned() {
    let mut resolver = DagResolver::new();
    let mut task = new_task("native", None, "");
    task.run = None;
    task.steps = Some(vec![evin_config::models::StepObject {
        run: None,
        uses: Some(evin_yaml::Spanned::new(
            "noop".to_string(),
            evin_yaml::Span::default(),
        )),
        with: None,
    }]);
    resolver.add_task(task);

    let mut engine = PipelineEngine::new(
        false,
        evin_action_sdk::PipelineMode::DryRun,
        HashMap::new(),
        vec![],
        None,
    );
    engine.register_action("noop", Box::new(NoopAction));

    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let report = engine.execute(resolver, log_tx).await;

    assert_eq!(report.planned, 1);
    assert_eq!(report.unverified, 0);
    assert!(report.uncertainty.is_empty());
}

#[tokio::test]
async fn merges_workspace_module_and_task_values_for_native_actions() {
    let mut resolver = DagResolver::new();
    let mut task = new_task("values", None, "");
    task.run = None;
    task.module_values = Some(HashMap::from([
        ("module_only".to_string(), "yes".to_string()),
        ("override".to_string(), "module".to_string()),
    ]));
    task.values = Some(HashMap::from([(
        "override".to_string(),
        "task".to_string(),
    )]));
    task.steps = Some(vec![evin_config::models::StepObject {
        run: None,
        uses: Some(evin_yaml::Spanned::new(
            "values".to_string(),
            evin_yaml::Span::default(),
        )),
        with: None,
    }]);
    resolver.add_task(task);

    let mut engine = PipelineEngine::new(
        false,
        evin_action_sdk::PipelineMode::Normal,
        HashMap::new(),
        vec![],
        None,
    );
    engine.config.workspace_values = HashMap::from([
        ("workspace_only".to_string(), "yes".to_string()),
        ("override".to_string(), "workspace".to_string()),
    ]);
    engine.register_action("values", Box::new(ValuesAction));

    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let report = engine.execute(resolver, log_tx).await;

    assert_eq!(report.planned, 1);
}

#[tokio::test]
async fn test_gitrules_injected_into_context() {
    let mut resolver = DagResolver::new();
    let mut task = new_task("gitrules_test", None, "");
    task.run = None;
    task.steps = Some(vec![evin_config::models::StepObject {
        run: None,
        uses: Some(evin_yaml::Spanned::new(
            "gitrules_action".to_string(),
            evin_yaml::Span::default(),
        )),
        with: None,
    }]);
    resolver.add_task(task);

    let mut engine = PipelineEngine::new(
        false,
        evin_action_sdk::PipelineMode::Normal,
        HashMap::new(),
        vec![],
        None,
    );

    let rules = evin_gitrules::GitRulesRoot {
        version: 1,
        name: "test".to_string(),
        description: None,
        commits: Some(evin_gitrules::CommitsNamespace {
            message_format: Some(evin_gitrules::FormatList::Single(
                "{type}: {description}".to_string(),
            )),
            ignore_patterns: vec![],
            fields: HashMap::new(),
            policies: None,
        }),
        branches: None,
        releases: None,
        authorship: None,
        history: None,
        tags: None,
        pull_requests: None,
        files: None,
        lfs: None,
        submodules: None,
        remotes: None,
    };

    engine.config.gitrules = Some(std::sync::Arc::new(rules));
    engine.register_action("gitrules_action", Box::new(GitRulesAction));

    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let report = engine.execute(resolver, log_tx).await;

    assert_eq!(report.planned, 1);
}

#[tokio::test]
async fn test_parallel_task_cancellation_on_failure() {
    let mut resolver = DagResolver::new();
    let task_fail = new_task("fail_fast", None, "exit 1");
    let task_long = new_task("long_task", None, "sleep 10");

    resolver.add_task(task_fail);
    resolver.add_task(task_long);

    let mut engine = PipelineEngine::new(
        false,
        evin_action_sdk::PipelineMode::Normal,
        HashMap::new(),
        vec![],
        None,
    );
    engine.config.forced_execution = Some(evin_config::workspace::ExecutionMode::Parallel);

    let start = std::time::Instant::now();
    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let report = engine.execute(resolver, log_tx).await;

    assert!(report.error.is_some());
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
}

#[tokio::test]
async fn test_linear_task_execution_by_default() {
    let mut resolver = DagResolver::new();
    let task_a = new_task("task_a", None, "echo a");
    let task_b = new_task("task_b", None, "echo b");

    resolver.add_task(task_a);
    resolver.add_task(task_b);

    let engine = PipelineEngine::new(
        false,
        evin_action_sdk::PipelineMode::Normal,
        HashMap::new(),
        vec![],
        None,
    );

    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let report = engine.execute(resolver, log_tx).await;
    assert!(report.error.is_none());
}
