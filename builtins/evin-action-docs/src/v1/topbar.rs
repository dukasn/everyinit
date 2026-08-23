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

use crate::v1::presentation::{self, Presentation};
use serde::Deserialize;
use std::collections::BTreeSet;

pub const MATTER_VARIANTS: &[&str] = &[
    "deepspace",
    "collider",
    "industrial",
    "genome",
    "blueprint",
    "crt",
    "aurora",
    "cyber",
    "prismatic",
    "brutalist",
    "holo",
    "geo",
    "luxury",
];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum TopbarSetting {
    Disabled(bool),
    Enabled(Topbar),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Topbar {
    #[serde(default)]
    pub id: Option<String>,
    pub variant: String,
    #[serde(default)]
    pub badge: Option<String>,
    pub message: String,
    #[serde(default)]
    pub action: Option<TopbarAction>,
    #[serde(default)]
    pub dismissible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopbarAction {
    pub label: String,
    pub href: String,
}

impl TopbarSetting {
    pub fn enabled(&self) -> Option<&Topbar> {
        match self {
            Self::Enabled(topbar) => Some(topbar),
            Self::Disabled(_) => None,
        }
    }

    pub fn validate(&self, theme: Option<&str>) -> Result<(), String> {
        match self {
            Self::Disabled(false) => Ok(()),
            Self::Disabled(true) => Err("topbar must be a mapping or false".to_string()),
            Self::Enabled(topbar) => topbar.validate(theme),
        }
    }
}

pub fn resolve(
    root: Option<&TopbarSetting>,
    fallback: Option<&TopbarSetting>,
    presentation: Option<&Presentation>,
) -> Result<Option<Topbar>, String> {
    let Some(topbar) = root.or(fallback).and_then(TopbarSetting::enabled) else {
        return Ok(None);
    };
    let mut resolved = topbar.clone();
    resolved.variant = presentation::resolve_variant(&topbar.variant, presentation)?;
    Ok(Some(resolved))
}

/// Configures Matter's shared presentation stylesheet before compilation.
/// Variants and consumer shells are build-time values, so unused visual
/// treatments, tokens, and component CSS never reach the generated stylesheet.
pub fn configure_matter_scss(
    raw_scss: &str,
    theme: &str,
    variants: &BTreeSet<String>,
    topbar_enabled: bool,
    spotlight_enabled: bool,
) -> String {
    if theme != "matter" {
        return raw_scss.to_string();
    }

    let variants = variants
        .iter()
        .map(|variant| format!("\"{variant}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "$evin-presentation-variants: ({variants});\n$evin-topbar-enabled: {topbar_enabled};\n$evin-spotlight-enabled: {spotlight_enabled};\n{raw_scss}"
    )
}

impl Topbar {
    pub fn validate(&self, theme: Option<&str>) -> Result<(), String> {
        presentation::validate_variant(&self.variant, theme, true)?;
        if self.message.trim().is_empty() {
            return Err("topbar.message must be a non-empty string".to_string());
        }
        if let Some(badge) = &self.badge
            && badge.trim().is_empty()
        {
            return Err("topbar.badge must not be empty when provided".to_string());
        }
        if self.dismissible {
            let Some(id) = self.id.as_deref() else {
                return Err("topbar.id is required when topbar.dismissible is true".to_string());
            };
            if !is_stable_id(id) {
                return Err(
                    "topbar.id must contain only ASCII letters, digits, '_' or '-'".to_string(),
                );
            }
        } else if let Some(id) = &self.id
            && !is_stable_id(id)
        {
            return Err(
                "topbar.id must contain only ASCII letters, digits, '_' or '-'".to_string(),
            );
        }
        if let Some(action) = &self.action {
            if action.label.trim().is_empty() {
                return Err("topbar.action.label must be a non-empty string".to_string());
            }
            presentation::validate_href(&action.href, "topbar.action.href")?;
        }
        Ok(())
    }
}

pub fn from_metadata(value: &serde_json::Value) -> Result<TopbarSetting, String> {
    serde_json::from_value(value.clone())
        .map_err(|error| format!("invalid `topbar` metadata: {error}"))
}

fn is_stable_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topbar() -> Topbar {
        Topbar {
            id: Some("canary-1".to_string()),
            variant: "genome".to_string(),
            badge: Some("Canary".to_string()),
            message: "A release is coming.".to_string(),
            action: Some(TopbarAction {
                label: "Watch".to_string(),
                href: "/releases".to_string(),
            }),
            dismissible: true,
        }
    }

    #[test]
    fn validates_a_matter_topbar() {
        assert!(topbar().validate(Some("matter")).is_ok());
    }

    #[test]
    fn every_matter_variant_is_selectable() {
        for variant in MATTER_VARIANTS {
            let mut value = topbar();
            value.variant = (*variant).to_string();
            assert!(value.validate(Some("matter")).is_ok(), "{variant}");
        }
    }

    #[test]
    fn rejects_invalid_setting_values() {
        let mut value = topbar();
        value.variant = "Not valid".to_string();
        assert!(value.validate(None).unwrap_err().contains("kebab-case"));
        value.variant = "genome".to_string();
        value.id = None;
        assert!(value.validate(None).unwrap_err().contains("id is required"));
        assert!(TopbarSetting::Disabled(true).validate(None).is_err());
    }

    #[test]
    fn permits_custom_layout_variants() {
        let mut value = topbar();
        value.variant = "brand-alert".to_string();
        assert!(value.validate(Some("custom")).is_ok());
        assert!(value.validate(Some("matter")).is_err());
    }

    #[test]
    fn root_setting_wins_over_the_action_fallback() {
        let fallback = TopbarSetting::Enabled(topbar());
        let disabled = TopbarSetting::Disabled(false);
        assert!(
            resolve(Some(&disabled), Some(&fallback), None)
                .unwrap()
                .is_none()
        );

        let mut root_topbar = topbar();
        root_topbar.variant = "aurora".to_string();
        let root = TopbarSetting::Enabled(root_topbar);
        assert_eq!(
            resolve(Some(&root), Some(&fallback), None)
                .unwrap()
                .unwrap()
                .variant,
            "aurora"
        );
        assert_eq!(
            resolve(None, Some(&fallback), None)
                .unwrap()
                .unwrap()
                .variant,
            "genome"
        );
    }

    #[test]
    fn configures_matter_scss_with_only_the_selected_variant() {
        let variants = BTreeSet::from(["genome".to_string()]);
        let configured =
            configure_matter_scss("@use \"core/banner\";", "matter", &variants, true, false);
        assert!(configured.starts_with("$evin-presentation-variants: (\"genome\");"));
        assert!(
            configure_matter_scss("body {}", "custom", &variants, true, false).starts_with("body")
        );
        assert!(
            configure_matter_scss("body {}", "matter", &BTreeSet::new(), false, false)
                .starts_with("$evin-presentation-variants: ();")
        );
    }
}
