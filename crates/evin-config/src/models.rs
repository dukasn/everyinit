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

use evin_yaml::Spanned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Component, Path};
use std::time::Duration;

fn default_watch_debounce() -> String {
    "300ms".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WatchConfig {
    pub paths: Vec<String>,
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default = "default_watch_debounce")]
    pub debounce: String,
}

impl WatchConfig {
    pub fn debounce_duration(&self) -> Result<Duration, String> {
        let value = self.debounce.trim();
        let (number, multiplier) = if let Some(ms) = value.strip_suffix("ms") {
            (ms, 1_u64)
        } else if let Some(seconds) = value.strip_suffix('s') {
            (seconds, 1_000_u64)
        } else {
            return Err("expected a duration such as '300ms' or '1s'".to_string());
        };
        let amount = number
            .parse::<u64>()
            .map_err(|_| "duration must contain a positive integer".to_string())?;
        if amount == 0 {
            return Err("duration must be greater than zero".to_string());
        }
        Ok(Duration::from_millis(amount.saturating_mul(multiplier)))
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.paths.is_empty() {
            return Err("watch.paths must contain at least one pattern".to_string());
        }
        self.debounce_duration()?;
        for pattern in self.paths.iter().chain(&self.ignore) {
            if pattern.trim().is_empty() {
                return Err("watch patterns cannot be empty".to_string());
            }
            let path = Path::new(pattern);
            if path.is_absolute()
                || path
                    .components()
                    .any(|component| component == Component::ParentDir)
            {
                return Err(format!(
                    "watch pattern '{pattern}' must be workspace-relative and cannot contain '..'"
                ));
            }
            globset::Glob::new(pattern)
                .map_err(|error| format!("invalid watch pattern '{pattern}': {error}"))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerObject {
    pub hooks: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepObject {
    pub run: Option<String>,
    pub uses: Option<Spanned<String>>,
    pub with: Option<evin_yaml::SpannedValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EnvDeclaration {
    List(Vec<String>),
    Map(HashMap<String, String>),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskObject {
    #[serde(skip)]
    pub source_file: Option<String>,
    #[serde(default)]
    pub name: String,
    pub description: Option<String>,
    pub stage: Option<String>,
    pub needs: Option<Vec<String>>,
    pub on: Option<TriggerObject>,
    pub env: Option<EnvDeclaration>,
    pub presets: Option<Vec<String>>,
    pub module_env: Option<HashMap<String, String>>,
    pub values: Option<HashMap<String, String>>,
    pub module_values: Option<HashMap<String, String>>,
    pub cwd: Option<String>,
    pub pure: Option<bool>,
    pub parallel: Option<bool>,
    pub watch: Option<WatchConfig>,
    pub run: Option<String>,
    pub steps: Option<Vec<StepObject>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watch(paths: &[&str], ignore: &[&str], debounce: &str) -> WatchConfig {
        WatchConfig {
            paths: paths.iter().map(|value| value.to_string()).collect(),
            ignore: ignore.iter().map(|value| value.to_string()).collect(),
            debounce: debounce.to_string(),
        }
    }

    #[test]
    fn watch_accepts_relative_globs_and_positive_durations() {
        let milliseconds = watch(&["crates/**"], &["target/**"], "300ms");
        assert!(milliseconds.validate().is_ok());
        assert_eq!(
            milliseconds.debounce_duration().unwrap(),
            Duration::from_millis(300)
        );

        let seconds = watch(&["builtins/**"], &[], "2s");
        assert_eq!(seconds.debounce_duration().unwrap(), Duration::from_secs(2));
    }

    #[test]
    fn watch_rejects_empty_unsafe_or_invalid_configuration() {
        assert!(watch(&[], &[], "300ms").validate().is_err());
        assert!(watch(&[""], &[], "300ms").validate().is_err());
        assert!(watch(&["/tmp/**"], &[], "300ms").validate().is_err());
        assert!(
            watch(&["crates/../secrets/**"], &[], "300ms")
                .validate()
                .is_err()
        );
        assert!(watch(&["["], &[], "300ms").validate().is_err());
        assert!(watch(&["crates/**"], &[], "0ms").validate().is_err());
        assert!(watch(&["crates/**"], &[], "1m").validate().is_err());
    }

    #[test]
    fn watch_defaults_to_three_hundred_milliseconds_when_deserialized() {
        let parsed: WatchConfig = serde_yaml::from_str("paths:\n  - crates/**\n").unwrap();
        assert_eq!(parsed.debounce, "300ms");
    }
}
