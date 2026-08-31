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

#![allow(clippy::result_large_err)]
pub mod archiver;
pub mod config;
pub mod strategies;

use archiver::{ArchiveEntry, Archiver};
use config::{ArchiveConfig, LayoutMode};
use strategies::targz::TarGzArchiver;

use evin_action_sdk::RunnerContext;
use evin_yaml::template::pipeline::resolve_templates;
use glob::Pattern;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub fn validate_config(
    with_payload: Option<&evin_yaml::SpannedValue>,
) -> Result<(), evin_yaml::ConfigError> {
    let payload = with_payload
        .cloned()
        .unwrap_or(evin_yaml::SpannedValue::Null(Default::default()));
    let config: ArchiveConfig = evin_action_sdk::parse_config(Some(&payload)).map_err(|error| {
        error.map_message(|message| evin_locales::t!("invalid-config", error = message))
    })?;
    config.validate().map_err(|d| evin_yaml::ConfigError {
        message: evin_locales::render_issue(&d.code),
        span: None,
    })
}

pub async fn execute(
    ctx: &RunnerContext,
    with_payload: Option<evin_yaml::SpannedValue>,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let mut config: ArchiveConfig = ctx.load_config(with_payload.as_ref()).map_err(|e| {
        evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("invalid-config").arg("error", e.to_string()),
        )
    })?;
    config.validate()?;

    config.path = resolve_templates(&config.path, ctx)
        .map_err(|e| evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(e.to_string())))?;
    if let Some(prefix) = &config.prefix {
        config.prefix = Some(resolve_templates(prefix, ctx).map_err(|e| {
            evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(e.to_string()))
        })?);
    }
    config.include = config
        .include
        .iter()
        .map(|pattern| {
            resolve_templates(pattern, ctx).map_err(|e| {
                evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(e.to_string()))
            })
        })
        .collect::<Result<_, _>>()?;
    config.exclude = config
        .exclude
        .iter()
        .map(|pattern| {
            resolve_templates(pattern, ctx).map_err(|e| {
                evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(e.to_string()))
            })
        })
        .collect::<Result<_, _>>()?;

    // 1. Resolve Globs & Apply Mapping
    let files = resolve_and_map_files(ctx, &config)?;
    if files.is_empty() {
        if ctx.mode == evin_action_sdk::PipelineMode::DryRun && !ctx.uncertainty().is_empty() {
            ctx.mark_unverified(format!(
                "Archive inputs could not be verified: {}",
                config.include.join(", ")
            ));
            ctx.log
                .plan(evin_action_sdk::activity::ArchivePlanUnresolved {
                    path: config.path.clone(),
                });
            ctx.state.set_output("path", &config.path);
            ctx.state.set_output("size", "unknown");
            return Ok(());
        }
        return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
            "no-files-matched",
        )));
    }

    // 2. Route to Strategy
    let archiver: Box<dyn Archiver> = match config.format.as_str() {
        "tar.gz" => Box::new(TarGzArchiver {
            compression_level: config.compression_level,
        }),
        unsupported => {
            return Err(evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("unsupported-format").arg("format", unsupported),
            ));
        }
    };

    // 3. Execute
    let output_path = ctx.fs.get_safe_physical_path(&config.path).map_err(|e| {
        evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("issue-archive-safe-path-failed")
                .arg("context", "archive path")
                .arg("error", e.to_string()),
        )
    })?;

    if ctx.mode == evin_action_sdk::PipelineMode::DryRun {
        ctx.log.plan(evin_action_sdk::activity::ArchivePlan {
            files: files.len(),
            path: config.path.clone(),
        });
        ctx.state.set_output("path", &config.path);
        ctx.state.set_output("size", "0");
        return Ok(());
    }

    // Ensure the parent directory of the output file exists
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("create-output-dir-failed")
                    .arg("error", e.to_string()),
            )
        })?;
    }

    evin_action_sdk::action_event!(
        ctx.log,
        evin_action_sdk::activity::ArchiveCompressing {
            files: files.len(),
            path: config.path.clone(),
        }
    );
    let files_len = files.len();
    tokio::task::spawn_blocking(move || archiver.create_archive(&output_path, &files))
        .await
        .map_err(|e| {
            evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(format!(
                "Blocking task failed: {}",
                e
            )))
        })??;

    ctx.state.set_output("path", &config.path);
    if let Ok(metadata) =
        tokio::fs::metadata(ctx.fs.get_safe_physical_path(&config.path).unwrap()).await
    {
        ctx.state.set_output("size", &metadata.len().to_string());
        ctx.log.debug_event(evin_action_sdk::debug::ArchiveOutput {
            format: config.format.clone(),
            size_bytes: metadata.len(),
            files: files_len,
        });
    }

    Ok(())
}

fn resolve_and_map_files(
    ctx: &RunnerContext,
    config: &ArchiveConfig,
) -> Result<Vec<ArchiveEntry>, evin_diagnostic::Diagnostic> {
    let mut mapped_files: BTreeMap<PathBuf, ArchiveEntry> = BTreeMap::new();
    let prefix = config.prefix.as_deref().unwrap_or("");

    let mut exclude_patterns = Vec::new();
    for ex in &config.exclude {
        exclude_patterns.push(Pattern::new(ex).map_err(|e| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("invalid-glob").arg("error", e.to_string()),
            )
        })?);
    }

    for pattern in &config.include {
        for path in ctx.fs.matching_files(pattern).map_err(|e| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-archive-safe-path-failed")
                    .arg("context", "source")
                    .arg("error", e.to_string()),
            )
        })? {
            let rel_path = match path.strip_prefix(ctx.fs.task_cwd()) {
                Ok(p) => p,
                Err(_) => {
                    if config.layout == LayoutMode::Relative {
                        return Err(evin_locales::diagnostic(
                            evin_diagnostic::IssueCode::new("relative-path-escape")
                                .arg("path", path.display().to_string()),
                        ));
                    }
                    path.strip_prefix(ctx.fs.workspace_root()).unwrap_or(&path)
                }
            };

            if exclude_patterns.iter().any(|p| p.matches_path(rel_path)) {
                continue;
            }

            // Apply Flatten & Prefix logic
            let dest_path = if config.layout == LayoutMode::Flat {
                let file_name = path.file_name().ok_or_else(|| {
                    evin_locales::diagnostic(
                        evin_diagnostic::IssueCode::new("invalid-file")
                            .arg("path", path.display().to_string()),
                    )
                })?;
                PathBuf::from(prefix).join(file_name)
            } else {
                PathBuf::from(prefix).join(rel_path)
            };

            let safe_source = ctx
                .fs
                .get_safe_physical_path(&path.to_string_lossy())
                .map_err(|e| {
                    evin_locales::diagnostic(
                        evin_diagnostic::IssueCode::new("issue-archive-safe-path-failed")
                            .arg("context", "source")
                            .arg("error", e.to_string()),
                    )
                })?;

            if let Some(existing) = mapped_files.get(&dest_path) {
                if existing.source != safe_source {
                    return Err(evin_locales::diagnostic(
                        evin_diagnostic::IssueCode::new("path-collision")
                            .arg("destination", dest_path.display().to_string())
                            .arg("source1", existing.source.display().to_string())
                            .arg("source2", safe_source.display().to_string()),
                    ));
                }
            } else {
                mapped_files.insert(
                    dest_path.clone(),
                    ArchiveEntry {
                        source: safe_source,
                        destination: dest_path,
                    },
                );
            }
        }
    }

    Ok(mapped_files.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ArchiveAction;
    use evin_action_sdk::Action;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn spanned_value(value: serde_json::Value) -> evin_yaml::SpannedValue {
        let span = evin_yaml::Span::default();
        match value {
            serde_json::Value::Null => evin_yaml::SpannedValue::Null(span),
            serde_json::Value::Bool(value) => evin_yaml::SpannedValue::Bool(value, span),
            serde_json::Value::Number(value) => {
                if let Some(value) = value.as_i64() {
                    evin_yaml::SpannedValue::Integer(value, span)
                } else {
                    evin_yaml::SpannedValue::Float(value.as_f64().unwrap_or_default(), span)
                }
            }
            serde_json::Value::String(value) => evin_yaml::SpannedValue::String(value, span),
            serde_json::Value::Array(values) => evin_yaml::SpannedValue::Sequence(
                values.into_iter().map(spanned_value).collect(),
                span,
            ),
            serde_json::Value::Object(values) => evin_yaml::SpannedValue::Mapping(
                values
                    .into_iter()
                    .map(|(key, value)| {
                        (
                            evin_yaml::SpannedValue::String(key, span),
                            spanned_value(value),
                        )
                    })
                    .collect(),
                span,
            ),
        }
    }

    fn spanned_mapping(with: HashMap<String, serde_json::Value>) -> evin_yaml::SpannedValue {
        evin_yaml::SpannedValue::Mapping(
            with.into_iter()
                .map(|(key, value)| {
                    (
                        evin_yaml::SpannedValue::String(key, evin_yaml::Span::default()),
                        spanned_value(value),
                    )
                })
                .collect(),
            evin_yaml::Span::default(),
        )
    }

    #[tokio::test]
    async fn test_archive_creates_archive() {
        let temp_dir = TempDir::new().unwrap();
        let target_dir = temp_dir.path().join("target");
        let output_tar = temp_dir.path().join("out.tar.gz");

        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("file1.txt"), "hello").unwrap();
        std::fs::write(target_dir.join("file2.txt"), "world").unwrap();

        let mut with = HashMap::new();
        with.insert(
            "path".to_string(),
            serde_json::Value::String(output_tar.to_string_lossy().to_string()),
        );
        with.insert(
            "include".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String("target/*.txt".to_string())]),
        );
        with.insert(
            "layout".to_string(),
            serde_json::Value::String("flat".to_string()),
        );

        let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let ctx = RunnerContext::new(
            None,
            "archive_test".to_string(),
            false,
            evin_action_sdk::PipelineMode::Normal,
            log_tx,
            vec![],
            Some(temp_dir.path().to_string_lossy().to_string()),
            std::collections::HashMap::new(),
            temp_dir.path().to_path_buf(),
            std::collections::HashMap::new(),
            std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
                temp_dir.path().to_path_buf(),
                evin_action_sdk::PipelineMode::Normal,
                evin_action_sdk::Logger::new(
                    "test".to_string(),
                    tokio::sync::mpsc::unbounded_channel().0,
                    std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
                ),
            )),
            std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
        );

        let action = ArchiveAction;
        let res = action.execute(&ctx, Some(spanned_mapping(with))).await;

        assert!(res.is_ok(), "Archive execution failed: {:?}", res);
        assert!(output_tar.exists(), "Archive was not created!");
        let outputs = ctx.state.outputs.lock().unwrap();
        assert_eq!(
            outputs.get("path"),
            Some(&output_tar.to_string_lossy().to_string())
        );
        assert!(outputs.contains_key("size"));
        assert!(!outputs.contains_key("archive_path"));
        assert!(!outputs.contains_key("archive_size"));
    }

    #[tokio::test]
    async fn test_archive_collision() {
        let temp_dir = TempDir::new().unwrap();
        let dir_a = temp_dir.path().join("a");
        let dir_b = temp_dir.path().join("b");
        let output_tar = temp_dir.path().join("out.tar.gz");

        std::fs::create_dir_all(&dir_a).unwrap();
        std::fs::create_dir_all(&dir_b).unwrap();
        std::fs::write(dir_a.join("file.txt"), "hello").unwrap();
        std::fs::write(dir_b.join("file.txt"), "world").unwrap();

        let mut with = HashMap::new();
        with.insert(
            "path".to_string(),
            serde_json::Value::String(output_tar.to_string_lossy().to_string()),
        );
        with.insert(
            "include".to_string(),
            serde_json::Value::Array(vec![
                serde_json::Value::String("a/*.txt".to_string()),
                serde_json::Value::String("b/*.txt".to_string()),
            ]),
        );
        with.insert(
            "layout".to_string(),
            serde_json::Value::String("flat".to_string()),
        );

        let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let ctx = RunnerContext::new(
            None,
            "archive_test".to_string(),
            false,
            evin_action_sdk::PipelineMode::Normal,
            log_tx,
            vec![],
            Some(temp_dir.path().to_string_lossy().to_string()),
            std::collections::HashMap::new(),
            temp_dir.path().to_path_buf(),
            std::collections::HashMap::new(),
            std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
                temp_dir.path().to_path_buf(),
                evin_action_sdk::PipelineMode::Normal,
                evin_action_sdk::Logger::new(
                    "test".to_string(),
                    tokio::sync::mpsc::unbounded_channel().0,
                    std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
                ),
            )),
            std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
        );

        let action = ArchiveAction;
        let res = action.execute(&ctx, Some(spanned_mapping(with))).await;

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(
            err.to_string().contains("collision"),
            "Expected collision error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_archive_exclude_globs() {
        let temp_dir = TempDir::new().unwrap();
        let target_dir = temp_dir.path().join("target");
        let output_tar = temp_dir.path().join("out.tar.gz");

        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("file1.txt"), "hello").unwrap();
        std::fs::write(target_dir.join("file2.tmp"), "world").unwrap();

        let mut with = HashMap::new();
        with.insert(
            "path".to_string(),
            serde_json::Value::String(output_tar.to_string_lossy().to_string()),
        );
        with.insert(
            "include".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String("target/*".to_string())]),
        );
        with.insert(
            "exclude".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String("*.tmp".to_string())]),
        );
        with.insert(
            "layout".to_string(),
            serde_json::Value::String("flat".to_string()),
        );

        let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let ctx = RunnerContext::new(
            None,
            "archive_test".to_string(),
            false,
            evin_action_sdk::PipelineMode::Normal,
            log_tx,
            vec![],
            Some(temp_dir.path().to_string_lossy().to_string()),
            std::collections::HashMap::new(),
            temp_dir.path().to_path_buf(),
            std::collections::HashMap::new(),
            std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
                temp_dir.path().to_path_buf(),
                evin_action_sdk::PipelineMode::Normal,
                evin_action_sdk::Logger::new(
                    "test".to_string(),
                    tokio::sync::mpsc::unbounded_channel().0,
                    std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
                ),
            )),
            std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
        );

        let action = ArchiveAction;
        let res = action.execute(&ctx, Some(spanned_mapping(with))).await;

        assert!(res.is_ok(), "Archive execution failed: {:?}", res);
    }
    #[tokio::test]
    async fn test_archive_relative_layout_outside_cwd() {
        let temp_dir = TempDir::new().unwrap();
        let module_a = temp_dir.path().join("module_a");
        let module_b = temp_dir.path().join("module_b");
        let output_tar = temp_dir.path().join("out.tar.gz");

        std::fs::create_dir_all(&module_a).unwrap();
        std::fs::create_dir_all(&module_b).unwrap();
        std::fs::write(module_a.join("file.txt"), "hello").unwrap();
        std::fs::write(module_b.join("file.txt"), "world").unwrap();

        let mut with = HashMap::new();
        with.insert(
            "path".to_string(),
            serde_json::Value::String(output_tar.to_string_lossy().to_string()),
        );
        with.insert(
            "include".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String(
                "../module_b/*.txt".to_string(),
            )]),
        );
        with.insert(
            "layout".to_string(),
            serde_json::Value::String("relative".to_string()),
        );

        let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let ctx = RunnerContext::new(
            None,
            "archive_test".to_string(),
            false,
            evin_action_sdk::PipelineMode::Normal,
            log_tx,
            vec![],
            Some(module_a.to_string_lossy().to_string()), // CWD is module_a
            std::collections::HashMap::new(),
            temp_dir.path().to_path_buf(), // workspace_root is temp_dir
            std::collections::HashMap::new(),
            std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
                temp_dir.path().to_path_buf(),
                evin_action_sdk::PipelineMode::Normal,
                evin_action_sdk::Logger::new(
                    "test".to_string(),
                    tokio::sync::mpsc::unbounded_channel().0,
                    std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
                ),
            )),
            std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
        );

        let action = ArchiveAction;
        let res = action.execute(&ctx, Some(spanned_mapping(with))).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_archive_template_interpolation() {
        let temp_dir = TempDir::new().unwrap();
        let target_dir = temp_dir.path().join("target");
        let output_tar = temp_dir.path().join("dist/release-1.2.3.tar.gz");

        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("file1.txt"), "hello").unwrap();

        let mut with = HashMap::new();
        with.insert(
            "path".to_string(),
            serde_json::Value::String(
                temp_dir
                    .path()
                    .join("dist/release-${bump.version}.tar.gz")
                    .to_string_lossy()
                    .to_string(),
            ),
        );
        with.insert(
            "include".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String("target/*.txt".to_string())]),
        );
        with.insert(
            "layout".to_string(),
            serde_json::Value::String("flat".to_string()),
        );
        with.insert(
            "prefix".to_string(),
            serde_json::Value::String("evin-${bump.version}".to_string()),
        );

        let mut bump_outputs = HashMap::new();
        bump_outputs.insert("version".to_string(), "1.2.3".to_string());
        let mut inputs = HashMap::new();
        inputs.insert("bump".to_string(), bump_outputs);

        let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let ctx = RunnerContext::new(
            None,
            "archive_test".to_string(),
            false,
            evin_action_sdk::PipelineMode::Normal,
            log_tx,
            vec![],
            Some(temp_dir.path().to_string_lossy().to_string()),
            std::collections::HashMap::new(),
            temp_dir.path().to_path_buf(),
            inputs,
            std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
                temp_dir.path().to_path_buf(),
                evin_action_sdk::PipelineMode::Normal,
                evin_action_sdk::Logger::new(
                    "test".to_string(),
                    tokio::sync::mpsc::unbounded_channel().0,
                    std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
                ),
            )),
            std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
        );

        let action = ArchiveAction;
        let res = action.execute(&ctx, Some(spanned_mapping(with))).await;

        assert!(res.is_ok(), "Archive execution failed: {:?}", res);
        assert!(output_tar.exists(), "Interpolated Archive was not created!");
        let outputs = ctx.state.outputs.lock().unwrap();
        assert_eq!(
            outputs.get("path"),
            Some(&output_tar.to_string_lossy().to_string())
        );

        // Verify the prefix inside the tar
        use flate2::read::GzDecoder;
        use std::fs::File;
        use tar::Archive;

        let tar_gz = File::open(output_tar).unwrap();
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        let mut entries = archive.entries().unwrap();
        let first_entry = entries.next().unwrap().unwrap();
        let path = first_entry.path().unwrap().to_string_lossy().to_string();

        // Use forward slashes for cross-platform matching if needed, though this is unix default
        assert!(
            path.contains("evin-1.2.3"),
            "Prefix was not interpolated, got: {}",
            path
        );
    }

    #[tokio::test]
    async fn dry_run_missing_shell_artifact_is_unverified_not_failed() {
        let temp = TempDir::new().unwrap();
        let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let dry_run = evin_action_sdk::dry_run::DryRunState::default();
        let git = std::sync::Arc::new(evin_action_sdk::git::GitProvider::new_with_dry_run(
            temp.path().to_path_buf(),
            evin_action_sdk::PipelineMode::DryRun,
            evin_action_sdk::Logger::new(
                "package".to_string(),
                log_tx.clone(),
                std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
            ),
            dry_run.clone(),
        ));
        let ctx = RunnerContext::new_with_dry_run(
            None,
            "package".to_string(),
            false,
            evin_action_sdk::PipelineMode::DryRun,
            log_tx,
            vec![],
            None,
            HashMap::new(),
            temp.path().to_path_buf(),
            [("build".to_string(), HashMap::new())]
                .into_iter()
                .collect(),
            ["build".to_string()].into_iter().collect(),
            git,
            std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
            dry_run,
        );
        ctx.mark_unverified("Depends on skipped build");

        let with = evin_yaml::parse_yaml(
            "path: dist/app.tar.gz\ninclude: [target/release/app]\nlayout: flat",
        )
        .unwrap();
        ArchiveAction.execute(&ctx, Some(with)).await.unwrap();

        let outputs = ctx.state.outputs.lock().unwrap();
        assert_eq!(
            outputs.get("path").map(String::as_str),
            Some("dist/app.tar.gz")
        );
        assert_eq!(outputs.get("size").map(String::as_str), Some("unknown"));
        assert!(!ctx.uncertainty().is_empty());
        assert!(!temp.path().join("dist/app.tar.gz").exists());
    }
}
