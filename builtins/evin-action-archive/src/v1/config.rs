// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

#![allow(clippy::result_large_err)]
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
