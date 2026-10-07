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

use evin_config::parser::EverInitConfig;
use evin_diagnostic::{Diagnostic, IssueCode, Severity};
use evin_pipeline::dag::DagResolver;
use evin_ui::discovery::{
    CheckIssueView, CheckReportView, CheckSelectionFailureView, CheckedManifest, ManifestKind,
    WorkspaceIdentity, render_check, render_check_selection_failure,
};
use std::collections::{BTreeMap, HashSet};
use std::io::IsTerminal;
use std::path::PathBuf;

pub async fn check(
    workspace_only: bool,
    module_filter: Option<&str>,
    target_filter: Option<&str>,
) -> Result<bool, Diagnostic> {
    let actions = crate::commands::builtin_actions();
    let semantic = evin_config::WorkspaceSemanticAnalysis::build(
        evin_config::WorkspaceSnapshot::load(&evin_config::source::FilesystemSource).await,
        &actions,
    );
    let config = match semantic.config.as_ref() {
        Some(config) => config,
        None => {
            let mut diagnostics = semantic.index.diagnostics.clone();
            if diagnostics.is_empty() {
                diagnostics.push(evin_locales::diagnostic(IssueCode::custom(
                    "workspace configuration could not be loaded",
                )));
            }
            let view = failed_report_many(unresolved_identity(), diagnostics);
            emit(render_check(&view, terminal_width(), None), false);
            return Ok(false);
        }
    };
    match check_view_with_findings(
        config,
        workspace_only,
        module_filter,
        target_filter,
        Some(&semantic.findings),
    ) {
        Ok(mut view) => {
            view.issues
                .extend(semantic.index.diagnostics.iter().cloned().map(issue_view));
            if !workspace_only && module_filter.is_none() && target_filter.is_none() {
                view.issues.extend(release_lock_issues(config).await);
            }
            let valid = view.is_valid();
            emit(render_check(&view, terminal_width(), None), valid);
            Ok(valid)
        }
        Err(diagnostic)
            if is_selection_error(&diagnostic) && semantic.index.diagnostics.is_empty() =>
        {
            let kind = if diagnostic.code.key == "issue-check-unknown-module" {
                evin_locales::t!("check-kind-unknown-module")
            } else {
                evin_locales::t!("check-kind-unknown-target")
            };
            let selected = module_filter.or(target_filter).unwrap_or_default();
            let detail = diagnostic
                .hint
                .clone()
                .unwrap_or_else(|| evin_locales::render_issue(&diagnostic.code));
            emit(
                render_check_selection_failure(
                    &CheckSelectionFailureView {
                        workspace: identity(config),
                        target: selected.to_string(),
                        kind,
                        detail,
                    },
                    terminal_width(),
                ),
                false,
            );
            Ok(false)
        }
        Err(diagnostic) => {
            let mut diagnostics = semantic.index.diagnostics.clone();
            let dag_missing_dependency_is_unknown_module = diagnostic.code.key
                == "issue-missing-dependency"
                && diagnostic
                    .code
                    .args
                    .iter()
                    .find(|(name, _)| *name == "dep")
                    .and_then(|(_, dependency)| dependency.rsplit_once(':'))
                    .is_some_and(|(module, _)| {
                        diagnostics.iter().any(|existing| {
                            existing.code.key == "issue-check-unknown-module"
                                && existing
                                    .code
                                    .args
                                    .iter()
                                    .any(|(name, value)| *name == "module" && value == module)
                        })
                    });
            if !dag_missing_dependency_is_unknown_module
                && !diagnostics.iter().any(|existing| {
                    existing.code.key == diagnostic.code.key
                        && existing.code.args == diagnostic.code.args
                })
            {
                diagnostics.push(diagnostic);
            }
            let view = failed_report_many(identity(config), diagnostics);
            emit(render_check(&view, terminal_width(), None), false);
            Ok(false)
        }
    }
}

async fn release_lock_issues(config: &EverInitConfig) -> Vec<CheckIssueView> {
    let mut issues = Vec::new();
    let Some(rules) = config.gitrules.as_ref() else {
        return issues;
    };
    let policies = rules
        .releases
        .as_ref()
        .map(|release| &release.packages)
        .unwrap_or(&EMPTY_PACKAGES);
    if let Err(error) = evin_action_sdk::release::validate_policies(policies) {
        issues.push(issue_view(evin_locales::diagnostic(IssueCode::custom(
            error.to_string(),
        ))));
        return issues;
    }
    if policies.is_empty() {
        return issues;
    }
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let lock = match evin_action_sdk::release::read_lock(&root) {
        Ok(Some(lock)) => lock,
        Ok(None) => return issues,
        Err(error) => {
            issues.push(issue_view(
                evin_locales::diagnostic(IssueCode::custom(error.to_string())).with_file_location(
                    evin_action_sdk::release::RELEASE_LOCK_PATH,
                    None,
                    None,
                ),
            ));
            return issues;
        }
    };
    if let Err(error) = lock.validate(policies) {
        issues.push(issue_view(
            evin_locales::diagnostic(IssueCode::custom(error.to_string())).with_file_location(
                evin_action_sdk::release::RELEASE_LOCK_PATH,
                None,
                None,
            ),
        ));
    }
    if semver::Version::parse(&lock.release.version).is_err() {
        issues.push(issue_view(
            evin_locales::diagnostic(IssueCode::custom(format!(
                "release lock version '{}' is not valid SemVer",
                lock.release.version
            )))
            .with_file_location(
                evin_action_sdk::release::RELEASE_LOCK_PATH,
                None,
                None,
            ),
        ));
    }
    let expected_tag = evin_yaml::template::format::render(
        rules
            .tags
            .as_ref()
            .and_then(|tags| tags.name_format.as_ref())
            .and_then(|formats| formats.iter().next())
            .map(String::as_str)
            .unwrap_or("v{version}"),
        &std::collections::HashMap::from([("version", lock.release.version.as_str())]),
    );
    if expected_tag != lock.release.tag {
        issues.push(issue_view(
            evin_locales::diagnostic(IssueCode::custom(format!(
                "release lock tag '{}' does not format from version '{}'",
                lock.release.tag, lock.release.version
            )))
            .with_file_location(
                evin_action_sdk::release::RELEASE_LOCK_PATH,
                None,
                None,
            ),
        ));
    }
    match evin_action_sdk::release::fingerprint_index(&root, policies).await {
        Ok(fingerprints) => {
            for (id, fingerprint) in fingerprints {
                if lock
                    .packages
                    .get(&id)
                    .is_some_and(|package| package.fingerprint != fingerprint)
                {
                    issues.push(issue_view(
                        evin_locales::diagnostic(IssueCode::custom(format!(
                            "release lock is stale for package '{id}'"
                        )))
                        .with_file_location(
                            evin_action_sdk::release::RELEASE_LOCK_PATH,
                            None,
                            None,
                        ),
                    ));
                }
            }
        }
        Err(error) => issues.push(issue_view(evin_locales::diagnostic(IssueCode::custom(
            error.to_string(),
        )))),
    }
    if let Ok(commits) = evin_git::history::fetch_all_commits(&root).await {
        for commit in commits {
            if evin_action_sdk::git::extract_tag(&commit.refs)
                .is_some_and(|tag| tag == lock.release.tag)
            {
                break;
            }
            if commit
                .files_changed
                .iter()
                .any(|path| path == evin_action_sdk::release::RELEASE_LOCK_PATH)
            {
                let trailer = commit
                    .message
                    .lines()
                    .any(|line| line.trim() == evin_action_sdk::release::PACKAGE_RELEASE_TRAILER);
                if commit.subject != evin_action_sdk::release::PACKAGE_RELEASE_SUBJECT || !trailer {
                    issues.push(issue_view(evin_locales::diagnostic(IssueCode::custom(format!("lock-changing commit {} is missing the canonical package-release subject or trailer", commit.hash)))));
                }
            }
        }
    }
    let latest_tag = std::process::Command::new("git")
        .args(["describe", "--tags", "--match", "v[0-9]*", "--abbrev=0"])
        .current_dir(&root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string());
    if latest_tag
        .as_deref()
        .is_some_and(|tag| tag != lock.release.tag)
    {
        issues.push(issue_view(
            evin_locales::diagnostic(IssueCode::custom(format!(
                "release lock tag '{}' does not match latest semantic tag '{}'",
                lock.release.tag,
                latest_tag.unwrap()
            )))
            .with_file_location(
                evin_action_sdk::release::RELEASE_LOCK_PATH,
                None,
                None,
            ),
        ));
    }
    issues
}

static EMPTY_PACKAGES: std::sync::LazyLock<
    std::collections::BTreeMap<String, evin_gitrules::PackageReleasePolicy>,
> = std::sync::LazyLock::new(std::collections::BTreeMap::new);

pub fn check_view(
    config: &EverInitConfig,
    workspace_only: bool,
    module_filter: Option<&str>,
    target_filter: Option<&str>,
) -> Result<CheckReportView, Diagnostic> {
    check_view_with_findings(config, workspace_only, module_filter, target_filter, None)
}

fn check_view_with_findings(
    config: &EverInitConfig,
    workspace_only: bool,
    module_filter: Option<&str>,
    target_filter: Option<&str>,
    supplied_findings: Option<&[evin_config::analysis::AnalysisFinding]>,
) -> Result<CheckReportView, Diagnostic> {
    let mut issues = config
        .warnings
        .iter()
        .map(|warning| {
            issue_view(
                evin_locales::diagnostic(
                    IssueCode::new("issue-check-workspace-warning").arg("message", warning),
                )
                .with_severity(Severity::Warning),
            )
        })
        .collect::<Vec<_>>();

    // 1. Build and validate DAG with resolver
    let mut resolver = DagResolver::new();
    for task in &config.tasks {
        resolver.add_task(task.clone());
    }

    let sorted = if let Some(target) = target_filter {
        let registry = crate::resolver::TargetRegistry::build(config.tasks.clone());
        let (_, matched_names) = registry.resolve(target)?;
        resolver
            .resolve_targets(&matched_names)
            .map_err(|err| evin_locales::diagnostic(err.into()))?
    } else if let Some(module) = module_filter {
        let prefix = format!("{}:", module);
        let module_tasks: Vec<String> = config
            .tasks
            .iter()
            .filter(|t| t.name.starts_with(&prefix) || t.name == module)
            .map(|t| t.name.clone())
            .collect();
        if module_tasks.is_empty() {
            let mut diagnostic = evin_locales::diagnostic(
                IssueCode::new("issue-check-unknown-module").arg("module", module),
            );
            if let Some(suggestion) = crate::resolver::strategy::best_fuzzy_match(
                module,
                config
                    .modules_discovered
                    .iter()
                    .map(|(name, _, _)| name.as_str()),
            ) {
                diagnostic = diagnostic.with_hint(evin_locales::t!(
                    "suggestion-hint",
                    suggestion = evin_ui::utils::suggestion(suggestion)
                ));
            }
            return Err(diagnostic);
        }
        resolver
            .resolve_targets(&module_tasks)
            .map_err(|err| evin_locales::diagnostic(err.into()))?
    } else if workspace_only {
        let ws_tasks: Vec<String> = config
            .tasks
            .iter()
            .filter(|t| {
                t.source_file.as_deref() == Some(evin_config::paths::WORKSPACE_MANIFEST)
                    || !t.name.contains(':')
            })
            .map(|t| t.name.clone())
            .collect();
        resolver
            .resolve_targets(&ws_tasks)
            .map_err(|err| evin_locales::diagnostic(err.into()))?
    } else {
        resolver
            .resolve()
            .map_err(|err| evin_locales::diagnostic(err.into()))?
    };

    let validated_task_set: HashSet<&str> = sorted.iter().map(|s| s.as_str()).collect();
    let validated_tasks: Vec<_> = config
        .tasks
        .iter()
        .filter(|t| validated_task_set.contains(t.name.as_str()))
        .collect();

    // 2. Reuse the side-effect-free configuration analyzer. Repository state
    // validation remains in `release_lock_issues`, outside this layer.
    let mut manifest_issues = BTreeMap::<String, Vec<CheckIssueView>>::new();
    let owned_findings;
    let findings = if let Some(findings) = supplied_findings {
        findings
    } else {
        owned_findings =
            evin_config::analysis::analyze(config, &crate::commands::builtin_actions()).findings;
        &owned_findings
    };
    for finding in findings
        .iter()
        .filter(|finding| validated_task_set.contains(finding.task_name.as_str()))
        .cloned()
    {
        let issue = issue_view(finding.diagnostic);
        if let Some(source_file) = finding.source_file {
            manifest_issues.entry(source_file).or_default().push(issue);
        } else {
            issues.push(issue);
        }
    }

    // 3. Collect checked manifest files
    let mut checked_files = Vec::new();

    let ws_task_count = validated_tasks
        .iter()
        .filter(|t| {
            t.source_file.as_deref() == Some(evin_config::paths::WORKSPACE_MANIFEST)
                || !t.name.contains(':')
        })
        .count();
    if ws_task_count > 0 || (!workspace_only && module_filter.is_none() && target_filter.is_none())
    {
        checked_files.push(CheckedManifest {
            path: crate::commands::file_uri(evin_config::paths::WORKSPACE_MANIFEST)?,
            task_count: ws_task_count,
            kind: ManifestKind::Workspace,
            issues: manifest_issues
                .remove(evin_config::paths::WORKSPACE_MANIFEST)
                .unwrap_or_default(),
        });
    }

    for (_name, path, _total_count) in &config.modules_discovered {
        let count = validated_tasks
            .iter()
            .filter(|t| t.source_file.as_deref() == Some(path.as_str()))
            .count();
        if count > 0 || (!workspace_only && module_filter.is_none() && target_filter.is_none()) {
            let is_namespace = !path.ends_with(evin_config::paths::MODULE_MANIFEST);
            checked_files.push(CheckedManifest {
                path: crate::commands::file_uri(path)?,
                task_count: count,
                kind: if is_namespace {
                    ManifestKind::Namespace
                } else {
                    ManifestKind::Module
                },
                issues: manifest_issues.remove(path).unwrap_or_default(),
            });
        }
    }

    // Git rules
    let gitrules_path = PathBuf::from(evin_config::paths::GITRULES_MANIFEST);
    if gitrules_path.exists() {
        checked_files.push(CheckedManifest {
            path: crate::commands::file_uri(evin_config::paths::GITRULES_MANIFEST)?,
            task_count: 0,
            kind: ManifestKind::GitRules,
            issues: Vec::new(),
        });
    }

    // Environment
    let env_path = PathBuf::from(evin_config::paths::ENVIRONMENT_MANIFEST);
    if env_path.exists() {
        checked_files.push(CheckedManifest {
            path: crate::commands::file_uri(evin_config::paths::ENVIRONMENT_MANIFEST)?,
            task_count: 0,
            kind: ManifestKind::Environment,
            issues: Vec::new(),
        });
    }

    // Count modules vs namespaces
    let mut module_set = HashSet::new();
    let mut namespace_set = HashSet::new();
    for (name, path, _) in &config.modules_discovered {
        let count = validated_tasks
            .iter()
            .filter(|t| t.source_file.as_deref() == Some(path.as_str()))
            .count();
        if count > 0 || (!workspace_only && module_filter.is_none() && target_filter.is_none()) {
            if !path.ends_with(evin_config::paths::MODULE_MANIFEST) {
                namespace_set.insert(name.clone());
            } else {
                module_set.insert(name.clone());
            }
        }
    }

    let edges: usize = resolver
        .registry()
        .values()
        .filter_map(|t| t.needs.as_ref())
        .map(|n| n.len())
        .sum();

    for (_, unlocated) in manifest_issues {
        issues.extend(unlocated);
    }

    Ok(CheckReportView {
        workspace: WorkspaceIdentity {
            name: config.workspace_name.clone(),
            version: config.workspace_version.clone(),
        },
        checked_files,
        total_tasks: sorted.len(),
        total_stages: config.stages.len(),
        total_modules: module_set.len(),
        total_namespaces: namespace_set.len(),
        graph_nodes: sorted.len(),
        graph_edges: edges,
        graph_cycles: 0,
        issues,
    })
}

fn identity(config: &EverInitConfig) -> WorkspaceIdentity {
    WorkspaceIdentity {
        name: config.workspace_name.clone(),
        version: config.workspace_version.clone(),
    }
}

fn unresolved_identity() -> WorkspaceIdentity {
    WorkspaceIdentity {
        name: evin_locales::t!("check-workspace-unresolved"),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

fn issue_view(diagnostic: Diagnostic) -> CheckIssueView {
    let message = evin_locales::render_issue(&diagnostic.code);
    CheckIssueView {
        diagnostic,
        message,
    }
}

fn failed_report_many(
    workspace: WorkspaceIdentity,
    diagnostics: Vec<Diagnostic>,
) -> CheckReportView {
    let mut issues = Vec::new();
    for diagnostic in diagnostics {
        if issues.iter().any(|issue: &CheckIssueView| {
            issue.diagnostic.code.key == diagnostic.code.key
                && issue.diagnostic.code.args == diagnostic.code.args
        }) {
            continue;
        }
        issues.push(issue_view(diagnostic));
    }
    CheckReportView {
        workspace,
        checked_files: Vec::new(),
        total_tasks: 0,
        total_stages: 0,
        total_modules: 0,
        total_namespaces: 0,
        graph_nodes: 0,
        graph_edges: 0,
        graph_cycles: usize::from(
            issues
                .iter()
                .any(|issue| issue.diagnostic.code.key == "issue-cycle-detected"),
        ),
        issues,
    }
}

fn is_selection_error(diagnostic: &Diagnostic) -> bool {
    matches!(
        diagnostic.code.key,
        "issue-check-unknown-module"
            | "issue-invalid-target"
            | "issue-invalid-stage-target"
            | "issue-invalid-hook"
    )
}

fn emit(rendered: String, success: bool) {
    let tape = std::env::var_os("EVIN_TAPE").is_some();
    match output_destination(std::io::stdout().is_terminal(), tape, success) {
        OutputDestination::StyledStderr => eprint!("{rendered}"),
        OutputDestination::PlainStdout => {
            print!("{}", console::strip_ansi_codes(&rendered));
        }
        OutputDestination::PlainStderr => {
            eprint!("{}", console::strip_ansi_codes(&rendered));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputDestination {
    StyledStderr,
    PlainStdout,
    PlainStderr,
}

fn output_destination(stdout_is_terminal: bool, tape: bool, success: bool) -> OutputDestination {
    if stdout_is_terminal || tape {
        OutputDestination::StyledStderr
    } else if success {
        OutputDestination::PlainStdout
    } else {
        OutputDestination::PlainStderr
    }
}

fn terminal_width() -> usize {
    console::Term::stderr()
        .size_checked()
        .map(|(_, width)| usize::from(width))
        .unwrap_or(evin_ui::constants::FALLBACK_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use evin_config::models::{StepObject, TaskObject};
    use evin_diagnostic::{FileLocation, Span};

    fn sample_config(tasks: Vec<TaskObject>) -> EverInitConfig {
        EverInitConfig {
            workspace_name: "test-workspace".into(),
            workspace_version: "1.0.0".into(),
            stages: vec!["Build".into(), "Test".into()],
            execution: None,
            tasks,
            expose: Vec::new(),
            workspace_env: std::collections::HashMap::new(),
            workspace_values: std::collections::HashMap::new(),
            modules: None,
            install_hint: None,
            hook_runner: None,
            gitrules: None,
            gitrules_source: evin_config::parser::GitRulesSource::Fallback,
            warnings: Vec::new(),
            modules_discovered: vec![
                ("release".into(), ".evin/release.flow".into(), 2),
                ("api".into(), "crates/api/.evin/module.flow".into(), 1),
            ],
            module_versions: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_check_view_all_valid() {
        let cfg = sample_config(vec![
            TaskObject {
                name: "build".into(),
                source_file: Some(".evin/workspace.flow".into()),
                stage: Some("Build".into()),
                ..Default::default()
            },
            TaskObject {
                name: "test".into(),
                source_file: Some(".evin/workspace.flow".into()),
                stage: Some("Test".into()),
                needs: Some(vec!["build".into()]),
                ..Default::default()
            },
            TaskObject {
                name: "release:doctor".into(),
                source_file: Some(".evin/release.flow".into()),
                ..Default::default()
            },
            TaskObject {
                name: "release:bump".into(),
                source_file: Some(".evin/release.flow".into()),
                needs: Some(vec!["release:doctor".into(), "build".into()]),
                ..Default::default()
            },
        ]);

        let report = check_view(&cfg, false, None, None).unwrap();
        assert_eq!(report.total_tasks, 4);
        assert_eq!(report.graph_nodes, 4);
        assert_eq!(report.graph_edges, 3);
        assert!(report.is_valid());
    }

    #[test]
    fn test_check_view_workspace_only() {
        let cfg = sample_config(vec![
            TaskObject {
                name: "build".into(),
                source_file: Some(".evin/workspace.flow".into()),
                ..Default::default()
            },
            TaskObject {
                name: "test".into(),
                source_file: Some(".evin/workspace.flow".into()),
                needs: Some(vec!["build".into()]),
                ..Default::default()
            },
            TaskObject {
                name: "release:bump".into(),
                source_file: Some(".evin/release.flow".into()),
                ..Default::default()
            },
        ]);

        let report = check_view(&cfg, true, None, None).unwrap();
        assert_eq!(report.total_tasks, 2);
    }

    #[test]
    fn test_check_view_module_filter() {
        let cfg = sample_config(vec![
            TaskObject {
                name: "build".into(),
                source_file: Some(".evin/workspace.flow".into()),
                ..Default::default()
            },
            TaskObject {
                name: "release:doctor".into(),
                source_file: Some(".evin/release.flow".into()),
                ..Default::default()
            },
            TaskObject {
                name: "release:bump".into(),
                source_file: Some(".evin/release.flow".into()),
                needs: Some(vec!["release:doctor".into(), "build".into()]),
                ..Default::default()
            },
        ]);

        let report = check_view(&cfg, false, Some("release"), None).unwrap();
        assert_eq!(report.total_tasks, 3); // release:doctor, release:bump, and dependency build
        assert_eq!(report.graph_edges, 2);
    }

    #[test]
    fn test_check_view_target_filter() {
        let cfg = sample_config(vec![
            TaskObject {
                name: "build".into(),
                ..Default::default()
            },
            TaskObject {
                name: "release:doctor".into(),
                ..Default::default()
            },
            TaskObject {
                name: "release:bump".into(),
                needs: Some(vec!["release:doctor".into()]),
                ..Default::default()
            },
        ]);

        let report = check_view(&cfg, false, None, Some("release:bump")).unwrap();
        assert_eq!(report.total_tasks, 2); // release:bump and release:doctor
    }

    #[test]
    fn test_check_view_detects_dependency_cycle() {
        let cfg = sample_config(vec![
            TaskObject {
                name: "task_a".into(),
                needs: Some(vec!["task_b".into()]),
                ..Default::default()
            },
            TaskObject {
                name: "task_b".into(),
                needs: Some(vec!["task_a".into()]),
                ..Default::default()
            },
        ]);

        let result = check_view(&cfg, false, None, None);
        assert!(result.is_err());
        let diag = result.unwrap_err();
        assert_eq!(diag.code.key, "issue-cycle-detected");
    }

    #[test]
    fn test_check_view_detects_missing_dependency() {
        let cfg = sample_config(vec![TaskObject {
            name: "task_a".into(),
            needs: Some(vec!["nonexistent_task".into()]),
            ..Default::default()
        }]);

        let result = check_view(&cfg, false, None, None);
        assert!(result.is_err());
        let diag = result.unwrap_err();
        assert_eq!(diag.code.key, "issue-missing-dependency");
    }

    #[test]
    fn workspace_warnings_are_non_fatal_findings() {
        let mut cfg = sample_config(vec![TaskObject {
            name: "build".into(),
            ..Default::default()
        }]);
        cfg.warnings.push("deprecated field".into());

        let report = check_view(&cfg, false, None, None).unwrap();

        assert!(report.is_valid());
        assert_eq!(report.warning_count(), 1);
        assert_eq!(report.issues[0].diagnostic.severity, Severity::Warning);
        assert_eq!(
            report.issues[0].diagnostic.code.key,
            "issue-check-workspace-warning"
        );
    }

    #[test]
    fn invalid_action_options_are_located_under_their_manifest() {
        let with = "site_url: https://example.com\ntile: Docs\n";
        let typo_offset = with.find("tile").unwrap();
        let cfg = sample_config(vec![TaskObject {
            name: "publish".into(),
            source_file: Some(".evin/workspace.flow".into()),
            steps: Some(vec![StepObject {
                run: None,
                uses: Some(evin_yaml::Spanned::new(
                    "evin/docs@v1".into(),
                    Span::default(),
                )),
                with: Some(evin_yaml::parse_yaml(with).unwrap()),
            }]),
            ..Default::default()
        }]);

        let report = check_view(&cfg, false, None, None).unwrap();

        assert!(!report.is_valid());
        assert_eq!(report.error_count(), 1);
        assert!(report.issues.is_empty());
        let issue = &report.checked_files[0].issues[0];
        assert_eq!(issue.diagnostic.code.key, "issue-action-config-invalid");
        assert!(matches!(
            issue.diagnostic.location,
            Some(FileLocation::ByteSpan {
                ref path,
                offset,
                length: 4,
                ..
            }) if path == ".evin/workspace.flow" && offset == typo_offset
        ));
    }

    #[test]
    fn check_aggregates_action_errors_across_manifests() {
        let invalid_step = || StepObject {
            run: None,
            uses: Some(evin_yaml::Spanned::new(
                "evin/docs@v1".into(),
                Span::default(),
            )),
            with: Some(
                evin_yaml::parse_yaml("site_url: https://example.com\ntile: Docs\n").unwrap(),
            ),
        };
        let cfg = sample_config(vec![
            TaskObject {
                name: "docs".into(),
                source_file: Some(".evin/workspace.flow".into()),
                steps: Some(vec![invalid_step()]),
                ..Default::default()
            },
            TaskObject {
                name: "release:docs".into(),
                source_file: Some(".evin/release.flow".into()),
                steps: Some(vec![invalid_step()]),
                ..Default::default()
            },
        ]);

        let report = check_view(&cfg, false, None, None).unwrap();

        assert_eq!(report.error_count(), 2);
        assert!(report.issues.is_empty());
        assert_eq!(report.checked_files[0].issues.len(), 1);
        assert_eq!(report.checked_files[1].issues.len(), 1);
    }

    #[test]
    fn every_builtin_rejects_unknown_top_level_options_with_a_span() {
        for (action_id, action) in crate::commands::builtin_actions() {
            let with = evin_yaml::parse_yaml("mistyped_option: true\n").unwrap();
            let error = action.validate_config(Some(&with)).unwrap_err();
            assert!(
                error.message.contains("unknown field"),
                "{action_id} returned an unexpected error: {error}"
            );
            assert!(
                error.span.is_some(),
                "{action_id} discarded the option span"
            );
        }
    }

    #[test]
    fn unknown_modules_use_a_check_specific_diagnostic_and_suggestion() {
        let cfg = sample_config(Vec::new());

        let diagnostic = check_view(&cfg, false, Some("releaze"), None).unwrap_err();

        assert_eq!(diagnostic.code.key, "issue-check-unknown-module");
        assert!(
            diagnostic
                .hint
                .as_deref()
                .is_some_and(|hint| hint.contains("release"))
        );
    }

    #[test]
    fn check_output_routing_keeps_failures_off_stdout() {
        assert_eq!(
            output_destination(true, false, true),
            OutputDestination::StyledStderr
        );
        assert_eq!(
            output_destination(false, true, false),
            OutputDestination::StyledStderr
        );
        assert_eq!(
            output_destination(false, false, true),
            OutputDestination::PlainStdout
        );
        assert_eq!(
            output_destination(false, false, false),
            OutputDestination::PlainStderr
        );
    }
}
