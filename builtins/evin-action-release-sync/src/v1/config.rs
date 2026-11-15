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

use serde::Deserialize;

fn default_state_path() -> String {
    ".evin/actions/release-sync/remote-state.v1.json".to_string()
}

fn default_plan_out_path() -> String {
    ".evin/actions/release-sync/plan.v1.json".to_string()
}

fn default_notes_dir() -> String {
    ".evin/actions/release-sync/notes".to_string()
}

fn default_artifacts_dir() -> String {
    "dist".to_string()
}

fn default_changelog_path() -> String {
    "CHANGELOG.md".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseSyncConfig {
    #[serde(default)]
    pub tag: Option<String>,

    #[serde(default)]
    pub title: Option<String>,

    #[serde(default = "default_state_path")]
    pub state: String,

    #[serde(default = "default_plan_out_path")]
    pub plan_out: String,

    #[serde(default = "default_notes_dir")]
    pub notes_dir: String,

    #[serde(default = "default_artifacts_dir")]
    pub artifacts_dir: String,

    #[serde(default = "default_changelog_path")]
    pub changelog: String,

    #[serde(default)]
    pub dry_run: bool,

    #[serde(default)]
    pub prune_orphans: bool,
}

impl Default for ReleaseSyncConfig {
    fn default() -> Self {
        Self {
            tag: None,
            title: None,
            state: default_state_path(),
            plan_out: default_plan_out_path(),
            notes_dir: default_notes_dir(),
            artifacts_dir: default_artifacts_dir(),
            changelog: default_changelog_path(),
            dry_run: false,
            prune_orphans: false,
        }
    }
}

impl ReleaseSyncConfig {
    pub fn from_with(
        with: Option<&evin_flow::SpannedValue>,
    ) -> Result<Self, evin_flow::ConfigError> {
        let config: Self = evin_action_sdk::parse_config(with)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), evin_flow::ConfigError> {
        if self.tag.as_ref().is_some_and(|tag| tag.trim().is_empty()) {
            return Err(evin_flow::ConfigError {
                message: "tag must not be empty when provided".to_string(),
                span: None,
            });
        }
        if self
            .title
            .as_ref()
            .is_some_and(|title| title.trim().is_empty())
        {
            return Err(evin_flow::ConfigError {
                message: "title must not be empty when provided".to_string(),
                span: None,
            });
        }
        if self.state.trim().is_empty() {
            return Err(evin_flow::ConfigError {
                message: "state file path must not be empty".to_string(),
                span: None,
            });
        }
        if self.plan_out.trim().is_empty() {
            return Err(evin_flow::ConfigError {
                message: "plan_out path must not be empty".to_string(),
                span: None,
            });
        }
        if self.notes_dir.trim().is_empty() {
            return Err(evin_flow::ConfigError {
                message: "notes_dir path must not be empty".to_string(),
                span: None,
            });
        }
        Ok(())
    }
}
