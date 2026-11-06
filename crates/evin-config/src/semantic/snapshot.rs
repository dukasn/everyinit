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

use crate::parser::{EverInitConfig, load_workspace_tasks_from_sync};
use crate::paths::{
    ENVIRONMENT_FILE, ENVIRONMENT_MANIFEST, GITRULES_FILE, MODULE_FILE, MODULE_MANIFEST,
    WORKSPACE_FILE, WORKSPACE_MANIFEST, is_canonical_manifest, is_flow_manifest,
};
use crate::source::{WorkspaceDocuments, WorkspaceSource, normalize};
use evin_action_sdk::{Action, ActionDescriptor, ActionFieldDescriptor};
use evin_diagnostic::Diagnostic;
use evin_flow::template::{TemplateReferenceKind, references};
use evin_flow::{EvinHeader, Span, SpannedValue, validate_evin_header};
use std::collections::{BTreeMap, BTreeSet, HashMap};
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

/// Standard LSP semantic token roles emitted by Evin's configuration index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticTokenKind {
    Namespace,
    Property,
    Modifier,
    Function,
    EnumMember,
    Method,
    Variable,
    Parameter,
    String,
}

pub const STANDARD_SEMANTIC_TOKEN_TYPES: &[&str] = &[
    "namespace",
    "property",
    "modifier",
    "function",
    "enumMember",
    "method",
    "variable",
    "parameter",
    "string",
];

impl SemanticTokenKind {
    pub const fn lsp_name(self) -> &'static str {
        match self {
            Self::Namespace => "namespace",
            Self::Property => "property",
            Self::Modifier => "modifier",
            Self::Function => "function",
            Self::EnumMember => "enumMember",
            Self::Method => "method",
            Self::Variable => "variable",
            Self::Parameter => "parameter",
            Self::String => "string",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticTokenFact {
    pub range: SourceRange,
    pub kind: SemanticTokenKind,
}

impl SourceRange {
    pub fn contains(&self, path: &Path, offset: usize) -> bool {
        self.path == normalize(path.to_owned()) && self.start <= offset && offset < self.end
    }
    pub fn contains_inclusive(&self, path: &Path, offset: usize) -> bool {
        self.path == normalize(path.to_owned()) && self.start <= offset && offset <= self.end
    }
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceDocument {
    pub path: PathBuf,
    pub kind: DocumentKind,
    pub header: Option<EvinHeader>,
    pub text: String,
    pub ast: Option<SpannedValue>,
    /// Best-effort parse used only to anchor completion in unfinished buffers.
    /// It must never contribute validated semantic facts.
    pub completion_ast: Option<SpannedValue>,
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
        let mut discovery_failed = false;
        let mut texts = match WorkspaceDocuments::load(source, WORKSPACE_MANIFEST) {
            Ok(documents) => documents.texts,
            Err(diagnostic) => {
                diagnostics.push(diagnostic);
                discovery_failed = true;
                BTreeMap::new()
            }
        };
        // Preserve a root buffer even if a malformed workspace prevents normal
        // include discovery.
        let root = PathBuf::from(WORKSPACE_MANIFEST);
        if !texts.contains_key(&root)
            && source.exists(&root)
            && let Ok(text) = source.read_to_string(&root)
        {
            texts.insert(root.clone(), text);
        }
        if discovery_failed {
            supplement_reachable_documents(source, &mut texts, &mut diagnostics);
        }

        let mut documents = BTreeMap::new();
        for (path, text) in texts {
            let path = normalize(path);
            let mut document_diagnostics = Vec::new();
            let header_validation = validate_evin_header(&text, Some(&path));
            let header_is_valid = header_validation.is_ok();
            let header = match header_validation {
                Ok(header) => header,
                Err(diagnostic) => {
                    push_unique(&mut document_diagnostics, diagnostic.clone());
                    push_unique(&mut diagnostics, diagnostic);
                    None
                }
            };
            let kind = document_kind(&path, header.is_some());
            let (ast, completion_ast) = match evin_flow::parse_yaml(&text) {
                Ok(ast) => (Some(ast), None),
                Err(error) => {
                    let diagnostic = error
                        .into_diagnostic("Flow Syntax Error", Some(path.display().to_string()));
                    push_unique(&mut document_diagnostics, diagnostic.clone());
                    push_unique(&mut diagnostics, diagnostic);
                    let recovered = if header_is_valid {
                        recover_incomplete_flow(&text)
                    } else {
                        None
                    };
                    (None, recovered)
                }
            };
            if kind == DocumentKind::Environment
                && let Err(diagnostic) =
                    <crate::environment::EnvironmentFlow as evin_flow::YamlDocument>::from_str(
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
                    completion_ast,
                    diagnostics: document_diagnostics,
                },
            );
        }
        validate_manifest_references(source, &documents, &mut diagnostics);
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

fn recover_incomplete_flow(text: &str) -> Option<SpannedValue> {
    let mut stack = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    let mut in_comment = false;
    let mut previous = ' ';
    for character in text.chars() {
        if character == '\n' {
            in_comment = false;
            previous = character;
            continue;
        }
        if in_comment {
            continue;
        }
        if escaped {
            escaped = false;
            previous = character;
            continue;
        }
        if character == '\\' && quote == Some('"') {
            escaped = true;
            previous = character;
            continue;
        }
        if let Some(current_quote) = quote {
            if character == current_quote {
                quote = None;
            }
            previous = character;
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '#' if previous.is_whitespace() => in_comment = true,
            '[' | '{' => stack.push(character),
            ']' if stack.pop() != Some('[') => return None,
            '}' if stack.pop() != Some('{') => return None,
            _ => {}
        }
        previous = character;
    }
    if quote.is_none() && stack.is_empty() {
        return None;
    }
    let insertion = text.trim_end_matches(char::is_whitespace).len();
    let mut repaired = String::with_capacity(text.len() + stack.len() + usize::from(quote.is_some()));
    repaired.push_str(&text[..insertion]);
    if let Some(quote) = quote {
        repaired.push(quote);
    }
    for delimiter in stack.into_iter().rev() {
        repaired.push(if delimiter == '[' { ']' } else { '}' });
    }
    repaired.push_str(&text[insertion..]);
    evin_flow::parse_yaml(&repaired).ok()
}

fn supplement_reachable_documents(
    source: &dyn WorkspaceSource,
    texts: &mut BTreeMap<PathBuf, String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let workspace_path = PathBuf::from(WORKSPACE_MANIFEST);
    let Some(workspace_text) = texts.get(&workspace_path).cloned() else {
        return;
    };
    let Some(workspace_ast) = evin_flow::parse_yaml(&workspace_text)
        .ok()
        .or_else(|| recover_incomplete_flow(&workspace_text))
    else {
        return;
    };
    let root = workspace_path
        .parent()
        .and_then(Path::parent)
        .unwrap_or(Path::new(""));

    let includes = mapping_get(&workspace_ast, "workspace")
        .and_then(|value| {
            mapping_get(value, "include").or_else(|| mapping_get(value, "includes"))
        });
    for (_, path, _) in include_source_entries(includes) {
        let path = normalize(if Path::new(&path).is_absolute() {
            PathBuf::from(&path)
        } else {
            root.join(&path)
        });
        supplement_read(source, texts, diagnostics, &path);
    }

    if let Some(SpannedValue::Sequence(modules, _)) =
        mapping_get(&workspace_ast, "workspace").and_then(|value| mapping_get(value, "modules"))
    {
        for module in modules {
            let SpannedValue::String(directory, _) = module else {
                continue;
            };
            let module_path = normalize(root.join(directory).join(MODULE_MANIFEST));
            if !source.exists(&module_path) {
                continue;
            }
            if !texts.contains_key(&module_path) {
                supplement_read(source, texts, diagnostics, &module_path);
            }
            let Some(module_text) = texts.get(&module_path) else {
                continue;
            };
            let Ok(module_ast) = evin_flow::parse_yaml(module_text) else {
                continue;
            };
            let module_root = module_path
                .parent()
                .and_then(Path::parent)
                .unwrap_or(root);
            let includes = mapping_get(&module_ast, "module")
                .and_then(|value| {
                    mapping_get(value, "include").or_else(|| mapping_get(value, "includes"))
                });
            for (_, path, _) in include_source_entries(includes) {
                let path = normalize(if Path::new(&path).is_absolute() {
                    PathBuf::from(&path)
                } else {
                    module_root.join(&path)
                });
                supplement_read(source, texts, diagnostics, &path);
            }
        }
    }

    if let Some(SpannedValue::String(rules, _)) =
        mapping_get(&workspace_ast, "workspace").and_then(|value| mapping_get(value, "rules"))
        && !rules.starts_with("http")
    {
        supplement_read(source, texts, diagnostics, &normalize(root.join(rules)));
    }
    supplement_read(
        source,
        texts,
        diagnostics,
        &normalize(root.join(ENVIRONMENT_MANIFEST)),
    );
}

fn supplement_read(
    source: &dyn WorkspaceSource,
    texts: &mut BTreeMap<PathBuf, String>,
    diagnostics: &mut Vec<Diagnostic>,
    path: &Path,
) {
    if texts.contains_key(path) || !source.exists(path) {
        return;
    }
    match source.read_to_string(path) {
        Ok(text) => {
            texts.insert(path.to_owned(), text);
        }
        Err(error) => diagnostics.push(
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-file-parse-failed")
                    .arg("file_path", path.display().to_string())
                    .arg("error", error),
            )
            .with_file_location(path.display().to_string(), None, None),
        ),
    }
}

fn validate_manifest_references(
    source: &dyn WorkspaceSource,
    documents: &BTreeMap<PathBuf, WorkspaceDocument>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    diagnostics.retain(|diagnostic| {
        !(diagnostic.code.key == "issue-file-parse-failed"
            && diagnostic
                .code
                .args
                .iter()
                .any(|(key, value)| *key == "error" && value == "included namespace file does not exist"))
    });

    let workspace_path = Path::new(WORKSPACE_MANIFEST);
    let Some(workspace) = documents.get(workspace_path) else {
        return;
    };
    let Some(workspace_ast) = workspace.ast.as_ref() else {
        return;
    };
    let root = workspace_path
        .parent()
        .and_then(Path::parent)
        .unwrap_or(Path::new(""));

    if let Some(SpannedValue::Sequence(modules, _)) = mapping_get(workspace_ast, "workspace")
        .and_then(|value| mapping_get(value, "modules"))
    {
        for module in modules {
            let SpannedValue::String(directory, span) = module else {
                continue;
            };
            let manifest = normalize(root.join(directory).join(MODULE_MANIFEST));
            if !source.exists(&manifest) {
                diagnostics.push(
                    evin_locales::diagnostic(
                        evin_diagnostic::IssueCode::new("issue-module-manifest-missing")
                            .arg("module_dir", directory),
                    )
                    .with_file_span(
                        workspace.path.display().to_string(),
                        normalized_span(&workspace.text, *span).offset,
                        normalized_span(&workspace.text, *span).length.max(1),
                        None,
                    ),
                );
            }
        }
    }

    for document in documents.values() {
        let Some(ast) = document.ast.as_ref() else {
            continue;
        };
        let (includes, base) = match document.kind {
            DocumentKind::Workspace => (
                mapping_get(ast, "workspace")
                    .and_then(|value| {
                        mapping_get(value, "include").or_else(|| mapping_get(value, "includes"))
                    }),
                root,
            ),
            DocumentKind::Module => (
                mapping_get(ast, "module")
                    .and_then(|value| {
                        mapping_get(value, "include").or_else(|| mapping_get(value, "includes"))
                    }),
                document
                    .path
                    .parent()
                    .and_then(Path::parent)
                    .unwrap_or(root),
            ),
            _ => continue,
        };
        for (_, include, span) in include_source_entries(includes) {
            let path = normalize(if Path::new(&include).is_absolute() {
                PathBuf::from(&include)
            } else {
                base.join(&include)
            });
            if !source.exists(&path) {
                diagnostics.push(
                    evin_locales::diagnostic(
                        evin_diagnostic::IssueCode::new("issue-namespace-manifest-missing")
                            .arg("path", &include),
                    )
                    .with_file_span(
                        document.path.display().to_string(),
                        normalized_span(&document.text, span).offset,
                        normalized_span(&document.text, span).length.max(1),
                        None,
                    ),
                );
            }
        }
    }
}

fn normalized_span(source: &str, span: Span) -> Span {
    let start = character_offset_to_byte(source, span.offset);
    let end = character_offset_to_byte(source, span.offset.saturating_add(span.length));
    Span {
        offset: start,
        length: end.saturating_sub(start),
        ..span
    }
}

fn push_unique(diagnostics: &mut Vec<Diagnostic>, diagnostic: Diagnostic) {
    if !diagnostics.contains(&diagnostic) {
        diagnostics.push(diagnostic);
    }
}

fn document_kind(path: &Path, has_header: bool) -> DocumentKind {
    let file = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if is_canonical_manifest(path) && file == WORKSPACE_FILE {
        DocumentKind::Workspace
    } else if is_canonical_manifest(path) && file == MODULE_FILE {
        DocumentKind::Module
    } else if is_canonical_manifest(path) && file == GITRULES_FILE {
        DocumentKind::GitRules
    } else if is_canonical_manifest(path) && file == ENVIRONMENT_FILE {
        DocumentKind::Environment
    } else if is_flow_manifest(path) || has_header {
        DocumentKind::Namespace
    } else {
        DocumentKind::Unknown
    }
}
