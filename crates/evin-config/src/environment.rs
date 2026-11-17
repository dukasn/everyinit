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
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

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

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentSchema {
    #[serde(rename = "$schema")]
    pub schema: String,
    #[serde(default)]
    pub presets: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub environments: HashMap<String, EnvVarDef>,
}

impl Default for EnvironmentSchema {
    fn default() -> Self {
        Self {
            schema: "https://dl.everyinit.dev/schema/v1/environment".to_string(),
            presets: HashMap::new(),
            environments: HashMap::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum EnvironmentParseError {
    #[error("JSON parsing failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("JSON Schema validation failed: {0}")]
    Schema(String),
}

pub fn parse_environment_json(content: &str) -> Result<EnvironmentSchema, EnvironmentParseError> {
    let value: Value = serde_json::from_str(content)?;
    let schema: Value =
        serde_json::from_str(include_str!("../../../schemas/v1/environment.schema.json"))?;
    let validator = jsonschema::validator_for(&schema)
        .map_err(|error| EnvironmentParseError::Schema(error.to_string()))?;
    let errors = validator
        .iter_errors(&value)
        .map(|error| format!("{} {error}", error.instance_path()))
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        return Err(EnvironmentParseError::Schema(errors.join("; ")));
    }
    Ok(serde_json::from_value(value)?)
}

#[derive(schemars::JsonSchema, Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum EnvScope {
    Internal,
    Process,
}

fn default_scopes() -> Vec<EnvScope> {
    vec![EnvScope::Internal, EnvScope::Process]
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(schemars::JsonSchema, Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EnvVarType {
    String,
    Number,
    Boolean,
    Secret,
}

const EDITOR_STRING: evin_flow::schema::Schema = evin_flow::schema::Schema::String;
const EDITOR_STRING_LIST: evin_flow::schema::Schema =
    evin_flow::schema::Schema::Sequence(&EDITOR_STRING);
const EDITOR_VARIABLE: evin_flow::schema::Schema = evin_flow::schema::Schema::Object(&[
    evin_flow::schema::Field::new(
        "type",
        evin_flow::schema::Schema::Enum(&["string", "number", "boolean", "secret"]),
        "flow-schema-environment-variable-type",
    )
    .required()
    .catalog("environment.type"),
    evin_flow::schema::Field::new(
        "required",
        evin_flow::schema::Schema::Boolean,
        "flow-schema-require-the-variable-to-be-present",
    )
    .catalog("environment.required"),
    evin_flow::schema::Field::new(
        "pattern",
        EDITOR_STRING,
        "flow-schema-regular-expression-for-validation",
    )
    .catalog("environment.pattern")
    .constraints(&[evin_flow::schema::ValueConstraint::Regex]),
    evin_flow::schema::Field::new(
        "description",
        EDITOR_STRING,
        "flow-schema-human-readable-variable-description",
    )
    .catalog("environment.description"),
    evin_flow::schema::Field::new(
        "default",
        evin_flow::schema::Schema::Union(&[
            evin_flow::schema::Schema::String,
            evin_flow::schema::Schema::Number,
            evin_flow::schema::Schema::Boolean,
        ]),
        "flow-schema-fallback-value-converted-to-a-string",
    )
    .catalog("environment.default"),
    evin_flow::schema::Field::new(
        "scopes",
        evin_flow::schema::Schema::Sequence(&evin_flow::schema::Schema::Enum(&[
            "Internal", "Process",
        ])),
        "flow-schema-variable-injection-scopes",
    )
    .non_nullable()
    .default_value("[Internal, Process]")
    .catalog("environment.scopes"),
    evin_flow::schema::Field::new(
        "stages",
        EDITOR_STRING_LIST,
        "flow-schema-stage-export-filter",
    )
    .non_nullable()
    .default_value("[]")
    .catalog("environment.stages"),
]);

pub const EDITOR_SCHEMA: evin_flow::schema::Schema = evin_flow::schema::Schema::Object(&[
    evin_flow::schema::Field::new(
        "$schema",
        evin_flow::schema::Schema::String,
        "flow-schema-environment-json-schema-uri",
    )
    .required()
    .catalog("environment.$schema"),
    evin_flow::schema::Field::new(
        "presets",
        evin_flow::schema::Schema::Map(&EDITOR_STRING_LIST),
        "flow-schema-named-variable-presets",
    )
    .non_nullable()
    .default_value("{}")
    .catalog("environment.presets"),
    evin_flow::schema::Field::new(
        "environments",
        evin_flow::schema::Schema::Map(&EDITOR_VARIABLE),
        "flow-schema-variable-definitions-keyed-by-environment-variable-name",
    )
    .non_nullable()
    .default_value("{}")
    .catalog("environment.environments"),
]);

#[cfg(test)]
mod tests {
    use super::{EnvironmentParseError, parse_environment_json};

    #[test]
    fn environment_json_requires_the_versioned_schema_and_rejects_unknown_fields() {
        assert!(matches!(
            parse_environment_json(r#"{"presets":{}}"#),
            Err(EnvironmentParseError::Schema(_))
        ));
        assert!(matches!(
            parse_environment_json(
                r#"{"$schema":"https://dl.everyinit.dev/schema/v2/environment"}"#
            ),
            Err(EnvironmentParseError::Schema(_))
        ));
        assert!(matches!(
            parse_environment_json(
                r#"{"$schema":"https://dl.everyinit.dev/schema/v1/environment","version":1}"#
            ),
            Err(EnvironmentParseError::Schema(_))
        ));
    }

    #[test]
    fn environment_json_deserializes_schema_defaults_and_scalar_values() {
        let schema = parse_environment_json(
            r#"{"$schema":"https://dl.everyinit.dev/schema/v1/environment","environments":{"PORT":{"type":"number","default":8080},"CI":{"type":"boolean","default":true}}}"#,
        )
        .unwrap();
        assert_eq!(schema.environments.len(), 2);
        assert_eq!(schema.environments["PORT"].default.as_deref(), Some("8080"));
        assert_eq!(schema.environments["CI"].default.as_deref(), Some("true"));
    }
}
