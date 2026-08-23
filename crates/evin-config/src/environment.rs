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
