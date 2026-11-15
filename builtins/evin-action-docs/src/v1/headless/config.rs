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
use std::collections::HashSet;
use std::path::PathBuf;

fn default_source_dir() -> PathBuf {
    PathBuf::from("docs")
}

fn default_out() -> PathBuf {
    PathBuf::from("dist/docs-data")
}

fn default_extract_frontmatter() -> bool {
    true
}

fn default_reference_prefix() -> String {
    "REF".to_string()
}

fn default_asset_dir() -> String {
    "assets".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadlessConfig {
    #[serde(default = "default_source_dir")]
    pub source_dir: PathBuf,
    #[serde(default = "default_out")]
    pub out: PathBuf,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default = "default_asset_dir")]
    pub asset_dir: String,
    #[serde(default = "default_extract_frontmatter")]
    pub extract_frontmatter: bool,
    #[serde(default)]
    pub strict_filenames: bool,
    #[serde(default = "default_reference_prefix")]
    pub reference_prefix: String,
    #[serde(default = "default_true")]
    pub validate_links: bool,
    #[serde(default)]
    pub i18n: Option<HeadlessI18n>,
    #[serde(default)]
    pub versions: Vec<VersionSource>,
    #[serde(default)]
    pub latest_version: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadlessI18n {
    pub default_locale: String,
    pub locales: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionSource {
    pub id: String,
    #[serde(default)]
    pub source_dir: Option<PathBuf>,
}

pub fn validate_config(
    with: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    let config: HeadlessConfig = evin_action_sdk::parse_config(with)?;
    validate(&config).map_err(|message| evin_flow::ConfigError {
        message,
        span: None,
    })
}

pub(super) fn validate(config: &HeadlessConfig) -> Result<(), String> {
    let prefix = config.reference_prefix.trim();
    if prefix.is_empty() || !prefix.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err("reference_prefix must contain only ASCII letters and digits".into());
    }
    if let Some(i18n) = &config.i18n {
        if i18n.locales.is_empty() {
            return Err("i18n.locales must contain at least one locale".into());
        }
        let mut locales = HashSet::new();
        for locale in &i18n.locales {
            let parsed = locale
                .parse::<unic_langid::LanguageIdentifier>()
                .map_err(|_| format!("i18n locale '{locale}' is not a valid language tag"))?;
            if parsed.to_string() != *locale {
                return Err(format!(
                    "i18n locale '{locale}' must use canonical language-tag casing"
                ));
            }
            if !locales.insert(parsed.to_string()) {
                return Err(format!("i18n.locales contains duplicate locale '{locale}'"));
            }
        }
        let default = i18n
            .default_locale
            .parse::<unic_langid::LanguageIdentifier>()
            .map_err(|_| "i18n.default_locale is not a valid language tag".to_string())?;
        if default.to_string() != i18n.default_locale {
            return Err("i18n.default_locale must use canonical language-tag casing".into());
        }
        if !locales.contains(&default.to_string()) {
            return Err("i18n.default_locale must be included in i18n.locales".into());
        }
    }
    if config.versions.is_empty() {
        if config
            .latest_version
            .as_deref()
            .is_some_and(|id| id != "latest")
        {
            return Err("latest_version must be omitted when versions is empty".into());
        }
    } else {
        let mut ids = HashSet::new();
        for version in &config.versions {
            if !valid_segment(&version.id) {
                return Err(format!(
                    "version id '{}' must be a path-safe identifier",
                    version.id
                ));
            }
            if !ids.insert(version.id.clone()) {
                return Err(format!("versions contains duplicate id '{}'", version.id));
            }
        }
        let latest = config
            .latest_version
            .as_deref()
            .ok_or_else(|| "latest_version is required when versions is configured".to_string())?;
        if !ids.contains(latest) {
            return Err(format!(
                "latest_version '{latest}' is not listed in versions"
            ));
        }
    }
    if config.out.as_os_str().is_empty() {
        return Err("out must not be empty".into());
    }
    crate::v1::assets::validate_asset_dir(&config.asset_dir)?;
    Ok(())
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && !matches!(value, "." | "..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}
