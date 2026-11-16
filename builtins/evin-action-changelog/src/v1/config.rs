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

use super::ChangelogError;
use evin_diagnostic::IssueCode;
use evin_flow::template::pipeline::validate_template_syntax;
use serde::Deserialize;
use std::collections::HashMap;

fn default_path() -> String {
    "CHANGELOG.md".to_string()
}

fn default_include_unreleased() -> bool {
    true
}

fn default_preamble() -> String {
    "# Changelog\nAll notable changes to this project will be documented in this file.\n\n"
        .to_string()
}

fn default_show_hashes() -> bool {
    true
}

fn default_show_scopes() -> bool {
    true
}

fn default_unreleased_heading() -> String {
    "Unreleased".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChangelogConfig {
    #[serde(default = "default_path")]
    pub(super) path: String,
    #[serde(default = "default_include_unreleased")]
    pub(super) include_unreleased: bool,
    #[serde(default = "default_preamble")]
    pub(super) preamble: String,
    #[serde(default = "default_show_hashes")]
    pub(super) show_hashes: bool,
    #[serde(default = "default_show_scopes")]
    pub(super) show_scopes: bool,
    #[serde(default = "default_unreleased_heading")]
    pub(super) unreleased_heading: String,
    #[serde(default)]
    pub(super) sections: HashMap<String, String>,
    #[serde(default)]
    pub(super) version: Option<String>,
    #[serde(default)]
    pub(super) label: Option<String>,
    #[serde(default)]
    pub(super) notes_path: Option<String>,
    #[serde(default)]
    pub(super) section: Option<String>,
}

impl Default for ChangelogConfig {
    fn default() -> Self {
        Self {
            path: default_path(),
            include_unreleased: default_include_unreleased(),
            preamble: default_preamble(),
            show_hashes: default_show_hashes(),
            show_scopes: default_show_scopes(),
            unreleased_heading: default_unreleased_heading(),
            sections: HashMap::new(),
            version: None,
            label: None,
            notes_path: None,
            section: None,
        }
    }
}

impl ChangelogConfig {
    pub(super) fn from_with(with: Option<evin_flow::SpannedValue>) -> Result<Self, ChangelogError> {
        let config: Self = evin_action_sdk::parse_config(with.as_ref())
            .map_err(ChangelogError::InvalidConfiguration)?;
        config
            .validate()
            .map_err(ChangelogError::ConfigurationIssue)?;
        Ok(config)
    }

    pub(super) fn validate(&self) -> Result<(), IssueCode> {
        if self.path.trim().is_empty() {
            return Err(IssueCode::new("changelog-path-must-not-be-empty"));
        }
        if self
            .notes_path
            .as_deref()
            .is_some_and(|path| path.trim().is_empty())
        {
            return Err(IssueCode::new("notes-path-must-not-be-empty-when-provided"));
        }
        if let Some(section) = &self.section {
            if section.trim().is_empty() || self.notes_path.is_none() {
                return Err(IssueCode::new(
                    "section-must-be-non-empty-and-requires-notes-path",
                ));
            }
            validate_template_syntax(section).map_err(|error| {
                IssueCode::new("section-template-invalid").arg("error", error.to_string())
            })?;
        }
        if self.unreleased_heading.trim().is_empty() {
            return Err(IssueCode::new("unreleased-heading-must-not-be-empty"));
        }
        if self.sections.keys().any(|key| key.trim().is_empty()) {
            return Err(IssueCode::new("sections-keys-must-not-be-empty"));
        }
        if let Some(version) = &self.version {
            let is_empty = version.trim().is_empty();
            if is_empty {
                return Err(IssueCode::new("version-must-not-be-empty"));
            }
            validate_template_syntax(version).map_err(|e| {
                IssueCode::new("issue-changelog-version-invalid").arg("error", e.to_string())
            })?;
        } else if !self.include_unreleased && self.section.is_none() {
            return Err(IssueCode::new(
                "version-is-required-when-include-unreleased-is-false",
            ));
        }
        if let Some(label) = &self.label {
            if label.trim().is_empty() {
                return Err(IssueCode::new("label-must-not-be-empty-when-provided"));
            }
            validate_template_syntax(label).map_err(|e| {
                IssueCode::new("label-template-invalid").arg("error", e.to_string())
            })?;
        }
        Ok(())
    }
}
