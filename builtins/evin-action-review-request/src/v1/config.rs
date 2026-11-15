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

use evin_flow::template::pipeline::validate_template_syntax;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum Wanted {
    Flag(bool),
    Text(String),
}

impl Default for Wanted {
    fn default() -> Self {
        Self::Flag(true)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Commands {
    pub(super) find: String,
    pub(super) create: Option<String>,
    pub(super) update: Option<String>,
    pub(super) close: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReviewRequestConfig {
    pub(super) branch: String,
    pub(super) base: String,
    #[serde(default)]
    pub(super) wanted: Wanted,
    pub(super) title: Option<String>,
    pub(super) body_path: Option<String>,
    pub(super) commands: Commands,
    #[serde(default)]
    pub(super) context: BTreeMap<String, String>,
}

impl ReviewRequestConfig {
    pub(super) fn from_with(
        with: Option<evin_flow::SpannedValue>,
    ) -> Result<Self, evin_flow::ConfigError> {
        let config: Self = evin_action_sdk::parse_config(with.as_ref())?;
        config
            .validate()
            .map_err(|message| evin_flow::ConfigError {
                message,
                span: None,
            })?;
        Ok(config)
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        let required = [
            ("branch", &self.branch),
            ("base", &self.base),
            ("commands.find", &self.commands.find),
        ];
        for (name, value) in required {
            if value.trim().is_empty() {
                return Err(format!("{name} must not be empty"));
            }
        }
        let optional = [
            ("title", self.title.as_ref()),
            ("body_path", self.body_path.as_ref()),
            ("commands.create", self.commands.create.as_ref()),
            ("commands.update", self.commands.update.as_ref()),
            ("commands.close", self.commands.close.as_ref()),
        ];
        for (name, value) in optional {
            if value.is_some_and(|v| v.trim().is_empty()) {
                return Err(format!("{name} must not be empty when provided"));
            }
        }
        // Wanted can only be checked up front when it is a literal.
        let wanted_literal = match &self.wanted {
            Wanted::Flag(flag) => Some(*flag),
            Wanted::Text(text) => {
                validate_template_syntax(text).map_err(|e| e.to_string())?;
                parse_flag(text).ok()
            }
        };
        if wanted_literal != Some(false) && self.title.is_none() {
            return Err("title is required when a review request is wanted".to_string());
        }
        for template in [Some(&self.branch), Some(&self.base), self.title.as_ref()]
            .into_iter()
            .chain([self.body_path.as_ref()])
            .flatten()
            .chain(self.context.values())
        {
            validate_template_syntax(template).map_err(|e| e.to_string())?;
        }
        if let Some(path) = &self.body_path
            && (path.starts_with('/') || path.split(['/', '\\']).any(|part| part == ".."))
        {
            return Err("body_path must stay inside the workspace".to_string());
        }
        for key in self.context.keys() {
            if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(format!(
                    "context key '{key}' may only contain letters, digits, and underscores"
                ));
            }
        }
        Ok(())
    }
}

pub(super) fn parse_flag(text: &str) -> Result<bool, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("wanted must be true or false, got '{other}'")),
    }
}
