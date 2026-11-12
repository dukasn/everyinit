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

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Disabled(false) => Ok(()),
            Self::Disabled(true) => Err("topbar must be a mapping or false".to_string()),
            Self::Enabled(topbar) => topbar.validate(),
        }
    }
}

pub fn resolve(setting: Option<&TopbarSetting>) -> Result<Option<Topbar>, String> {
    let Some(topbar) = setting.and_then(TopbarSetting::enabled) else {
        return Ok(None);
    };
    topbar.validate()?;
    Ok(Some(topbar.clone()))
}

pub fn configure_matter_scss(
    raw_scss: &str,
    topbar_enabled: bool,
    spotlight_enabled: bool,
) -> String {
    format!(
        "$matter-topbar-enabled: {topbar_enabled};\n$matter-spotlight-enabled: {spotlight_enabled};\n{raw_scss}"
    )
}

impl Topbar {
    pub fn validate(&self) -> Result<(), String> {
        if self.message.trim().is_empty() {
            return Err("topbar.message must be a non-empty string".to_string());
        }
        if self
            .badge
            .as_ref()
            .is_some_and(|badge| badge.trim().is_empty())
        {
            return Err("topbar.badge must not be empty when provided".to_string());
        }
        if self.dismissible && self.id.is_none() {
            return Err("topbar.id is required when topbar.dismissible is true".to_string());
        }
        if self.id.as_deref().is_some_and(|id| !is_stable_id(id)) {
            return Err(
                "topbar.id must contain only ASCII letters, digits, '_' or '-'".to_string(),
            );
        }
        if let Some(action) = &self.action {
            if action.label.trim().is_empty() {
                return Err("topbar.action.label must be a non-empty string".to_string());
            }
            validate_href(&action.href, "topbar.action.href")?;
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

fn validate_href(value: &str, field: &str) -> Result<(), String> {
    let href = value.trim();
    if href.starts_with('/') && !href.starts_with("//") {
        return Ok(());
    }
    let external = url::Url::parse(href).ok().is_some_and(|url| {
        (url.scheme() == "https" && url.host_str().is_some()) || url.scheme() == "mailto"
    });
    external
        .then_some(())
        .ok_or_else(|| format!("{field} must be a site route, absolute HTTPS URL, or mailto link"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topbar() -> Topbar {
        Topbar {
            id: Some("canary-1".to_string()),
            badge: Some("Notice".to_string()),
            message: "A release is coming.".to_string(),
            action: Some(TopbarAction {
                label: "Read more".to_string(),
                href: "/releases".to_string(),
            }),
            dismissible: true,
        }
    }

    #[test]
    fn validates_neutral_topbar_content_and_routes() {
        assert!(topbar().validate().is_ok());
        let mut invalid = topbar();
        invalid.action.as_mut().unwrap().href = "javascript:alert(1)".to_string();
        assert!(invalid.validate().is_err());
        invalid = topbar();
        invalid.id = None;
        assert!(invalid.validate().unwrap_err().contains("id is required"));
        assert!(TopbarSetting::Disabled(true).validate().is_err());
    }

    #[test]
    fn resolve_honors_disabled_and_enabled_settings() {
        assert!(
            resolve(Some(&TopbarSetting::Disabled(false)))
                .unwrap()
                .is_none()
        );
        let setting = TopbarSetting::Enabled(topbar());
        assert_eq!(resolve(Some(&setting)).unwrap(), Some(topbar()));
    }

    #[test]
    fn scss_configuration_only_selects_component_presence() {
        let configured = configure_matter_scss("body {}", true, false);
        assert!(configured.starts_with("$matter-topbar-enabled: true;"));
        assert!(configured.contains("$matter-spotlight-enabled: false;"));
    }
}
