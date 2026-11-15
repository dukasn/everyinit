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
use evin_action_sdk::RunnerContext;
use evin_flow::template::pipeline::{
    resolve_templates_with_self, validate_template_syntax_with_self,
};
use regex::Regex;
use semver::{Prerelease, Version};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum FileConfig {
    Auto(String),
    Custom(CustomFileConfig),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct CustomFileConfig {
    pub path: String,
    #[serde(rename = "match")]
    pub match_pattern: String,
    pub write: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(default, deny_unknown_fields)]
pub struct GitConfig {
    pub commit: bool,
    pub tag: bool,
    pub defer: bool,
    pub message: Option<String>,
    pub include: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VersionBumperConfig {
    pub files: Vec<FileConfig>,
    pub git: GitConfig,
    pub read_only: bool,
    pub fixed_version: Option<String>,
    pub initial_version: String,
    pub stable: bool,
    pub prerelease: Option<String>,
    pub channel: Option<String>,
    pub require_change: bool,
}

impl Default for VersionBumperConfig {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            git: GitConfig::default(),
            read_only: false,
            fixed_version: None,
            initial_version: "0.1.0".to_string(),
            stable: false,
            prerelease: None,
            channel: None,
            require_change: false,
        }
    }
}

impl VersionBumperConfig {
    pub fn from_with(
        with: Option<evin_flow::SpannedValue>,
    ) -> Result<Self, evin_flow::ConfigError> {
        let with = with.unwrap_or_else(|| {
            evin_flow::SpannedValue::Mapping(Default::default(), evin_flow::Span::default())
        });
        let config: Self = evin_action_sdk::parse_config(Some(&with))?;
        config.validate().map_err(|e| evin_flow::ConfigError {
            message: e.to_string(),
            span: None,
        })?;
        Ok(config)
    }

    pub(super) fn validate(&self) -> Result<(), evin_diagnostic::Diagnostic> {
        Version::parse(&self.initial_version)
            .map_err(|e| format!("initial_version must be valid SemVer: {e}"))?;
        if self.stable && self.prerelease.is_some() {
            return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                "stable-and-prerelease-conflict",
            )));
        }
        if self.stable && self.channel.is_some() {
            return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                "stable-and-channel-conflict",
            )));
        }
        if self.prerelease.is_some() && self.channel.is_some() {
            return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                "prerelease-and-channel-conflict",
            )));
        }
        if let Some(prerelease) = &self.prerelease {
            if prerelease.trim().is_empty() {
                return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                    "prerelease-must-not-be-empty",
                )));
            }
            Prerelease::new(&format!("{prerelease}.1"))
                .map_err(|e| format!("prerelease is invalid: {e}"))?;
        }
        if self.git.tag && !self.git.commit {
            return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                "git-tag-requires-git-commit",
            )));
        }
        if self.git.defer && (!self.git.commit || !self.git.tag) {
            return Err("git.defer requires both git.commit and git.tag to be true"
                .to_string()
                .into());
        }
        if self.read_only
            && (!self.files.is_empty()
                || self.git.commit
                || self.git.tag
                || self.git.defer
                || !self.git.include.is_empty())
        {
            return Err(
                "read_only cannot be combined with file updates or Git mutation"
                    .to_string()
                    .into(),
            );
        }
        if let Some(version) = &self.fixed_version {
            if !version.contains("${") {
                Version::parse(version)
                    .map_err(|error| format!("fixed_version must be valid SemVer: {error}"))?;
            }
            if !self.read_only {
                return Err("fixed_version is only valid with read_only: true"
                    .to_string()
                    .into());
            }
        }
        if let Some(msg) = &self.git.message {
            if msg.trim().is_empty() {
                return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                    "git-message-must-not-be-empty",
                )));
            }
            validate_tokens(msg, &["version"])?;
        }
        if self.git.include.iter().any(|path| path.trim().is_empty()) {
            return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                "git-include-paths-must-not-be-empty",
            )));
        }
        for file in &self.files {
            validate_file(file)?;
        }
        Ok(())
    }
}

fn validate_tokens(text: &str, allowed: &[&str]) -> Result<(), evin_diagnostic::Diagnostic> {
    validate_template_syntax_with_self(text, allowed)
        .map_err(|e| e.to_string().into())
        .and_then(|_| {
            static RE: OnceLock<Regex> = OnceLock::new();
            let regex = RE.get_or_init(|| Regex::new(r"\$\{(?P<body>[^{}]+)\}").unwrap());
            for captures in regex.captures_iter(text) {
                let body = captures.name("body").unwrap().as_str();
                if !allowed.contains(&body) {
                    return Err(evin_locales::diagnostic(
                        evin_diagnostic::IssueCode::new(
                            "issue-version-bumper-unsupported-template",
                        )
                        .arg("template", body),
                    ));
                }
            }
            Ok(())
        })
}

pub(super) fn resolve_local_template(
    text: &str,
    ctx: &RunnerContext,
    values: &HashMap<String, String>,
) -> Result<String, String> {
    resolve_templates_with_self(text, ctx, values)
        .map_err(|e| e.to_string())
        .map_err(|e| e.to_string())
}

pub(super) fn manifest_kind(path: &str) -> Option<&'static str> {
    match Path::new(path).file_name()?.to_str()? {
        "Cargo.toml" | "pyproject.toml" => Some("toml"),
        "package.json" => Some("json"),
        _ => None,
    }
}

pub(super) fn toml_version_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?m)^version\s*=\s*["']([^"']+)["']"#).unwrap())
}

pub(super) fn json_version_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?m)"version"\s*:\s*["']([^"']+)["']"#).unwrap())
}

fn validate_file(file: &FileConfig) -> Result<(), evin_diagnostic::Diagnostic> {
    match file {
        FileConfig::Auto(path) => {
            if path.trim().is_empty() {
                return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                    "file-path-must-not-be-empty",
                )));
            }
            manifest_kind(path).ok_or_else(|| {
                format!("automatic version detection is unsupported for '{path}'")
            })?;
        }
        FileConfig::Custom(config) => {
            if config.path.trim().is_empty() {
                return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                    "custom-file-path-must-not-be-empty",
                )));
            }
            if config.match_pattern.trim().is_empty() {
                return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                    "custom-file-match-must-not-be-empty",
                )));
            }
            if config.write.trim().is_empty() {
                return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
                    "custom-file-write-must-not-be-empty",
                )));
            }
            validate_tokens(&config.match_pattern, &["previous_version"])?;
            validate_tokens(&config.write, &["version"])?;
            let pattern = config
                .match_pattern
                .replace("${previous_version}", &regex::escape("0.0.0"));
            Regex::new(&pattern).map_err(|e| format!("custom file match is invalid: {e}"))?;
        }
    }
    Ok(())
}

pub fn validate_config(
    with: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    VersionBumperConfig::from_with(with.cloned()).map(|_| ())
}
