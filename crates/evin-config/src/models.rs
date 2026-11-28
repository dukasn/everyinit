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

use evin_flow::Spanned;
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
    pub with: Option<evin_flow::SpannedValue>,
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

use evin_flow::schema::{Field as EditorField, FieldRole as EditorRole, Schema as EditorSchema};

const EDITOR_STRING: EditorSchema = EditorSchema::String;
const EDITOR_STRING_LIST: EditorSchema = EditorSchema::Sequence(&EDITOR_STRING);
const EDITOR_STRING_MAP: EditorSchema = EditorSchema::Map(&EDITOR_STRING);
const EDITOR_ENV: EditorSchema = EditorSchema::Union(&[EDITOR_STRING_LIST, EDITOR_STRING_MAP]);
const EDITOR_TRIGGER: EditorSchema = EditorSchema::Object(&[EditorField::new(
    "hooks",
    EDITOR_STRING_LIST,
    "flow-schema-hooks-that-select-this-task",
)
.catalog("task.on.hooks")
.value_role(EditorRole::EnumMember)
.snippet("hooks: [${1}]")]);
const EDITOR_WATCH: EditorSchema = EditorSchema::Object(&[
    EditorField::new(
        "paths",
        EDITOR_STRING_LIST,
        "flow-schema-workspace-relative-glob-patterns-to-watch",
    )
    .required()
    .catalog("task.watch.paths")
    .snippet("paths: [${1}]"),
    EditorField::new(
        "ignore",
        EDITOR_STRING_LIST,
        "flow-schema-workspace-relative-glob-patterns-to-ignore",
    )
    .non_nullable()
    .default_value("[]")
    .catalog("task.watch.ignore")
    .snippet("ignore: [${1}]"),
    EditorField::new(
        "debounce",
        EditorSchema::String,
        "flow-schema-delay-before-the-task-graph-reruns",
    )
    .non_nullable()
    .default_value("300ms")
    .catalog("task.watch.debounce")
    .snippet("debounce: ${1:300ms}"),
]);
const EDITOR_STEP: EditorSchema = EditorSchema::Object(&[
    EditorField::new("run", EditorSchema::String, "flow-schema-esh-command-step")
        .catalog("task.steps.run")
        .snippet("run: ${1}"),
    EditorField::new(
        "uses",
        EditorSchema::String,
        "flow-schema-built-in-action-identifier",
    )
    .catalog("task.steps.uses")
    .snippet("uses: ${1}"),
    EditorField::new(
        "with",
        EditorSchema::Any,
        "flow-schema-action-specific-configuration",
    )
    .catalog("task.steps.with")
    .snippet("with:\n  ${1:field}: ${2:value}"),
]);

pub const TASK_EDITOR_SCHEMA: EditorSchema = EditorSchema::Object(&[
    EditorField::new(
        "name",
        EditorSchema::String,
        "flow-schema-optional-task-name-override",
    )
    .non_nullable()
    .default_value("empty string"),
    EditorField::new(
        "description",
        EditorSchema::String,
        "flow-schema-human-readable-task-label",
    )
    .catalog("task.description")
    .snippet("description: \"${1}\""),
    EditorField::new(
        "stage",
        EditorSchema::String,
        "flow-schema-declared-workspace-stage-for-this-task",
    )
    .catalog("task.stage")
    .value_role(EditorRole::EnumMember)
    .snippet("stage: ${1}"),
    EditorField::new(
        "needs",
        EDITOR_STRING_LIST,
        "flow-schema-task-prerequisites",
    )
    .catalog("task.needs")
    .role(EditorRole::Modifier)
    .snippet("needs: [${1}]"),
    EditorField::new(
        "on",
        EDITOR_TRIGGER,
        "flow-schema-hook-trigger-configuration",
    )
    .catalog("task.on")
    .role(EditorRole::Modifier)
    .snippet("on:\n  hooks: [${1}]"),
    EditorField::new(
        "env",
        EDITOR_ENV,
        "flow-schema-environment-selection-or-literal-task-variables",
    )
    .catalog("task.env")
    .snippet("env:\n  ${1:NAME}: ${2:value}"),
    EditorField::new(
        "presets",
        EDITOR_STRING_LIST,
        "flow-schema-environment-variable-presets-selected-by-the-task",
    )
    .catalog("task.presets")
    .value_role(EditorRole::EnumMember)
    .snippet("presets: [${1}]"),
    EditorField::new(
        "module_env",
        EDITOR_STRING_MAP,
        "flow-schema-module-provided-task-environment-overrides",
    ),
    EditorField::new(
        "values",
        EDITOR_STRING_MAP,
        "flow-schema-task-local-template-values",
    )
    .catalog("task.values")
    .snippet("values:\n  ${1:name}: ${2:value}"),
    EditorField::new(
        "module_values",
        EDITOR_STRING_MAP,
        "flow-schema-module-provided-template-values",
    ),
    EditorField::new(
        "cwd",
        EditorSchema::String,
        "flow-schema-working-directory-for-task-commands-and-actions",
    )
    .catalog("task.cwd")
    .snippet("cwd: ${1}"),
    EditorField::new(
        "pure",
        EditorSchema::Boolean,
        "flow-schema-disable-the-default-host-environment-safelist",
    )
    .catalog("task.pure")
    .snippet("pure: ${1:false}"),
    EditorField::new(
        "parallel",
        EditorSchema::Boolean,
        "flow-schema-allow-concurrent-dispatch-when-prerequisites-are-ready",
    )
    .catalog("task.parallel")
    .snippet("parallel: ${1:false}"),
    EditorField::new(
        "watch",
        EDITOR_WATCH,
        "flow-schema-watch-supervision-configuration",
    )
    .catalog("task.watch")
    .role(EditorRole::Modifier)
    .snippet("watch:\n  paths: [${1}]"),
    EditorField::new(
        "run",
        EditorSchema::String,
        "flow-schema-single-esh-command-body",
    )
    .catalog("task.run")
    .snippet("run: ${1}"),
    EditorField::new(
        "steps",
        EditorSchema::Sequence(&EDITOR_STEP),
        "flow-schema-ordered-command-and-action-steps",
    )
    .catalog("task.steps")
    .role(EditorRole::Modifier)
    .snippet("steps:\n  - run: ${1}"),
]);

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
        let outside_workspace = std::env::temp_dir()
            .join("evin-watch-outside-workspace")
            .to_string_lossy()
            .into_owned();
        assert!(
            watch(&[&outside_workspace], &[], "300ms")
                .validate()
                .is_err()
        );
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
