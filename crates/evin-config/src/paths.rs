// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

//! Canonical paths and recognition for Evin Flow manifests.

use std::path::Path;

pub const CONFIG_DIR: &str = ".evin";
pub const FLOW_EXTENSION: &str = "flow";
pub const WORKSPACE_FILE: &str = "workspace.flow";
pub const MODULE_FILE: &str = "module.flow";
pub const ENVIRONMENT_FILE: &str = "environment.flow";
pub const GITRULES_FILE: &str = "gitrules.flow";
pub const WORKSPACE_MANIFEST: &str = ".evin/workspace.flow";
pub const MODULE_MANIFEST: &str = ".evin/module.flow";
pub const ENVIRONMENT_MANIFEST: &str = ".evin/environment.flow";
pub const GITRULES_MANIFEST: &str = ".evin/gitrules.flow";

pub fn is_canonical_manifest(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == CONFIG_DIR)
        && path.extension().and_then(|extension| extension.to_str()) == Some(FLOW_EXTENSION)
}

pub fn is_flow_manifest(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some(FLOW_EXTENSION)
}

/// Returns the relative path to the `.evin` directory containing `source_file`,
/// or `.evin` if at the workspace root or not located within a specific module.
pub fn find_evin_dir(source_file: &Path) -> std::path::PathBuf {
    let mut accumulated = std::path::PathBuf::new();
    for component in source_file.components() {
        accumulated.push(component);
        if component.as_os_str() == CONFIG_DIR {
            return accumulated;
        }
    }
    std::path::PathBuf::from(CONFIG_DIR)
}

/// Returns the module root directory containing `source_file` (the parent of `.evin`),
/// or `None` if `source_file` is at the workspace root or lacks an `.evin` segment.
pub fn find_module_root(source_file: &Path) -> Option<std::path::PathBuf> {
    let evin_dir = find_evin_dir(source_file);
    if evin_dir == Path::new(CONFIG_DIR) {
        None
    } else {
        evin_dir.parent().map(|p| p.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::{find_evin_dir, find_module_root, is_canonical_manifest, is_flow_manifest};
    use std::path::Path;

    #[test]
    fn recognizes_only_flow_files_in_the_evin_directory_as_canonical() {
        assert!(is_canonical_manifest(Path::new(".evin/workspace.flow")));
        assert!(is_canonical_manifest(Path::new(
            "crates/api/.evin/module.flow"
        )));
        assert!(!is_canonical_manifest(Path::new(".evin/workspace.yaml")));
        assert!(!is_canonical_manifest(Path::new("automation.flow")));
        assert!(is_flow_manifest(Path::new("automation.flow")));
    }

    #[test]
    fn resolves_evin_dir_and_module_root() {
        assert_eq!(
            find_evin_dir(Path::new(".evin/workspace.flow")),
            Path::new(".evin")
        );
        assert_eq!(find_module_root(Path::new(".evin/workspace.flow")), None);

        assert_eq!(
            find_evin_dir(Path::new("crates/evin/.evin/module.flow")),
            Path::new("crates/evin/.evin")
        );
        assert_eq!(
            find_module_root(Path::new("crates/evin/.evin/module.flow")),
            Some(Path::new("crates/evin").to_path_buf())
        );

        assert_eq!(
            find_evin_dir(Path::new("crates/evin/.evin/package.flow")),
            Path::new("crates/evin/.evin")
        );
        assert_eq!(
            find_module_root(Path::new("crates/evin/.evin/package.flow")),
            Some(Path::new("crates/evin").to_path_buf())
        );

        assert_eq!(
            find_evin_dir(Path::new("random/file.txt")),
            Path::new(".evin")
        );
        assert_eq!(find_module_root(Path::new("random/file.txt")), None);
    }
}
