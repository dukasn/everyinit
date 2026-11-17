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

use crate::engine::{CliContext, Phase, PipelineEmitter};
use crate::events::PipelineEvent;
use async_trait::async_trait;
use evin_action_sdk::debug::StructuredDebugEvent;
use evin_diagnostic::Diagnostic;

pub struct InitWorkspacePhase {
    pub workspace: bool,
    pub policy: bool,
    pub env: bool,
    pub all: bool,
    pub module: Option<String>,
    pub target_path: Option<std::path::PathBuf>,
}

fn write_config_file(
    file_path: &std::path::Path,
    content: &str,
    cwd: &std::path::Path,
    verbose: bool,
    emitter: &PipelineEmitter,
) -> Result<(), Diagnostic> {
    let clean_target = evin_io::clean_path(file_path);
    let target_display = clean_target
        .strip_prefix(cwd)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| clean_target.display().to_string());

    if clean_target.exists() {
        emitter.emit(PipelineEvent::DebugLog(
            None,
            evin_action_sdk::debug::FileGenerated {
                target: target_display,
                action: "skipped_exists".into(),
            }
            .into_record(),
        ));
    } else {
        std::fs::write(&clean_target, content).map_err(|e| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-init-file-write-failed")
                    .arg("path", clean_target.display().to_string())
                    .arg("error", e.to_string()),
            )
        })?;
        emitter.emit(PipelineEvent::DebugLog(
            None,
            evin_action_sdk::debug::FileGenerated {
                target: target_display.clone(),
                action: "created".into(),
            }
            .into_record(),
        ));
        if verbose {
            let lines: Vec<String> = content.lines().map(String::from).collect();
            let title = format!(
                "{}{}",
                evin_ui::render::SYMBOL_FENCED_FILE_PREFIX,
                target_display
            );
            emitter.emit(PipelineEvent::FencedOutput {
                title,
                lines,
                footer_label: Some("created".into()),
            });
        }
    }
    Ok(())
}

pub struct ResolveInitPhase;

#[async_trait]
impl Phase for ResolveInitPhase {
    fn locale_key(&self) -> String {
        "ui.phase.resolving_configuration".into()
    }

    async fn execute(
        &self,
        ctx: &mut CliContext,
        emitter: &PipelineEmitter,
    ) -> Result<(), Diagnostic> {
        if ctx.debug {
            super::emit_resolving_configuration_debug_logs(emitter).await;
        }
        Ok(())
    }
}

#[async_trait]
impl Phase for InitWorkspacePhase {
    fn locale_key(&self) -> String {
        "ui.phase.initialize_workspace".into()
    }

    async fn execute(
        &self,
        ctx: &mut CliContext,
        emitter: &PipelineEmitter,
    ) -> Result<(), Diagnostic> {
        let mut do_workspace = self.workspace;
        let mut do_policy = self.policy;
        let mut do_env = self.env;
        let do_module = self.module.clone();

        if self.all {
            do_workspace = true;
            do_policy = true;
            do_env = true;
        } else if !do_workspace && !do_policy && !do_env && do_module.is_none() {
            do_workspace = true;
        }

        let cwd = match std::env::current_dir() {
            Ok(p) => p,
            Err(e) => {
                return Err(evin_locales::diagnostic(
                    evin_diagnostic::IssueCode::new("issue-init-directory-create-failed")
                        .arg("path", ".")
                        .arg("error", format!("Current working directory invalid: {}", e)),
                ));
            }
        };
        let raw_target = match &self.target_path {
            Some(p) if p.is_absolute() => p.clone(),
            Some(p) => cwd.join(p),
            None => cwd.clone(),
        };
        let mut target_dir = evin_io::clean_path(&raw_target);
        if target_dir.is_relative() {
            target_dir = evin_io::clean_path(&cwd.join(target_dir));
        }

        if !target_dir.exists() {
            std::fs::create_dir_all(&target_dir).map_err(|e| {
                evin_locales::diagnostic(
                    evin_diagnostic::IssueCode::new("issue-init-directory-create-failed")
                        .arg("path", target_dir.display().to_string())
                        .arg("error", e.to_string()),
                )
            })?;
        }
        let evin_dir = evin_io::clean_path(&target_dir.join(".evin"));

        let is_writable = if !evin_dir.exists() {
            std::fs::create_dir_all(&evin_dir).map_err(|e| {
                evin_locales::diagnostic(
                    evin_diagnostic::IssueCode::new("issue-init-directory-create-failed")
                        .arg("path", evin_dir.display().to_string())
                        .arg("error", e.to_string()),
                )
            })?;
            true
        } else {
            std::fs::metadata(&evin_dir)
                .map(|m| !m.permissions().readonly())
                .unwrap_or(false)
        };

        let target_dir_display = evin_dir
            .strip_prefix(&cwd)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| evin_dir.display().to_string());

        emitter.emit(PipelineEvent::DebugLog(
            None,
            evin_action_sdk::debug::DirectoryChecked {
                target: target_dir_display,
                is_writable,
            }
            .into_record(),
        ));

        if do_workspace {
            let workspace_content = evin_locales::bundle::get("init-template-workspace");
            write_config_file(
                &evin_dir.join(evin_config::paths::WORKSPACE_FILE),
                &workspace_content,
                &cwd,
                ctx.verbose,
                emitter,
            )?;
        }

        if do_policy {
            write_config_file(
                &evin_dir.join(evin_config::paths::POLICY_FILE),
                include_str!("../../../../templates/policy.json"),
                &cwd,
                ctx.verbose,
                emitter,
            )?;
        }

        if do_env {
            write_config_file(
                &evin_dir.join(evin_config::paths::ENVIRONMENT_FILE),
                include_str!("../../../../templates/environment.json"),
                &cwd,
                ctx.verbose,
                emitter,
            )?;
        }

        if let Some(module_path) = do_module {
            let module_dir = evin_io::clean_path(&target_dir.join(&module_path).join(".evin"));
            if !module_dir.exists() {
                let _ = std::fs::create_dir_all(&module_dir);
            }
            let module_content = evin_locales::bundle::get("init-template-module");
            write_config_file(
                &module_dir.join(evin_config::paths::MODULE_FILE),
                &module_content,
                &cwd,
                ctx.verbose,
                emitter,
            )?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_init_default_only_workspace() {
        let dir = tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let emitter = PipelineEmitter::new(tx);
        let mut ctx = CliContext::default();

        let phase = InitWorkspacePhase {
            workspace: false,
            policy: false,
            env: false,
            all: false,
            module: None,
            target_path: Some(dir.path().to_path_buf()),
        };

        phase.execute(&mut ctx, &emitter).await.unwrap();

        assert!(dir.path().join(".evin/workspace.flow").exists());
        assert!(
            std::fs::read_to_string(dir.path().join(".evin/workspace.flow"))
                .unwrap()
                .starts_with("#!evin/v1\n")
        );
        assert!(!dir.path().join(".evin/policy.json").exists());
        assert!(!dir.path().join(".evin/environment.json").exists());
    }

    #[tokio::test]
    async fn test_init_all_flags() {
        let dir = tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let emitter = PipelineEmitter::new(tx);
        let mut ctx = CliContext::default();

        let phase = InitWorkspacePhase {
            workspace: false,
            policy: false,
            env: false,
            all: true,
            module: None,
            target_path: Some(dir.path().to_path_buf()),
        };

        phase.execute(&mut ctx, &emitter).await.unwrap();

        assert!(dir.path().join(".evin/workspace.flow").exists());
        assert!(dir.path().join(".evin/policy.json").exists());
        assert!(dir.path().join(".evin/environment.json").exists());
        assert!(
            std::fs::read_to_string(dir.path().join(".evin/workspace.flow"))
                .unwrap()
                .starts_with("#!evin/v1\n")
        );
        for (name, schema) in [
            ("policy.json", "https://dl.everyinit.dev/schema/v1/policy"),
            (
                "environment.json",
                "https://dl.everyinit.dev/schema/v1/environment",
            ),
        ] {
            let content = std::fs::read_to_string(dir.path().join(".evin").join(name)).unwrap();
            let value: serde_json::Value = serde_json::from_str(&content).unwrap();
            assert_eq!(value["$schema"], schema);
            if name == "policy.json" {
                evin_repository_policy::parse_str(&content, "json", None).unwrap();
            } else {
                evin_config::environment::parse_environment_json(&content).unwrap();
            }
        }
    }

    #[tokio::test]
    async fn test_init_selective_flags() {
        let dir = tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let emitter = PipelineEmitter::new(tx);
        let mut ctx = CliContext::default();

        let phase = InitWorkspacePhase {
            workspace: false,
            policy: true,
            env: true,
            all: false,
            module: None,
            target_path: Some(dir.path().to_path_buf()),
        };

        phase.execute(&mut ctx, &emitter).await.unwrap();

        assert!(!dir.path().join(".evin/workspace.flow").exists());
        assert!(dir.path().join(".evin/policy.json").exists());
        assert!(dir.path().join(".evin/environment.json").exists());
        for (name, schema) in [
            ("policy.json", "https://dl.everyinit.dev/schema/v1/policy"),
            (
                "environment.json",
                "https://dl.everyinit.dev/schema/v1/environment",
            ),
        ] {
            let content = std::fs::read_to_string(dir.path().join(".evin").join(name)).unwrap();
            let value: serde_json::Value = serde_json::from_str(&content).unwrap();
            assert_eq!(value["$schema"], schema);
            if name == "policy.json" {
                evin_repository_policy::parse_str(&content, "json", None).unwrap();
            } else {
                evin_config::environment::parse_environment_json(&content).unwrap();
            }
        }
    }

    #[tokio::test]
    async fn test_init_module_emits_an_evin_header() {
        let dir = tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let emitter = PipelineEmitter::new(tx);
        let mut ctx = CliContext::default();
        let phase = InitWorkspacePhase {
            workspace: false,
            policy: false,
            env: false,
            all: false,
            module: Some("crates/example".into()),
            target_path: Some(dir.path().to_path_buf()),
        };

        phase.execute(&mut ctx, &emitter).await.unwrap();
        assert!(
            std::fs::read_to_string(dir.path().join("crates/example/.evin/module.flow"))
                .unwrap()
                .starts_with("#!evin/v1\n")
        );
    }
}
