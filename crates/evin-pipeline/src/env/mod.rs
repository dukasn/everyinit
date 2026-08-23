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

use evin_config::environment::EnvironmentYaml;
use evin_config::models::EnvDeclaration;
use evin_diagnostic::Spanned;
use evin_diagnostic::{Diagnostic, IssueCode};
use std::collections::HashMap;

pub trait EnvironmentProvider {
    fn get_var(&self, key: &str) -> Option<String>;
}

pub struct OsEnvironmentProvider;

impl EnvironmentProvider for OsEnvironmentProvider {
    fn get_var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// Loads and validates the global environment against the schema.
#[allow(clippy::result_large_err)]
pub fn load_global_env(
    workspace_root: &std::path::Path,
    workspace_env: &HashMap<String, String>,
    schema: Option<&EnvironmentYaml>,
) -> Result<HashMap<String, String>, Diagnostic> {
    let mut global = HashMap::new();
    let mut parsed_spans: HashMap<String, Spanned<String>> = HashMap::new();

    let env_path = workspace_root.join(".env");

    // 1. Load .env using our shiny new parser
    #[allow(clippy::collapsible_if)]
    if env_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&env_path) {
            {
                let pairs = evin_dotenv::parse(&content)?;
                for (k_spanned, v_spanned) in pairs {
                    global.insert(k_spanned.value.clone(), v_spanned.value.clone());
                    parsed_spans.insert(k_spanned.value.clone(), v_spanned);
                }
            }
        }
    }

    // 2. Merge workspace config envs
    global.extend(workspace_env.clone());

    // 3. Validate against schema
    if let Some(schema) = schema {
        for (key, def) in &schema.environments {
            if let Some(val) = global.get(key) {
                if let Some(pattern) = &def.pattern
                    && let Ok(re) = regex::Regex::new(pattern)
                    && !re.is_match(val)
                {
                    let mut diag = evin_locales::diagnostic(
                        IssueCode::new("issue-invalid-env-format")
                            .arg("key", key)
                            .arg("expected", pattern),
                    );

                    if let Some(spanned_val) = parsed_spans.get(key) {
                        diag = diag.with_file_span(
                            env_path.to_string_lossy().to_string(),
                            spanned_val.span.offset,
                            spanned_val.span.length,
                            None,
                        );
                    }
                    return Err(diag);
                }
            } else if let Some(default_val) = &def.default {
                global.insert(key.clone(), default_val.clone());
            } else if def.required.unwrap_or(false) {
                return Err(evin_locales::diagnostic(
                    IssueCode::new("issue-missing-env-var").arg("key", key),
                ));
            }
        }
    }

    Ok(global)
}

pub fn builtin_presets(name: &str) -> Option<&'static [&'static str]> {
    match name.to_lowercase().as_str() {
        "rust" => Some(&[
            "CARGO_HOME",
            "RUST_BACKTRACE",
            "RUSTFLAGS",
            "CARGO_TARGET_DIR",
        ]),
        "node" => Some(&["NODE_ENV", "NPM_CONFIG_COLOR", "PATH"]),
        "ci" => Some(&[
            "CI",
            "CONTINUOUS_INTEGRATION",
            "GITHUB_ACTIONS",
            "GIT_BRANCH",
            "GIT_COMMIT",
        ]),
        "python" => Some(&["PYTHONPATH", "PYTHONUNBUFFERED", "VIRTUAL_ENV"]),
        "aws" => Some(&["AWS_REGION", "AWS_DEFAULT_REGION", "AWS_PROFILE"]),
        "docker" => Some(&["DOCKER_HOST", "DOCKER_BUILDKIT", "DOCKER_CONFIG"]),
        _ => None,
    }
}

/// Builds a strictly isolated environment map based on the task declaration and presets.
#[allow(clippy::too_many_arguments)]
pub fn build_task_env<P: EnvironmentProvider>(
    pure: bool,
    global_env: &HashMap<String, String>,
    module_env: &HashMap<String, String>,
    task_declaration: Option<&EnvDeclaration>,
    presets: Option<&[String]>,
    provider: &P,
    schema: Option<&EnvironmentYaml>,
    stage: &str,
) -> HashMap<String, String> {
    let mut task_env = HashMap::new();

    // Tier 1: System Safelist
    if !pure {
        let default_safelist = ["PATH", "HOME", "USER", "SHELL", "TERM"];
        for key in default_safelist {
            if let Some(val) = provider.get_var(key) {
                task_env.insert(key.to_string(), val);
            }
        }
    }

    // Module env acts as an override pool
    let mut pool = global_env.clone();
    pool.extend(module_env.clone());

    // Expand presets if specified
    if let Some(preset_names) = presets {
        for preset_name in preset_names {
            // Check built-in presets
            if let Some(keys) = builtin_presets(preset_name) {
                for &k in keys {
                    if let Some(v) = pool.get(k) {
                        task_env.insert(k.to_string(), v.clone());
                    } else if let Some(v) = provider.get_var(k) {
                        task_env.insert(k.to_string(), v);
                    }
                }
            }

            // Check custom presets in schema
            if let Some(schema) = schema
                && let Some(keys) = schema.presets.get(preset_name)
            {
                for k in keys {
                    if let Some(v) = pool.get(k) {
                        task_env.insert(k.clone(), v.clone());
                    } else if let Some(v) = provider.get_var(k) {
                        task_env.insert(k.clone(), v);
                    }
                }
            }
        }
    }

    match task_declaration {
        Some(EnvDeclaration::List(keys)) => {
            for k in keys {
                if let Some(v) = pool.get(k) {
                    task_env.insert(k.clone(), v.clone());
                } else if let Some(v) = provider.get_var(k) {
                    task_env.insert(k.clone(), v);
                }
            }
        }
        Some(EnvDeclaration::Map(map)) => {
            task_env.extend(map.clone());
        }
        None => {
            if presets.is_none() {
                // Default: inherit all pool envs if neither env nor presets are specified
                task_env.extend(pool);
            }
        }
    }

    // Apply schema-based stage and scope filtering
    if let Some(schema) = schema {
        task_env.retain(|key, _| {
            if let Some(def) = schema.environments.get(key) {
                def.is_active_for_stage(stage) && def.is_exported_to_process()
            } else {
                true
            }
        });
    }

    task_env
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider(HashMap<String, String>);
    impl EnvironmentProvider for MockProvider {
        fn get_var(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
    }

    #[test]
    fn test_builtin_preset_expansion() {
        let mut mock_vars = HashMap::new();
        mock_vars.insert("CARGO_HOME".to_string(), "/home/user/.cargo".to_string());
        mock_vars.insert("RUST_BACKTRACE".to_string(), "1".to_string());
        let provider = MockProvider(mock_vars);

        let global_env = HashMap::new();
        let module_env = HashMap::new();
        let presets = vec!["rust".to_string()];

        let env = build_task_env(
            true,
            &global_env,
            &module_env,
            None,
            Some(&presets),
            &provider,
            None,
            "Build",
        );

        assert_eq!(env.get("CARGO_HOME").unwrap(), "/home/user/.cargo");
        assert_eq!(env.get("RUST_BACKTRACE").unwrap(), "1");
        assert_eq!(env.get("PATH"), None);
    }
}
