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

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    Linear,
    Parallel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    pub name: String,
    pub version: String,
    pub modules: Option<Vec<String>>,
    pub install_hint: Option<String>,
    pub rules: Option<String>,
    pub execution: Option<ExecutionMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceYaml {
    pub workspace: WorkspaceConfig,
    pub stages: Vec<evin_yaml::Spanned<String>>,
    #[serde(default)]
    pub expose: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub values: HashMap<String, String>,
    #[serde(default)]
    pub tasks: Option<HashMap<evin_yaml::Spanned<String>, crate::models::TaskObject>>,
    #[serde(skip)]
    pub gitrules: Option<std::sync::Arc<evin_gitrules::GitRulesRoot>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleConfig {
    pub name: String,
    pub version: String,
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleYaml {
    pub module: ModuleConfig,
    #[serde(default)]
    pub values: HashMap<String, String>,
    #[serde(default)]
    pub tasks: Option<HashMap<evin_yaml::Spanned<String>, crate::models::TaskObject>>,
}

impl evin_yaml::YamlDocument for WorkspaceYaml {}
impl evin_yaml::YamlDocument for ModuleYaml {}
