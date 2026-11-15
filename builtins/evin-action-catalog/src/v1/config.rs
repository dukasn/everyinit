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

use std::path::PathBuf;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CatalogConfig {
    #[serde(default = "default_source")]
    pub(super) source_dir: PathBuf,
    #[serde(default = "default_output")]
    pub(super) out: PathBuf,
    #[serde(default)]
    pub(super) include: Vec<String>,
    #[serde(default)]
    pub(super) exclude: Vec<String>,
    #[serde(default = "default_true")]
    pub(super) extract_frontmatter: bool,
    #[serde(default = "default_reference_prefix")]
    pub(super) reference_prefix: String,
}

fn default_source() -> PathBuf {
    "docs".into()
}

fn default_output() -> PathBuf {
    "dist/catalog.json".into()
}

fn default_reference_prefix() -> String {
    "REF".into()
}

fn default_true() -> bool {
    true
}
