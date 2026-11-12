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

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

fn is_secret_name(key: &str) -> bool {
    let k = key.to_uppercase();
    k.contains("SECRET")
        || k.contains("PASSWORD")
        || k.contains("PASS")
        || k.contains("TOKEN")
        || k.contains("KEY")
        || k.contains("AUTH")
        || k.contains("CREDENTIAL")
        || k.contains("PRIVATE")
}

#[derive(Default, Debug)]
pub struct SecretsProvider {
    secrets: HashMap<String, String>,
    /// Sorted by length descending to redact longer overlapping strings first.
    redaction_values: Vec<String>,
    runtime_redaction_values: std::sync::RwLock<Vec<String>>,
}

impl Clone for SecretsProvider {
    fn clone(&self) -> Self {
        Self {
            secrets: self.secrets.clone(),
            redaction_values: self.redaction_values.clone(),
            runtime_redaction_values: std::sync::RwLock::new(
                self.runtime_redaction_values
                    .read()
                    .map(|values| values.clone())
                    .unwrap_or_default(),
            ),
        }
    }
}

impl SecretsProvider {
    /// Initializes the SecretsProvider by loading system variables and parsing a `.env` file if it exists.
    pub fn new(workspace_root: &Path) -> Self {
        let mut secrets = HashMap::new();

        // 1. Load from .env file (if it exists).
        // Zero-dependency parser logic:
        let env_path = workspace_root.join(".env");
        if let Ok(content) = fs::read_to_string(&env_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                // Ignore empty lines and comments
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }

                // Parse KEY=VALUE
                if let Some(idx) = trimmed.find('=') {
                    let key = trimmed[..idx].trim().to_string();
                    let mut value = trimmed[idx + 1..].trim();

                    // Basic quote stripping (e.g., "secret" -> secret)
                    if ((value.starts_with('"') && value.ends_with('"'))
                        || (value.starts_with('\'') && value.ends_with('\'')))
                        && value.len() >= 2
                    {
                        value = &value[1..value.len() - 1];
                    }

                    if !key.is_empty() && !value.is_empty() {
                        secrets.insert(key, value.to_string());
                    }
                }
            }
        }

        // 2. Load system environment variables matching secret key patterns
        for (k, v) in env::vars() {
            if !k.is_empty() && !v.is_empty() && is_secret_name(&k) {
                secrets.insert(k, v);
            }
        }

        let mut provider = Self {
            secrets,
            redaction_values: Vec::new(),
            runtime_redaction_values: std::sync::RwLock::new(Vec::new()),
        };
        provider.prepare_redactor();
        provider
    }

    /// Register an explicit secret key/value pair for redaction.
    pub fn register_secret(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let val_str = value.into();
        if !val_str.is_empty() {
            self.secrets.insert(key.into(), val_str);
            self.prepare_redactor();
        }
    }

    /// Retrieve a secret securely
    pub fn get(&self, key: &str) -> Option<&String> {
        self.secrets.get(key)
    }

    /// Update internal list of redactable values.
    fn prepare_redactor(&mut self) {
        let mut values: Vec<String> = self
            .secrets
            .values()
            .filter(|v| v.len() >= 4) // Only redact secrets >= 4 characters
            .cloned()
            .collect();

        // Sort by length descending, so we replace larger overlapping tokens first
        values.sort_by_key(|b| std::cmp::Reverse(b.len()));
        self.redaction_values = values;
    }

    /// Redact any known secret within the given text, replacing it with `***`.
    /// Adds a state output to the redactor when its key looks credential-like.
    pub fn register_output_value_for_redaction(&self, key: &str, value: &str) {
        if !is_secret_name(key) || value.len() < 4 {
            return;
        }
        if let Ok(mut values) = self.runtime_redaction_values.write()
            && !values.iter().any(|existing| existing == value)
        {
            values.push(value.to_string());
            values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        }
    }

    pub fn redact(&self, text: &str) -> String {
        self.redact_with_count(text).0
    }

    /// Redacts known values and returns the number of replacements performed.
    pub fn redact_with_count(&self, text: &str) -> (String, usize) {
        let mut values = self.redaction_values.clone();
        if let Ok(runtime_values) = self.runtime_redaction_values.read() {
            values.extend(runtime_values.iter().cloned());
        }
        values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        values.dedup();
        let mut sanitized = text.to_string();
        let mut redaction_count = 0;
        for secret in values {
            if !secret.is_empty() && sanitized.contains(&secret) {
                redaction_count += sanitized.match_indices(&secret).count();
                sanitized = sanitized.replace(&secret, "***");
            }
        }
        (sanitized, redaction_count)
    }

    #[cfg(test)]
    pub(crate) fn runtime_redaction_values(&self) -> Vec<String> {
        self.runtime_redaction_values
            .read()
            .map(|values| values.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::SecretsProvider;

    #[test]
    fn redaction_counts_each_replacement_and_keeps_runtime_values_isolated_on_clone() {
        let mut original = SecretsProvider::default();
        original.register_secret("API_TOKEN", "base-secret-value");
        let task = original.clone();
        task.register_output_value_for_redaction(
            "DERIVED_TOKEN",
            "prefix-base-secret-value-suffix",
        );

        let (redacted, count) = task.redact_with_count(
            "prefix-base-secret-value-suffix base-secret-value prefix-base-secret-value-suffix",
        );
        assert_eq!(redacted, "*** *** ***");
        assert_eq!(count, 3);
        assert!(original.runtime_redaction_values().is_empty());
        assert_eq!(
            task.runtime_redaction_values(),
            ["prefix-base-secret-value-suffix"]
        );
    }
}
