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
use evin_action_sdk::packaging::{LinuxArch, PackageFormat};
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

    pub include: Option<Vec<String>>,

    #[serde(default)]
    pub exclude: Vec<String>,

    pub prefix: Option<String>,

    #[serde(default)]
    pub layout: LayoutMode,

    pub compression_level: Option<u32>,

    pub architecture: Option<LinuxArch>,
    pub package: Option<String>,
    pub release: Option<String>,
}

fn default_format() -> String {
    "tar.gz".to_string()
}

impl ArchiveConfig {
    pub fn validate(&self) -> Result<(), Diagnostic> {
        let Some(format) = PackageFormat::parse(&self.format) else {
            return Err(evin_locales::diagnostic(
                IssueCode::new("unsupported-format").arg("format", &self.format),
            ));
        };
        if self.path.trim().is_empty() {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "archive-path-must-not-be-empty",
            )));
        }
        validate_template_syntax(&self.format).map_err(|e| {
            evin_locales::diagnostic(
                IssueCode::new("issue-archive-path-invalid").arg("error", e.to_string()),
            )
        })?;
        if format == PackageFormat::TarGz {
            validate_template_syntax(&self.path).map_err(|e| {
                evin_locales::diagnostic(
                    IssueCode::new("issue-archive-path-invalid").arg("error", e.to_string()),
                )
            })?;
        } else {
            let protected = self
                .path
                .replace("{{", "__EVIN_INTRINSIC_OPEN__")
                .replace("}}", "__EVIN_INTRINSIC_CLOSE__");
            validate_template_syntax(&protected).map_err(|e| {
                evin_locales::diagnostic(
                    IssueCode::new("issue-archive-path-invalid").arg("error", e.to_string()),
                )
            })?;
            let tokens = evin_action_sdk::packaging::PackageTokens {
                name: "name".into(),
                version: "version".into(),
                arch: "arch".into(),
                revision: 0,
                maintainer: "maintainer".into(),
                summary: "summary".into(),
            };
            evin_action_sdk::packaging::render_intrinsic_tokens(&self.path, &tokens).map_err(
                |e| {
                    evin_locales::diagnostic(
                        IssueCode::new("issue-archive-path-invalid").arg("error", e.to_string()),
                    )
                },
            )?;
        }
        if format == PackageFormat::TarGz && self.include.as_ref().is_none_or(Vec::is_empty) {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "include-must-contain-at-least-one-pattern",
            )));
        }
        if self
            .include
            .as_ref()
            .is_some_and(|include| include.iter().any(|value| value.trim().is_empty()))
        {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "include-patterns-must-not-be-empty",
            )));
        }
        for pattern in self.include.as_deref().unwrap_or_default() {
            validate_template_syntax(pattern).map_err(|e| {
                evin_locales::diagnostic(
                    IssueCode::new("issue-archive-path-invalid").arg("error", e.to_string()),
                )
            })?;
        }
        if self.exclude.iter().any(|value| value.trim().is_empty()) {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "exclude-patterns-must-not-be-empty",
            )));
        }
        for pattern in &self.exclude {
            validate_template_syntax(pattern).map_err(|e| {
                evin_locales::diagnostic(
                    IssueCode::new("issue-archive-path-invalid").arg("error", e.to_string()),
                )
            })?;
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
        match format {
            PackageFormat::TarGz => {
                if self.architecture.is_some() || self.package.is_some() || self.release.is_some() {
                    return Err(evin_locales::diagnostic(IssueCode::custom(
                        "architecture, package, and release are only valid for distro package formats",
                    )));
                }
            }
            PackageFormat::Deb | PackageFormat::Apk => {
                if self.architecture.is_none() {
                    return Err(evin_locales::diagnostic(IssueCode::custom(
                        "architecture is required for distro package formats",
                    )));
                }
                if self
                    .package
                    .as_ref()
                    .is_none_or(|value| value.trim().is_empty())
                    || self
                        .release
                        .as_ref()
                        .is_none_or(|value| value.trim().is_empty())
                {
                    return Err(evin_locales::diagnostic(IssueCode::custom(
                        "package and release are required for distro package formats",
                    )));
                }
                if self.package.as_ref().is_some_and(|package| {
                    !package.chars().all(|character| {
                        character.is_ascii_lowercase()
                            || character.is_ascii_digit()
                            || character == '-'
                    })
                }) {
                    return Err(evin_locales::diagnostic(IssueCode::custom(
                        "package must be a literal lowercase package ID",
                    )));
                }
                if self.include.is_some()
                    || !self.exclude.is_empty()
                    || self.prefix.is_some()
                    || self.compression_level.is_some()
                    || self.layout != LayoutMode::Flat
                {
                    return Err(evin_locales::diagnostic(IssueCode::custom(
                        "include, exclude, prefix, layout, and compression_level are only valid for tar.gz",
                    )));
                }
            }
        }
        Ok(())
    }
}
