// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

use evin_action_sdk::{Action, InvocationFlags};
use evin_config::models::TaskObject;
use evin_diagnostic::{Diagnostic, IssueCode};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

#[cfg(test)]
use evin_yaml::{Span, SpannedValue};

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
    evin_config::analysis::validate_task_contracts(tasks, actions)
        .into_iter()
        .map(|finding| ActionValidationFinding {
            task_name: finding.task_name,
            source_file: finding.source_file,
            step_index: finding.step_index,
            action_id: finding.action_id,
            diagnostic: finding.diagnostic,
        })
        .collect()
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
            source_file: Some(".evin/workspace.flow".into()),
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
