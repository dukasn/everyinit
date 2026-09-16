// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

use evin_action_sdk::{Action, InvocationFlags};
use evin_config::models::TaskObject;
use evin_diagnostic::{Diagnostic, IssueCode};
use evin_yaml::{ConfigError, Span, SpannedValue};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InvocationResolution {
    pub action_flags: HashMap<String, InvocationFlags>,
    pub hook_payload: Vec<String>,
}

#[allow(clippy::result_large_err)]
pub fn resolve_invocation_flags<'a>(
    tasks: impl IntoIterator<Item = &'a TaskObject>,
    actions: &HashMap<String, Arc<dyn Action>>,
    payload: &[String],
) -> Result<InvocationResolution, Diagnostic> {
    let action_ids = tasks
        .into_iter()
        .flat_map(|task| task.steps.iter().flatten())
        .filter_map(|step| step.uses.as_ref().map(|uses| uses.as_str().to_string()))
        .collect::<BTreeSet<_>>();

    let mut owners = BTreeMap::new();
    for action_id in action_ids {
        let Some(action) = actions.get(&action_id) else {
            continue;
        };
        for flag in action.invocation_flags() {
            if let Some(existing) = owners.insert(flag.name(), action_id.clone())
                && existing != action_id
            {
                return Err(evin_locales::diagnostic(
                    IssueCode::new("issue-invocation-flag-ambiguous")
                        .arg("flag", format!("--{}", flag.name()))
                        .arg("first_action", existing)
                        .arg("second_action", action_id),
                ));
            }
        }
    }

    let mut resolution = InvocationResolution::default();
    for value in payload {
        let Some(flag) = value.strip_prefix("--") else {
            resolution.hook_payload.push(value.clone());
            continue;
        };
        let Some(action_id) = owners.get(flag) else {
            return Err(evin_locales::diagnostic(
                IssueCode::new("issue-invocation-flag-unknown").arg("flag", value),
            ));
        };
        resolution
            .action_flags
            .entry(action_id.clone())
            .or_default()
            .insert(flag);
    }
    Ok(resolution)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionValidationFinding {
    pub task_name: String,
    pub source_file: Option<String>,
    pub step_index: Option<usize>,
    pub action_id: Option<String>,
    pub diagnostic: Diagnostic,
}

pub fn validate_task_contracts<'a>(
    tasks: impl IntoIterator<Item = &'a TaskObject>,
    actions: &HashMap<String, Arc<dyn Action>>,
) -> Vec<ActionValidationFinding> {
    let mut tasks = tasks.into_iter().collect::<Vec<_>>();
    tasks.sort_by(|left, right| left.name.cmp(&right.name));

    let mut findings = Vec::new();
    for task in tasks {
        if task.run.is_some() && task.steps.is_some() {
            findings.push(ActionValidationFinding {
                task_name: task.name.clone(),
                source_file: task.source_file.clone(),
                step_index: None,
                action_id: None,
                diagnostic: evin_locales::diagnostic(
                    IssueCode::new("issue-task-defines-run-and-steps").arg("task_name", &task.name),
                ),
            });
            continue;
        }

        let Some(steps) = &task.steps else {
            continue;
        };
        for (step_index, step) in steps.iter().enumerate() {
            let Some(uses) = &step.uses else {
                continue;
            };
            let action_id = uses.as_str().to_string();
            let diagnostic = match actions.get(uses.as_str()) {
                Some(action) => match action.validate_task_config(
                    &evin_action_sdk::ActionValidationContext {
                        task_name: &task.name,
                        needs: task.needs.as_deref().unwrap_or_default(),
                    },
                    step.with.as_ref(),
                ) {
                    Ok(()) => (!action.handles_template_validation())
                        .then(|| first_template_error(step.with.as_ref()))
                        .flatten()
                        .map(|error| invalid_action_config_diagnostic(task, error)),
                    Err(error) => Some(invalid_action_config_diagnostic(task, error)),
                },
                None => Some(with_source_span(
                    evin_locales::diagnostic(
                        IssueCode::new("issue-unknown-native-action").arg("action", uses.as_str()),
                    ),
                    task.source_file.as_deref(),
                    uses.span(),
                )),
            };

            if let Some(diagnostic) = diagnostic {
                findings.push(ActionValidationFinding {
                    task_name: task.name.clone(),
                    source_file: task.source_file.clone(),
                    step_index: Some(step_index),
                    action_id: Some(action_id),
                    diagnostic,
                });
            }
        }
    }
    findings
}

fn invalid_action_config_diagnostic(task: &TaskObject, error: ConfigError) -> Diagnostic {
    let clean_error = error
        .message
        .replace("unknown field", "Unknown field")
        .replace("missing field", "Missing field")
        .replace("invalid type:", "Invalid type:")
        .replace(", expected one of", ".\nAllowed fields:")
        .replace(", expected", ".\nExpected:");
    let title = task.description.as_deref().unwrap_or(task.name.as_str());
    let diagnostic = evin_locales::diagnostic(
        IssueCode::new("issue-action-config-invalid")
            .arg("task", title)
            .arg("clean_error", clean_error),
    );
    match error.span {
        Some(span) => with_source_span(diagnostic, task.source_file.as_deref(), span),
        None => diagnostic,
    }
}

fn with_source_span(diagnostic: Diagnostic, source_file: Option<&str>, span: Span) -> Diagnostic {
    let Some(source_file) = source_file else {
        return diagnostic;
    };
    if span.length > 1 {
        diagnostic.with_file_span(source_file, span.offset, span.length, None)
    } else {
        diagnostic.with_file_location(source_file, Some(span.line), Some(span.column))
    }
}

fn first_template_error(value: Option<&SpannedValue>) -> Option<ConfigError> {
    let value = value?;
    match value {
        SpannedValue::String(text, span) => {
            evin_yaml::template::pipeline::validate_template_syntax(text)
                .err()
                .map(|error| ConfigError {
                    message: error.to_string(),
                    span: Some(*span),
                })
        }
        SpannedValue::Sequence(values, _) => values
            .iter()
            .find_map(|value| first_template_error(Some(value))),
        SpannedValue::Mapping(values, _) => values.iter().find_map(|(key, value)| {
            first_template_error(Some(key)).or_else(|| first_template_error(Some(value)))
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use evin_action_sdk::{InvocationFlag, RunnerContext};
    use evin_config::models::StepObject;
    use serde::Deserialize;

    struct StrictAction;

    struct HostAction;
    struct DuplicateHostAction;

    const HOST_FLAG: &[InvocationFlag] = &[InvocationFlag::new("host")];

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StrictConfig {
        required: String,
    }

    #[async_trait]
    impl Action for StrictAction {
        fn validate_config(
            &self,
            with: Option<&SpannedValue>,
        ) -> Result<(), evin_yaml::ConfigError> {
            evin_action_sdk::parse_config::<StrictConfig>(with).map(|config| {
                let _ = config.required;
            })
        }

        async fn execute(
            &self,
            _ctx: &RunnerContext,
            _with: Option<SpannedValue>,
        ) -> Result<(), Diagnostic> {
            Ok(())
        }
    }

    #[async_trait]
    impl Action for HostAction {
        fn invocation_flags(&self) -> &'static [InvocationFlag] {
            HOST_FLAG
        }

        fn validate_config(
            &self,
            _with: Option<&SpannedValue>,
        ) -> Result<(), evin_yaml::ConfigError> {
            Ok(())
        }

        async fn execute(
            &self,
            _ctx: &RunnerContext,
            _with: Option<SpannedValue>,
        ) -> Result<(), Diagnostic> {
            Ok(())
        }
    }

    #[async_trait]
    impl Action for DuplicateHostAction {
        fn invocation_flags(&self) -> &'static [InvocationFlag] {
            HOST_FLAG
        }

        fn validate_config(
            &self,
            _with: Option<&SpannedValue>,
        ) -> Result<(), evin_yaml::ConfigError> {
            Ok(())
        }

        async fn execute(
            &self,
            _ctx: &RunnerContext,
            _with: Option<SpannedValue>,
        ) -> Result<(), Diagnostic> {
            Ok(())
        }
    }

    fn registry() -> HashMap<String, Arc<dyn Action>> {
        HashMap::from([(
            "test/strict@v1".to_string(),
            Arc::new(StrictAction) as Arc<dyn Action>,
        )])
    }

    fn action_task(name: &str, yaml: &str) -> TaskObject {
        TaskObject {
            name: name.into(),
            source_file: Some(".evin/workspace.yaml".into()),
            steps: Some(vec![StepObject {
                run: None,
                uses: Some(evin_yaml::Spanned::new(
                    "test/strict@v1".into(),
                    Span::default(),
                )),
                with: Some(evin_yaml::parse_yaml(yaml).unwrap()),
            }]),
            ..Default::default()
        }
    }

    fn invocation_task(name: &str, action: &str) -> TaskObject {
        TaskObject {
            name: name.into(),
            steps: Some(vec![StepObject {
                run: None,
                uses: Some(evin_yaml::Spanned::new(action.into(), Span::default())),
                with: None,
            }]),
            ..Default::default()
        }
    }

    #[test]
    fn collects_findings_in_deterministic_task_order() {
        let second = action_task("z-task", "typo: value\n");
        let first = action_task("a-task", "wrong: value\n");

        let findings = validate_task_contracts([&second, &first], &registry());

        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].task_name, "a-task");
        assert_eq!(findings[1].task_name, "z-task");
        assert_eq!(findings[0].step_index, Some(0));
    }

    #[test]
    fn unknown_field_keeps_the_key_span() {
        let task = action_task("build", "typo: value\n");

        let finding = validate_task_contracts([&task], &registry()).remove(0);

        assert_eq!(finding.diagnostic.code.key, "issue-action-config-invalid");
        assert!(matches!(
            finding.diagnostic.location,
            Some(evin_diagnostic::FileLocation::ByteSpan {
                offset: 0,
                length: 4,
                ..
            })
        ));
    }

    #[test]
    fn schema_failure_suppresses_a_second_template_finding_for_the_step() {
        let task = action_task("build", "typo: '{{ old.syntax }}'\n");

        let findings = validate_task_contracts([&task], &registry());

        assert_eq!(findings.len(), 1);
        assert!(
            findings[0]
                .diagnostic
                .code
                .args
                .iter()
                .any(|(name, value)| *name == "clean_error" && value.contains("Unknown field"))
        );
    }

    #[test]
    fn unknown_actions_are_located_at_the_uses_value() {
        let mut task = action_task("build", "required: value\n");
        task.steps.as_mut().unwrap()[0].uses = Some(evin_yaml::Spanned::new(
            "test/missing@v1".into(),
            Span {
                line: 8,
                column: 15,
                offset: 90,
                length: 15,
            },
        ));

        let finding = validate_task_contracts([&task], &registry()).remove(0);

        assert_eq!(finding.diagnostic.code.key, "issue-unknown-native-action");
        assert!(finding.diagnostic.location.is_some());
    }

    #[test]
    fn missing_and_invalidly_typed_options_are_rejected() {
        let missing = action_task("missing", "{}\n");
        let invalid_type = action_task("invalid-type", "required: [value]\n");

        let findings = validate_task_contracts([&missing, &invalid_type], &registry());

        assert_eq!(findings.len(), 2);
        let messages = findings
            .iter()
            .flat_map(|finding| finding.diagnostic.code.args.iter())
            .filter(|(name, _)| *name == "clean_error")
            .map(|(_, value)| value.as_str())
            .collect::<Vec<_>>();
        assert!(
            messages
                .iter()
                .any(|message| message.contains("Missing field"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("Invalid type"))
        );
    }

    #[test]
    fn malformed_templates_are_reported_after_schema_validation() {
        let task = action_task("build", "required: '{{ old.syntax }}'\n");

        let findings = validate_task_contracts([&task], &registry());

        assert_eq!(findings.len(), 1);
        assert!(findings[0].diagnostic.location.is_some());
    }

    #[test]
    fn tasks_cannot_define_run_and_steps_together() {
        let mut task = action_task("build", "required: value\n");
        task.run = Some("true".into());

        let findings = validate_task_contracts([&task], &registry());

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].step_index, None);
        assert_eq!(
            findings[0].diagnostic.code.key,
            "issue-task-defines-run-and-steps"
        );
    }

    #[test]
    fn invocation_flags_are_routed_only_to_the_declaring_action() {
        let task = invocation_task("docs", "test/docs@v1");
        let actions = HashMap::from([(
            "test/docs@v1".to_string(),
            Arc::new(HostAction) as Arc<dyn Action>,
        )]);

        let resolution = resolve_invocation_flags(
            [&task],
            &actions,
            &["--host".to_string(), ".git/COMMIT_EDITMSG".to_string()],
        )
        .unwrap();

        assert!(resolution.action_flags["test/docs@v1"].contains("host"));
        assert_eq!(resolution.hook_payload, [".git/COMMIT_EDITMSG"]);
    }

    #[test]
    fn unknown_invocation_flags_fail_before_execution() {
        let task = invocation_task("docs", "test/docs@v1");
        let actions = HashMap::from([(
            "test/docs@v1".to_string(),
            Arc::new(HostAction) as Arc<dyn Action>,
        )]);

        let error =
            resolve_invocation_flags([&task], &actions, &["--watch".to_string()]).unwrap_err();
        assert_eq!(error.code.key, "issue-invocation-flag-unknown");
    }

    #[test]
    fn duplicate_invocation_flag_declarations_are_rejected() {
        let docs = invocation_task("docs", "test/docs@v1");
        let other = invocation_task("other", "test/other@v1");
        let actions = HashMap::from([
            (
                "test/docs@v1".to_string(),
                Arc::new(HostAction) as Arc<dyn Action>,
            ),
            (
                "test/other@v1".to_string(),
                Arc::new(DuplicateHostAction) as Arc<dyn Action>,
            ),
        ]);

        let error = resolve_invocation_flags([&docs, &other], &actions, &[]).unwrap_err();
        assert_eq!(error.code.key, "issue-invocation-flag-ambiguous");
    }
}
