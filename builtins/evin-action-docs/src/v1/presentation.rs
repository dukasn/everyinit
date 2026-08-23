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

use crate::v1::topbar::MATTER_VARIANTS;
use serde::Deserialize;
use url::Url;

pub const INHERIT_VARIANT: &str = "inherit";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Presentation {
    pub variant: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spotlight {
    #[serde(default = "inherit_variant")]
    pub variant: String,
    #[serde(default)]
    pub badge: Option<String>,
    pub title: String,
    pub description: String,
    pub action: PresentationAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationAction {
    pub label: String,
    pub href: String,
}

pub fn inherit_variant() -> String {
    INHERIT_VARIANT.to_string()
}

pub fn from_metadata(value: &serde_json::Value) -> Result<Presentation, String> {
    serde_json::from_value(value.clone())
        .map_err(|error| format!("invalid `presentation` metadata: {error}"))
}

pub fn resolve_variant(value: &str, presentation: Option<&Presentation>) -> Result<String, String> {
    if value == INHERIT_VARIANT {
        return presentation
            .map(|setting| setting.variant.clone())
            .ok_or_else(|| {
                "variant: inherit requires a root or action-level presentation.variant".to_string()
            });
    }
    Ok(value.to_string())
}

impl Presentation {
    pub fn validate(&self, theme: Option<&str>) -> Result<(), String> {
        validate_variant(&self.variant, theme, false)
    }
}

impl Spotlight {
    pub fn validate(&self, theme: Option<&str>) -> Result<(), String> {
        validate_variant(&self.variant, theme, true)?;
        validate_non_empty("spotlight.title", &self.title)?;
        validate_non_empty("spotlight.description", &self.description)?;
        if let Some(badge) = &self.badge {
            validate_non_empty("spotlight.badge", badge)?;
        }
        validate_non_empty("spotlight.action.label", &self.action.label)?;
        validate_href(&self.action.href, "spotlight.action.href")
    }

    pub fn resolve_variant(&self, presentation: Option<&Presentation>) -> Result<Self, String> {
        let mut resolved = self.clone();
        resolved.variant = resolve_variant(&self.variant, presentation)?;
        Ok(resolved)
    }
}

pub fn validate_variant(
    value: &str,
    theme: Option<&str>,
    allow_inherit: bool,
) -> Result<(), String> {
    if allow_inherit && value == INHERIT_VARIANT {
        return Ok(());
    }
    if !is_kebab_case(value) {
        return Err("presentation.variant must be a non-empty kebab-case identifier".to_string());
    }
    if theme.is_some_and(|theme| theme == "matter") && !MATTER_VARIANTS.contains(&value) {
        return Err(format!(
            "presentation variant '{value}' is not supported by the Matter layout; use one of: {}",
            MATTER_VARIANTS.join(", ")
        ));
    }
    Ok(())
}

pub fn validate_href(value: &str, field: &str) -> Result<(), String> {
    let href = value.trim();
    if href.is_empty() {
        return Err(format!(
            "{field} must be a non-empty site route, HTTPS URL, or mailto link"
        ));
    }
    if href.starts_with('/') && !href.starts_with("//") {
        return Ok(());
    }
    let external = Url::parse(href).ok().is_some_and(|url| {
        (url.scheme() == "https" && url.host_str().is_some()) || url.scheme() == "mailto"
    });
    external
        .then_some(())
        .ok_or_else(|| format!("{field} must be a site route, absolute HTTPS URL, or mailto link"))
}

pub fn validate_non_empty(field: &str, value: &str) -> Result<(), String> {
    (!value.trim().is_empty())
        .then_some(())
        .ok_or_else(|| format!("{field} must be a non-empty string"))
}

fn is_kebab_case(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.contains("--")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherited_variants_require_a_presentation_default() {
        assert!(resolve_variant("inherit", None).is_err());
        let presentation = Presentation {
            variant: "holo".to_string(),
        };
        assert_eq!(
            resolve_variant("inherit", Some(&presentation)).unwrap(),
            "holo"
        );
        assert_eq!(resolve_variant("geo", Some(&presentation)).unwrap(), "geo");
    }

    #[test]
    fn spotlight_requires_complete_route_safe_content() {
        let valid: Spotlight = serde_json::from_value(serde_json::json!({
            "title": "A launch",
            "description": "A concise invitation.",
            "action": { "label": "Explore", "href": "/getting-started" }
        }))
        .unwrap();
        assert_eq!(valid.variant, "inherit");
        assert!(valid.validate(Some("matter")).is_ok());

        let invalid: Spotlight = serde_json::from_value(serde_json::json!({
            "title": "A launch",
            "description": "A concise invitation.",
            "action": { "label": "Explore", "href": "http://invalid.example" }
        }))
        .unwrap();
        assert!(invalid.validate(None).is_err());
    }
}
