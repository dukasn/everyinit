// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 of the License.

#![allow(clippy::result_large_err)]

//! File access for configuration loading. The overlay implementation is the
//! editor-safe counterpart to normal filesystem discovery.

use evin_flow::YamlDocument;
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::paths::{
    ENVIRONMENT_MANIFEST, MODULE_MANIFEST, is_canonical_manifest, is_flow_manifest,
};

/// Whether a document is eligible for Evin-aware editor treatment. Eligibility
/// is deliberately separate from workspace discovery: a marked external Flow
/// file still must be included by a workspace or module to join that graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvinDocumentRecognition {
    pub canonical_path: bool,
    pub header: Option<evin_flow::EvinHeader>,
}

pub fn recognize_evin_document(
    path: &Path,
    text: &str,
) -> Result<Option<EvinDocumentRecognition>, evin_diagnostic::Diagnostic> {
    let canonical_path = is_canonical_manifest(path);
    let header = if is_flow_manifest(path) {
        evin_flow::validate_evin_header(text, Some(path))?
    } else {
        None
    };
    Ok(
        (canonical_path || header.is_some()).then_some(EvinDocumentRecognition {
            canonical_path,
            header,
        }),
    )
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceDocuments {
    pub texts: BTreeMap<PathBuf, String>,
}

impl WorkspaceDocuments {
    /// Collect every configuration document reachable from a workspace root.
    /// Contents always come through `WorkspaceSource`, so open editor buffers
    /// shadow the filesystem without executing workspace logic.
    pub fn load(
        source: &dyn WorkspaceSource,
        workspace_path: impl Into<PathBuf>,
    ) -> Result<Self, evin_diagnostic::Diagnostic> {
        use evin_flow::YamlDocument;
        let workspace_path = normalize(workspace_path.into());
        let workspace_text = read(source, &workspace_path)?;
        let workspace =
            crate::workspace::WorkspaceFlow::from_str(&workspace_text, Some(&workspace_path))?;
        let mut result = Self {
            texts: BTreeMap::from([(workspace_path.clone(), workspace_text)]),
        };
        let root = workspace_path
            .parent()
            .and_then(|path| path.parent())
            .unwrap_or(Path::new(""));
        for path in includes(&workspace.workspace.include, root) {
            result.load_namespace(source, path)?;
        }
        for module_dir in workspace.workspace.modules.unwrap_or_default() {
            let module_path = normalize(root.join(module_dir).join(MODULE_MANIFEST));
            if !source.exists(&module_path) {
                continue;
            }
            let text = read(source, &module_path)?;
            let module = crate::workspace::ModuleFlow::from_str(&text, Some(&module_path))?;
            result.texts.insert(module_path.clone(), text);
            let module_root = module_path
                .parent()
                .and_then(|path| path.parent())
                .unwrap_or(root);
            for path in includes(&module.module.include, module_root) {
                result.load_namespace(source, path)?;
            }
        }
        if let Some(policy) = workspace
            .workspace
            .policy
            .filter(|path| !path.starts_with("http"))
        {
            let path = normalize(root.join(policy));
            if source.exists(&path) {
                result.texts.insert(path.clone(), read(source, &path)?);
            }
        }
        let environment = normalize(root.join(ENVIRONMENT_MANIFEST));
        if source.exists(&environment) {
            result
                .texts
                .entry(environment.clone())
                .or_insert(read(source, &environment)?);
        }
        Ok(result)
    }
    fn load_namespace(
        &mut self,
        source: &dyn WorkspaceSource,
        path: PathBuf,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        let path = normalize(path);
        if self.texts.contains_key(&path) {
            return Ok(());
        }
        if !source.exists(&path) {
            return Err(missing(&path));
        }
        let text = read(source, &path)?;
        let document = crate::workspace::NamespaceFlow::from_str(&text, Some(&path))?;
        self.texts.insert(path.clone(), text);
        let base = path.parent().unwrap_or(Path::new(""));
        for child in includes(&document.include, base) {
            self.load_namespace(source, child)?;
        }
        Ok(())
    }
}

fn includes(include: &Option<crate::workspace::IncludeConfig>, root: &Path) -> Vec<PathBuf> {
    match include {
        Some(crate::workspace::IncludeConfig::List(paths)) => paths
            .iter()
            .map(|path| normalize(root.join(path)))
            .collect(),
        Some(crate::workspace::IncludeConfig::Map(paths)) => paths
            .values()
            .map(|path| normalize(root.join(path)))
            .collect(),
        None => vec![],
    }
}
fn read(source: &dyn WorkspaceSource, path: &Path) -> Result<String, evin_diagnostic::Diagnostic> {
    source.read_to_string(path).map_err(|error| {
        evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("issue-file-parse-failed")
                .arg("file_path", path.display().to_string())
                .arg("error", error.to_string()),
        )
    })
}
fn missing(path: &Path) -> evin_diagnostic::Diagnostic {
    evin_locales::diagnostic(
        evin_diagnostic::IssueCode::new("issue-file-parse-failed")
            .arg("file_path", path.display().to_string())
            .arg("error", "included namespace file does not exist"),
    )
    .with_file_location(path.display().to_string(), None, None)
}

pub trait WorkspaceSource: Send + Sync {
    fn exists(&self, path: &Path) -> bool;
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    fn list_dir(&self, path: &Path) -> io::Result<Vec<WorkspacePathEntry>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePathEntry {
    pub name: String,
    pub is_directory: bool,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FilesystemSource;
impl WorkspaceSource for FilesystemSource {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }
    fn list_dir(&self, path: &Path) -> io::Result<Vec<WorkspacePathEntry>> {
        std::fs::read_dir(path)?
            .map(|entry| {
                let entry = entry?;
                Ok(WorkspacePathEntry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    is_directory: entry.file_type()?.is_dir(),
                })
            })
            .collect()
    }
}

/// A memory overlay which shadows filesystem files by normalized relative path.
#[derive(Debug, Default, Clone)]
pub struct OverlaySource {
    overlays: BTreeMap<PathBuf, String>,
    fallback: FilesystemSource,
    root: Option<PathBuf>,
}
impl OverlaySource {
    pub fn for_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: Some(root.into()),
            ..Self::default()
        }
    }
    pub fn insert(&mut self, path: impl Into<PathBuf>, text: impl Into<String>) {
        self.overlays.insert(normalize(path.into()), text.into());
    }
    pub fn remove(&mut self, path: impl AsRef<Path>) {
        self.overlays.remove(&normalize(path.as_ref().to_owned()));
    }

    fn filesystem_path(&self, path: &Path) -> PathBuf {
        match &self.root {
            Some(root) if path.is_relative() => root.join(path),
            _ => path.to_owned(),
        }
    }
}
impl WorkspaceSource for OverlaySource {
    fn exists(&self, path: &Path) -> bool {
        self.overlays.contains_key(&normalize(path.to_owned()))
            || self.fallback.exists(&self.filesystem_path(path))
    }
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        self.overlays
            .get(&normalize(path.to_owned()))
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| self.fallback.read_to_string(&self.filesystem_path(path)))
    }
    fn list_dir(&self, path: &Path) -> io::Result<Vec<WorkspacePathEntry>> {
        use std::collections::BTreeMap;

        let mut entries = BTreeMap::<String, bool>::new();
        if let Ok(filesystem_entries) = self.fallback.list_dir(&self.filesystem_path(path)) {
            entries.extend(
                filesystem_entries
                    .into_iter()
                    .map(|entry| (entry.name, entry.is_directory)),
            );
        }
        let directory = normalize(path.to_owned());
        for overlay in self.overlays.keys() {
            let Ok(relative) = overlay.strip_prefix(&directory) else {
                continue;
            };
            let mut components = relative.components();
            let Some(first) = components.next() else {
                continue;
            };
            let name = first.as_os_str().to_string_lossy().into_owned();
            let is_directory = components.next().is_some();
            entries
                .entry(name)
                .and_modify(|current| *current |= is_directory)
                .or_insert(is_directory);
        }
        Ok(entries
            .into_iter()
            .map(|(name, is_directory)| WorkspacePathEntry { name, is_directory })
            .collect())
    }
}

pub fn normalize(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::{OverlaySource, WorkspaceSource, recognize_evin_document};
    use std::path::Path;

    #[test]
    fn overlay_shadows_and_can_remove_a_filesystem_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(".evin/workspace.flow");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "disk").unwrap();
        let mut source = OverlaySource::default();
        source.insert(&path, "buffer");
        assert_eq!(source.read_to_string(&path).unwrap(), "buffer");
        source.remove(&path);
        assert_eq!(source.read_to_string(&path).unwrap(), "disk");
        assert!(!source.exists(Path::new("definitely-not-an-evin-file")));
    }

    #[test]
    fn document_loader_follows_an_unsaved_included_namespace() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let workspace = root.join(".evin/workspace.flow");
        std::fs::create_dir_all(workspace.parent().unwrap()).unwrap();
        std::fs::write(&workspace, "workspace: { name: app, version: 1.0.0, include: [.evin/packages.flow] }\nstages: []\n").unwrap();
        let mut source = OverlaySource::default();
        source.insert(
            root.join(".evin/packages.flow"),
            "namespace: { name: packages }\ntasks: {}\n",
        );
        let documents = super::WorkspaceDocuments::load(&source, &workspace).unwrap();
        assert_eq!(documents.texts.len(), 2);
    }

    #[test]
    fn document_recognition_accepts_canonical_paths_or_a_valid_header() {
        let canonical = recognize_evin_document(
            Path::new(".evin/workspace.flow"),
            "workspace: { name: app, version: 1.0.0 }\n",
        )
        .unwrap()
        .unwrap();
        assert!(canonical.canonical_path);
        assert!(canonical.header.is_none());

        assert!(
            recognize_evin_document(
                Path::new(".evin/workspace.yaml"),
                "workspace: { name: app, version: 1.0.0 }\n",
            )
            .unwrap()
            .is_none()
        );

        let marked = recognize_evin_document(
            Path::new("automation.flow"),
            "#!evin/v1\nnamespace: { name: automation }\n",
        )
        .unwrap()
        .unwrap();
        assert!(!marked.canonical_path);
        assert_eq!(marked.header.unwrap().version, 1);

        assert!(
            recognize_evin_document(Path::new("notes.flow"), "title: notes\n")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn document_recognition_reports_invalid_evin_headers() {
        let diagnostic =
            recognize_evin_document(Path::new("automation.flow"), "#!evin/v2\n").unwrap_err();
        assert_eq!(diagnostic.code.key, "issue-evin-header-invalid");
    }
}
