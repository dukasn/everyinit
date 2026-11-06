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

#[derive(Debug, Clone)]
pub struct SemanticSymbol {
    pub kind: SemanticKind,
    pub name: String,
    pub range: SourceRange,
    pub detail: String,
    pub target: Option<SourceRange>,
    pub stale: bool,
    pub owner: Option<String>,
    pub catalog_target: Option<CatalogTarget>,
    pub token_kind: Option<SemanticTokenKind>,
}

#[derive(Debug, Clone)]
pub struct CompletionCandidate {
    pub label: String,
    pub detail: String,
    pub documentation: String,
    pub snippet: String,
    pub catalog_target: Option<CatalogTarget>,
    pub replacement: Option<SourceRange>,
    pub filter_text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HoverInfo {
    pub range: SourceRange,
    pub markdown: String,
    pub stale: bool,
    pub catalog_target: Option<CatalogTarget>,
}

#[derive(Debug, Clone)]
struct ActionInvocation {
    action_id_range: SourceRange,
    with_range: Option<SourceRange>,
    action_id: String,
    task_name: String,
    fields: Vec<(String, SourceRange)>,
    format: Option<String>,
    outputs: Vec<String>,
}

#[derive(Debug, Clone)]
struct SchemaContext {
    range: SourceRange,
    schema: evin_flow::schema::Schema,
    present: BTreeSet<String>,
    keys: Vec<SourceRange>,
    occupied: Vec<SourceRange>,
    path: String,
}

#[derive(Debug, Clone)]
struct SchemaChoiceContext {
    range: SourceRange,
    choices: &'static [&'static str],
    description_key: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReferenceCompletionKind {
    Task,
    Stage,
    Preset,
    Environment,
    Hook,
    ModulePath,
    NamespacePath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TemplateComponentKind {
    Unqualified,
    Environment,
    TaskOutput,
}

#[derive(Debug, Clone)]
struct TemplateComponentContext {
    kind: TemplateComponentKind,
    qualifier: String,
    prefix: String,
    replacement: SourceRange,
}

#[derive(Debug, Clone)]
struct ReferenceCompletionContext {
    range: SourceRange,
    owner: Option<String>,
    kind: ReferenceCompletionKind,
    keys_only: bool,
    blocked: Vec<SourceRange>,
    base: Option<PathBuf>,
}

enum CompletionContextResult {
    Candidates(Vec<CompletionCandidate>),
    Empty,
    Unsupported,
}

impl CompletionContextResult {
    fn from_candidates(candidates: Vec<CompletionCandidate>) -> Self {
        if candidates.is_empty() {
            Self::Empty
        } else {
            Self::Candidates(candidates)
        }
    }

    fn into_candidates(self) -> Vec<CompletionCandidate> {
        match self {
            Self::Candidates(candidates) => candidates,
            Self::Empty | Self::Unsupported => Vec::new(),
        }
    }
}

#[derive(Clone, Copy)]
struct FieldIndexContext<'a> {
    action_id: &'a str,
    descriptor: Option<&'a ActionDescriptor>,
    owner: Option<&'a str>,
}

/// Immutable semantic facts.  Its API deliberately speaks in workspace paths
/// and byte offsets; an LSP adapter performs UTF-16 conversion at its edge.
pub struct WorkspaceSemanticIndex {
    pub diagnostics: Vec<Diagnostic>,
    pub symbols: Vec<SemanticSymbol>,
    documents: BTreeMap<PathBuf, WorkspaceDocument>,
    actions: Vec<ActionInvocation>,
    schema_contexts: Vec<SchemaContext>,
    schema_choices: Vec<SchemaChoiceContext>,
    reference_contexts: Vec<ReferenceCompletionContext>,
    schema_value_tokens: Vec<SemanticTokenFact>,
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
        mut diagnostics: Vec<Diagnostic>,
        config: Option<&EverInitConfig>,
        actions: &HashMap<String, Arc<dyn Action>>,
    ) -> Self {
        let documents = documents
            .into_iter()
            .map(|(path, mut document)| {
                if let Some(ast) = &mut document.ast {
                    normalize_ast_spans(ast, &document.text);
                }
                if let Some(ast) = &mut document.completion_ast {
                    normalize_ast_spans(ast, &document.text);
                }
                (path, document)
            })
            .collect();
        diagnostics.retain(|diagnostic| {
            !matches!(
                diagnostic.code.key,
                "issue-invalid-stage"
                    | "issue-invalid-stage-casing"
                    | "issue-invalid-task-casing"
            )
        });
        let mut index = Self {
            diagnostics,
            symbols: Vec::new(),
            documents,
            actions: Vec::new(),
            schema_contexts: Vec::new(),
            schema_choices: Vec::new(),
            reference_contexts: Vec::new(),
            schema_value_tokens: Vec::new(),
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
            .unwrap_or_else(|| task_names_from_documents(&index.documents));
        let paths = index.documents.keys().cloned().collect::<Vec<_>>();
        for path in paths {
            let Some(document) = index.documents.get(&path).cloned() else {
                continue;
            };
            let Some(ast) = document.ast.clone() else {
                if let Some(completion_ast) = document.completion_ast.clone() {
                    let symbol_count = index.symbols.len();
                    let token_count = index.schema_value_tokens.len();
                    index_document(&mut index, &document, &completion_ast, &task_names, actions);
                    // Recovery gives completion its syntax context and local
                    // action shape; recovered declarations never become
                    // cross-document semantic facts.
                    index.symbols.truncate(symbol_count);
                    index.schema_value_tokens.truncate(token_count);
                }
                continue;
            };
            index_document(&mut index, &document, &ast, &task_names, actions);
        }
        validate_workspace_references(&mut index);
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

    /// Return standard semantic-token roles for one valid Evin document.
    /// Unknown mapping keys remain properties; recognized Evin syntax overlays
    /// those generic facts with its more specific role.
    pub fn semantic_tokens(&self, path: &Path) -> Vec<SemanticTokenFact> {
        let Some(document) = self.document(path) else {
            return Vec::new();
        };
        if document.kind == DocumentKind::Unknown {
            return Vec::new();
        }
        if document
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.key == "issue-yaml-parse-failed")
        {
            return Vec::new();
        }
        let Some(ast) = document.ast.as_ref() else {
            return Vec::new();
        };

        let mut properties = Vec::new();
        let mut strings = Vec::new();
        collect_flow_token_facts(ast, &document.path, &mut properties, &mut strings);

        let mut overlays = BTreeMap::<(usize, usize), SemanticTokenKind>::new();
        for symbol in self
            .symbols
            .iter()
            .filter(|symbol| symbol.range.path == document.path)
        {
            let Some(kind) = semantic_symbol_token_kind(symbol) else {
                continue;
            };
            overlays.insert((symbol.range.start, symbol.range.end), kind);
        }
        for token in self
            .schema_value_tokens
            .iter()
            .filter(|token| token.range.path == document.path)
        {
            overlays.insert((token.range.start, token.range.end), token.kind);
        }
        for choice in self
            .schema_choices
            .iter()
            .filter(|choice| choice.range.path == document.path)
        {
            overlays.insert(
                (choice.range.start, choice.range.end),
                SemanticTokenKind::EnumMember,
            );
        }

        let mut output = properties
            .into_iter()
            .filter(|token| !overlays.contains_key(&(token.range.start, token.range.end)))
            .collect::<Vec<_>>();
        for string_range in &strings {
            let mut cursor = string_range.start;
            for ((start, end), kind) in overlays
                .iter()
                .filter(|((start, end), _)| *start < string_range.end && *end > string_range.start)
            {
                if *start < cursor {
                    continue;
                }
                let start = (*start).max(string_range.start);
                let end = (*end).min(string_range.end);
                if cursor < start {
                    output.push(SemanticTokenFact {
                        range: SourceRange {
                            path: document.path.clone(),
                            start: cursor,
                            end: start,
                        },
                        kind: SemanticTokenKind::String,
                    });
                }
                output.push(SemanticTokenFact {
                    range: SourceRange {
                        path: document.path.clone(),
                        start,
                        end,
                    },
                    kind: *kind,
                });
                cursor = end;
            }
            if cursor < string_range.end {
                output.push(SemanticTokenFact {
                    range: SourceRange {
                        path: document.path.clone(),
                        start: cursor,
                        end: string_range.end,
                    },
                    kind: SemanticTokenKind::String,
                });
            }
        }

        for ((start, end), kind) in overlays {
            if strings
                .iter()
                .any(|range| range.start <= start && end <= range.end)
            {
                continue;
            }
            output.push(SemanticTokenFact {
                range: SourceRange {
                    path: document.path.clone(),
                    start,
                    end,
                },
                kind,
            });
        }

        output.sort_by(|left, right| {
            (left.range.start, left.range.end, left.kind).cmp(&(
                right.range.start,
                right.range.end,
                right.kind,
            ))
        });
        output.dedup_by(|left, right| left.range == right.range && left.kind == right.kind);
        output.sort_by_key(|token| (token.range.start, token.range.end));
        output
    }

    pub fn document(&self, path: &Path) -> Option<&WorkspaceDocument> {
        self.documents.get(&normalize(path.to_owned()))
    }

    pub fn documents(&self) -> impl Iterator<Item = &WorkspaceDocument> {
        self.documents.values()
    }

    pub fn node_at(&self, path: &Path, offset: usize) -> Option<&SemanticSymbol> {
        let document = self.document(path)?;
        if document.is_trivia_at(offset) {
            return None;
        }
        self.symbols
            .iter()
            .filter(|symbol| symbol.range.contains(path, offset))
            .filter(|symbol| {
                symbol.detail != schema_description("flow-schema-configuration-document")
            })
            .min_by_key(|symbol| symbol.range.len())
    }

    pub fn hover_at(&self, path: &Path, offset: usize) -> Option<HoverInfo> {
        let symbol = self.node_at(path, offset)?;
        Some(HoverInfo {
            range: symbol.range.clone(),
            markdown: format!("**{}**\n\n{}", symbol.name, symbol.detail),
            stale: symbol.stale,
            catalog_target: symbol.catalog_target.clone(),
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
        self.completions_with_source(path, offset, actions, None)
            .into_candidates()
    }

    pub fn completions_at_with_source(
        &self,
        path: &Path,
        offset: usize,
        actions: &HashMap<String, Arc<dyn Action>>,
        source: &dyn WorkspaceSource,
    ) -> Vec<CompletionCandidate> {
        self.completions_with_source(path, offset, actions, Some(source))
            .into_candidates()
    }

    fn completions_with_source(
        &self,
        path: &Path,
        offset: usize,
        actions: &HashMap<String, Arc<dyn Action>>,
        source: Option<&dyn WorkspaceSource>,
    ) -> CompletionContextResult {
        let Some(document) = self.document(path) else {
            return CompletionContextResult::Unsupported;
        };
        if document.is_comment_at(offset) {
            return CompletionContextResult::Empty;
        }

        if let Some(context) = self
            .reference_contexts
            .iter()
            .filter(|context| context.range.contains_inclusive(path, offset))
            .min_by_key(|context| context.range.len())
        {
            if context.keys_only
                && context
                    .blocked
                    .iter()
                    .any(|range| range.contains_inclusive(path, offset))
            {
                return CompletionContextResult::Empty;
            }
            let replacement = completion_word_range(document, offset, false);
            let mut candidates = self.reference_candidates(
                context,
                path,
                replacement.clone(),
                source,
            );
            for candidate in &mut candidates {
                candidate.replacement = Some(replacement.clone());
                candidate.filter_text = Some(candidate.label.clone());
            }
            return CompletionContextResult::from_candidates(candidates);
        }

        for invocation in &self.actions {
            if invocation.action_id_range.contains_inclusive(path, offset) {
                let replacement = completion_word_range(document, offset, false);
                let mut candidates = actions
                    .iter()
                    .map(|(id, action)| {
                        let description = action
                            .editor_descriptor()
                            .map(|descriptor| descriptor.summary.to_owned())
                            .unwrap_or_else(|| "Native action".into());
                        CompletionCandidate {
                            label: id.clone(),
                            detail: "Native action".into(),
                            documentation: description,
                            snippet: id.clone(),
                            catalog_target: Some(CatalogTarget::action(id)),
                            replacement: Some(replacement.clone()),
                            filter_text: Some(id.clone()),
                        }
                    })
                    .collect::<Vec<_>>();
                candidates.sort_by(|left, right| left.label.cmp(&right.label));
                return CompletionContextResult::from_candidates(candidates);
            }
            if !invocation
                .with_range
                .as_ref()
                .is_some_and(|range| range.contains_inclusive(path, offset))
            {
                continue;
            }
            if let Some(template) = template_component_context(document, offset) {
                return CompletionContextResult::from_candidates(
                    self.template_candidates(invocation, path, template),
                );
            }
            if let Some(descriptor) = actions
                .get(&invocation.action_id)
                .and_then(|action| action.editor_descriptor())
            {
                let in_value = invocation
                    .fields
                    .iter()
                    .any(|(_, range)| range.contains_inclusive(path, offset));
                if in_value {
                    let mut values = enum_value_candidates(descriptor, invocation, path, offset);
                    let replacement = completion_word_range(document, offset, false);
                    for candidate in &mut values {
                        candidate.replacement = Some(replacement.clone());
                        candidate.filter_text = Some(candidate.label.clone());
                    }
                    return CompletionContextResult::from_candidates(values);
                }
                let mut candidates = action_field_candidates(descriptor, invocation, path, offset);
                let replacement = completion_word_range(document, offset, true);
                let has_colon = mapping_key_has_colon(&document.text, replacement.end);
                for candidate in &mut candidates {
                    candidate.replacement = Some(replacement.clone());
                    candidate.filter_text = Some(
                        candidate
                            .label
                            .rsplit('.')
                            .next()
                            .unwrap_or(&candidate.label)
                            .to_owned(),
                    );
                    if has_colon {
                        candidate.snippet = candidate
                            .label
                            .rsplit('.')
                            .next()
                            .unwrap_or(&candidate.label)
                            .to_owned();
                    }
                }
                return CompletionContextResult::from_candidates(candidates);
            }
        }

        match self.schema_completion_candidates(path, offset) {
            Some(candidates) => CompletionContextResult::from_candidates(candidates),
            None => CompletionContextResult::Unsupported,
        }
    }

    fn schema_completion_candidates(
        &self,
        path: &Path,
        offset: usize,
    ) -> Option<Vec<CompletionCandidate>> {
        if let Some(choice) = self
            .schema_choices
            .iter()
            .filter(|choice| choice.range.contains_inclusive(path, offset))
            .min_by_key(|choice| choice.range.len())
        {
            let document = self.document(path)?;
            let replacement = completion_word_range(document, offset, false);
            return Some(
                choice
                    .choices
                    .iter()
                    .map(|value| CompletionCandidate {
                        label: (*value).into(),
                        detail: "enum value".into(),
                        documentation: schema_description(
                            choice.description_key,
                        ),
                        snippet: (*value).into(),
                        catalog_target: None,
                        replacement: Some(replacement.clone()),
                        filter_text: Some((*value).into()),
                    })
                    .collect(),
            );
        }
        let document = self.document(path)?;
        let context = self
            .schema_contexts
            .iter()
            .filter(|context| context.range.contains_inclusive(path, offset))
            .min_by_key(|context| context.range.len())?;
        let evin_flow::schema::Schema::Object(fields) = context.schema else {
            return Some(Vec::new());
        };
        let key_position = context
            .keys
            .iter()
            .any(|range| range.contains_inclusive(path, offset))
            || (!context
                .occupied
                .iter()
                .any(|range| range.contains_inclusive(path, offset))
                && !mapping_value_precedes_cursor(&document.text, offset))
            || matches!(document.text.as_bytes().get(offset), Some(b'{' | b'}'));
        if !key_position {
            return Some(Vec::new());
        }
        let replacement = completion_word_range(document, offset, true);
        let has_colon = mapping_key_has_colon(&document.text, replacement.end);
        Some(
            fields
                .iter()
                .filter(|field| {
                    !context
                        .present
                        .contains(field.alias_of.unwrap_or(field.name))
                })
                .map(|field| CompletionCandidate {
                    label: field.name.into(),
                    detail: field_detail(*field),
                    documentation: schema_description(
                        field.description_key,
                    ),
                    snippet: if has_colon {
                        field.name.to_owned()
                    } else {
                        field
                            .snippet
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("{}: ${{1}}", field.name))
                    },
                    catalog_target: Some(CatalogTarget(
                        field
                            .catalog_target
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("{}.{}", context.path, field.name)),
                    )),
                    replacement: Some(replacement.clone()),
                    filter_text: Some(field.name.into()),
                })
                .collect(),
        )
    }

    fn reference_candidates(
        &self,
        context: &ReferenceCompletionContext,
        path: &Path,
        replacement: SourceRange,
        source: Option<&dyn WorkspaceSource>,
    ) -> Vec<CompletionCandidate> {
        let mut labels = BTreeMap::<String, (String, String)>::new();
        match context.kind {
            ReferenceCompletionKind::Task => {
                for (label, detail) in self.task_reference_labels(path, context.owner.as_deref()) {
                    labels.insert(label, (detail, "task reference".into()));
                }
                // A parent scope is itself a useful partial completion before
                // the task component has been typed.
                let prefix = source_slice(&self.documents, path, &replacement);
                if !prefix.contains(':') {
                    for scope in document_task_scopes(&self.documents).into_values() {
                        labels.entry(format!("{scope}:")).or_insert_with(|| {
                            ("Task scope".into(), "task scope".into())
                        });
                    }
                }
            }
            ReferenceCompletionKind::Stage => {
                for symbol in self.symbols.iter().filter(|symbol| symbol.kind == SemanticKind::Stage) {
                    labels.insert(
                        symbol.name.clone(),
                        (symbol.detail.clone(), "stage".into()),
                    );
                }
            }
            ReferenceCompletionKind::Preset => {
                let mut presets = crate::environment::BUILTIN_ENVIRONMENT_PRESETS
                    .iter()
                    .map(|preset| (*preset).to_owned())
                    .collect::<BTreeSet<_>>();
                for document in self.documents.values().filter(|document| {
                    document.kind == DocumentKind::Environment
                }) {
                    if let Some(SpannedValue::Mapping(entries, _)) = document
                        .ast
                        .as_ref()
                        .and_then(|ast| mapping_get(ast, "presets"))
                    {
                        presets.extend(entries.iter().filter_map(|(key, _)| match key {
                            SpannedValue::String(name, _) => Some(name.clone()),
                            _ => None,
                        }));
                    }
                }
                labels.extend(presets.into_iter().map(|name| {
                    (name, ("Environment preset".into(), "environment preset".into()))
                }));
            }
            ReferenceCompletionKind::Environment => {
                let names = self
                    .documents
                    .values()
                    .filter(|document| document.kind == DocumentKind::Environment)
                    .filter_map(|document| document.ast.as_ref())
                    .filter_map(|ast| mapping_get(ast, "environments"))
                    .filter_map(|value| match value {
                        SpannedValue::Mapping(entries, _) => Some(entries),
                        _ => None,
                    })
                    .flatten()
                    .filter_map(|(key, _)| match key {
                        SpannedValue::String(name, _) => Some(name.clone()),
                        _ => None,
                    });
                labels.extend(names.map(|name| {
                    (name, ("Declared environment variable".into(), "environment".into()))
                }));
            }
            ReferenceCompletionKind::Hook => {
                labels.extend(evin_policy::KNOWN_HOOKS.iter().map(|hook| {
                    ((*hook).to_owned(), ("Git hook".into(), "hook".into()))
                }));
            }
            ReferenceCompletionKind::ModulePath
            | ReferenceCompletionKind::NamespacePath => {
                let prefix = source_slice(&self.documents, path, &replacement);
                let Some(source) = source else {
                    return Vec::new();
                };
                let Some(base) = context.base.as_deref() else {
                    return Vec::new();
                };
                if Path::new(prefix).is_absolute()
                    || Path::new(prefix)
                        .components()
                        .any(|component| matches!(component, std::path::Component::ParentDir))
                {
                    return Vec::new();
                }
                let (directory_prefix, leaf_prefix) = if prefix.ends_with('/') {
                    (prefix, "")
                } else if let Some((directory, leaf)) = prefix.rsplit_once('/') {
                    (directory, leaf)
                } else {
                    ("", prefix)
                };
                let directory_prefix = directory_prefix.trim_end_matches('/');
                let directory = normalize(base.join(directory_prefix));
                let entries = source.list_dir(&directory).unwrap_or_default();
                for entry in entries {
                    if matches!(entry.name.as_str(), ".git" | "target" | "node_modules" | ".codex") {
                        continue;
                    }
                    if !entry.name.starts_with(leaf_prefix) {
                        continue;
                    }
                    let relative = if directory_prefix.is_empty() {
                        entry.name.clone()
                    } else {
                        format!("{directory_prefix}/{}", entry.name)
                    };
                    if entry.is_directory {
                        let module_exists = source.exists(
                            &normalize(directory.join(&entry.name).join(crate::paths::MODULE_MANIFEST)),
                        );
                        if context.kind == ReferenceCompletionKind::ModulePath && !module_exists {
                            labels.insert(
                                format!("{relative}/"),
                                ("Directory".into(), "path directory".into()),
                            );
                        } else if context.kind == ReferenceCompletionKind::ModulePath {
                            labels.insert(
                                relative,
                                ("Evin module directory".into(), "module path".into()),
                            );
                        } else {
                            labels.insert(
                                format!("{relative}/"),
                                ("Directory".into(), "path directory".into()),
                            );
                        }
                    } else if context.kind == ReferenceCompletionKind::NamespacePath
                        && namespace_path_candidate(source, &directory.join(&entry.name))
                    {
                        labels.insert(
                            relative,
                            ("Evin namespace document".into(), "namespace path".into()),
                        );
                    }
                }
            }
        }
        let prefix = source_slice(&self.documents, path, &replacement);
        labels
            .into_iter()
            .filter(|(label, _)| label.starts_with(prefix))
            .map(|(label, (detail, documentation))| CompletionCandidate {
                label: label.clone(),
                detail,
                documentation,
                snippet: label.clone(),
                catalog_target: None,
                replacement: Some(replacement.clone()),
                filter_text: Some(label),
            })
            .collect()
    }

    fn task_reference_labels(&self, path: &Path, owner: Option<&str>) -> Vec<(String, String)> {
        let declared = self
            .symbols
            .iter()
            .filter(|symbol| symbol.kind == SemanticKind::Task)
            .collect::<Vec<_>>();
        let current_task = owner.unwrap_or_default();
        let current_scope = current_task.rsplit_once(':').map(|(scope, _)| scope);
        let scopes = declared
            .iter()
            .filter_map(|task| task.name.rsplit_once(':').map(|(scope, _)| scope))
            .collect::<BTreeSet<_>>();
        let mut labels = BTreeMap::<String, String>::new();
        let workspace_name = self
            .document(Path::new(crate::paths::WORKSPACE_MANIFEST))
            .and_then(|document| document.ast.as_ref())
            .and_then(|ast| mapping_get(ast, "workspace"))
            .and_then(|workspace| mapping_get(workspace, "name"))
            .and_then(|value| match value {
                SpannedValue::String(name, _) => Some(name.as_str()),
                _ => None,
            })
            .unwrap_or_default();
        for task in declared {
            if task.name == current_task {
                continue;
            }
            let (scope, short) = task
                .name
                .rsplit_once(':')
                .map(|(scope, short)| (Some(scope), short))
                .unwrap_or((None, task.name.as_str()));
            if scope == current_scope && current_scope.is_some() {
                labels.insert(short.into(), format!("Local task in {}", scope.unwrap()));
                labels.insert(task.name.clone(), format!("Qualified task in {}", scope.unwrap()));
            } else if scope.is_none() {
                let shadowed = current_scope.is_some_and(|current| {
                    declared_name_in_scope(&self.symbols, current, short)
                });
                let label = if shadowed && !workspace_name.is_empty() {
                    format!("{workspace_name}:{short}")
                } else {
                    short.to_owned()
                };
                labels.insert(label, "Workspace task".into());
            } else {
                labels.insert(task.name.clone(), format!("Task in {}", scope.unwrap()));
            }
        }
        // Offer the canonical fully-qualified names as well as shorthand. This
        // permits a user to start with a parent scope and supports ambiguous
        // tasks without changing the resolver's reference rules.
        for task in self.symbols.iter().filter(|symbol| symbol.kind == SemanticKind::Task) {
            if task.name != current_task && task.name.contains(':') {
                labels.entry(task.name.clone()).or_insert_with(|| {
                    let scope = task.name.rsplit_once(':').map(|(scope, _)| scope).unwrap_or("");
                    format!("Qualified task in {scope}")
                });
            }
        }
        let _ = scopes;
        let _ = path;
        labels.into_iter().collect()
    }

    fn template_candidates(
        &self,
        invocation: &ActionInvocation,
        path: &Path,
        template: TemplateComponentContext,
    ) -> Vec<CompletionCandidate> {
        let mut candidates = BTreeMap::<String, String>::new();
        match template.kind {
            TemplateComponentKind::Environment => {
                for symbol in self.symbols.iter().filter(|symbol| {
                    symbol.kind == SemanticKind::EnvironmentVariable
                        && symbol.name != "environment"
                }) {
                    candidates.insert(
                        format!("env.{}", symbol.name),
                        "Environment variable".into(),
                    );
                }
            }
            TemplateComponentKind::Unqualified => {
                let owner = invocation.task_name.as_str();
                let owner_scope = owner.rsplit_once(':').map(|(scope, _)| scope);
                let scopes = document_task_scopes(&self.documents);
                for symbol in self.symbols.iter().filter(|symbol| symbol.kind == SemanticKind::Value) {
                    let visible = symbol.owner.as_deref() == Some(owner)
                        || symbol.range.path == Path::new(crate::paths::WORKSPACE_MANIFEST)
                        || scopes.get(&symbol.range.path).is_some_and(|scope| {
                            owner_scope.is_some_and(|owner_scope| {
                                owner_scope == scope || owner_scope.starts_with(&format!("{scope}:"))
                            })
                        });
                    if visible {
                        candidates.insert(symbol.name.clone(), "Template value".into());
                    }
                }
                let needed = self
                    .symbols
                    .iter()
                    .filter(|symbol| {
                        symbol.kind == SemanticKind::TaskReference
                            && symbol.owner.as_deref() == Some(invocation.task_name.as_str())
                    })
                    .map(|reference| self.resolve_dependency_name(path, &invocation.task_name, &reference.name))
                    .collect::<BTreeSet<_>>();
                for dependency in needed {
                    for producer in self.actions.iter().filter(|action| action.task_name == dependency) {
                        for output in &producer.outputs {
                            candidates.insert(
                                format!("{dependency}.{output}"),
                                format!("Output from task {dependency}"),
                            );
                        }
                    }
                }
            }
            TemplateComponentKind::TaskOutput => {
                let needed = self
                    .symbols
                    .iter()
                    .filter(|symbol| {
                        symbol.kind == SemanticKind::TaskReference
                            && symbol.owner.as_deref() == Some(invocation.task_name.as_str())
                    })
                    .map(|reference| self.resolve_dependency_name(path, &invocation.task_name, &reference.name))
                    .collect::<BTreeSet<_>>();
                for dependency in needed {
                    for producer in self.actions.iter().filter(|action| action.task_name == dependency) {
                        for output in &producer.outputs {
                            candidates.insert(
                                format!("{dependency}.{output}"),
                                format!("Output from task {dependency}"),
                            );
                        }
                    }
                }
            }
        }
        candidates
            .into_iter()
            .filter(|(candidate, _)| {
                candidate
                    .strip_prefix(&template.qualifier)
                    .is_some_and(|remaining| remaining.starts_with(&template.prefix))
            })
            .map(|(label, detail)| CompletionCandidate {
                label: label.clone(),
                detail,
                documentation: "Evin template reference".into(),
                snippet: label
                    .strip_prefix(&template.qualifier)
                    .unwrap_or(&label)
                    .to_owned(),
                catalog_target: None,
                replacement: Some(template.replacement.clone()),
                filter_text: Some(
                    label
                        .strip_prefix(&template.qualifier)
                        .unwrap_or(&label)
                        .to_owned(),
                ),
            })
            .collect()
    }

    fn resolve_dependency_name(&self, path: &Path, owner: &str, raw: &str) -> String {
        let workspace_name = self
            .document(Path::new(crate::paths::WORKSPACE_MANIFEST))
            .and_then(|document| document.ast.as_ref())
            .and_then(|ast| mapping_get(ast, "workspace"))
            .and_then(|workspace| mapping_get(workspace, "name"))
            .and_then(|value| match value {
                SpannedValue::String(name, _) => Some(name.as_str()),
                _ => None,
            })
            .unwrap_or_default();
        if !workspace_name.is_empty()
            && let Some(bare) = raw.strip_prefix(&format!("{workspace_name}:"))
        {
            return bare.to_owned();
        }
        let local_scope = owner.rsplit_once(':').map(|(scope, _)| scope);
        let local_match = self.symbols.iter().any(|symbol| {
            symbol.kind == SemanticKind::Task
                && symbol.range.path == path
                && symbol.name.rsplit(':').next() == Some(raw)
                && symbol.name.rsplit_once(':').map(|(scope, _)| scope) == local_scope
        });
        if local_match
            && let Some(scope) = local_scope
        {
            format!("{scope}:{raw}")
        } else {
            raw.to_owned()
        }
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

fn character_offset_to_byte(source: &str, character_offset: usize) -> usize {
    source
        .char_indices()
        .nth(character_offset)
        .map_or(source.len(), |(byte_offset, _)| byte_offset)
}

fn normalize_ast_spans(value: &mut SpannedValue, source: &str) {
    let span = match value {
        SpannedValue::Null(span)
        | SpannedValue::Bool(_, span)
        | SpannedValue::Integer(_, span)
        | SpannedValue::Float(_, span)
        | SpannedValue::String(_, span)
        | SpannedValue::Sequence(_, span)
        | SpannedValue::Mapping(_, span) => span,
    };
    let start = character_offset_to_byte(source, span.offset);
    let end = character_offset_to_byte(source, span.offset.saturating_add(span.length));
    span.offset = start;
    span.length = end.saturating_sub(start);

    match value {
        SpannedValue::Sequence(values, _) => {
            for value in values {
                normalize_ast_spans(value, source);
            }
        }
        SpannedValue::Mapping(entries, _) => {
            for (key, value) in entries {
                normalize_ast_spans(key, source);
                normalize_ast_spans(value, source);
            }
        }
        _ => {}
    }
}

impl WorkspaceDocument {
    /// Whether a byte offset lies in non-interactive trivia (comments,
    /// indentation whitespace, blank lines, or line breaks).
    pub fn is_trivia_at(&self, offset: usize) -> bool {
        if offset >= self.text.len() {
            return true;
        }
        if self.is_comment_at(offset) {
            return true;
        }
        let bytes = self.text.as_bytes();
        if matches!(bytes[offset], b'\n' | b'\r') {
            return true;
        }
        let line_start = self.text[..offset].rfind('\n').map_or(0, |index| index + 1);
        let indent_len = self.text[line_start..]
            .chars()
            .take_while(|character| *character == ' ' || *character == '\t')
            .count();
        if offset < line_start + indent_len {
            return true;
        }
        let line_end = self.text[offset..]
            .find('\n')
            .map_or(self.text.len(), |index| offset + index);
        if self.text[line_start..line_end].trim().is_empty() {
            return true;
        }
        false
    }

    /// Whether a byte offset lies in a Flow comment. Editor features return
    /// nothing there. Offsets inside a parsed scalar are never comments, which
    /// keeps `#` lines inside multi-line quoted or block scalars meaningful.
    pub fn is_comment_at(&self, offset: usize) -> bool {
        evin_flow::is_comment_at(&self.text, offset)
            && !self
                .ast
                .as_ref()
                .is_some_and(|ast| scalar_contains(ast, offset))
    }
}

fn scalar_contains(value: &SpannedValue, offset: usize) -> bool {
    match value {
        SpannedValue::Sequence(values, _) => values.iter().any(|value| scalar_contains(value, offset)),
        SpannedValue::Mapping(entries, _) => entries
            .iter()
            .any(|(key, value)| scalar_contains(key, offset) || scalar_contains(value, offset)),
        scalar => {
            let span = scalar.span();
            span.length > 0 && span.offset <= offset && offset < span.offset + span.length
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
