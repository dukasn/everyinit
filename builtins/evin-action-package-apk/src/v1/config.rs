// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

use evin_action_sdk::{package_action::PackageIdentity, packaging::LinuxArch};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    pub(super) output_dir: String,
    pub(super) architecture: LinuxArch,
    pub(super) release: String,
    pub(super) package: PackageIdentity,
    pub(super) license: String,
    #[serde(default)]
    pub(super) depends: Vec<String>,
    #[serde(default)]
    pub(super) scripts: Scripts,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Scripts {
    pub(super) pre_install: Option<String>,
    pub(super) post_install: Option<String>,
    pub(super) pre_deinstall: Option<String>,
    pub(super) post_deinstall: Option<String>,
}
