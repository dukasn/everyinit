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

use evin_config::models::EnvDeclaration;
use evin_pipeline::env::{EnvironmentProvider, build_task_env};
use serde_yaml::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn flow(path: &str) -> Value {
    serde_yaml::from_str(&fs::read_to_string(root().join(path)).unwrap()).unwrap()
}

struct ProvidedEnvironment;

impl EnvironmentProvider for ProvidedEnvironment {
    fn get_var(&self, key: &str) -> Option<String> {
        Some(format!("provided-{key}"))
    }
}

fn assert_forwarded(path: &str, task_name: &str, keys: &[&str]) {
    let document = flow(path);
    let task = &document["tasks"][task_name];
    let declaration: EnvDeclaration = serde_yaml::from_value(task["env"].clone()).unwrap();
    let schema = serde_yaml::from_value(flow(".evin/environment.flow")).unwrap();
    let environment = build_task_env(
        false,
        &HashMap::new(),
        &HashMap::new(),
        Some(&declaration),
        None,
        &ProvidedEnvironment,
        Some(&schema),
        task["stage"].as_str().unwrap(),
    );
    for key in keys {
        assert_eq!(
            environment.get(*key),
            Some(&format!("provided-{key}")),
            "{path}: {task_name} must forward {key} to its scripts"
        );
    }
    assert!(!environment.contains_key("UNRELATED_SECRET"));
}

#[test]
fn github_publish_scripts_receive_their_repository_credentials_and_context() {
    for task in ["github-publish", "github-publish-prebuilt"] {
        assert_forwarded(
            ".evin/release.flow",
            task,
            &[
                "GITHUB_REPOSITORY",
                "GITHUB_TOKEN",
                "GITHUB_API_URL",
                "GITHUB_STEP_SUMMARY",
                "RELEASE_TAG",
            ],
        );
    }
}

#[test]
fn mirror_metadata_scripts_receive_source_identity_and_url_overrides() {
    for task in ["metadata", "metadata-prebuilt"] {
        assert_forwarded(
            "infra/.evin/mirror.flow",
            task,
            &[
                "RELEASE_CHANNEL",
                "RELEASE_COMMIT",
                "RELEASE_BASE_URL",
                "RELEASE_TARGET",
                "RELEASE_VERSION",
                "RELEASE_SIGNING_KEY_FILE",
            ],
        );
    }
}

#[test]
fn repository_run_commands_do_not_use_action_output_or_environment_templates() {
    for directory in [
        ".evin",
        "crates/evin/.evin",
        "crates/esh/.evin",
        "docs/.evin",
        "infra/.evin",
    ] {
        for entry in fs::read_dir(root().join(directory)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|extension| extension != "flow") {
                continue;
            }
            let document: Value =
                serde_yaml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            let Some(tasks) = document["tasks"].as_mapping() else {
                continue;
            };
            for (name, task) in tasks {
                let mut commands = Vec::new();
                if let Some(command) = task["run"].as_str() {
                    commands.push(command);
                }
                if let Some(steps) = task["steps"].as_sequence() {
                    commands.extend(steps.iter().filter_map(|step| step["run"].as_str()));
                }
                for command in commands {
                    for tail in command.split("${").skip(1) {
                        let Some((expression, _)) = tail.split_once('}') else {
                            continue;
                        };
                        // Shell default/substring operators are valid; action references
                        // use env.NAME or task:namespace.output with no shell operator.
                        let action_reference = expression.starts_with("env.")
                            || expression.split_once('.').is_some_and(|(task, output)| {
                                !task.is_empty()
                                    && !output.is_empty()
                                    && task
                                        .chars()
                                        .all(|c| c.is_ascii_alphanumeric() || "_:-".contains(c))
                                    && output
                                        .chars()
                                        .all(|c| c.is_ascii_alphanumeric() || c == '_')
                                    && !task.contains(":-")
                                    && !task.contains(":+")
                            });
                        assert!(
                            !action_reference,
                            "{}: task {} uses action template ${{{expression}}} in a shell command",
                            path.display(),
                            name.as_str().unwrap()
                        );
                    }
                }
            }
        }
    }
}
