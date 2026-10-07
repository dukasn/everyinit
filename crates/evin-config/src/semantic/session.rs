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

/// Overlay-aware state for editor hosts. A malformed current document never
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
            .filter(|document| {
                document.ast.is_none()
                    || document
                        .diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic.code.key == "issue-yaml-parse-failed")
            })
            .map(|document| document.path.clone())
            .collect::<Vec<_>>();
        self.stale_symbols.clear();
        if let Some(previous) = &self.last_valid {
            for path in invalid {
                let symbols = previous
                    .symbols
                    .iter()
                    .filter(|symbol| symbol.range.path == path)
                    .filter(|symbol| {
                        matches!(symbol.kind, SemanticKind::Task | SemanticKind::Value)
                            || symbol.catalog_target.is_some()
                    })
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
            .all(|document| {
                document.ast.is_some()
                    && !document
                        .diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic.code.key == "issue-yaml-parse-failed")
            })
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
        let document = self.current.as_ref()?.document(&path)?;
        if document.is_trivia_at(offset) {
            return None;
        }
        let text = document.text.as_str();
        self.stale_symbols.get(&path)?.iter().find(|symbol| {
            symbol.range.contains(&path, offset)
                && (matches!(symbol.kind, SemanticKind::Task | SemanticKind::Value)
                    || symbol.catalog_target.is_some())
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
        schema_contexts: index.schema_contexts.clone(),
        schema_choices: index.schema_choices.clone(),
        reference_contexts: index.reference_contexts.clone(),
        schema_value_tokens: index.schema_value_tokens.clone(),
    }
}
