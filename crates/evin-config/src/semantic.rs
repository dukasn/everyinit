// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 of the License.

//! Position-addressable, editor-neutral facts for Evin configuration.

use crate::parser::{EverInitConfig, load_workspace_tasks_from_sync};
use crate::source::{WorkspaceDocuments, WorkspaceSource, normalize};
use evin_action_sdk::{Action, ActionDescriptor, ActionFieldDescriptor};
use evin_diagnostic::Diagnostic;
use evin_yaml::template::{TemplateReferenceKind, references};
use evin_yaml::{EvinHeader, Span, SpannedValue, validate_evin_header};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Workspace,
    Module,
    Namespace,
    GitRules,
    Environment,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRange {
    pub path: PathBuf,
    pub start: usize,
    pub end: usize,
}

impl SourceRange {
    fn contains(&self, path: &Path, offset: usize) -> bool {
        self.path == normalize(path.to_owned()) && self.start <= offset && offset <= self.end
    }
    fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceDocument {
    pub path: PathBuf,
    pub kind: DocumentKind,
    pub header: Option<EvinHeader>,
    pub text: String,
    pub ast: Option<SpannedValue>,
    pub diagnostics: Vec<Diagnostic>,
}

/// The configuration loading result used by semantic consumers.  A failed
/// document is still retained with its current text and diagnostic, so editor
/// hosts never need to fall back to writing an unsaved buffer to disk.
pub struct WorkspaceSnapshot {
    pub config: Option<EverInitConfig>,
    pub documents: BTreeMap<PathBuf, WorkspaceDocument>,
    pub diagnostics: Vec<Diagnostic>,
}

impl WorkspaceSnapshot {
    pub async fn load(source: &dyn WorkspaceSource) -> Self {
        Self::load_sync(source)
    }

    pub fn load_sync(source: &dyn WorkspaceSource) -> Self {
        let mut diagnostics = Vec::new();
        let mut texts = match WorkspaceDocuments::load(source, ".evin/workspace.yaml") {
            Ok(documents) => documents.texts,
            Err(diagnostic) => {
                diagnostics.push(diagnostic);
                BTreeMap::new()
            }
        };
        // Preserve a root buffer even if a malformed workspace prevents normal
        // include discovery.
        let root = PathBuf::from(".evin/workspace.yaml");
        if !texts.contains_key(&root)
            && source.exists(&root)
            && let Ok(text) = source.read_to_string(&root)
        {
            texts.insert(root.clone(), text);
        }

        let mut documents = BTreeMap::new();
        for (path, text) in texts {
            let path = normalize(path);
            let kind = document_kind(&path);
            let mut document_diagnostics = Vec::new();
            let header = match validate_evin_header(&text, Some(&path)) {
                Ok(header) => header,
                Err(diagnostic) => {
                    push_unique(&mut document_diagnostics, diagnostic.clone());
                    push_unique(&mut diagnostics, diagnostic);
                    None
                }
            };
            let ast = match evin_yaml::parse_yaml(&text) {
                Ok(ast) => Some(ast),
                Err(error) => {
                    let diagnostic = error
                        .into_diagnostic("YAML Syntax Error", Some(path.display().to_string()));
                    push_unique(&mut document_diagnostics, diagnostic.clone());
                    push_unique(&mut diagnostics, diagnostic);
                    None
                }
            };
            if kind == DocumentKind::Environment
                && let Err(diagnostic) =
                    <crate::environment::EnvironmentYaml as evin_yaml::YamlDocument>::from_str(
                        &text,
                        Some(&path),
                    )
            {
                push_unique(&mut document_diagnostics, diagnostic.clone());
                push_unique(&mut diagnostics, diagnostic);
            }
            documents.insert(
                path.clone(),
                WorkspaceDocument {
                    path,
                    kind,
                    header,
                    text,
                    ast,
                    diagnostics: document_diagnostics,
                },
            );
        }
        let config = match load_workspace_tasks_from_sync(source) {
            Ok(config) => Some(config),
            Err(diagnostic) => {
                push_unique(&mut diagnostics, diagnostic);
                None
            }
        };
        Self {
            config,
            documents,
            diagnostics,
        }
    }
}

fn push_unique(diagnostics: &mut Vec<Diagnostic>, diagnostic: Diagnostic) {
    if !diagnostics.contains(&diagnostic) {
        diagnostics.push(diagnostic);
    }
}

fn document_kind(path: &Path) -> DocumentKind {
    let file = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if file == "workspace.yaml" {
        DocumentKind::Workspace
    } else if file == "module.yaml" {
        DocumentKind::Module
    } else if file.contains("gitrules") {
        DocumentKind::GitRules
    } else if file == "environment.yaml" {
        DocumentKind::Environment
    } else if matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("yaml" | "yml")
    ) {
        DocumentKind::Namespace
    } else {
        DocumentKind::Unknown
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticKind {
    Workspace,
    Module,
    Namespace,
    Stage,
    Task,
    TaskReference,
    Value,
    Action,
    ActionField,
    TemplateValue,
    TemplateEnvironment,
    TemplateTaskOutput,
    EnvironmentVariable,
    PackagePolicy,
}

#[derive(Debug, Clone)]
pub struct SemanticSymbol {
    pub kind: SemanticKind,
    pub name: String,
    pub range: SourceRange,
    pub detail: String,
    pub target: Option<SourceRange>,
    pub stale: bool,
    pub owner: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CompletionCandidate {
    pub label: String,
    pub detail: String,
    pub documentation: String,
    pub snippet: String,
}

#[derive(Debug, Clone)]
pub struct HoverInfo {
    pub range: SourceRange,
    pub markdown: String,
    pub stale: bool,
}

#[derive(Debug, Clone)]
struct ActionInvocation {
    range: SourceRange,
    with_range: Option<SourceRange>,
    action_id: String,
    task_name: String,
    fields: Vec<(String, SourceRange)>,
    format: Option<String>,
    outputs: Vec<String>,
}

/// Immutable semantic facts.  Its API deliberately speaks in workspace paths
/// and byte offsets; an LSP adapter performs UTF-16 conversion at its edge.
pub struct WorkspaceSemanticIndex {
    pub diagnostics: Vec<Diagnostic>,
    pub symbols: Vec<SemanticSymbol>,
    documents: BTreeMap<PathBuf, WorkspaceDocument>,
    actions: Vec<ActionInvocation>,
}

/// Owned configuration plus its side-effect-free semantic and validation
/// results.  CLI and future editor hosts can share this boundary without
/// reloading files or running native actions.
pub struct WorkspaceSemanticAnalysis {
    pub config: Option<EverInitConfig>,
    pub index: WorkspaceSemanticIndex,
    pub findings: Vec<crate::analysis::AnalysisFinding>,
}

impl WorkspaceSemanticAnalysis {
    pub fn build(snapshot: WorkspaceSnapshot, actions: &HashMap<String, Arc<dyn Action>>) -> Self {
        let WorkspaceSnapshot {
            config,
            documents,
            diagnostics,
        } = snapshot;
        let findings = config
            .as_ref()
            .map(|config| crate::analysis::analyze(config, actions).findings)
            .unwrap_or_default();
        Self {
            index: WorkspaceSemanticIndex::build_parts(
                documents,
                diagnostics,
                config.as_ref(),
                actions,
            ),
            config,
            findings,
        }
    }
}

impl WorkspaceSemanticIndex {
    pub fn build(snapshot: WorkspaceSnapshot, actions: &HashMap<String, Arc<dyn Action>>) -> Self {
        let WorkspaceSnapshot {
            config,
            documents,
            diagnostics,
        } = snapshot;
        Self::build_parts(documents, diagnostics, config.as_ref(), actions)
    }

    fn build_parts(
        documents: BTreeMap<PathBuf, WorkspaceDocument>,
        diagnostics: Vec<Diagnostic>,
        config: Option<&EverInitConfig>,
        actions: &HashMap<String, Arc<dyn Action>>,
    ) -> Self {
        let mut index = Self {
            diagnostics,
            symbols: Vec::new(),
            documents,
            actions: Vec::new(),
        };
        let task_names = config
            .map(|config| {
                config
                    .tasks
                    .iter()
                    .map(|task| {
                        (
                            normalize(PathBuf::from(
                                task.source_file.as_deref().unwrap_or_default(),
                            )),
                            task.name.clone(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let paths = index.documents.keys().cloned().collect::<Vec<_>>();
        for path in paths {
            let Some(document) = index.documents.get(&path).cloned() else {
                continue;
            };
            let Some(ast) = document.ast.clone() else {
                continue;
            };
            index_document(&mut index, &document, &ast, &task_names, actions);
        }
        index.resolve_value_targets();
        index.symbols.sort_by(|left, right| {
            (
                &left.range.path,
                left.range.start,
                left.range.end,
                &left.name,
            )
                .cmp(&(
                    &right.range.path,
                    right.range.start,
                    right.range.end,
                    &right.name,
                ))
        });
        index
    }

    pub fn document(&self, path: &Path) -> Option<&WorkspaceDocument> {
        self.documents.get(&normalize(path.to_owned()))
    }

    pub fn documents(&self) -> impl Iterator<Item = &WorkspaceDocument> {
        self.documents.values()
    }

    pub fn node_at(&self, path: &Path, offset: usize) -> Option<&SemanticSymbol> {
        self.symbols
            .iter()
            .filter(|symbol| symbol.range.contains(path, offset))
            .min_by_key(|symbol| symbol.range.len())
    }

    pub fn hover_at(&self, path: &Path, offset: usize) -> Option<HoverInfo> {
        let symbol = self.node_at(path, offset)?;
        Some(HoverInfo {
            range: symbol.range.clone(),
            markdown: format!("**{}**\n\n{}", symbol.name, symbol.detail),
            stale: symbol.stale,
        })
    }

    pub fn definition_at(&self, path: &Path, offset: usize) -> Option<SourceRange> {
        self.node_at(path, offset)?.target.clone()
    }

    pub fn references_at(&self, path: &Path, offset: usize) -> Vec<SourceRange> {
        let Some(target) = self
            .node_at(path, offset)
            .and_then(|symbol| symbol.target.clone().or_else(|| Some(symbol.range.clone())))
        else {
            return Vec::new();
        };
        self.symbols
            .iter()
            .filter(|symbol| symbol.target.as_ref() == Some(&target))
            .map(|symbol| symbol.range.clone())
            .collect()
    }

    pub fn document_symbols(&self, path: &Path) -> Vec<&SemanticSymbol> {
        let path = normalize(path.to_owned());
        self.symbols
            .iter()
            .filter(|symbol| symbol.range.path == path)
            .filter(|symbol| {
                matches!(
                    symbol.kind,
                    SemanticKind::Workspace
                        | SemanticKind::Module
                        | SemanticKind::Namespace
                        | SemanticKind::EnvironmentVariable
                        | SemanticKind::Task
                        | SemanticKind::Value
                        | SemanticKind::PackagePolicy
                )
            })
            .collect()
    }

    pub fn workspace_symbols(&self, query: &str) -> Vec<&SemanticSymbol> {
        let query = query.to_ascii_lowercase();
        self.symbols
            .iter()
            .filter(|symbol| symbol.name.to_ascii_lowercase().contains(&query))
            .collect()
    }

    pub fn completions_at(
        &self,
        path: &Path,
        offset: usize,
        actions: &HashMap<String, Arc<dyn Action>>,
    ) -> Vec<CompletionCandidate> {
        let Some(document) = self.document(path) else {
            return Vec::new();
        };
        let mut candidates = match document.kind {
            DocumentKind::Workspace => {
                keys(&["workspace", "stages", "env", "values", "tasks", "expose"])
            }
            DocumentKind::Module => keys(&["module", "env", "values", "tasks"]),
            DocumentKind::Namespace => keys(&["namespace", "env", "values", "tasks"]),
            DocumentKind::GitRules => keys(&["releases"]),
            DocumentKind::Environment => environment_candidates(),
            DocumentKind::Unknown => Vec::new(),
        };
        for invocation in &self.actions {
            if !invocation.range.contains(path, offset) {
                continue;
            }
            if let Some(descriptor) = actions
                .get(&invocation.action_id)
                .and_then(|action| action.editor_descriptor())
            {
                if invocation
                    .with_range
                    .as_ref()
                    .is_some_and(|range| range.contains(path, offset))
                {
                    candidates.extend(action_field_candidates(
                        descriptor, invocation, path, offset,
                    ));
                    candidates.extend(enum_value_candidates(descriptor, invocation, path, offset));
                } else {
                    candidates.push(CompletionCandidate {
                        label: invocation.action_id.clone(),
                        detail: "Native action".into(),
                        documentation: descriptor.summary.into(),
                        snippet: invocation.action_id.clone(),
                    });
                }
            }
        }
        candidates.sort_by(|left, right| left.label.cmp(&right.label));
        candidates.dedup_by(|left, right| left.label == right.label);
        candidates
    }

    fn resolve_value_targets(&mut self) {
        let symbols = self.symbols.clone();
        let actions = self.actions.clone();
        for symbol in &mut self.symbols {
            let target = match symbol.kind {
                SemanticKind::TemplateValue => find_value_target(
                    &symbols,
                    &symbol.range.path,
                    symbol.owner.as_deref(),
                    &symbol.name,
                ),
                SemanticKind::TemplateTaskOutput => {
                    resolve_task_output(&actions, &symbols, &symbol.name)
                }
                SemanticKind::TaskReference => symbols
                    .iter()
                    .find(|candidate| {
                        candidate.kind == SemanticKind::Task && candidate.name == symbol.name
                    })
                    .map(|candidate| candidate.range.clone()),
                _ => None,
            };
            symbol.target = target;
        }
    }
}

fn resolve_task_output(
    actions: &[ActionInvocation],
    symbols: &[SemanticSymbol],
    reference: &str,
) -> Option<SourceRange> {
    let (task, output) = reference.rsplit_once('.')?;
    let task_symbol = symbols
        .iter()
        .find(|candidate| candidate.kind == SemanticKind::Task && candidate.name == task)
        .or_else(|| {
            symbols.iter().find(|candidate| {
                candidate.kind == SemanticKind::Task
                    && candidate.name.rsplit(':').next() == Some(task)
            })
        })?;
    actions
        .iter()
        .filter(|invocation| invocation.task_name == task_symbol.name)
        .any(|invocation| invocation.outputs.iter().any(|name| name == output))
        .then(|| task_symbol.range.clone())
}

/// Overlay-aware state for editor hosts.  A malformed current document never
/// contributes stale facts to cross-file answers; callers can inspect its
/// retained local symbols through `stale_symbols`.
#[derive(Default)]
pub struct WorkspaceAnalysisSession {
    last_valid: Option<WorkspaceSemanticIndex>,
    pub current: Option<WorkspaceSemanticIndex>,
    pub stale_symbols: BTreeMap<PathBuf, Vec<SemanticSymbol>>,
}

impl WorkspaceAnalysisSession {
    pub fn update(
        &mut self,
        snapshot: WorkspaceSnapshot,
        actions: &HashMap<String, Arc<dyn Action>>,
    ) {
        self.update_index(WorkspaceSemanticIndex::build(snapshot, actions));
    }

    pub fn update_index(&mut self, index: WorkspaceSemanticIndex) {
        let invalid = index
            .documents
            .values()
            .filter(|document| document.ast.is_none())
            .map(|document| document.path.clone())
            .collect::<Vec<_>>();
        self.stale_symbols.clear();
        if let Some(previous) = &self.last_valid {
            for path in invalid {
                let symbols = previous
                    .document_symbols(&path)
                    .into_iter()
                    .cloned()
                    .map(|mut symbol| {
                        symbol.stale = true;
                        symbol
                    })
                    .collect::<Vec<_>>();
                if !symbols.is_empty() {
                    self.stale_symbols.insert(path, symbols);
                }
            }
        }
        if index
            .documents
            .values()
            .all(|document| document.ast.is_some())
        {
            self.last_valid = Some(index);
            self.current = self.last_valid.as_ref().map(clone_index);
        } else {
            self.current = Some(index);
        }
    }

    /// Return a retained local declaration only when the current invalid
    /// buffer still contains the same declaration at the same byte range.
    /// Stale data is deliberately never used for definitions or references.
    pub fn stale_symbol_at(&self, path: &Path, offset: usize) -> Option<&SemanticSymbol> {
        let path = normalize(path.to_owned());
        let text = self.current.as_ref()?.document(&path)?.text.as_str();
        self.stale_symbols.get(&path)?.iter().find(|symbol| {
            symbol.range.contains(&path, offset)
                && matches!(symbol.kind, SemanticKind::Task | SemanticKind::Value)
                && text
                    .get(symbol.range.start..symbol.range.end)
                    .is_some_and(|current| current == symbol.name)
        })
    }

    pub fn stale_document_symbols(&self, path: &Path) -> Vec<&SemanticSymbol> {
        let path = normalize(path.to_owned());
        let Some(text) = self
            .current
            .as_ref()
            .and_then(|index| index.document(&path))
            .map(|document| document.text.as_str())
        else {
            return Vec::new();
        };
        self.stale_symbols
            .get(&path)
            .into_iter()
            .flatten()
            .filter(|symbol| {
                matches!(symbol.kind, SemanticKind::Task | SemanticKind::Value)
                    && text
                        .get(symbol.range.start..symbol.range.end)
                        .is_some_and(|current| current == symbol.name)
            })
            .collect()
    }
}

fn clone_index(index: &WorkspaceSemanticIndex) -> WorkspaceSemanticIndex {
    WorkspaceSemanticIndex {
        diagnostics: index.diagnostics.clone(),
        symbols: index.symbols.clone(),
        documents: index.documents.clone(),
        actions: index.actions.clone(),
    }
}

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
        DocumentKind::GitRules => "GitRules",
        DocumentKind::Environment => "environment",
        DocumentKind::Unknown => return,
    };
    index.symbols.push(symbol(
        document,
        if document.kind == DocumentKind::GitRules {
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
        "Configuration document.",
    ));
    if document.kind == DocumentKind::GitRules {
        index_gitrules(index, document, ast);
        return;
    }
    if document.kind == DocumentKind::Environment {
        index_environment(index, document, ast);
        return;
    }
    if let Some(values) = mapping_get(ast, "values") {
        index_values(index, document, values, None);
    }
    if let Some(SpannedValue::Sequence(entries, _)) = mapping_get(ast, "stages") {
        for entry in entries {
            if let SpannedValue::String(name, span) = entry {
                index.symbols.push(symbol(
                    index_document_ref(document),
                    SemanticKind::Stage,
                    name,
                    *span,
                    "Workspace stage.",
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
            "Evin task.",
        ));
        index_task(index, document, value, &full_name, actions);
    }
}

fn index_document_ref(document: &WorkspaceDocument) -> &WorkspaceDocument {
    document
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
                "Configuration value.",
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
    if let Some(values) = mapping_get(task, "values") {
        index_values(index, document, values, Some(task_name));
    }
    if let Some(SpannedValue::Sequence(entries, _)) = mapping_get(task, "needs") {
        for entry in entries {
            if let SpannedValue::String(name, span) = entry {
                index.symbols.push(symbol(
                    document,
                    SemanticKind::TaskReference,
                    name,
                    *span,
                    "Task prerequisite.",
                ));
            }
        }
    }
    let Some(steps) = mapping_get(task, "steps") else {
        return;
    };
    let SpannedValue::Sequence(entries, _) = steps else {
        return;
    };
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
            "Native action.",
        );
        action_symbol.owner = Some(task_name.to_owned());
        index.symbols.push(action_symbol);
        let with = mapping_get(step, "with");
        let mut fields = Vec::new();
        if let Some(with) = with {
            collect_fields(
                with,
                "",
                document,
                &mut fields,
                index,
                actions
                    .get(action_id)
                    .and_then(|action| action.editor_descriptor()),
                Some(task_name),
            );
        }
        let format = fields
            .iter()
            .find(|(path, _)| path == "format")
            .and_then(|(_, range)| scalar_at(document, range));
        index.actions.push(ActionInvocation {
            range: value_range(document, step),
            with_range: with.map(|value| value_range(document, value)),
            action_id: action_id.clone(),
            task_name: task_name.to_owned(),
            fields,
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

fn collect_fields(
    value: &SpannedValue,
    prefix: &str,
    document: &WorkspaceDocument,
    fields: &mut Vec<(String, SourceRange)>,
    index: &mut WorkspaceSemanticIndex,
    descriptor: Option<&ActionDescriptor>,
    owner: Option<&str>,
) {
    if let SpannedValue::Sequence(entries, _) = value {
        for entry in entries {
            collect_fields(entry, prefix, document, fields, index, descriptor, owner);
        }
        return;
    }
    let SpannedValue::Mapping(entries, _) = value else {
        index_templates(index, document, value, owner);
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
        let detail = descriptor
            .and_then(|descriptor| {
                descriptor
                    .fields
                    .iter()
                    .find(|field| field.path == path)
                    .map(|field| field.description)
            })
            .unwrap_or("Action configuration field.");
        index.symbols.push(symbol(
            document,
            SemanticKind::ActionField,
            &path,
            *key_span,
            detail,
        ));
        fields.push((path.clone(), value_range(document, value)));
        collect_fields(value, &path, document, fields, index, descriptor, owner);
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
                        "Resolved configuration value.".to_owned(),
                    ),
                    TemplateReferenceKind::Environment { key } => (
                        SemanticKind::TemplateEnvironment,
                        key,
                        "Environment variable; not statically navigable.".to_owned(),
                    ),
                    TemplateReferenceKind::TaskOutput { task, key } => (
                        SemanticKind::TemplateTaskOutput,
                        format!("{task}.{key}"),
                        format!("Declared output '{key}' from prerequisite task."),
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

fn index_gitrules(
    index: &mut WorkspaceSemanticIndex,
    document: &WorkspaceDocument,
    ast: &SpannedValue,
) {
    let Some(releases) = mapping_get(ast, "releases") else {
        return;
    };
    let Some(packages) = mapping_get(releases, "packages") else {
        return;
    };
    let SpannedValue::Mapping(entries, _) = packages else {
        return;
    };
    for (key, _) in entries {
        if let SpannedValue::String(name, span) = key {
            index.symbols.push(symbol(
                document,
                SemanticKind::PackagePolicy,
                name,
                *span,
                "GitRules release package policy.",
            ));
        }
    }
}

fn index_environment(
    index: &mut WorkspaceSemanticIndex,
    document: &WorkspaceDocument,
    ast: &SpannedValue,
) {
    let Some(environments) = mapping_get(ast, "environments") else {
        return;
    };
    let SpannedValue::Mapping(entries, _) = environments else {
        return;
    };
    for (key, value) in entries {
        if let SpannedValue::String(name, span) = key {
            index.symbols.push(symbol(
                document,
                SemanticKind::EnvironmentVariable,
                name,
                *span,
                "Declared environment variable.",
            ));
            index_templates(index, document, value, None);
        }
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
                        && candidate.range.path == Path::new(".evin/workspace.yaml")
                })
                .map(|candidate| candidate.range.clone())
        })
}

fn action_field_candidates(
    descriptor: &ActionDescriptor,
    invocation: &ActionInvocation,
    path: &Path,
    offset: usize,
) -> Vec<CompletionCandidate> {
    let containing = invocation
        .fields
        .iter()
        .filter(|(_, range)| range.contains(path, offset))
        .max_by_key(|(field, _)| field.matches('.').count())
        .map(|(field, _)| field.as_str());
    let prefix = containing
        .map(|field| {
            let nested = format!("{field}.");
            if descriptor
                .fields
                .iter()
                .any(|candidate| candidate.path.starts_with(&nested))
            {
                nested
            } else {
                field
                    .rsplit_once('.')
                    .map(|(parent, _)| format!("{parent}."))
                    .unwrap_or_default()
            }
        })
        .unwrap_or_default();
    descriptor
        .fields
        .iter()
        .filter(|field| condition_matches(field, invocation.format.as_deref()))
        .filter(|field| {
            field
                .path
                .strip_prefix(&prefix)
                .is_some_and(|rest| !rest.contains('.'))
        })
        .filter(|field| {
            !invocation
                .fields
                .iter()
                .any(|(existing, _)| existing == field.path)
        })
        .map(|field| CompletionCandidate {
            label: field.path.into(),
            detail: action_field_detail(field),
            documentation: field.description.into(),
            snippet: field.snippet(),
        })
        .collect()
}

fn condition_matches(field: &ActionFieldDescriptor, format: Option<&str>) -> bool {
    let Some(condition) = field.condition else {
        return true;
    };
    let Some((key, choices)) = condition.split_once('=') else {
        return true;
    };
    key != "format" || format.is_some_and(|value| choices.split('|').any(|choice| choice == value))
}

fn action_field_detail(field: &ActionFieldDescriptor) -> String {
    let required = if field.required {
        "required"
    } else {
        "optional"
    };
    format!(
        "{} {required}",
        match field.kind {
            evin_action_sdk::ActionFieldKind::String => "string",
            evin_action_sdk::ActionFieldKind::Integer => "integer",
            evin_action_sdk::ActionFieldKind::Boolean => "boolean",
            evin_action_sdk::ActionFieldKind::List => "list",
            evin_action_sdk::ActionFieldKind::Object => "object",
            evin_action_sdk::ActionFieldKind::Enum(_) => "enum",
        }
    )
}

fn enum_value_candidates(
    descriptor: &ActionDescriptor,
    invocation: &ActionInvocation,
    path: &Path,
    offset: usize,
) -> Vec<CompletionCandidate> {
    let Some((field_path, _)) = invocation
        .fields
        .iter()
        .filter(|(_, range)| range.contains(path, offset))
        .max_by_key(|(field, _)| field.matches('.').count())
    else {
        return Vec::new();
    };
    let Some(ActionFieldDescriptor {
        kind: evin_action_sdk::ActionFieldKind::Enum(values),
        description,
        ..
    }) = descriptor
        .fields
        .iter()
        .find(|field| field.path == field_path)
    else {
        return Vec::new();
    };
    values
        .iter()
        .map(|value| CompletionCandidate {
            label: (*value).to_owned(),
            detail: "enum value".into(),
            documentation: (*description).to_owned(),
            snippet: (*value).to_owned(),
        })
        .collect()
}

fn keys(keys: &[&str]) -> Vec<CompletionCandidate> {
    keys.iter()
        .map(|key| CompletionCandidate {
            label: (*key).into(),
            detail: "Configuration key".into(),
            documentation: "Evin configuration field.".into(),
            snippet: format!("{key}: ${{1}}"),
        })
        .collect()
}

fn environment_candidates() -> Vec<CompletionCandidate> {
    let mut candidates = keys(&["version", "presets", "environments"]);
    candidates.extend(keys(&[
        "type",
        "required",
        "pattern",
        "description",
        "default",
        "scopes",
        "stages",
    ]));
    for value in ["string", "number", "boolean", "secret"] {
        candidates.push(CompletionCandidate {
            label: value.into(),
            detail: "environment variable type".into(),
            documentation: "Allowed environment variable type.".into(),
            snippet: value.into(),
        });
    }
    candidates
}

fn mapping_get<'a>(value: &'a SpannedValue, wanted: &str) -> Option<&'a SpannedValue> {
    let SpannedValue::Mapping(entries, _) = value else {
        return None;
    };
    entries.iter().find_map(|(key, value)| match key {
        SpannedValue::String(name, _) if name == wanted => Some(value),
        _ => None,
    })
}

fn scalar_at(document: &WorkspaceDocument, range: &SourceRange) -> Option<String> {
    document.text.get(range.start..range.end).map(|value| {
        value
            .trim_matches(|character: char| {
                character == '\'' || character == '"' || character.is_whitespace()
            })
            .to_owned()
    })
}

fn range(document: &WorkspaceDocument, span: Span) -> SourceRange {
    SourceRange {
        path: document.path.clone(),
        start: span.offset,
        end: span.offset.saturating_add(span.length.max(1)),
    }
}

fn value_range(document: &WorkspaceDocument, value: &SpannedValue) -> SourceRange {
    let span = value.span();
    let mut end = span.offset.saturating_add(span.length.max(1));
    match value {
        SpannedValue::Sequence(values, _) => {
            for value in values {
                end = end.max(value_range(document, value).end);
            }
        }
        SpannedValue::Mapping(values, _) => {
            for (key, value) in values {
                end = end.max(value_range(document, key).end);
                end = end.max(value_range(document, value).end);
            }
        }
        _ => {}
    }
    SourceRange {
        path: document.path.clone(),
        start: span.offset,
        end,
    }
}

fn symbol(
    document: &WorkspaceDocument,
    kind: SemanticKind,
    name: &str,
    span: Span,
    detail: impl Into<String>,
) -> SemanticSymbol {
    SemanticSymbol {
        kind,
        name: name.into(),
        range: range(document, span),
        detail: detail.into(),
        target: None,
        stale: false,
        owner: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::OverlaySource;
    use evin_action_sdk::{
        ActionDescriptor, ActionFieldDescriptor, ActionFieldKind, ActionOutputDescriptor,
        RunnerContext,
    };

    const DEMO_DESCRIPTOR: ActionDescriptor = ActionDescriptor {
        summary: "Test action.",
        fields: &[
            ActionFieldDescriptor {
                path: "format",
                kind: ActionFieldKind::Enum(&["arch", "tar"]),
                required: true,
                description: "Output format.",
                condition: None,
            },
            ActionFieldDescriptor {
                path: "arch_only",
                kind: ActionFieldKind::String,
                required: false,
                description: "Arch setting.",
                condition: Some("format=arch"),
            },
            ActionFieldDescriptor {
                path: "tar_only",
                kind: ActionFieldKind::String,
                required: false,
                description: "Tar setting.",
                condition: Some("format=tar"),
            },
            ActionFieldDescriptor {
                path: "label",
                kind: ActionFieldKind::String,
                required: false,
                description: "Artifact label.",
                condition: None,
            },
        ],
        outputs: &[ActionOutputDescriptor {
            name: "artifact",
            description: "Artifact output.",
        }],
    };

    struct DemoAction;
    #[async_trait::async_trait]
    impl Action for DemoAction {
        fn editor_descriptor(&self) -> Option<&'static ActionDescriptor> {
            Some(&DEMO_DESCRIPTOR)
        }
        fn validate_config(&self, _: Option<&SpannedValue>) -> Result<(), evin_yaml::ConfigError> {
            Ok(())
        }
        async fn execute(
            &self,
            _: &RunnerContext,
            _: Option<SpannedValue>,
        ) -> Result<(), Diagnostic> {
            Ok(())
        }
    }

    fn actions() -> HashMap<String, Arc<dyn Action>> {
        HashMap::from([(
            "test/action@v1".into(),
            Arc::new(DemoAction) as Arc<dyn Action>,
        )])
    }

    #[tokio::test]
    async fn snapshot_keeps_raw_documents_and_template_definitions() {
        let mut source = OverlaySource::default();
        source.insert(".evin/workspace.yaml", "workspace: { name: app, version: 1.0.0, include: [.evin/packages.yaml] }\nstages: [Build]\nvalues: { artifact: dist }\n");
        source.insert(".evin/packages.yaml", "namespace: { name: packages }\ntasks:\n  build:\n    values: { artifact: local }\n    steps:\n      - uses: evin/archive@v1\n        with: { format: arch, package: { name: '${artifact}' } }\n");
        let snapshot = WorkspaceSnapshot::load(&source).await;
        assert_eq!(snapshot.documents.len(), 2);
        let index = WorkspaceSemanticIndex::build(snapshot, &HashMap::new());
        let reference = index
            .symbols
            .iter()
            .find(|symbol| symbol.kind == SemanticKind::TemplateValue)
            .unwrap();
        assert_eq!(reference.name, "artifact");
        assert!(reference.target.is_some());
    }

    #[tokio::test]
    async fn snapshot_retains_headers_and_reports_an_invalid_header_once() {
        let mut source = OverlaySource::default();
        source.insert(
            ".evin/workspace.yaml",
            "#!evin/v1\nworkspace: { name: app, version: 1.0.0 }\nstages: []\n",
        );
        let snapshot = WorkspaceSnapshot::load(&source).await;
        assert_eq!(
            snapshot
                .documents
                .get(&PathBuf::from(".evin/workspace.yaml"))
                .and_then(|document| document.header)
                .map(|header| header.version),
            Some(1)
        );

        source.insert(".evin/workspace.yaml", "#!evin/v2\nworkspace: [\n");
        let snapshot = WorkspaceSnapshot::load(&source).await;
        let document = snapshot
            .documents
            .get(&PathBuf::from(".evin/workspace.yaml"))
            .unwrap();
        assert!(document.header.is_none());
        assert!(document.ast.is_none());
        assert_eq!(
            snapshot
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code.key == "issue-evin-header-invalid")
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn invalid_overlay_keeps_only_stale_local_symbols() {
        let mut source = OverlaySource::default();
        let valid = "workspace: { name: app, version: 1.0.0 }\nstages: []\ntasks: { build: { run: echo } }\n";
        source.insert(".evin/workspace.yaml", valid);
        let mut session = WorkspaceAnalysisSession::default();
        session.update(WorkspaceSnapshot::load(&source).await, &HashMap::new());
        let build = session
            .current
            .as_ref()
            .unwrap()
            .symbols
            .iter()
            .find(|symbol| symbol.kind == SemanticKind::Task && symbol.name == "build")
            .unwrap()
            .range
            .clone();
        source.insert(".evin/workspace.yaml", format!("{valid}broken: [\n"));
        session.update(WorkspaceSnapshot::load(&source).await, &HashMap::new());
        assert!(
            session
                .stale_symbols
                .contains_key(&PathBuf::from(".evin/workspace.yaml"))
        );
        assert!(
            session
                .stale_symbol_at(&build.path, build.start)
                .is_some_and(|symbol| symbol.stale)
        );
        assert!(
            session
                .current
                .as_ref()
                .unwrap()
                .symbols
                .iter()
                .all(|symbol| symbol.target.is_none())
        );
    }

    #[tokio::test]
    async fn index_resolves_task_values_outputs_and_conditional_completion() {
        let mut source = OverlaySource::default();
        let text = "workspace: { name: app, version: 1.0.0 }\nstages: []\nvalues: { label: workspace }\ntasks:\n  build:\n    values: { label: local }\n    steps:\n      - uses: test/action@v1\n        with:\n          format: arch\n          label: '${label}'\n  publish:\n    needs: [build]\n    steps:\n      - uses: test/action@v1\n        with: { label: '${build.artifact}' }\n";
        source.insert(".evin/workspace.yaml", text);
        let index =
            WorkspaceSemanticIndex::build(WorkspaceSnapshot::load(&source).await, &actions());
        let value_reference = index
            .symbols
            .iter()
            .find(|symbol| symbol.kind == SemanticKind::TemplateValue)
            .unwrap();
        let target = index
            .definition_at(&value_reference.range.path, value_reference.range.start)
            .unwrap();
        assert_eq!(target.path, PathBuf::from(".evin/workspace.yaml"));
        assert!(index.symbols.iter().any(
            |symbol| symbol.kind == SemanticKind::TemplateTaskOutput && symbol.target.is_some()
        ));
        let offset = text.find("format: arch").unwrap() + "format: arch".len();
        let completions =
            index.completions_at(Path::new(".evin/workspace.yaml"), offset, &actions());
        assert!(
            completions
                .iter()
                .any(|candidate| candidate.label == "arch_only")
        );
        assert!(
            !completions
                .iter()
                .any(|candidate| candidate.label == "tar_only")
        );
        assert!(
            completions
                .iter()
                .any(|candidate| candidate.label == "arch")
        );
    }
}
