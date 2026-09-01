// Copyright (C) 2026 Eduardo Marques Santos
//
// Package-manifest discovery belongs to configuration loading so actions never
// parse a second, runtime-only configuration format.

use evin_action_sdk::packaging::{PackageManifest, PackageManifestCatalog, PackageManifestSource};
use evin_yaml::YamlDocument;
use std::path::Path;
use std::sync::Arc;

#[allow(clippy::result_large_err)]
pub fn load_package_catalog(
    workspace_root: &Path,
    modules: Option<&Vec<String>>,
) -> Result<Arc<PackageManifestCatalog>, evin_diagnostic::Diagnostic> {
    let mut roots = vec![workspace_root.to_path_buf()];
    if let Some(modules) = modules {
        roots.extend(modules.iter().map(|module| workspace_root.join(module)));
    }
    roots.sort();
    roots.dedup();

    let mut manifests = Vec::new();
    for root in roots {
        let path = root.join(".evin/package.yaml");
        if !path.exists() {
            continue;
        }
        let manifest = PackageManifest::from_file(&path)?;
        manifest.validate().map_err(|error| {
            evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(error.to_string()))
                .with_file_location(path.to_string_lossy().to_string(), None, None)
        })?;
        manifests.push(PackageManifestSource {
            root: root.canonicalize().unwrap_or(root),
            path: path.canonicalize().unwrap_or(path),
            manifest,
        });
    }
    Ok(Arc::new(PackageManifestCatalog::new(manifests)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn nearest_module_manifest_wins_without_merging() {
        let temp = TempDir::new().unwrap();
        let root_manifest = temp.path().join(".evin/package.yaml");
        let module = temp.path().join("app");
        std::fs::create_dir_all(root_manifest.parent().unwrap()).unwrap();
        std::fs::create_dir_all(module.join(".evin")).unwrap();
        let manifest = "version: 1\nidentity:\n  name: app\n  summary: app\n  maintainer: A <a@b>\n  license: MIT\n";
        std::fs::write(&root_manifest, manifest.replace("name: app", "name: root")).unwrap();
        std::fs::write(module.join(".evin/package.yaml"), manifest).unwrap();
        let catalog = load_package_catalog(temp.path(), Some(&vec!["app".into()])).unwrap();
        assert_eq!(
            catalog.select(&module).unwrap().manifest.identity.name,
            "app"
        );
    }
}
