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

fn index_document(
    index: &mut WorkspaceSemanticIndex,
    document: &WorkspaceDocument,
    ast: &SpannedValue,
    task_names: &[(PathBuf, String)],
    actions: &HashMap<String, Arc<dyn Action>>,
) {
    let root_name = match document.kind {
        DocumentKind::Workspace => "workspace",
        DocumentKind::Module => "module",
        DocumentKind::Namespace => "namespace",
        DocumentKind::RepositoryPolicy => "RepositoryPolicy",
        DocumentKind::Environment => "environment",
        DocumentKind::Unknown => return,
    };
    let mut root = symbol(
        document,
        if document.kind == DocumentKind::RepositoryPolicy {
            SemanticKind::PackagePolicy
        } else {
            match document.kind {
                DocumentKind::Workspace => SemanticKind::Workspace,
                DocumentKind::Module => SemanticKind::Module,
                DocumentKind::Environment => SemanticKind::EnvironmentVariable,
                _ => SemanticKind::Namespace,
            }
        },
        root_name,
        ast.span(),
        schema_description("flow-schema-configuration-document"),
    );
    root.catalog_target = Some(CatalogTarget::configuration_path(&[root_name]));
    index.symbols.push(root);
    let schema = match document.kind {
        DocumentKind::Workspace => crate::workspace::WORKSPACE_EDITOR_SCHEMA,
        DocumentKind::Module => crate::workspace::MODULE_EDITOR_SCHEMA,
        DocumentKind::Namespace => crate::workspace::NAMESPACE_EDITOR_SCHEMA,
        DocumentKind::RepositoryPolicy => evin_repository_policy::editor_schema::POLICY_SCHEMA,
        DocumentKind::Environment => crate::environment::EDITOR_SCHEMA,
        DocumentKind::Unknown => return,
    };
    resolve_schema_node(index, document, ast, schema, root_name, false, 0);
    if document.kind == DocumentKind::Environment {
        if let Some(SpannedValue::Mapping(presets, _)) = mapping_get(ast, "presets") {
            for (_, values) in presets {
                index.reference_contexts.push(ReferenceCompletionContext {
                    range: value_range(document, values),
                    owner: None,
                    kind: ReferenceCompletionKind::Environment,
                    keys_only: false,
                    blocked: Vec::new(),
                    base: None,
                });
            }
        }
        if let Some(SpannedValue::Mapping(environments, _)) = mapping_get(ast, "environments") {
            for (_, definition) in environments {
                if let Some(stages) = mapping_get(definition, "stages") {
                    index.reference_contexts.push(ReferenceCompletionContext {
                        range: value_range(document, stages),
                        owner: None,
                        kind: ReferenceCompletionKind::Stage,
                        keys_only: false,
                        blocked: Vec::new(),
                        base: None,
                    });
                }
            }
        }
    }
    if matches!(
        document.kind,
        DocumentKind::RepositoryPolicy | DocumentKind::Environment
    ) {
        return;
    }
    match document.kind {
        DocumentKind::Workspace => {
            if let Some(modules) = mapping_get(ast, "workspace")
                .and_then(|workspace| mapping_get(workspace, "modules"))
            {
                index.reference_contexts.push(ReferenceCompletionContext {
                    range: value_range(document, modules),
                    owner: None,
                    kind: ReferenceCompletionKind::ModulePath,
                    keys_only: false,
                    blocked: Vec::new(),
                    base: Some(PathBuf::new()),
                });
            }
            if let Some(workspace) = mapping_get(ast, "workspace")
                && let Some(include) = mapping_get(workspace, "include")
                    .or_else(|| mapping_get(workspace, "includes"))
            {
                index_include_path_contexts(index, document, include, PathBuf::new());
            }
            if let Some(exposed) = mapping_get(ast, "expose") {
                index.reference_contexts.push(ReferenceCompletionContext {
                    range: value_range(document, exposed),
                    owner: None,
                    kind: ReferenceCompletionKind::Task,
                    keys_only: false,
                    blocked: Vec::new(),
                    base: None,
                });
            }
        }
        DocumentKind::Module => {
            if let Some(module) = mapping_get(ast, "module")
                && let Some(include) = mapping_get(module, "include")
                    .or_else(|| mapping_get(module, "includes"))
            {
                let base = document
                    .path
                    .parent()
                    .and_then(Path::parent)
                    .unwrap_or(Path::new(""))
                    .to_owned();
                index_include_path_contexts(index, document, include, base);
            }
        }
        DocumentKind::Namespace => {
            if let Some(include) = mapping_get(ast, "include").or_else(|| mapping_get(ast, "includes")) {
                let base = document.path.parent().unwrap_or(Path::new(""));
                index_include_path_contexts(index, document, include, base.to_owned());
            }
        }
        _ => {}
    }
    if let Some(values) = mapping_get(ast, "values") {
        index_values(index, document, values, None);
    }
    if let Some(SpannedValue::Sequence(entries, _)) = mapping_get(ast, "stages") {
        for entry in entries {
            if let SpannedValue::String(name, span) = entry {
                index.symbols.push(symbol(
                    document,
                    SemanticKind::Stage,
                    name,
                    *span,
                    schema_description("flow-schema-workspace-stage"),
                ));
            }
        }
    }
    let Some(tasks) = mapping_get(ast, "tasks") else {
        return;
    };
    let SpannedValue::Mapping(entries, _) = tasks else {
        return;
    };
    for (key, value) in entries {
        let SpannedValue::String(short_name, key_span) = key else {
            continue;
        };
        let full_name = task_names
            .iter()
            .find(|(path, name)| {
                *path == document.path && name.rsplit(':').next() == Some(short_name.as_str())
            })
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| short_name.clone());
        index.symbols.push(symbol(
            document,
            SemanticKind::Task,
            &full_name,
            *key_span,
            schema_description("flow-schema-evin-task"),
        ));
        index_task(index, document, value, &full_name, actions);
    }
}

fn index_include_path_contexts(
    index: &mut WorkspaceSemanticIndex,
    document: &WorkspaceDocument,
    include: &SpannedValue,
    base: PathBuf,
) {
    match include {
        SpannedValue::Sequence(_, _) => {
            index.reference_contexts.push(ReferenceCompletionContext {
                range: value_range(document, include),
                owner: None,
                kind: ReferenceCompletionKind::NamespacePath,
                keys_only: false,
                blocked: Vec::new(),
                base: Some(base),
            });
        }
        SpannedValue::Mapping(entries, _) => {
            for (_, value) in entries {
                index.reference_contexts.push(ReferenceCompletionContext {
                    range: value_range(document, value),
                    owner: None,
                    kind: ReferenceCompletionKind::NamespacePath,
                    keys_only: false,
                    blocked: Vec::new(),
                    base: Some(base.clone()),
                });
            }
        }
        _ => {}
    }
}

fn index_values(
    index: &mut WorkspaceSemanticIndex,
    document: &WorkspaceDocument,
    values: &SpannedValue,
    owner: Option<&str>,
) {
    let SpannedValue::Mapping(entries, _) = values else {
        return;
    };
    for (key, value) in entries {
        if let SpannedValue::String(name, span) = key {
            let mut declaration = symbol(
                document,
                SemanticKind::Value,
                name,
                *span,
                schema_description("flow-schema-configuration-value"),
            );
            declaration.owner = owner.map(str::to_owned);
            index.symbols.push(declaration);
            index_templates(index, document, value, owner);
        }
    }
}

fn index_task(
    index: &mut WorkspaceSemanticIndex,
    document: &WorkspaceDocument,
    task: &SpannedValue,
    task_name: &str,
    actions: &HashMap<String, Arc<dyn Action>>,
) {
    for (field, kind) in [
        ("needs", ReferenceCompletionKind::Task),
        ("presets", ReferenceCompletionKind::Preset),
        ("stage", ReferenceCompletionKind::Stage),
    ] {
        if let Some(value) = mapping_get(task, field) {
            let range = value_range(document, value);
            index.reference_contexts.push(ReferenceCompletionContext {
                range,
                owner: Some(task_name.to_owned()),
                kind,
                keys_only: false,
                blocked: Vec::new(),
                base: None,
            });
        }
    }
    if let Some(value) = mapping_get(task, "env") {
        let blocked = match value {
            SpannedValue::Mapping(entries, _) => entries
                .iter()
                .map(|(_, value)| value_range(document, value))
                .collect(),
            _ => Vec::new(),
        };
        index.reference_contexts.push(ReferenceCompletionContext {
            range: value_range(document, value),
            owner: Some(task_name.to_owned()),
            kind: ReferenceCompletionKind::Environment,
            keys_only: matches!(value, SpannedValue::Mapping(_, _)),
            blocked,
            base: None,
        });
    }
    if let Some(on) = mapping_get(task, "on")
        && let Some(hooks) = mapping_get(on, "hooks")
    {
        index.reference_contexts.push(ReferenceCompletionContext {
            range: value_range(document, hooks),
            owner: Some(task_name.to_owned()),
            kind: ReferenceCompletionKind::Hook,
            keys_only: false,
            blocked: Vec::new(),
            base: None,
        });
    }
    if let Some(values) = mapping_get(task, "values") {
        index_values(index, document, values, Some(task_name));
    }
    if let Some(SpannedValue::Sequence(entries, _)) = mapping_get(task, "needs") {
        for entry in entries {
            if let SpannedValue::String(name, span) = entry {
                let mut reference = symbol(
                    document,
                    SemanticKind::TaskReference,
                    name,
                    *span,
                    schema_description("flow-schema-task-prerequisite"),
                );
                reference.owner = Some(task_name.to_owned());
                index.symbols.push(reference);
            }
        }
    }
    if let Some(SpannedValue::Sequence(entries, _)) = mapping_get(task, "steps") {
        for step in entries {
            let Some(uses) = mapping_get(step, "uses") else {
                continue;
            };
            let SpannedValue::String(action_id, uses_span) = uses else {
                continue;
            };
            let mut action_symbol = symbol(
                document,
                SemanticKind::Action,
                action_id,
                *uses_span,
                schema_description("flow-schema-native-action"),
            );
            action_symbol.owner = Some(task_name.to_owned());
            action_symbol.catalog_target = Some(CatalogTarget::action(action_id));
            index.symbols.push(action_symbol);
            let with = mapping_get(step, "with");
            let mut action_fields = Vec::new();
            if let Some(with) = with {
                collect_fields(
                    with,
                    "",
                    document,
                    &mut action_fields,
                    index,
                    FieldIndexContext {
                        action_id,
                        descriptor: actions
                            .get(action_id)
                            .and_then(|action| action.editor_descriptor()),
                        owner: Some(task_name),
                    },
                );
            }
            let format = action_fields
                .iter()
                .find(|(path, _)| path == "format")
                .and_then(|(_, range)| scalar_at(document, range));
            index.actions.push(ActionInvocation {
                action_id_range: range(document, *uses_span),
                with_range: with.map(|value| value_range(document, value)),
                action_id: action_id.clone(),
                task_name: task_name.to_owned(),
                fields: action_fields,
                format,
                outputs: actions
                    .get(action_id)
                    .and_then(|action| action.editor_descriptor())
                    .map(|descriptor| {
                        descriptor
                            .outputs
                            .iter()
                            .map(|output| output.name.to_owned())
                            .collect()
                    })
                    .unwrap_or_default(),
            });
        }
    }
}

fn collect_fields(
    value: &SpannedValue,
    prefix: &str,
    document: &WorkspaceDocument,
    fields: &mut Vec<(String, SourceRange)>,
    index: &mut WorkspaceSemanticIndex,
    context: FieldIndexContext<'_>,
) {
    if let SpannedValue::Sequence(entries, _) = value {
        for entry in entries {
            collect_fields(entry, prefix, document, fields, index, context);
        }
        return;
    }
    let SpannedValue::Mapping(entries, _) = value else {
        index_templates(index, document, value, context.owner);
        return;
    };
    for (key, value) in entries {
        let SpannedValue::String(name, key_span) = key else {
            continue;
        };
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}.{name}")
        };
        let detail = context
            .descriptor
            .and_then(|descriptor| {
                descriptor
                    .fields
                    .iter()
                    .find(|field| field.path == path)
                    .map(|field| field.description.to_owned())
            })
            .unwrap_or_else(|| {
                evin_locales::render_message("flow-schema-action-configuration-field", &[])
            });
        let mut field_symbol = symbol(
            document,
            SemanticKind::ActionField,
            &path,
            *key_span,
            detail,
        );
        field_symbol.catalog_target = Some(CatalogTarget::action_field(context.action_id, &path));
        index.symbols.push(field_symbol);
        fields.push((path.clone(), value_range(document, value)));
        collect_fields(value, &path, document, fields, index, context);
    }
}

fn index_templates(
    index: &mut WorkspaceSemanticIndex,
    document: &WorkspaceDocument,
    value: &SpannedValue,
    owner: Option<&str>,
) {
    match value {
        SpannedValue::String(text, span) => {
            let Ok(references) = references(text) else {
                return;
            };
            for reference in references {
                let (kind, name, detail) = match reference.kind {
                    TemplateReferenceKind::Value { key } => (
                    SemanticKind::TemplateValue,
                    key,
                    schema_description("flow-schema-resolved-configuration-value"),
                    ),
                    TemplateReferenceKind::Environment { key } => (
                    SemanticKind::TemplateEnvironment,
                    key,
                    schema_description("flow-schema-environment-variable-not-statically-navigable"),
                    ),
                    TemplateReferenceKind::TaskOutput { task, key } => (
                        SemanticKind::TemplateTaskOutput,
                        format!("{task}.{key}"),
                        evin_locales::render_message(
                            "flow-schema-declared-task-output",
                            &[("key", key.clone())],
                        ),
                    ),
                };
                index.symbols.push(SemanticSymbol {
                    kind,
                    name,
                    range: SourceRange {
                        path: document.path.clone(),
                        start: span.offset + reference.offset,
                        end: span.offset + reference.offset + reference.length,
                    },
                    detail,
                    target: None,
                    stale: false,
                    owner: owner.map(str::to_owned),
                    catalog_target: None,
                    token_kind: None,
                });
            }
        }
        SpannedValue::Sequence(values, _) => {
            for value in values {
                index_templates(index, document, value, owner);
            }
        }
        SpannedValue::Mapping(values, _) => {
            for (key, value) in values {
                index_templates(index, document, key, owner);
                index_templates(index, document, value, owner);
            }
        }
        _ => {}
    }
}

fn find_value_target(
    symbols: &[SemanticSymbol],
    source: &Path,
    task_name: Option<&str>,
    key: &str,
) -> Option<SourceRange> {
    task_name
        .and_then(|task| {
            symbols
                .iter()
                .find(|candidate| {
                    candidate.kind == SemanticKind::Value
                        && candidate.name == key
                        && candidate.owner.as_deref() == Some(task)
                })
                .map(|candidate| candidate.range.clone())
        })
        .or_else(|| {
            symbols
                .iter()
                .filter(|candidate| candidate.kind == SemanticKind::Value && candidate.name == key)
                .filter(|candidate| candidate.range.path == source && candidate.owner.is_none())
                .map(|candidate| candidate.range.clone())
                .next()
        })
        .or_else(|| {
            symbols
                .iter()
                .find(|candidate| {
                    candidate.kind == SemanticKind::Value
                        && candidate.name == key
                        && candidate.range.path == Path::new(".evin/workspace.flow")
                })
                .map(|candidate| candidate.range.clone())
        })
}
