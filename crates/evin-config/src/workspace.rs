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

/// Command used by generated Git hooks to invoke Evin.
///
/// The default is the `evin` executable found in `PATH`. Repositories that
/// develop Evin itself can instead point hooks at a Cargo alias, so hooks
/// always execute the source tree that is being changed on every platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookRunner {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IncludeConfig {
    List(Vec<String>),
    Map(HashMap<String, String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    pub name: String,
    pub version: String,
    pub modules: Option<Vec<String>>,
    #[serde(default, alias = "includes")]
    pub include: Option<IncludeConfig>,
    pub install_hint: Option<String>,
    pub hook_runner: Option<HookRunner>,
    pub rules: Option<String>,
    pub execution: Option<ExecutionMode>,
}

#[cfg(test)]
mod tests {
    use super::{ModuleYaml, WorkspaceConfig};

    #[test]
    fn hook_runner_deserializes_with_arguments() {
        let config: WorkspaceConfig = serde_yaml::from_str(
            r#"
name: example
version: 1.0.0
hook_runner:
  command: cargo
  args: [run, --locked, -p, evin, --]
"#,
        )
        .unwrap();

        let runner = config.hook_runner.unwrap();
        assert_eq!(runner.command, "cargo");
        assert_eq!(runner.args, ["run", "--locked", "-p", "evin", "--"]);
    }

    #[test]
    fn module_version_is_optional_for_workspace_version_inheritance() {
        let config: ModuleYaml = serde_yaml::from_str(
            r#"
module:
  name: example
tasks: {}
"#,
        )
        .unwrap();

        assert_eq!(config.module.version, None);
    }

    #[test]
    fn module_can_still_declare_an_independent_version() {
        let config: ModuleYaml = serde_yaml::from_str(
            r#"
module:
  name: example
  version: 2.3.4
tasks: {}
"#,
        )
        .unwrap();

        assert_eq!(config.module.version.as_deref(), Some("2.3.4"));
    }

    #[test]
    fn module_yaml_deserializes_root_variables() {
        let config: ModuleYaml = serde_yaml::from_str(
            r#"
module:
  name: api
env:
  API_LOG: debug
values:
  artifact_name: api-server
"#,
        )
        .unwrap();

        assert_eq!(config.env.get("API_LOG"), Some(&"debug".to_string()));
        assert_eq!(
            config.values.get("artifact_name"),
            Some(&"api-server".to_string())
        );
    }
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
    /// Optional module-specific version. When omitted, the module inherits
    /// the enclosing workspace version.
    pub version: Option<String>,
    #[serde(default, alias = "includes")]
    pub include: Option<IncludeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleYaml {
    pub module: ModuleConfig,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub values: HashMap<String, String>,
    #[serde(default)]
    pub tasks: Option<HashMap<evin_yaml::Spanned<String>, crate::models::TaskObject>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamespaceConfig {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamespaceYaml {
    pub namespace: NamespaceConfig,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub values: HashMap<String, String>,
    #[serde(default)]
    pub tasks: Option<HashMap<evin_yaml::Spanned<String>, crate::models::TaskObject>>,
}

impl evin_yaml::YamlDocument for WorkspaceYaml {}
impl evin_yaml::YamlDocument for ModuleYaml {}
impl evin_yaml::YamlDocument for NamespaceYaml {}

#[cfg(test)]
mod include_tests {
    use super::*;

    #[test]
    fn workspace_include_deserializes_list_and_map_variants() {
        let list_config: WorkspaceConfig = serde_yaml::from_str(
            r#"
name: example
version: 1.0.0
include:
  - .evin/release.yaml
  - .evin/maintenance.yaml
"#,
        )
        .unwrap();

        assert_eq!(
            list_config.include,
            Some(IncludeConfig::List(vec![
                ".evin/release.yaml".to_string(),
                ".evin/maintenance.yaml".to_string(),
            ]))
        );

        let map_config: WorkspaceConfig = serde_yaml::from_str(
            r#"
name: example
version: 1.0.0
includes:
  release: .evin/release.yaml
  maintenance: .evin/maintenance.yaml
"#,
        )
        .unwrap();

        let mut expected = HashMap::new();
        expected.insert("release".to_string(), ".evin/release.yaml".to_string());
        expected.insert(
            "maintenance".to_string(),
            ".evin/maintenance.yaml".to_string(),
        );
        assert_eq!(map_config.include, Some(IncludeConfig::Map(expected)));
    }

    #[test]
    fn namespace_yaml_deserializes_correctly() {
        let yaml = r#"
namespace:
  name: release
  description: "Release engineering and distribution pipelines"

env:
  RELEASE_CHANNEL: canary

values:
  staging_dir: "dist/release-stage"

tasks:
  doctor:
    description: "Release doctor check"
    steps:
      - run: cargo run -p esh -- scripts/release/doctor.esh
"#;
        let parsed = <NamespaceYaml as evin_yaml::YamlDocument>::from_str(yaml, None).unwrap();
        assert_eq!(parsed.namespace.name, "release");
        assert_eq!(
            parsed.namespace.description.as_deref(),
            Some("Release engineering and distribution pipelines")
        );
        assert_eq!(
            parsed.env.get("RELEASE_CHANNEL"),
            Some(&"canary".to_string())
        );
        assert_eq!(
            parsed.values.get("staging_dir"),
            Some(&"dist/release-stage".to_string())
        );
        assert!(parsed.tasks.is_some());
        let tasks = parsed.tasks.unwrap();
        assert_eq!(tasks.len(), 1);
        let task_name = tasks.keys().next().unwrap();
        assert_eq!(task_name.value, "doctor");
    }

    #[test]
    fn nested_module_and_namespace_variables_are_rejected() {
        let module_error = serde_yaml::from_str::<ModuleYaml>(
            r#"
module:
  name: api
  env:
    API_LOG: debug
"#,
        )
        .unwrap_err();
        assert!(module_error.to_string().contains("unknown field `env`"));

        let namespace_env_error = serde_yaml::from_str::<NamespaceYaml>(
            r#"
namespace:
  name: release
  env:
    RELEASE_CHANNEL: canary
"#,
        )
        .unwrap_err();
        assert!(
            namespace_env_error
                .to_string()
                .contains("unknown field `env`")
        );

        let namespace_values_error = serde_yaml::from_str::<NamespaceYaml>(
            r#"
namespace:
  name: release
  values:
    staging_dir: dist
"#,
        )
        .unwrap_err();
        assert!(
            namespace_values_error
                .to_string()
                .contains("unknown field `values`")
        );
    }

    #[test]
    fn module_include_deserializes_list_and_map_variants() {
        let list_config: ModuleYaml = serde_yaml::from_str(
            r#"
module:
  name: api
  include:
    - .evin/routes.yaml
    - .evin/db.yaml
"#,
        )
        .unwrap();
        assert_eq!(
            list_config.module.include,
            Some(IncludeConfig::List(vec![
                ".evin/routes.yaml".to_string(),
                ".evin/db.yaml".to_string()
            ]))
        );

        let map_config: ModuleYaml = serde_yaml::from_str(
            r#"
module:
  name: api
  include:
    routes: .evin/routes.yaml
"#,
        )
        .unwrap();
        let mut expected = HashMap::new();
        expected.insert("routes".to_string(), ".evin/routes.yaml".to_string());
        assert_eq!(
            map_config.module.include,
            Some(IncludeConfig::Map(expected))
        );
    }
}
