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

#![allow(clippy::result_large_err)]
use evin_diagnostic::{Diagnostic, IssueCode};
use evin_yaml::template::pipeline::validate_template_syntax;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LayoutMode {
    #[default]
    Flat,
    Relative,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveConfig {
    #[serde(default = "default_format")]
    pub format: String,

    pub path: String,

    pub include: Vec<String>,

    #[serde(default)]
    pub exclude: Vec<String>,

    pub prefix: Option<String>,

    #[serde(default)]
    pub layout: LayoutMode,

    pub compression_level: Option<u32>,
}

fn default_format() -> String {
    "tar.gz".to_string()
}

impl ArchiveConfig {
    pub fn validate(&self) -> Result<(), Diagnostic> {
        if self.format != "tar.gz" {
            return Err(evin_locales::diagnostic(
                IssueCode::new("unsupported-format").arg("format", &self.format),
            ));
        }
        if self.path.trim().is_empty() {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "archive-path-must-not-be-empty",
            )));
        }
        validate_template_syntax(&self.path).map_err(|e| {
            evin_locales::diagnostic(
                IssueCode::new("issue-archive-path-invalid").arg("error", e.to_string()),
            )
        })?;
        if self.include.is_empty() {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "include-must-contain-at-least-one-pattern",
            )));
        }
        if self.include.iter().any(|value| value.trim().is_empty()) {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "include-patterns-must-not-be-empty",
            )));
        }
        if self.exclude.iter().any(|value| value.trim().is_empty()) {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "exclude-patterns-must-not-be-empty",
            )));
        }
        if self
            .prefix
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "prefix-must-not-be-empty-when-provided",
            )));
        }
        if let Some(prefix) = &self.prefix {
            validate_template_syntax(prefix).map_err(|e| {
                evin_locales::diagnostic(
                    IssueCode::new("issue-archive-prefix-invalid").arg("error", e.to_string()),
                )
            })?;
        }
        if let Some(level) = self.compression_level
            && !(1..=9).contains(&level)
        {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "compression-level-invalid",
            )));
        }
        Ok(())
    }
}
