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

#[cfg(test)]
mod tests {
    use super::{is_canonical_manifest, is_flow_manifest};
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
}
