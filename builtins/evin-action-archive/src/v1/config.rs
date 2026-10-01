// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

#![allow(clippy::result_large_err)]
use evin_action_sdk::packaging::LinuxArch;
use evin_diagnostic::{Diagnostic, IssueCode};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LayoutMode {
    #[default]
    Flat,
    Relative,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageConfig {
    pub name: String,
    pub summary: String,
    #[serde(default)]
    pub description: Option<String>,
    pub maintainer: String,
    pub license: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub payload: Vec<PayloadMapping>,
    #[serde(default)]
    pub targets: PackageTargets,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayloadMapping {
    pub source: String,
    pub strip_prefix: String,
    pub destination: String,
    #[serde(default)]
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageTargets {
    #[serde(default)]
    pub deb: Option<DebTarget>,
    #[serde(default)]
    pub apk: Option<ApkTarget>,
    #[serde(default)]
    pub arch: Option<ArchTarget>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebTarget {
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub scripts: DebScripts,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebScripts {
    pub preinst: Option<String>,
    pub postinst: Option<String>,
    pub prerm: Option<String>,
    pub postrm: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApkTarget {
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub scripts: ApkScripts,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApkScripts {
    pub pre_install: Option<String>,
    pub post_install: Option<String>,
    pub pre_deinstall: Option<String>,
    pub post_deinstall: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchTarget {
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub optdepends: Vec<String>,
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub conflicts: Vec<String>,
    #[serde(default)]
    pub replaces: Vec<String>,
    #[serde(default)]
    pub backup: Vec<String>,
    #[serde(default)]
    pub scripts: ArchScripts,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchScripts {
    pub install: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveConfig {
    #[serde(default = "default_format")]
    pub format: String,
    pub path: Option<String>,
    pub output_dir: Option<String>,
    pub include: Option<Vec<String>>,
    #[serde(default)]
    pub exclude: Vec<String>,
    pub prefix: Option<String>,
    #[serde(default)]
    pub layout: LayoutMode,
    pub compression_level: Option<u32>,
    pub architecture: Option<LinuxArch>,
    pub release: Option<String>,
    pub package: Option<PackageConfig>,
}

fn default_format() -> String {
    "tar.gz".to_string()
}

impl ArchiveConfig {
    pub fn package_format(&self) -> Result<evin_action_sdk::packaging::PackageFormat, Diagnostic> {
        evin_action_sdk::packaging::PackageFormat::parse(&self.format).ok_or_else(|| {
            evin_locales::diagnostic(
                IssueCode::new("unsupported-format").arg("format", &self.format),
            )
        })
    }

    pub fn validate(&self) -> Result<(), Diagnostic> {
        use evin_action_sdk::packaging::PackageFormat;
        let format = self.package_format()?;
        if let Some(level) = self.compression_level
            && !(1..=9).contains(&level)
        {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "compression-level-invalid",
            )));
        }
        match format {
            PackageFormat::TarGz => {
                if self.path.as_deref().is_none_or(str::is_empty) {
                    return Err(evin_locales::diagnostic(IssueCode::new(
                        "archive-path-must-not-be-empty",
                    )));
                }
                if self.output_dir.is_some()
                    || self.architecture.is_some()
                    || self.release.is_some()
                    || self.package.is_some()
                {
                    return Err(evin_locales::diagnostic(IssueCode::custom(
                        "output_dir, architecture, release, and package are only valid for distro package formats",
                    )));
                }
                if self.include.as_ref().is_none_or(Vec::is_empty) {
                    return Err(evin_locales::diagnostic(IssueCode::new(
                        "include-must-contain-at-least-one-pattern",
                    )));
                }
            }
            PackageFormat::Deb | PackageFormat::Apk | PackageFormat::Arch => {
                if self.output_dir.as_deref().is_none_or(str::is_empty)
                    || self.architecture.is_none()
                    || self.release.as_deref().is_none_or(str::is_empty)
                    || self.package.is_none()
                {
                    return Err(evin_locales::diagnostic(IssueCode::custom(
                        "output_dir, architecture, release, and package are required for distro package formats",
                    )));
                }
                if self.path.is_some()
                    || self.include.is_some()
                    || !self.exclude.is_empty()
                    || self.prefix.is_some()
                    || self.compression_level.is_some()
                    || self.layout != LayoutMode::Flat
                {
                    return Err(evin_locales::diagnostic(IssueCode::custom(
                        "path, include, exclude, prefix, layout, and compression_level are only valid for tar.gz",
                    )));
                }
            }
        }
        Ok(())
    }
}
