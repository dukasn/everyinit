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

#![allow(clippy::result_large_err)]

use crate::models::TaskObject;
use crate::workspace::{ModuleYaml, WorkspaceYaml};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitRulesSource {
    Local(String),
    Fallback,
}

pub struct EverInitConfig {
    pub workspace_name: String,
    pub workspace_version: String,
    pub stages: Vec<String>,
    pub execution: Option<crate::workspace::ExecutionMode>,
    pub tasks: Vec<TaskObject>,
    pub expose: Vec<String>,
    pub workspace_env: HashMap<String, String>,
    pub workspace_values: HashMap<String, String>,
    pub modules: Option<Vec<String>>,
    pub install_hint: Option<String>,
    pub hook_runner: Option<crate::workspace::HookRunner>,
    pub gitrules: Option<std::sync::Arc<evin_gitrules::GitRulesRoot>>,
    pub gitrules_source: GitRulesSource,
    pub warnings: Vec<String>,
    pub modules_discovered: Vec<(String, String, usize)>,
    pub module_versions: HashMap<String, String>,
    pub package_manifests: std::sync::Arc<evin_action_sdk::packaging::PackageManifestCatalog>,
}

fn resolved_module_version(
    module: &crate::workspace::ModuleConfig,
    workspace_version: &str,
) -> String {
    module
        .version
        .clone()
        .unwrap_or_else(|| workspace_version.to_string())
}

#[derive(Default, Clone, Copy)]
pub struct TaskParseContext<'a> {
    pub namespace: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub source_file: Option<&'a str>,
    pub module_env: Option<&'a HashMap<String, String>>,
    pub module_values: Option<&'a HashMap<String, String>>,
    pub valid_stages: &'a [evin_yaml::Spanned<String>],
    pub workspace_name: Option<&'a str>,
}

pub fn parse_tasks(
    tasks: HashMap<evin_yaml::Spanned<String>, TaskObject>,
    cx: TaskParseContext<'_>,
) -> Result<Vec<TaskObject>, evin_diagnostic::Diagnostic> {
    let mut parsed_tasks = Vec::new();
    let local_task_names: std::collections::HashSet<String> =
        tasks.keys().map(|k| k.value.clone()).collect();

    for (task_name, mut task) in tasks {
        let config_path = cx
            .source_file
            .map(|s| s.to_string())
            .or_else(|| cx.cwd.map(|p| format!("{}/.evin/module.yaml", p)))
            .unwrap_or_else(|| ".evin/workspace.yaml".to_string());

        task.source_file = Some(config_path.clone());

        // Validate task name format: ^[a-z0-9-]+$
        let is_valid_task_name = !task_name.value.is_empty()
            && task_name
                .value
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');

        if !is_valid_task_name {
            return Err(evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-invalid-task-casing")
                    .arg("task_name", &task_name.value),
            )
            .with_file_span(
                config_path,
                task_name.span.offset,
                task_name.span.length.max(1),
                None,
            ));
        }

        // 1. Assign fully qualified name
        task.name = if let Some(ns) = cx.namespace {
            format!("{}:{}", ns, task_name.value)
        } else {
            task_name.value.clone()
        };

        // 2. Namespace dependencies (needs)
        if let Some(needs) = &mut task.needs {
            for dep in needs.iter_mut() {
                // Rule 1: Explicit workspace reference via workspace name prefix (e.g. "evin-monorepo:build")
                if let Some(ws_name) = cx.workspace_name {
                    let ws_prefix = format!("{}:", ws_name);
                    if let Some(bare) = dep.strip_prefix(&ws_prefix) {
                        *dep = bare.to_string();
                        continue;
                    }
                }
                // Rule 2: Auto-qualify if name is local to this module
                if let Some(ns) = cx.namespace
                    && local_task_names.contains(dep.as_str())
                {
                    *dep = format!("{}:{}", ns, dep);
                }
                // Rule 3: Leave as-is (global fallthrough)
            }
        }

        // 3. Inherit defaults if missing
        if task.cwd.is_none()
            && let Some(dir) = cx.cwd
        {
            task.cwd = Some(dir.to_string());
        }

        if task.module_env.is_none()
            && let Some(env) = cx.module_env
        {
            task.module_env = Some(env.clone());
        }

        if task.module_values.is_none()
            && let Some(values) = cx.module_values
        {
            task.module_values = Some(values.clone());
        }

        // 4. Validate stage
        if let Some(stage) = &task.stage
            && !cx.valid_stages.is_empty()
            && !cx.valid_stages.iter().any(|s| &s.value == stage)
        {
            let stages_str = format!(
                "{:?}",
                cx.valid_stages
                    .iter()
                    .map(|s| s.value.clone())
                    .collect::<Vec<_>>()
            );
            return Err(evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-invalid-stage")
                    .arg("task_name", &task.name)
                    .arg("stage", stage)
                    .arg("valid_stages", stages_str),
            ));
        }

        if let Some(watch) = &task.watch
            && let Err(message) = watch.validate()
        {
            return Err(evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-watch-config-invalid")
                    .arg("task", &task.name)
                    .arg("error", message),
            )
            .with_file_location(config_path, None, None));
        }

        parsed_tasks.push(task);
    }

    Ok(parsed_tasks)
}

pub async fn load_workspace_tasks() -> Result<EverInitConfig, evin_diagnostic::Diagnostic> {
    let workspace_path = std::path::PathBuf::from(".evin/workspace.yaml");

    if !workspace_path.exists() {
        return Ok(EverInitConfig {
            workspace_name: "".to_string(),
            workspace_version: "".to_string(),
            stages: Vec::new(),
            execution: None,
            tasks: Vec::new(),
            expose: Vec::new(),
            workspace_env: HashMap::new(),
            workspace_values: HashMap::new(),
            modules: None,
            install_hint: None,
            hook_runner: None,
            gitrules: None,
            gitrules_source: GitRulesSource::Fallback,
            warnings: Vec::new(),
            modules_discovered: Vec::new(),
            module_versions: HashMap::new(),
            package_manifests: std::sync::Arc::new(
                evin_action_sdk::packaging::PackageManifestCatalog::default(),
            ),
        });
    }

    let workspace = <WorkspaceYaml as evin_yaml::YamlDocument>::from_file(&workspace_path)?;

    let mut gitrules = None;
    let mut gitrules_source = GitRulesSource::Fallback;
    let mut warnings = Vec::new();

    if let Some(rules_ref) = &workspace.workspace.rules {
        if rules_ref.starts_with("http://") || rules_ref.starts_with("https://") {
            warnings.push(format!(
                "remote rules URL '{}' is not supported; only local schemas are allowed",
                rules_ref
            ));
        } else {
            let local_rules_path = std::path::PathBuf::from(rules_ref);
            if local_rules_path.exists() {
                match evin_gitrules::parse_file(&local_rules_path) {
                    Ok(rules) => {
                        gitrules = Some(std::sync::Arc::new(rules));
                        gitrules_source = GitRulesSource::Local(rules_ref.clone());
                    }
                    Err(e) => {
                        if let evin_gitrules::ParserError::Diagnostic(d) = e {
                            return Err(d);
                        }
                        return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(
                            e.to_string(),
                        ))
                        .with_file_location(
                            local_rules_path.to_string_lossy().to_string(),
                            None,
                            None,
                        ));
                    }
                }
            } else {
                // Retain the configured source even before it exists so watch
                // mode can observe its creation and re-resolve the rules.
                gitrules_source = GitRulesSource::Local(rules_ref.clone());
                warnings.push(format!("local rules path '{}' does not exist", rules_ref));
            }
        }
    }

    let mut global_tasks = Vec::new();
    let expose = workspace.expose.clone();
    let workspace_env = workspace.env.clone();
    let workspace_values = workspace.values.clone();
    let valid_stages = workspace.stages.clone();
    for stage in &valid_stages {
        let is_valid_stage = !stage.value.is_empty()
            && stage.value.chars().next().unwrap().is_ascii_uppercase()
            && stage
                .value
                .chars()
                .skip(1)
                .all(|c| c.is_ascii_alphanumeric());

        if !is_valid_stage {
            return Err(evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-invalid-stage-casing")
                    .arg("stage", &stage.value),
            )
            .with_file_span(
                workspace_path.to_string_lossy().to_string(),
                stage.span.offset,
                stage.span.length.max(1),
                None,
            ));
        }
    }

    if let Some(tasks) = workspace.tasks {
        global_tasks.extend(parse_tasks(
            tasks,
            TaskParseContext {
                valid_stages: &valid_stages,
                workspace_name: Some(&workspace.workspace.name),
                ..Default::default()
            },
        )?);
    }

    let mut modules_discovered = Vec::new();
    let mut module_versions = HashMap::new();

    if let Some(ref include) = workspace.workspace.include {
        let includes: Vec<(Option<String>, String)> = match include {
            crate::workspace::IncludeConfig::List(list) => {
                list.iter().map(|path| (None, path.clone())).collect()
            }
            crate::workspace::IncludeConfig::Map(map) => {
                let mut entries: Vec<_> = map
                    .iter()
                    .map(|(k, v)| (Some(k.clone()), v.clone()))
                    .collect();
                entries.sort_by(|a, b| a.1.cmp(&b.1));
                entries
            }
        };

        for (explicit_name, file_path) in includes {
            let ns_path = std::path::PathBuf::from(&file_path);
            if !ns_path.exists() {
                return Err(evin_locales::diagnostic(
                    evin_diagnostic::IssueCode::new("issue-file-parse-failed")
                        .arg("file_path", &file_path)
                        .arg("error", "included namespace file does not exist"),
                )
                .with_file_location(&file_path, None, None));
            }

            let namespace_doc =
                <crate::workspace::NamespaceYaml as evin_yaml::YamlDocument>::from_file(&ns_path)?;

            let ns_name = explicit_name.unwrap_or(namespace_doc.namespace.name);

            let mut merged_values = namespace_doc.values;
            if let Some(ns_values) = namespace_doc.namespace.values {
                merged_values.extend(ns_values);
            }

            if let Some(tasks) = namespace_doc.tasks {
                let parsed = parse_tasks(
                    tasks,
                    TaskParseContext {
                        namespace: Some(&ns_name),
                        source_file: Some(&file_path),
                        module_env: namespace_doc.namespace.env.as_ref(),
                        module_values: Some(&merged_values),
                        valid_stages: &valid_stages,
                        workspace_name: Some(&workspace.workspace.name),
                        ..Default::default()
                    },
                )?;
                let count = parsed.len();
                global_tasks.extend(parsed);
                modules_discovered.push((ns_name, ns_path.display().to_string(), count));
            }
        }
    }

    if let Some(ref modules) = workspace.workspace.modules {
        for module_dir in modules {
            let mod_path = std::path::PathBuf::from(&module_dir).join(".evin/module.yaml");
            if !mod_path.exists() {
                continue;
            }

            let module = <ModuleYaml as evin_yaml::YamlDocument>::from_file(&mod_path)?;
            module_versions.insert(
                module.module.name.clone(),
                resolved_module_version(&module.module, &workspace.workspace.version),
            );

            if let Some(tasks) = module.tasks {
                let parsed = parse_tasks(
                    tasks,
                    TaskParseContext {
                        namespace: Some(&module.module.name),
                        cwd: Some(module_dir),
                        source_file: Some(&mod_path.display().to_string()),
                        module_env: module.module.env.as_ref(),
                        module_values: Some(&module.values),
                        valid_stages: &valid_stages,
                        workspace_name: Some(&workspace.workspace.name),
                    },
                )?;
                let count = parsed.len();
                global_tasks.extend(parsed);
                modules_discovered.push((
                    module.module.name.clone(),
                    mod_path.display().to_string(),
                    count,
                ));
            }

            if let Some(ref include) = module.module.include {
                let includes: Vec<(Option<String>, String)> = match include {
                    crate::workspace::IncludeConfig::List(list) => {
                        list.iter().map(|path| (None, path.clone())).collect()
                    }
                    crate::workspace::IncludeConfig::Map(map) => {
                        let mut entries: Vec<_> = map
                            .iter()
                            .map(|(k, v)| (Some(k.clone()), v.clone()))
                            .collect();
                        entries.sort_by(|a, b| a.1.cmp(&b.1));
                        entries
                    }
                };

                for (explicit_name, file_path) in includes {
                    let ns_path = if std::path::Path::new(&file_path).is_absolute() {
                        std::path::PathBuf::from(&file_path)
                    } else {
                        std::path::PathBuf::from(module_dir).join(&file_path)
                    };

                    if !ns_path.exists() {
                        return Err(evin_locales::diagnostic(
                            evin_diagnostic::IssueCode::new("issue-file-parse-failed")
                                .arg("file_path", &file_path)
                                .arg("error", "included namespace file does not exist"),
                        )
                        .with_file_location(&file_path, None, None));
                    }

                    let namespace_doc =
                        <crate::workspace::NamespaceYaml as evin_yaml::YamlDocument>::from_file(
                            &ns_path,
                        )?;

                    let raw_ns_name = explicit_name.unwrap_or(namespace_doc.namespace.name);
                    let full_ns_name = if raw_ns_name == module.module.name {
                        module.module.name.clone()
                    } else if raw_ns_name.starts_with(&format!("{}:", module.module.name)) {
                        raw_ns_name
                    } else {
                        format!("{}:{}", module.module.name, raw_ns_name)
                    };

                    let mut combined_env = module.module.env.clone().unwrap_or_default();
                    if let Some(ns_env) = &namespace_doc.namespace.env {
                        combined_env.extend(ns_env.clone());
                    }

                    let mut combined_values = module.values.clone();
                    combined_values.extend(namespace_doc.values);
                    if let Some(ns_vals) = namespace_doc.namespace.values {
                        combined_values.extend(ns_vals);
                    }

                    if let Some(tasks) = namespace_doc.tasks {
                        let parsed = parse_tasks(
                            tasks,
                            TaskParseContext {
                                namespace: Some(&full_ns_name),
                                cwd: Some(module_dir),
                                source_file: Some(&ns_path.display().to_string()),
                                module_env: if combined_env.is_empty() {
                                    None
                                } else {
                                    Some(&combined_env)
                                },
                                module_values: Some(&combined_values),
                                valid_stages: &valid_stages,
                                workspace_name: Some(&workspace.workspace.name),
                            },
                        )?;
                        let count = parsed.len();
                        global_tasks.extend(parsed);
                        modules_discovered.push((
                            full_ns_name,
                            ns_path.display().to_string(),
                            count,
                        ));
                    }
                }
            }
        }
    }

    let workspace_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let package_manifests = crate::package::load_package_catalog(
        &workspace_root,
        workspace.workspace.modules.as_ref(),
    )?;

    Ok(EverInitConfig {
        workspace_name: workspace.workspace.name.clone(),
        workspace_version: workspace.workspace.version.clone(),
        stages: valid_stages
            .iter()
            .map(|stage| stage.value.clone())
            .collect(),
        execution: workspace.workspace.execution,
        tasks: global_tasks,
        expose,
        workspace_env,
        workspace_values,
        modules: workspace.workspace.modules,
        install_hint: workspace.workspace.install_hint,
        hook_runner: workspace.workspace.hook_runner,
        gitrules,
        gitrules_source,
        warnings,
        modules_discovered,
        module_versions,
        package_manifests,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use evin_yaml::Spanned;

    #[test]
    fn module_version_inherits_or_overrides_workspace_version() {
        let inherited = crate::workspace::ModuleConfig {
            name: "inherited".to_string(),
            version: None,
            env: None,
            include: None,
        };
        let independent = crate::workspace::ModuleConfig {
            name: "independent".to_string(),
            version: Some("2.0.0".to_string()),
            env: None,
            include: None,
        };

        assert_eq!(resolved_module_version(&inherited, "1.2.3"), "1.2.3");
        assert_eq!(resolved_module_version(&independent, "1.2.3"), "2.0.0");
    }

    #[test]
    fn test_parse_tasks_empty() {
        let tasks = HashMap::new();
        let result = parse_tasks(tasks, TaskParseContext::default());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_tasks_simple() {
        let mut tasks = HashMap::new();
        let key = Spanned {
            value: "build".to_string(),
            span: evin_diagnostic::span::Span::default(),
        };
        let task = TaskObject {
            name: "build".to_string(),
            run: Some("echo hello".to_string()),
            ..Default::default()
        };
        tasks.insert(key, task);

        let result = parse_tasks(tasks, TaskParseContext::default());
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "build");
    }

    #[test]
    fn test_parse_tasks_invalid_casing() {
        let mut tasks = HashMap::new();
        let key = Spanned {
            value: "BuildTask".to_string(),
            span: evin_diagnostic::span::Span::default(),
        };
        let task = TaskObject {
            name: "BuildTask".to_string(),
            run: Some("echo hello".to_string()),
            ..Default::default()
        };
        tasks.insert(key, task);

        let result = parse_tasks(tasks, TaskParseContext::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_tasks_invalid_stage() {
        let mut tasks = HashMap::new();
        let key = Spanned {
            value: "build".to_string(),
            span: evin_diagnostic::span::Span::default(),
        };
        let task = TaskObject {
            name: "build".to_string(),
            stage: Some("nonexistent_stage".to_string()),
            run: Some("echo hello".to_string()),
            ..Default::default()
        };
        tasks.insert(key, task);

        let valid_stages = vec![Spanned {
            value: "build_stage".to_string(),
            span: evin_diagnostic::span::Span::default(),
        }];

        let result = parse_tasks(
            tasks,
            TaskParseContext {
                valid_stages: &valid_stages,
                ..Default::default()
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_tasks_namespace_prefixing() {
        let mut tasks = HashMap::new();
        let key = Spanned {
            value: "lint".to_string(),
            span: evin_diagnostic::span::Span::default(),
        };
        let task = TaskObject {
            name: "lint".to_string(),
            run: Some("cargo clippy".to_string()),
            ..Default::default()
        };
        tasks.insert(key, task);

        let result = parse_tasks(
            tasks,
            TaskParseContext {
                namespace: Some("my_module"),
                ..Default::default()
            },
        );
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed[0].name, "my_module:lint");
    }

    #[test]
    fn test_parse_tasks_with_custom_source_file() {
        let mut tasks = HashMap::new();
        let key = Spanned {
            value: "doctor".to_string(),
            span: evin_diagnostic::span::Span::default(),
        };
        let task = TaskObject {
            name: "doctor".to_string(),
            run: Some("cargo run -p esh -- doctor.esh".to_string()),
            ..Default::default()
        };
        tasks.insert(key, task);

        let result = parse_tasks(
            tasks,
            TaskParseContext {
                namespace: Some("release"),
                source_file: Some(".evin/release.yaml"),
                ..Default::default()
            },
        );
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed[0].name, "release:doctor");
        assert_eq!(parsed[0].source_file.as_deref(), Some(".evin/release.yaml"));
    }

    #[test]
    fn test_parse_tasks_workspace_prefix_override() {
        let mut tasks = HashMap::new();

        // Local module has its own 'build' task and a 'test' task that depends on workspace 'build'
        let build_key = Spanned {
            value: "build".to_string(),
            span: evin_diagnostic::span::Span::default(),
        };
        let build_task = TaskObject {
            name: "build".to_string(),
            run: Some("echo local build".to_string()),
            ..Default::default()
        };
        tasks.insert(build_key, build_task);

        let test_key = Spanned {
            value: "test".to_string(),
            span: evin_diagnostic::span::Span::default(),
        };
        let test_task = TaskObject {
            name: "test".to_string(),
            needs: Some(vec!["my_workspace:build".to_string(), "build".to_string()]),
            run: Some("echo test".to_string()),
            ..Default::default()
        };
        tasks.insert(test_key, test_task);

        let result = parse_tasks(
            tasks,
            TaskParseContext {
                namespace: Some("my_module"),
                workspace_name: Some("my_workspace"),
                ..Default::default()
            },
        );
        assert!(result.is_ok());
        let parsed = result.unwrap();

        let test = parsed.iter().find(|t| t.name == "my_module:test").unwrap();
        let needs = test.needs.as_ref().unwrap();

        // "my_workspace:build" should be stripped to "build" (referencing root task)
        assert_eq!(needs[0], "build");
        // Bare "build" should be qualified to "my_module:build" because "build" is local to my_module
        assert_eq!(needs[1], "my_module:build");
    }
}
