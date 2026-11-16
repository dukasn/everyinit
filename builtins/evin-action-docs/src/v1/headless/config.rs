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
        return Err(evin_locales::t!("docs-reference-prefix-invalid"));
    }
    if let Some(i18n) = &config.i18n {
        if i18n.locales.is_empty() {
            return Err(evin_locales::t!("docs-i18n-locales-required"));
        }
        let mut locales = HashSet::new();
        for locale in &i18n.locales {
            let parsed = locale
                .parse::<unic_langid::LanguageIdentifier>()
                .map_err(|_| evin_locales::t!("docs-i18n-locale-invalid", locale = locale))?;
            if parsed.to_string() != *locale {
                return Err(evin_locales::t!(
                    "docs-i18n-locale-noncanonical",
                    locale = locale
                ));
            }
            if !locales.insert(parsed.to_string()) {
                return Err(evin_locales::t!(
                    "docs-i18n-locale-duplicate",
                    locale = locale
                ));
            }
        }
        let default = i18n
            .default_locale
            .parse::<unic_langid::LanguageIdentifier>()
            .map_err(|_| evin_locales::t!("docs-i18n-default-locale-invalid"))?;
        if default.to_string() != i18n.default_locale {
            return Err(evin_locales::t!("docs-i18n-default-locale-noncanonical"));
        }
        if !locales.contains(&default.to_string()) {
            return Err(evin_locales::t!("docs-i18n-default-locale-missing"));
        }
    }
    if config.versions.is_empty() {
        if config
            .latest_version
            .as_deref()
            .is_some_and(|id| id != "latest")
        {
            return Err(evin_locales::t!(
                "docs-latest-version-requires-version-list"
            ));
        }
    } else {
        let mut ids = HashSet::new();
        for version in &config.versions {
            if !valid_segment(&version.id) {
                return Err(evin_locales::t!(
                    "docs-version-id-invalid",
                    version = version.id.as_str()
                ));
            }
            if !ids.insert(version.id.clone()) {
                return Err(evin_locales::t!(
                    "docs-version-id-duplicate",
                    version = version.id.as_str()
                ));
            }
        }
        let latest = config
            .latest_version
            .as_deref()
            .ok_or_else(|| evin_locales::t!("docs-latest-version-required"))?;
        if !ids.contains(latest) {
            return Err(evin_locales::t!(
                "docs-latest-version-unlisted",
                version = latest
            ));
        }
    }
    if config.out.as_os_str().is_empty() {
        return Err(evin_locales::t!("docs-output-path-must-not-be-empty"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_validation_errors_use_fluent_messages() {
        let cases = [
            (
                "reference_prefix: '?'\n",
                evin_locales::t!("docs-reference-prefix-invalid"),
            ),
            (
                "i18n:\n  default_locale: en\n  locales: []\n",
                evin_locales::t!("docs-i18n-locales-required"),
            ),
            (
                "i18n:\n  default_locale: en\n  locales: ['not a locale']\n",
                evin_locales::t!("docs-i18n-locale-invalid", locale = "not a locale"),
            ),
            (
                "i18n:\n  default_locale: en\n  locales: [pt_br]\n",
                evin_locales::t!("docs-i18n-locale-noncanonical", locale = "pt_br"),
            ),
            (
                "i18n:\n  default_locale: en\n  locales: [en, en]\n",
                evin_locales::t!("docs-i18n-locale-duplicate", locale = "en"),
            ),
            (
                "i18n:\n  default_locale: 'not a locale'\n  locales: [en]\n",
                evin_locales::t!("docs-i18n-default-locale-invalid"),
            ),
            (
                "i18n:\n  default_locale: EN\n  locales: [en]\n",
                evin_locales::t!("docs-i18n-default-locale-noncanonical"),
            ),
            (
                "i18n:\n  default_locale: pt\n  locales: [en]\n",
                evin_locales::t!("docs-i18n-default-locale-missing"),
            ),
            (
                "latest_version: v1\n",
                evin_locales::t!("docs-latest-version-requires-version-list"),
            ),
            (
                "versions:\n  - id: '../'\n",
                evin_locales::t!("docs-version-id-invalid", version = "../"),
            ),
            (
                "versions:\n  - id: v1\n  - id: v1\n",
                evin_locales::t!("docs-version-id-duplicate", version = "v1"),
            ),
            (
                "versions:\n  - id: v1\n",
                evin_locales::t!("docs-latest-version-required"),
            ),
            (
                "versions:\n  - id: v1\nlatest_version: v2\n",
                evin_locales::t!("docs-latest-version-unlisted", version = "v2"),
            ),
            (
                "out: ''\n",
                evin_locales::t!("docs-output-path-must-not-be-empty"),
            ),
            (
                "asset_dir: ../assets\n",
                evin_locales::t!("docs-asset-dir-invalid"),
            ),
        ];

        for (yaml, expected) in cases {
            let with = evin_flow::parse_yaml(yaml).unwrap();
            let config: HeadlessConfig = evin_action_sdk::parse_config(Some(&with)).unwrap();
            let actual = validate(&config).unwrap_err();
            assert!(
                !expected.starts_with("<missing locale key:"),
                "expected Fluent message to exist for {yaml:?}: {expected}"
            );
            assert_eq!(actual, expected, "{yaml:?}");
        }
    }
}
