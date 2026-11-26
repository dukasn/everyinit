// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 of the License.

//! Side-effect-free facts and findings for an Evin workspace.

use crate::models::TaskObject;
use crate::parser::EverInitConfig;
use evin_action_sdk::Action;
use evin_diagnostic::{Diagnostic, IssueCode, Severity};
use evin_flow::{ConfigError, Span, SpannedValue};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisFinding {
    pub task_name: String,
    pub source_file: Option<String>,
    pub step_index: Option<usize>,
    pub action_id: Option<String>,
    pub diagnostic: Diagnostic,
}

pub struct WorkspaceAnalysis<'a> {
    pub config: &'a EverInitConfig,
    pub findings: Vec<AnalysisFinding>,
}

/// Analyze loaded configuration only. This function performs no process,
/// filesystem, Git, environment, or action execution I/O.
pub fn analyze<'a>(
    config: &'a EverInitConfig,
    actions: &HashMap<String, Arc<dyn Action>>,
) -> WorkspaceAnalysis<'a> {
    let mut findings = validate_task_contracts(config.tasks.iter(), actions);
    findings.extend(task_output_dependency_findings(config, &findings));
    findings.extend(package_policy_findings(config, actions));
    findings.sort_by(|left, right| {
        (&left.source_file, &left.task_name, left.step_index).cmp(&(
            &right.source_file,
            &right.task_name,
            right.step_index,
        ))
    });
    WorkspaceAnalysis { config, findings }
}

fn task_output_dependency_findings(
    config: &EverInitConfig,
    contract_findings: &[AnalysisFinding],
) -> Vec<AnalysisFinding> {
    fn references(value: &SpannedValue, result: &mut Vec<(String, String, Span)>) {
        match value {
            SpannedValue::String(text, span) => {
                if let Ok(references) = evin_flow::template::references(text) {
                    for reference in references {
                        if let evin_flow::template::TemplateReferenceKind::TaskOutput {
                            task,
                            key,
                        } = reference.kind
                        {
                            result.push((task, key, *span));
                        }
                    }
                }
            }
            SpannedValue::Sequence(values, _) => {
                for value in values {
                    references(value, result);
                }
            }
            SpannedValue::Mapping(values, _) => {
                for (key, value) in values {
                    references(key, result);
                    references(value, result);
                }
            }
            _ => {}
        }
    }

    let mut findings = Vec::new();
    for task in &config.tasks {
        for (step_index, step) in task.steps.iter().flatten().enumerate() {
            let Some(uses) = &step.uses else { continue };
            if contract_findings.iter().any(|finding| {
                finding.task_name == task.name && finding.step_index == Some(step_index)
            }) {
                continue;
            }
            let mut outputs = Vec::new();
            if let Some(with) = &step.with {
                references(with, &mut outputs);
            }
            let needs = task.needs.as_deref().unwrap_or_default();
            for (producer, key, span) in outputs {
                // Match RunnerContext's exact, namespace, and workspace aliases.
                let scoped = (!producer.contains(':') && task.name.contains(':'))
                    .then(|| format!("{}:{producer}", task.name.split(':').next().unwrap()));
                let root = producer.strip_prefix(&format!("{}:", config.workspace_name));
                if needs.contains(&producer)
                    || scoped.as_ref().is_some_and(|name| needs.contains(name))
                    || root.is_some_and(|name| needs.iter().any(|need| need == name))
                {
                    continue;
                }
                findings.push(AnalysisFinding {
                    task_name: task.name.clone(),
                    source_file: task.source_file.clone(),
                    step_index: Some(step_index),
                    action_id: Some(uses.as_str().to_owned()),
                    diagnostic: with_source_span(
                        evin_locales::diagnostic(
                            IssueCode::new("issue-task-output-missing-dependency")
                                .arg("task", &task.name)
                                .arg("producer", &producer)
                                .arg("reference", format!("${{{producer}.{key}}}")),
                        ),
                        task.source_file.as_deref(),
                        span,
                    ),
                });
            }
        }
    }
    findings
}

pub fn validate_task_contracts<'a>(
    tasks: impl IntoIterator<Item = &'a TaskObject>,
    actions: &HashMap<String, Arc<dyn Action>>,
) -> Vec<AnalysisFinding> {
    let mut tasks = tasks.into_iter().collect::<Vec<_>>();
    tasks.sort_by(|left, right| left.name.cmp(&right.name));
    let mut findings = Vec::new();
    for task in tasks {
        if task.run.is_some() && task.steps.is_some() {
            findings.push(AnalysisFinding {
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
            let action_id = uses.as_str().to_owned();
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
                findings.push(AnalysisFinding {
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

fn package_policy_findings(
    config: &EverInitConfig,
    actions: &HashMap<String, Arc<dyn Action>>,
) -> Vec<AnalysisFinding> {
    let policies = config
        .policy
        .as_ref()
        .and_then(|rules| rules.releases.as_ref())
        .map(|release| &release.packages);
    let mut findings = Vec::new();
    for task in &config.tasks {
        let values = merged_values(config, task);
        for (step_index, step) in task.steps.iter().flatten().enumerate() {
            let Some(action_id) = step.uses.as_ref().map(|uses| uses.as_str()) else {
                continue;
            };
            let Some(contract) = actions
                .get(action_id)
                .and_then(|action| action.package_release_contract())
            else {
                continue;
            };
            let Some(with) = step.with.as_ref() else {
                continue;
            };
            let Some(package_name) = mapping_string_path(with, contract.package_name_path) else {
                continue;
            };
            let resolved = resolve_value(package_name.0, &values);
            let diagnostic = match resolved {
                ResolvedValue::Known(name) => match policies {
                    Some(policies) if policies.contains_key(&name) => {
                        (!policies[&name]
                            .targets
                            .iter()
                            .any(|target| target == contract.target))
                        .then(|| {
                            diagnostic_at(
                                task,
                                package_name.1,
                                format!(
                                    "RepositoryPolicy package policy '{name}' does not declare the native package target '{}'",
                                    contract.target
                                ),
                                Severity::Error,
                            )
                        })
                    }
                    _ => Some(diagnostic_at(task, package_name.1, format!("native package '{name}' has no matching RepositoryPolicy release policy"), Severity::Error)),
                },
                ResolvedValue::Dynamic => Some(diagnostic_at(task, package_name.1, "native package identity cannot be proven statically because it uses environment or task output templates".to_string(), Severity::Info)),
            };
            if let Some(diagnostic) = diagnostic {
                findings.push(AnalysisFinding {
                    task_name: task.name.clone(),
                    source_file: task.source_file.clone(),
                    step_index: Some(step_index),
                    action_id: Some(action_id.to_owned()),
                    diagnostic,
                });
            }
        }
    }
    findings
}

fn merged_values(config: &EverInitConfig, task: &TaskObject) -> HashMap<String, String> {
    let mut values = config.workspace_values.clone();
    values.extend(task.module_values.clone().unwrap_or_default());
    values.extend(task.values.clone().unwrap_or_default());
    values
}
enum ResolvedValue {
    Known(String),
    Dynamic,
}
fn resolve_value(value: &str, values: &HashMap<String, String>) -> ResolvedValue {
    if let Some(name) = value
        .strip_prefix("${")
        .and_then(|value| value.strip_suffix('}'))
    {
        return values
            .get(name)
            .cloned()
            .map(ResolvedValue::Known)
            .unwrap_or(ResolvedValue::Dynamic);
    }
    if value.contains("${") {
        ResolvedValue::Dynamic
    } else {
        ResolvedValue::Known(value.to_owned())
    }
}
fn mapping_value<'a>(value: &'a SpannedValue, key: &str) -> Option<&'a SpannedValue> {
    let SpannedValue::Mapping(entries, _) = value else {
        return None;
    };
    entries
        .iter()
        .find_map(|(entry_key, entry_value)| match entry_key {
            SpannedValue::String(name, _) if name == key => Some(entry_value),
            _ => None,
        })
}
fn mapping_string<'a>(value: &'a SpannedValue, key: &str) -> Option<(&'a str, Span)> {
    let SpannedValue::Mapping(entries, _) = value else {
        return None;
    };
    entries
        .iter()
        .find_map(|(entry_key, entry_value)| match (entry_key, entry_value) {
            (SpannedValue::String(name, _), SpannedValue::String(value, span)) if name == key => {
                Some((value.as_str(), *span))
            }
            _ => None,
        })
}
fn mapping_string_path<'a>(value: &'a SpannedValue, path: &str) -> Option<(&'a str, Span)> {
    let mut parts = path.split('.');
    let mut current = value;
    while let Some(part) = parts.next() {
        if parts.clone().next().is_none() {
            return match current {
                SpannedValue::Mapping(_, _) => mapping_string(current, part),
                _ => None,
            };
        }
        current = mapping_value(current, part)?;
    }
    None
}
fn diagnostic_at(task: &TaskObject, span: Span, message: String, severity: Severity) -> Diagnostic {
    with_source_span(
        evin_locales::diagnostic(IssueCode::custom(message)).with_severity(severity),
        task.source_file.as_deref(),
        span,
    )
}
fn invalid_action_config_diagnostic(task: &TaskObject, error: ConfigError) -> Diagnostic {
    let title = task.description.as_deref().unwrap_or(task.name.as_str());
    let clean_error = error
        .message
        .replace("unknown field", "Unknown field")
        .replace("missing field", "Missing field")
        .replace("invalid type:", "Invalid type:")
        .replace(", expected one of", ".\nAllowed fields:")
        .replace(", expected", ".\nExpected:");
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
            evin_flow::template::pipeline::validate_template_syntax(text)
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
    use crate::parser::RepositoryPolicySource;
    use evin_action_sdk::{Action, PackageReleaseContract, RunnerContext};
    use evin_flow::{Span, Spanned};
    use std::collections::BTreeMap;

    struct Archive;
    const ARCH_CONTRACT: PackageReleaseContract = PackageReleaseContract {
        package_name_path: "package.name",
        target: "arch",
    };
    #[async_trait::async_trait]
    impl Action for Archive {
        fn handles_template_validation(&self) -> bool {
            true
        }
        fn validate_config(&self, _: Option<&SpannedValue>) -> Result<(), ConfigError> {
            Ok(())
        }
        fn package_release_contract(&self) -> Option<&'static PackageReleaseContract> {
            Some(&ARCH_CONTRACT)
        }
        async fn execute(
            &self,
            _: &RunnerContext,
            _: Option<SpannedValue>,
        ) -> Result<(), Diagnostic> {
            Ok(())
        }
    }

    fn config(name: &str) -> EverInitConfig {
        let with = evin_flow::parse_yaml(&format!("package:\n  name: {name}\n")).unwrap();
        let task = TaskObject {
            name: "packages:arch".into(),
            source_file: Some(".evin/packages.flow".into()),
            steps: Some(vec![crate::models::StepObject {
                run: None,
                uses: Some(Spanned {
                    value: "evin/package-arch@v1".into(),
                    span: Span::default(),
                }),
                with: Some(with),
            }]),
            ..Default::default()
        };
        EverInitConfig {
            workspace_name: "example".into(),
            workspace_version: "1.0.0".into(),
            stages: vec![],
            execution: None,
            tasks: vec![task],
            expose: vec![],
            workspace_env: HashMap::new(),
            workspace_values: HashMap::from([("package_name".into(), "app".into())]),
            modules: None,
            install_hint: None,
            hook_runner: None,
            policy: Some(std::sync::Arc::new(
                evin_repository_policy::RepositoryPolicy {
                    releases: Some(evin_repository_policy::ReleasesNamespace {
                        packages: BTreeMap::from([(
                            "app".into(),
                            evin_repository_policy::PackageReleasePolicy {
                                watch: vec!["src/**".into()],
                                targets: vec!["deb".into()],
                                revision: Default::default(),
                            },
                        )]),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )),
            policy_source: RepositoryPolicySource::Fallback,
            warnings: vec![],
            modules_discovered: vec![],
            module_versions: HashMap::new(),
        }
    }

    #[test]
    fn resolved_package_name_requires_matching_policy_target() {
        let actions = HashMap::from([(
            "evin/package-arch@v1".into(),
            std::sync::Arc::new(Archive) as std::sync::Arc<dyn Action>,
        )]);
        let findings = analyze(&config("${package_name}"), &actions).findings;
        assert_eq!(findings.len(), 1);
        assert!(findings[0].diagnostic.to_string().contains(
            "RepositoryPolicy package policy 'app' does not declare the native package target 'arch'"
        ));
    }

    #[test]
    fn dynamic_package_name_is_informational_not_an_error() {
        let actions = HashMap::from([(
            "evin/package-arch@v1".into(),
            std::sync::Arc::new(Archive) as std::sync::Arc<dyn Action>,
        )]);
        let findings = analyze(&config("${env.PACKAGE_NAME}"), &actions).findings;
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].diagnostic.severity, Severity::Info);
    }
    #[test]
    fn output_references_require_direct_dependencies_and_allow_runtime_aliases() {
        let actions = HashMap::from([(
            "evin/package-arch@v1".into(),
            Arc::new(Archive) as Arc<dyn Action>,
        )]);
        for (reference, needs, expected) in [
            ("${build.version}", vec![], 1),
            ("${build.version}", vec!["middle"], 1),
            ("${build.version}", vec!["build"], 0),
            ("${build.version}", vec!["packages:build"], 0),
            ("${release:bump.version}", vec!["release:changelog"], 1),
            ("${release:bump.version}", vec!["release:bump"], 0),
            ("${example:build.version}", vec!["build"], 0),
            ("${env.VERSION}", vec![], 0),
            ("${version}", vec![], 0),
        ] {
            let mut config = config("app");
            config.tasks[0].needs = Some(needs.into_iter().map(str::to_owned).collect());
            config.tasks[0].steps.as_mut().unwrap()[0].with = Some(
                evin_flow::parse_yaml(&format!("nested:\n  values: ['{reference}']\n")).unwrap(),
            );
            let findings = analyze(&config, &actions).findings;
            let missing = findings
                .iter()
                .filter(|finding| {
                    finding.diagnostic.code.key == "issue-task-output-missing-dependency"
                })
                .collect::<Vec<_>>();
            assert_eq!(missing.len(), expected, "{reference}");
            if expected > 0 {
                assert_eq!(missing[0].diagnostic.severity, Severity::Error);
                assert!(missing[0].diagnostic.location.is_some());
                assert_eq!(missing[0].step_index, Some(0));
            }
        }
    }
}
