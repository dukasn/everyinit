// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

#![allow(clippy::result_large_err)]
use evin_action_sdk::packaging::{LinuxArch, PayloadMapping};
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
pub struct TarGzConfig {
    pub path: String,
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    pub prefix: Option<String>,
    #[serde(default)]
    pub layout: LayoutMode,
    pub compression_level: Option<u32>,
}

impl TarGzConfig {
    pub fn validate(&self) -> Result<(), Diagnostic> {
        if self.path.is_empty() {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "archive-path-must-not-be-empty",
            )));
        }
        if self.include.is_empty() {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "include-must-contain-at-least-one-pattern",
            )));
        }
        if self.include.iter().any(|pattern| pattern.trim().is_empty()) {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "include-patterns-must-not-be-empty",
            )));
        }
        if self.exclude.iter().any(|pattern| pattern.trim().is_empty()) {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "exclude-patterns-must-not-be-empty",
            )));
        }
        if self
            .prefix
            .as_ref()
            .is_some_and(|prefix| prefix.trim().is_empty())
        {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "prefix-must-not-be-empty-when-provided",
            )));
        }
        if self
            .compression_level
            .is_some_and(|level| !(1..=9).contains(&level))
        {
            return Err(evin_locales::diagnostic(IssueCode::new(
                "compression-level-invalid",
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageConfig {
    pub name: String,
    pub summary: String,
    pub maintainer: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub payload: Vec<PayloadMapping>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebConfig {
    pub output_dir: String,
    pub architecture: LinuxArch,
    pub release: String,
    pub package: PackageConfig,
    pub description: Option<String>,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApkConfig {
    pub output_dir: String,
    pub architecture: LinuxArch,
    pub release: String,
    pub package: PackageConfig,
    pub license: String,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchConfig {
    pub output_dir: String,
    pub architecture: LinuxArch,
    pub release: String,
    pub package: PackageConfig,
    pub license: String,
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

pub fn validate_base_package_config(output_dir: &str, release: &str) -> Result<(), Diagnostic> {
    if output_dir.trim().is_empty() || release.trim().is_empty() {
        return Err(evin_locales::diagnostic(IssueCode::new(
            "package-output-and-release-required",
        )));
    }
    Ok(())
}

pub fn validate_release_reference(release: &str, needs: &[String]) -> Result<(), IssueCode> {
    let task = release_reference_task(release)
        .ok_or_else(|| IssueCode::new("archive-release-reference-format-invalid"))?;
    if !needs.iter().any(|need| need == task) {
        return Err(IssueCode::new("archive-release-reference-missing-need").arg("task", task));
    }
    Ok(())
}

fn release_reference_task(value: &str) -> Option<&str> {
    value
        .strip_prefix("${")?
        .strip_suffix(".release}")
        .filter(|task| !task.is_empty())
}
