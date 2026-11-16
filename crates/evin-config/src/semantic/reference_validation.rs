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

fn task_names_from_documents(
    documents: &std::collections::BTreeMap<PathBuf, WorkspaceDocument>,
) -> Vec<(PathBuf, String)> {
    let scopes = document_task_scopes(documents);
    let mut tasks = Vec::new();
    for (path, document) in documents {
        let Some(ast) = &document.ast else {
            continue;
        };
        let Some(SpannedValue::Mapping(entries, _)) = mapping_get(ast, "tasks") else {
            continue;
        };
        let scope = scopes.get(path).map(String::as_str).unwrap_or_default();
        for (key, _) in entries {
            let SpannedValue::String(name, _) = key else {
                continue;
            };
            let qualified = if scope.is_empty() {
                name.clone()
            } else {
                format!("{scope}:{name}")
            };
            tasks.push((path.clone(), qualified));
        }
    }
    tasks
}

fn document_task_scopes(
    documents: &std::collections::BTreeMap<PathBuf, WorkspaceDocument>,
) -> std::collections::BTreeMap<PathBuf, String> {
    let mut scopes = std::collections::BTreeMap::new();
    for (path, document) in documents {
        match document.kind {
            DocumentKind::Module => {
                if let Some(name) = document_string(document, &["module", "name"]) {
                    scopes.insert(path.clone(), name);
                }
            }
            DocumentKind::Namespace => {
                if let Some(name) = document_string(document, &["namespace", "name"]) {
                    scopes.insert(path.clone(), name);
                }
            }
            _ => {}
        }
    }

    for (path, document) in documents {
        let Some(ast) = &document.ast else {
            continue;
        };
        match document.kind {
            DocumentKind::Workspace => {
                let root = path
                    .parent()
                    .and_then(Path::parent)
                    .unwrap_or(Path::new(""));
                let includes = mapping_get(ast, "workspace")
                    .and_then(|workspace| {
                        mapping_get(workspace, "include").or_else(|| mapping_get(workspace, "includes"))
                    });
                add_namespace_scopes(documents, &mut scopes, includes, root, None);
            }
            DocumentKind::Module => {
                let module_name = document_string(document, &["module", "name"]);
                let module_root = path
                    .parent()
                    .and_then(Path::parent)
                    .unwrap_or(Path::new(""));
                let includes = mapping_get(ast, "module")
                    .and_then(|module| {
                        mapping_get(module, "include").or_else(|| mapping_get(module, "includes"))
                    });
                add_namespace_scopes(
                    documents,
                    &mut scopes,
                    includes,
                    module_root,
                    module_name.as_deref(),
                );
            }
            _ => {}
        }
    }
    // Namespace includes form a tree layered on the workspace/module roots.
    // Walk until all reachable nested documents have inherited their scope.
    for _ in 0..documents.len() {
        let mut changed = false;
        for (path, document) in documents {
            if document.kind != DocumentKind::Namespace {
                continue;
            }
            let Some(parent_scope) = scopes.get(path).cloned() else {
                continue;
            };
            let Some(ast) = &document.ast else {
                continue;
            };
            let Some(includes) = mapping_get(ast, "include").or_else(|| mapping_get(ast, "includes")) else {
                continue;
            };
            let before = scopes.clone();
            add_namespace_scopes(
                documents,
                &mut scopes,
                Some(includes),
                path.parent().unwrap_or(Path::new("")),
                Some(&parent_scope),
            );
            changed |= scopes != before;
        }
        if !changed {
            break;
        }
    }
    scopes
}

fn add_namespace_scopes(
    documents: &std::collections::BTreeMap<PathBuf, WorkspaceDocument>,
    scopes: &mut std::collections::BTreeMap<PathBuf, String>,
    includes: Option<&SpannedValue>,
    base: &Path,
    module_name: Option<&str>,
) {
    for (alias, file) in include_entries(includes) {
        let namespace_path = normalize(if Path::new(&file).is_absolute() {
            PathBuf::from(&file)
        } else {
            base.join(&file)
        });
        let declared_name = documents
            .get(&namespace_path)
            .and_then(|document| document_string(document, &["namespace", "name"]));
        let Some(namespace_name) = alias.or(declared_name) else {
            continue;
        };
        let full_name = match module_name {
            Some(module) if namespace_name != module && !namespace_name.starts_with(&format!("{module}:")) => {
                format!("{module}:{namespace_name}")
            }
            _ => namespace_name,
        };
        scopes.insert(namespace_path, full_name);
    }
}

fn include_entries(value: Option<&SpannedValue>) -> Vec<(Option<String>, String)> {
    include_source_entries(value)
        .into_iter()
        .map(|(alias, path, _)| (alias, path))
        .collect()
}

fn include_source_entries(
    value: Option<&SpannedValue>,
) -> Vec<(Option<String>, String, Span)> {
    match value {
        Some(SpannedValue::Sequence(items, _)) => items
            .iter()
            .filter_map(|item| match item {
                SpannedValue::String(path, span) => Some((None, path.clone(), *span)),
                _ => None,
            })
            .collect(),
        Some(SpannedValue::Mapping(items, _)) => items
            .iter()
            .filter_map(|(key, value)| match (key, value) {
                (SpannedValue::String(alias, _), SpannedValue::String(path, span)) => {
                    Some((Some(alias.clone()), path.clone(), *span))
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn document_string(document: &WorkspaceDocument, path: &[&str]) -> Option<String> {
    let mut value = document.ast.as_ref()?;
    for segment in path {
        value = mapping_get(value, segment)?;
    }
    match value {
        SpannedValue::String(value, _) => Some(value.clone()),
        _ => None,
    }
}

fn validate_workspace_references(index: &mut WorkspaceSemanticIndex) {
    let workspace_path = Path::new(crate::paths::WORKSPACE_MANIFEST);
    let workspace = index.document(workspace_path);
    let workspace_name = workspace
        .and_then(|document| document_string(document, &["workspace", "name"]))
        .unwrap_or_default();
    let stages = workspace
        .and_then(|document| document.ast.as_ref())
        .and_then(|ast| mapping_get(ast, "stages"))
        .and_then(|value| match value {
            SpannedValue::Sequence(stages, _) => Some(stages),
            _ => None,
        })
        .into_iter()
        .flatten()
        .filter_map(|stage| match stage {
            SpannedValue::String(name, span) => Some((name.clone(), *span)),
            _ => None,
        })
        .collect::<Vec<_>>();
    validate_module_and_namespace_names(index);

    let mut seen_stages = std::collections::HashSet::new();
    for (name, span) in &stages {
        if !valid_stage_name(name) {
            push_reference_issue(
                index,
                "issue-invalid-stage-casing",
                [("stage", name.clone())],
                workspace_path,
                *span,
            );
        }
        if !seen_stages.insert(name) {
            push_reference_issue(
                index,
                "issue-duplicate-stage",
                [("stage", name.clone())],
                workspace_path,
                *span,
            );
        }
    }

    let task_symbols = index
        .symbols
        .iter()
        .filter(|symbol| symbol.kind == SemanticKind::Task)
        .cloned()
        .collect::<Vec<_>>();
    let mut declared_tasks = std::collections::BTreeSet::new();
    let local_tasks = task_symbols.iter().fold(
        std::collections::HashMap::<PathBuf, std::collections::HashSet<String>>::new(),
        |mut local, task| {
            local
                .entry(task.range.path.clone())
                .or_default()
                .insert(task.name.rsplit(':').next().unwrap_or(&task.name).to_owned());
            local
        },
    );
    let mut known_scopes = document_task_scopes(&index.documents)
        .into_values()
        .collect::<std::collections::BTreeSet<_>>();
    for task in &task_symbols {
        if let Some(scope) = task.name.rsplit_once(':').map(|(scope, _)| scope) {
            known_scopes.insert(scope.to_owned());
        }
    }

    for task in &task_symbols {
        let short = task.name.rsplit(':').next().unwrap_or(&task.name);
        if !declared_tasks.insert(task.name.clone()) {
            push_reference_issue(
                index,
                "issue-duplicate-task",
                [("task_name", task.name.clone())],
                &task.range.path,
                Span {
                    offset: task.range.start,
                    length: task.range.len(),
                    ..Span::default()
                },
            );
        }
        if !valid_task_name(short) {
            push_reference_issue(
                index,
                "issue-invalid-task-casing",
                [("task_name", short.to_owned())],
                &task.range.path,
                Span {
                    offset: task.range.start,
                    length: task.range.len(),
                    ..Span::default()
                },
            );
        }
    }

    let references = index
        .symbols
        .iter()
        .filter(|symbol| symbol.kind == SemanticKind::TaskReference)
        .cloned()
        .collect::<Vec<_>>();
    for reference in references {
        let raw = reference.name.as_str();
        let local = local_tasks
            .get(&reference.range.path)
            .is_some_and(|names| names.contains(raw));
        let dependency = if !workspace_name.is_empty()
            && let Some(bare) = raw.strip_prefix(&format!("{workspace_name}:"))
        {
            bare.to_owned()
        } else if local
            && let Some(owner) = reference.owner.as_deref()
            && let Some((scope, _)) = owner.rsplit_once(':')
        {
            format!("{scope}:{raw}")
        } else {
            raw.to_owned()
        };
        if declared_tasks.contains(&dependency) {
            continue;
        }
        let qualifier = dependency.rsplit_once(':').map(|(scope, _)| scope);
        let code = if qualifier.is_some_and(|scope| !known_scopes.contains(scope)) {
            "issue-check-unknown-module"
        } else {
            "issue-missing-dependency"
        };
        let mut diagnostic = if code == "issue-check-unknown-module" {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new(code)
                    .arg("module", qualifier.unwrap_or_default()),
            )
        } else {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new(code)
                    .arg("task", reference.owner.as_deref().unwrap_or_default())
                    .arg("dep", raw),
            )
        };
        let candidates = if code == "issue-check-unknown-module" {
            known_scopes.iter().map(String::as_str).collect::<Vec<_>>()
        } else if let Some(scope) = qualifier {
            declared_tasks
                .iter()
                .filter(|candidate| candidate.rsplit_once(':').map(|(s, _)| s) == Some(scope))
                .map(|candidate| candidate.rsplit(':').next().unwrap_or(candidate))
                .collect::<Vec<_>>()
        } else {
            declared_tasks
                .iter()
                .filter(|candidate| {
                    !candidate.contains(':')
                        || reference
                            .owner
                            .as_deref()
                            .and_then(|owner| owner.rsplit_once(':').map(|(scope, _)| scope))
                            .is_some_and(|scope| candidate.starts_with(&format!("{scope}:")))
                })
                .map(|candidate| candidate.rsplit(':').next().unwrap_or(candidate))
                .collect::<Vec<_>>()
        };
        if let Some(suggestion) = best_reference_suggestion(
            qualifier.and_then(|_| dependency.rsplit(':').next()).unwrap_or(raw),
            candidates,
        ) {
            diagnostic = diagnostic.with_hint(evin_locales::t!(
                "suggestion-hint",
                suggestion = suggestion
            ));
        }
        index.diagnostics.push(with_reference_span(
            diagnostic,
            &reference.range.path,
            reference.range.start,
            reference.range.len(),
        ));
    }

    let mut valid_presets = index
        .documents
        .values()
        .find(|document| document.kind == DocumentKind::Environment)
        .and_then(|document| document.ast.as_ref())
        .and_then(|ast| mapping_get(ast, "presets"))
        .and_then(|value| match value {
            SpannedValue::Mapping(entries, _) => Some(
                entries
                    .iter()
                    .filter_map(|(key, _)| match key {
                        SpannedValue::String(name, _) => Some(name.clone()),
                        _ => None,
                    })
                    .collect::<std::collections::BTreeSet<_>>(),
            ),
            _ => None,
        })
        .unwrap_or_default();
    valid_presets.extend(
        crate::environment::BUILTIN_ENVIRONMENT_PRESETS
            .iter()
            .map(|preset| (*preset).to_owned()),
    );
    let task_nodes = task_entries(&index.documents);
    for task in task_nodes {
        if let Some(SpannedValue::String(stage, span)) = mapping_get(&task.value, "stage")
            && !stages.iter().any(|(declared, _)| declared == stage)
        {
            let valid_stages = format!("{:?}", stages.iter().map(|(name, _)| name).collect::<Vec<_>>());
            let mut diagnostic = evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-invalid-stage")
                    .arg("task_name", &task.qualified_name)
                    .arg("stage", stage)
                    .arg("valid_stages", valid_stages),
            );
            if let Some(suggestion) =
                best_reference_suggestion(stage, stages.iter().map(|(name, _)| name.as_str()))
            {
                diagnostic = diagnostic.with_hint(evin_locales::t!(
                    "suggestion-hint",
                    suggestion = suggestion
                ));
            }
            index.diagnostics.push(with_reference_span(
                diagnostic,
                &task.path,
                span.offset,
                span.length,
            ));
        }
        if let Some(SpannedValue::Sequence(presets, _)) = mapping_get(&task.value, "presets") {
            for preset in presets {
                let SpannedValue::String(name, span) = preset else {
                    continue;
                };
                if valid_presets.contains(name) {
                    continue;
                }
                let mut diagnostic = evin_locales::diagnostic(
                    evin_diagnostic::IssueCode::new("issue-unknown-environment-preset")
                        .arg("task", &task.qualified_name)
                        .arg("preset", name),
                );
                if let Some(suggestion) =
                    best_reference_suggestion(name, valid_presets.iter().map(String::as_str))
                {
                    diagnostic = diagnostic.with_hint(evin_locales::t!(
                        "suggestion-hint",
                        suggestion = suggestion
                    ));
                }
                index.diagnostics.push(with_reference_span(
                    diagnostic,
                    &task.path,
                    span.offset,
                    span.length,
                ));
            }
        }
    }
    validate_exposed_tasks(index, &declared_tasks);
}

fn validate_exposed_tasks(
    index: &mut WorkspaceSemanticIndex,
    declared_tasks: &std::collections::BTreeSet<String>,
) {
    let workspace_path = Path::new(crate::paths::WORKSPACE_MANIFEST);
    let Some(SpannedValue::Sequence(entries, _)) = index
        .document(workspace_path)
        .and_then(|workspace| workspace.ast.as_ref())
        .and_then(|ast| mapping_get(ast, "expose"))
        .cloned()
    else {
        return;
    };
    for entry in &entries {
        let SpannedValue::String(name, span) = entry else {
            continue;
        };
        if declared_tasks.contains(name) {
            continue;
        }
        let mut diagnostic = evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("issue-missing-exposed-task").arg("task", name),
        );
        let candidates = declared_tasks
            .iter()
            .filter(|candidate| !candidate.contains(':'))
            .map(String::as_str);
        if let Some(suggestion) = best_reference_suggestion(name, candidates) {
            diagnostic = diagnostic.with_hint(evin_locales::t!(
                "suggestion-hint",
                suggestion = suggestion
            ));
        }
        index.diagnostics.push(with_reference_span(
            diagnostic,
            workspace_path,
            span.offset,
            span.length,
        ));
    }
}

fn validate_module_and_namespace_names(index: &mut WorkspaceSemanticIndex) {
    let mut modules = std::collections::BTreeSet::new();
    let mut namespaces = std::collections::BTreeSet::new();
    let scopes = document_task_scopes(&index.documents);
    let mut duplicates = Vec::new();
    for document in index.documents.values() {
        let (key, field, seen) = match document.kind {
            DocumentKind::Module => ("issue-duplicate-module", "module", &mut modules),
            DocumentKind::Namespace => ("issue-duplicate-namespace", "namespace", &mut namespaces),
            _ => continue,
        };
        let Some((name, span)) = document_string_span(document, &[field, "name"]) else {
            continue;
        };
        let identity = if document.kind == DocumentKind::Namespace {
            scopes.get(&document.path).cloned().unwrap_or(name)
        } else {
            name
        };
        if !seen.insert(identity.clone())
            && !(document.kind == DocumentKind::Namespace
                && is_same_scope_namespace_fragment(&index.documents, &scopes, &document.path, &identity))
        {
            duplicates.push((key, identity, document.path.clone(), span));
        }
    }
    for (key, name, path, span) in duplicates {
        push_reference_issue(index, key, [("name", name)], &path, span);
    }
}

fn is_same_scope_namespace_fragment(
    documents: &std::collections::BTreeMap<PathBuf, WorkspaceDocument>,
    scopes: &std::collections::BTreeMap<PathBuf, String>,
    child_path: &Path,
    scope: &str,
) -> bool {
    documents.iter().any(|(parent_path, parent)| {
        if parent.kind != DocumentKind::Namespace
            || scopes.get(parent_path).map(String::as_str) != Some(scope)
        {
            return false;
        }
        let Some(ast) = &parent.ast else {
            return false;
        };
        let includes = mapping_get(ast, "include").or_else(|| mapping_get(ast, "includes"));
        include_entries(includes).into_iter().any(|(_, file)| {
            let path = crate::source::normalize(if Path::new(&file).is_absolute() {
                PathBuf::from(file)
            } else {
                parent_path.parent().unwrap_or(Path::new("")).join(file)
            });
            (path == child_path && scopes.get(child_path).map(String::as_str) == Some(scope))
                || (*parent_path == child_path
                    && scopes.get(&path).map(String::as_str) == Some(scope))
        })
    })
}

fn document_string_span(document: &WorkspaceDocument, path: &[&str]) -> Option<(String, Span)> {
    let mut value = document.ast.as_ref()?;
    for segment in path {
        value = mapping_get(value, segment)?;
    }
    match value {
        SpannedValue::String(value, span) => Some((value.clone(), *span)),
        _ => None,
    }
}

struct TaskEntry {
    path: PathBuf,
    value: SpannedValue,
    qualified_name: String,
}

fn task_entries(
    documents: &std::collections::BTreeMap<PathBuf, WorkspaceDocument>,
) -> Vec<TaskEntry> {
    let scopes = document_task_scopes(documents);
    let mut result = Vec::new();
    for (path, document) in documents {
        let Some(ast) = &document.ast else {
            continue;
        };
        let Some(SpannedValue::Mapping(tasks, _)) = mapping_get(ast, "tasks") else {
            continue;
        };
        let scope = scopes.get(path).map(String::as_str).unwrap_or_default();
        for (key, value) in tasks {
            let SpannedValue::String(name, _) = key else {
                continue;
            };
            result.push(TaskEntry {
                path: path.clone(),
                value: value.clone(),
                qualified_name: if scope.is_empty() {
                    name.clone()
                } else {
                    format!("{scope}:{name}")
                },
            });
        }
    }
    result
}

fn valid_stage_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && chars.all(|character| character.is_ascii_alphanumeric())
}

fn valid_task_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
}

fn push_reference_issue<const N: usize>(
    index: &mut WorkspaceSemanticIndex,
    key: &'static str,
    args: [(&'static str, String); N],
    path: &Path,
    span: Span,
) {
    let mut issue = evin_diagnostic::IssueCode::new(key);
    for (name, value) in args {
        issue = issue.arg(name, value);
    }
    let diagnostic = with_reference_span(
        evin_locales::diagnostic(issue),
        path,
        span.offset,
        span.length,
    );
    index.diagnostics.push(diagnostic);
}

fn with_reference_span(
    diagnostic: evin_diagnostic::Diagnostic,
    path: &Path,
    offset: usize,
    length: usize,
) -> evin_diagnostic::Diagnostic {
    diagnostic.with_file_span(
        path.display().to_string(),
        offset,
        length.max(1),
        None,
    )
}

fn best_reference_suggestion<'a>(
    actual: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    let mut best = None::<(usize, String)>;
    for candidate in candidates {
        let distance = edit_distance(&actual.to_ascii_lowercase(), &candidate.to_ascii_lowercase());
        let threshold = (actual.chars().count() / 3).max(1);
        if distance <= threshold
            && best.as_ref().is_none_or(|(best_distance, _)| distance < *best_distance)
        {
            best = Some((distance, candidate.to_owned()));
        }
    }
    best.map(|(_, value)| value)
}

fn edit_distance(left: &str, right: &str) -> usize {
    let mut previous = (0..=right.chars().count()).collect::<Vec<_>>();
    for (row, left_char) in left.chars().enumerate() {
        let mut current = vec![row + 1];
        for (column, right_char) in right.chars().enumerate() {
            current.push(
                (previous[column + 1] + 1)
                    .min(current[column] + 1)
                    .min(previous[column] + usize::from(left_char != right_char)),
            );
        }
        previous = current;
    }
    *previous.last().unwrap_or(&0)
}
