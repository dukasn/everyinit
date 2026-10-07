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

/// Names of the built-in environment presets accepted by task declarations.
pub const BUILTIN_ENVIRONMENT_PRESETS: &[&str] = &["rust", "node", "ci", "python", "aws", "docker"];

/// Environment variable names supplied by a built-in task preset.
pub fn builtin_environment_preset(name: &str) -> Option<&'static [&'static str]> {
    match name.to_lowercase().as_str() {
        "rust" => Some(&[
            "CARGO_HOME",
            "RUST_BACKTRACE",
            "RUSTFLAGS",
            "CARGO_TARGET_DIR",
        ]),
        "node" => Some(&["NODE_ENV", "NPM_CONFIG_COLOR", "PATH"]),
        "ci" => Some(&[
            "CI",
            "CONTINUOUS_INTEGRATION",
            "GITHUB_ACTIONS",
            "GIT_BRANCH",
            "GIT_COMMIT",
        ]),
        "python" => Some(&["PYTHONPATH", "PYTHONUNBUFFERED", "VIRTUAL_ENV"]),
        "aws" => Some(&["AWS_REGION", "AWS_DEFAULT_REGION", "AWS_PROFILE"]),
        "docker" => Some(&["DOCKER_HOST", "DOCKER_BUILDKIT", "DOCKER_CONFIG"]),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentYaml {
    pub version: u32,
    #[serde(default)]
    pub presets: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub environments: HashMap<String, EnvVarDef>,
}

impl evin_yaml::YamlDocument for EnvironmentYaml {}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum EnvScope {
    Internal,
    Process,
}

fn default_scopes() -> Vec<EnvScope> {
    vec![EnvScope::Internal, EnvScope::Process]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnvVarDef {
    #[serde(rename = "type")]
    pub var_type: EnvVarType,
    pub required: Option<bool>,
    pub pattern: Option<String>,
    pub description: Option<String>,
    #[serde(default, deserialize_with = "deserialize_flexible_default")]
    pub default: Option<String>,
    #[serde(default = "default_scopes")]
    pub scopes: Vec<EnvScope>,
    #[serde(default)]
    pub stages: Vec<String>,
}

impl EnvVarDef {
    pub fn is_active_for_stage(&self, stage: &str) -> bool {
        self.stages.is_empty() || self.stages.iter().any(|s| s.eq_ignore_ascii_case(stage))
    }

    pub fn is_exported_to_process(&self) -> bool {
        self.scopes.contains(&EnvScope::Process)
    }

    pub fn is_available_internally(&self) -> bool {
        self.scopes.contains(&EnvScope::Internal)
    }
}

fn deserialize_flexible_default<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct FlexibleDefaultVisitor;

    impl<'de> Visitor<'de> for FlexibleDefaultVisitor {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string, integer, float, or boolean")
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v.to_string()))
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v.to_string()))
        }

        fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v.to_string()))
        }

        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v.to_string()))
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v.to_string()))
        }

        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v))
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(self)
        }
    }

    deserializer.deserialize_option(FlexibleDefaultVisitor)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EnvVarType {
    String,
    Number,
    Boolean,
    Secret,
}

const EDITOR_STRING: evin_yaml::schema::Schema = evin_yaml::schema::Schema::String;
const EDITOR_STRING_LIST: evin_yaml::schema::Schema =
    evin_yaml::schema::Schema::Sequence(&EDITOR_STRING);
const EDITOR_VARIABLE: evin_yaml::schema::Schema = evin_yaml::schema::Schema::Object(&[
    evin_yaml::schema::Field::new(
        "type",
        evin_yaml::schema::Schema::Enum(&["string", "number", "boolean", "secret"]),
        "flow-schema-environment-variable-type",
    )
    .required()
    .catalog("environment.type"),
    evin_yaml::schema::Field::new(
        "required",
        evin_yaml::schema::Schema::Boolean,
        "flow-schema-require-the-variable-to-be-present",
    )
    .catalog("environment.required"),
    evin_yaml::schema::Field::new(
        "pattern",
        EDITOR_STRING,
        "flow-schema-regular-expression-for-validation",
    )
    .catalog("environment.pattern")
    .constraints(&[evin_yaml::schema::ValueConstraint::Regex]),
    evin_yaml::schema::Field::new(
        "description",
        EDITOR_STRING,
        "flow-schema-human-readable-variable-description",
    )
    .catalog("environment.description"),
    evin_yaml::schema::Field::new(
        "default",
        evin_yaml::schema::Schema::Union(&[
            evin_yaml::schema::Schema::String,
            evin_yaml::schema::Schema::Number,
            evin_yaml::schema::Schema::Boolean,
        ]),
        "flow-schema-fallback-value-converted-to-a-string",
    )
    .catalog("environment.default"),
    evin_yaml::schema::Field::new(
        "scopes",
        evin_yaml::schema::Schema::Sequence(&evin_yaml::schema::Schema::Enum(&[
            "Internal", "Process",
        ])),
        "flow-schema-variable-injection-scopes",
    )
    .non_nullable()
    .default_value("[Internal, Process]")
    .catalog("environment.scopes"),
    evin_yaml::schema::Field::new(
        "stages",
        EDITOR_STRING_LIST,
        "flow-schema-stage-export-filter",
    )
    .non_nullable()
    .default_value("[]")
    .catalog("environment.stages"),
]);

pub const EDITOR_SCHEMA: evin_yaml::schema::Schema = evin_yaml::schema::Schema::Object(&[
    evin_yaml::schema::Field::new(
        "version",
        evin_yaml::schema::Schema::Integer,
        "flow-schema-environment-schema-version",
    )
    .required()
    .catalog("environment.version"),
    evin_yaml::schema::Field::new(
        "presets",
        evin_yaml::schema::Schema::Map(&EDITOR_STRING_LIST),
        "flow-schema-named-variable-presets",
    )
    .non_nullable()
    .default_value("{}")
    .catalog("environment.presets"),
    evin_yaml::schema::Field::new(
        "environments",
        evin_yaml::schema::Schema::Map(&EDITOR_VARIABLE),
        "flow-schema-variable-definitions-keyed-by-environment-variable-name",
    )
    .non_nullable()
    .default_value("{}")
    .catalog("environment.environments"),
]);
