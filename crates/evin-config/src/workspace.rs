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
    pub policy: Option<String>,
    pub execution: Option<ExecutionMode>,
}

#[cfg(test)]
mod tests {
    use super::{ModuleFlow, WorkspaceConfig};

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
        let config: ModuleFlow = serde_yaml::from_str(
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
        let config: ModuleFlow = serde_yaml::from_str(
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
    fn module_flow_deserializes_root_variables() {
        let config: ModuleFlow = serde_yaml::from_str(
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
pub struct WorkspaceFlow {
    pub workspace: WorkspaceConfig,
    pub stages: Vec<evin_flow::Spanned<String>>,
    #[serde(default)]
    pub expose: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub values: HashMap<String, String>,
    #[serde(default)]
    pub tasks: Option<HashMap<evin_flow::Spanned<String>, crate::models::TaskObject>>,
    #[serde(skip)]
    pub policy: Option<std::sync::Arc<evin_repository_policy::RepositoryPolicy>>,
}

pub type WorkspaceYaml = WorkspaceFlow;

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
pub struct ModuleFlow {
    pub module: ModuleConfig,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub values: HashMap<String, String>,
    #[serde(default)]
    pub tasks: Option<HashMap<evin_flow::Spanned<String>, crate::models::TaskObject>>,
}

pub type ModuleYaml = ModuleFlow;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamespaceConfig {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamespaceFlow {
    pub namespace: NamespaceConfig,
    #[serde(default, alias = "includes")]
    pub include: Option<IncludeConfig>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub values: HashMap<String, String>,
    #[serde(default)]
    pub tasks: Option<HashMap<evin_flow::Spanned<String>, crate::models::TaskObject>>,
}

pub type NamespaceYaml = NamespaceFlow;

use evin_flow::schema::{Field as EditorField, FieldRole as EditorRole, Schema as EditorSchema};

const EDITOR_STRING: EditorSchema = EditorSchema::String;
const EDITOR_STRING_LIST: EditorSchema = EditorSchema::Sequence(&EDITOR_STRING);
const EDITOR_STRING_MAP: EditorSchema = EditorSchema::Map(&EDITOR_STRING);
const EDITOR_TASKS: EditorSchema = EditorSchema::Map(&crate::models::TASK_EDITOR_SCHEMA);
const EDITOR_INCLUDE: EditorSchema = EditorSchema::Union(&[EDITOR_STRING_LIST, EDITOR_STRING_MAP]);
const EDITOR_HOOK_RUNNER: EditorSchema = EditorSchema::Object(&[
    EditorField::new(
        "command",
        EditorSchema::String,
        "flow-schema-executable-for-generated-git-hooks",
    )
    .required()
    .catalog("workspace.hook_runner.command"),
    EditorField::new(
        "args",
        EDITOR_STRING_LIST,
        "flow-schema-arguments-passed-to-the-hook-runner",
    )
    .non_nullable()
    .default_value("[]")
    .catalog("workspace.hook_runner.args"),
]);
const EDITOR_WORKSPACE: EditorSchema = EditorSchema::Object(&[
    EditorField::new(
        "name",
        EditorSchema::String,
        "flow-schema-workspace-identifier",
    )
    .required()
    .catalog("workspace.name"),
    EditorField::new(
        "version",
        EditorSchema::String,
        "flow-schema-workspace-version-string",
    )
    .required()
    .catalog("workspace.version"),
    EditorField::new(
        "modules",
        EDITOR_STRING_LIST,
        "flow-schema-directories-scanned-for-modules",
    )
    .catalog("workspace.modules"),
    EditorField::new(
        "include",
        EDITOR_INCLUDE,
        "flow-schema-external-namespace-flow-files",
    )
    .catalog("workspace.include"),
    EditorField::new("includes", EDITOR_INCLUDE, "flow-schema-alias-for-include")
        .alias_of("include")
        .catalog("workspace.include"),
    EditorField::new(
        "install_hint",
        EditorSchema::String,
        "flow-schema-message-shown-when-evin-is-missing",
    )
    .catalog("workspace.install_hint"),
    EditorField::new(
        "hook_runner",
        EDITOR_HOOK_RUNNER,
        "flow-schema-command-for-generated-git-hooks",
    )
    .catalog("workspace.hook_runner"),
    EditorField::new(
        "policy",
        EditorSchema::String,
        "flow-schema-path-to-repository-policy-configuration",
    )
    .catalog("workspace.policy"),
    EditorField::new(
        "execution",
        EditorSchema::Enum(&["linear", "parallel"]),
        "flow-schema-task-dispatch-execution-mode",
    )
    .catalog("workspace.execution"),
]);
const EDITOR_MODULE: EditorSchema = EditorSchema::Object(&[
    EditorField::new(
        "name",
        EditorSchema::String,
        "flow-schema-module-namespace-identifier",
    )
    .required()
    .catalog("module.name"),
    EditorField::new(
        "version",
        EditorSchema::String,
        "flow-schema-independent-module-version",
    )
    .catalog("module.version"),
    EditorField::new(
        "include",
        EDITOR_INCLUDE,
        "flow-schema-namespace-flow-files-declared-by-module",
    )
    .catalog("module.include"),
    EditorField::new("includes", EDITOR_INCLUDE, "flow-schema-alias-for-include")
        .alias_of("include")
        .catalog("module.include"),
]);
const EDITOR_NAMESPACE: EditorSchema = EditorSchema::Object(&[
    EditorField::new(
        "name",
        EditorSchema::String,
        "flow-schema-namespace-identifier-prepended-to-tasks",
    )
    .required()
    .catalog("namespace.name"),
    EditorField::new(
        "description",
        EditorSchema::String,
        "flow-schema-human-readable-namespace-description",
    )
    .catalog("namespace.description"),
]);

pub const WORKSPACE_EDITOR_SCHEMA: EditorSchema = EditorSchema::Object(&[
    EditorField::new(
        "workspace",
        EDITOR_WORKSPACE,
        "flow-schema-workspace-metadata",
    )
    .required()
    .role(EditorRole::Namespace)
    .catalog("workspace"),
    EditorField::new(
        "stages",
        EDITOR_STRING_LIST,
        "flow-schema-workspace-stage-definitions",
    )
    .required()
    .value_role(EditorRole::EnumMember)
    .catalog("workspace.stages"),
    EditorField::new(
        "env",
        EDITOR_STRING_MAP,
        "flow-schema-workspace-environment-values",
    )
    .non_nullable()
    .default_value("{}")
    .catalog("workspace.env"),
    EditorField::new(
        "values",
        EDITOR_STRING_MAP,
        "flow-schema-workspace-template-values",
    )
    .non_nullable()
    .default_value("{}")
    .catalog("workspace.values"),
    EditorField::new(
        "tasks",
        EDITOR_TASKS,
        "flow-schema-workspace-task-definitions",
    )
    .catalog("workspace.tasks"),
    EditorField::new(
        "expose",
        EDITOR_STRING_LIST,
        "flow-schema-tasks-exposed-to-callers",
    )
    .non_nullable()
    .default_value("[]")
    .catalog("workspace.expose"),
]);
pub const MODULE_EDITOR_SCHEMA: EditorSchema = EditorSchema::Object(&[
    EditorField::new("module", EDITOR_MODULE, "flow-schema-module-metadata")
        .required()
        .role(EditorRole::Namespace)
        .catalog("module"),
    EditorField::new(
        "env",
        EDITOR_STRING_MAP,
        "flow-schema-module-environment-values",
    )
    .non_nullable()
    .catalog("module.env"),
    EditorField::new(
        "values",
        EDITOR_STRING_MAP,
        "flow-schema-module-template-values",
    )
    .non_nullable()
    .catalog("module.values"),
    EditorField::new("tasks", EDITOR_TASKS, "flow-schema-module-task-definitions")
        .catalog("module.tasks"),
]);
pub const NAMESPACE_EDITOR_SCHEMA: EditorSchema = EditorSchema::Object(&[
    EditorField::new(
        "namespace",
        EDITOR_NAMESPACE,
        "flow-schema-namespace-metadata",
    )
    .required()
    .role(EditorRole::Namespace)
    .catalog("namespace"),
    EditorField::new(
        "include",
        EDITOR_INCLUDE,
        "flow-schema-nested-namespace-flow-files",
    )
    .catalog("namespace.include"),
    EditorField::new("includes", EDITOR_INCLUDE, "flow-schema-alias-for-include")
        .alias_of("include")
        .catalog("namespace.include"),
    EditorField::new(
        "env",
        EDITOR_STRING_MAP,
        "flow-schema-namespace-environment-values",
    )
    .non_nullable()
    .catalog("namespace.env"),
    EditorField::new(
        "values",
        EDITOR_STRING_MAP,
        "flow-schema-namespace-template-values",
    )
    .non_nullable()
    .catalog("namespace.values"),
    EditorField::new(
        "tasks",
        EDITOR_TASKS,
        "flow-schema-namespace-task-definitions",
    )
    .catalog("namespace.tasks"),
]);

impl evin_flow::YamlDocument for WorkspaceFlow {}
impl evin_flow::YamlDocument for ModuleFlow {}
impl evin_flow::YamlDocument for NamespaceFlow {}

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
  - .evin/release.flow
  - .evin/maintenance.flow
"#,
        )
        .unwrap();

        assert_eq!(
            list_config.include,
            Some(IncludeConfig::List(vec![
                ".evin/release.flow".to_string(),
                ".evin/maintenance.flow".to_string(),
            ]))
        );

        let map_config: WorkspaceConfig = serde_yaml::from_str(
            r#"
name: example
version: 1.0.0
includes:
  release: .evin/release.flow
  maintenance: .evin/maintenance.flow
"#,
        )
        .unwrap();

        let mut expected = HashMap::new();
        expected.insert("release".to_string(), ".evin/release.flow".to_string());
        expected.insert(
            "maintenance".to_string(),
            ".evin/maintenance.flow".to_string(),
        );
        assert_eq!(map_config.include, Some(IncludeConfig::Map(expected)));
    }

    #[test]
    fn namespace_flow_deserializes_correctly() {
        let flow = r#"
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
      - run: cargo run -p esh -- scripts/release/check/doctor.esh
"#;
        let parsed = <NamespaceFlow as evin_flow::YamlDocument>::from_str(flow, None).unwrap();
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
        let module_error = serde_yaml::from_str::<ModuleFlow>(
            r#"
module:
  name: api
  env:
    API_LOG: debug
"#,
        )
        .unwrap_err();
        assert!(module_error.to_string().contains("unknown field `env`"));

        let namespace_env_error = serde_yaml::from_str::<NamespaceFlow>(
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

        let namespace_values_error = serde_yaml::from_str::<NamespaceFlow>(
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
        let list_config: ModuleFlow = serde_yaml::from_str(
            r#"
module:
  name: api
  include:
    - .evin/routes.flow
    - .evin/db.flow
"#,
        )
        .unwrap();
        assert_eq!(
            list_config.module.include,
            Some(IncludeConfig::List(vec![
                ".evin/routes.flow".to_string(),
                ".evin/db.flow".to_string()
            ]))
        );

        let map_config: ModuleFlow = serde_yaml::from_str(
            r#"
module:
  name: api
  include:
    routes: .evin/routes.flow
"#,
        )
        .unwrap();
        let mut expected = HashMap::new();
        expected.insert("routes".to_string(), ".evin/routes.flow".to_string());
        assert_eq!(
            map_config.module.include,
            Some(IncludeConfig::Map(expected))
        );
    }
}
