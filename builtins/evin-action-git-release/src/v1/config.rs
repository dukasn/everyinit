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

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum StageMode {
    #[default]
    All,
    Existing,
}

fn default_remote() -> String {
    "origin".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum Enabled {
    Flag(bool),
    Text(String),
}

impl Default for Enabled {
    fn default() -> Self {
        Self::Flag(true)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PushConfig {
    #[serde(default = "default_remote")]
    pub(super) remote: String,
    pub(super) branch: Option<String>,
    #[serde(default)]
    pub(super) force: bool,
    #[serde(default)]
    pub(super) prune_stale_tags: bool,
    pub(super) base: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GitReleaseConfig {
    #[serde(default)]
    pub(super) stage: StageMode,
    pub(super) message: Option<String>,
    pub(super) tag: Option<String>,
    #[serde(default)]
    pub(super) enabled: Enabled,
    pub(super) push: Option<PushConfig>,
}

impl GitReleaseConfig {
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
        if let Some(msg) = &self.message {
            validate_template_syntax(msg).map_err(|e| e.to_string())?;
            if msg.trim().is_empty() {
                return Err(evin_locales::t!("message-must-not-be-empty"));
            }
        }
        if let Some(tag) = &self.tag {
            if tag.trim().is_empty() {
                return Err(evin_locales::t!("tag-must-not-be-empty-when-provided"));
            }
            validate_template_syntax(tag).map_err(|e| e.to_string())?;
        }
        if let Enabled::Text(text) = &self.enabled {
            validate_template_syntax(text).map_err(|e| e.to_string())?;
        }
        if let Some(push) = &self.push {
            if push.remote.trim().is_empty() {
                return Err("push.remote must not be empty".to_string());
            }
            if push.branch.as_deref().is_some_and(|b| b.trim().is_empty()) {
                return Err("push.branch must not be empty when provided".to_string());
            }
            if push.prune_stale_tags {
                if self.tag.is_none() {
                    return Err("push.prune_stale_tags requires tag".to_string());
                }
                if push.base.as_deref().is_none_or(|b| b.trim().is_empty()) {
                    return Err("push.prune_stale_tags requires push.base".to_string());
                }
            } else if push.base.is_some() {
                return Err("push.base is only valid with push.prune_stale_tags".to_string());
            }
        }
        Ok(())
    }
}
