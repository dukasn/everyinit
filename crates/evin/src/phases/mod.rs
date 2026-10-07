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

pub mod init;
pub mod run;
pub mod sync;

use evin_action_sdk::debug::StructuredDebugEvent;

pub async fn emit_resolving_configuration_debug_logs(emitter: &crate::engine::PipelineEmitter) {
    let loc_diag = evin_locales::bundle::get_diagnostics();
    emitter.emit(crate::events::PipelineEvent::DebugLog(
        None,
        evin_action_sdk::debug::LocaleDiagnostics {
            locale_env: loc_diag.locale_env,
            resolved_lang: loc_diag.resolved_lang,
            loaded_resources: loc_diag.loaded_resources,
            fallback_occurred: loc_diag.fallback_occurred,
        }
        .into_record(),
    ));

    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| p.canonicalize().ok().or(Some(p)))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let config_path = cwd.join(evin_config::paths::WORKSPACE_MANIFEST);
    let config_origin = if config_path.exists() {
        evin_config::paths::WORKSPACE_MANIFEST.to_string()
    } else {
        "none".to_string()
    };
    let is_writable = std::fs::metadata(&cwd)
        .map(|m| !m.permissions().readonly())
        .unwrap_or(false);
    let write_permissions = if is_writable {
        "writable".to_string()
    } else {
        "readonly".to_string()
    };

    emitter.emit(crate::events::PipelineEvent::DebugLog(
        None,
        evin_action_sdk::debug::WorkspaceContext {
            workspace_root: ".".to_string(),
            current_cwd: ".".to_string(),
            config_origin,
            write_permissions,
        }
        .into_record(),
    ));

    let branch = evin_git::state::current_branch(&cwd)
        .await
        .unwrap_or_default();
    let sha = evin_git::state::short_sha(&cwd).await.unwrap_or_default();
    let dirty = evin_git::state::is_dirty(&cwd).await.unwrap_or(false);
    let git_branch = if branch.is_empty() {
        "not_a_repo".to_string()
    } else {
        branch
    };
    let head_commit = if sha.is_empty() {
        "none".to_string()
    } else {
        sha
    };
    let working_tree = if dirty {
        "dirty".to_string()
    } else {
        "clean".to_string()
    };
    let hooks_dir = ".git/hooks".to_string();

    emitter.emit(crate::events::PipelineEvent::DebugLog(
        None,
        evin_action_sdk::debug::GitState {
            git_branch,
            head_commit,
            working_tree,
            hooks_dir,
        }
        .into_record(),
    ));

    let mut env_sources_list = Vec::new();
    if cwd.join(".env").exists() {
        env_sources_list.push(".env");
    }
    if cwd.join(".env.local").exists() {
        env_sources_list.push(".env.local");
    }
    if cwd.join(evin_config::paths::ENVIRONMENT_MANIFEST).exists() {
        env_sources_list.push(evin_config::paths::ENVIRONMENT_MANIFEST);
    }
    let env_sources = env_sources_list.into_iter().map(String::from).collect();

    let exported_vars_count = std::env::vars().count();

    emitter.emit(crate::events::PipelineEvent::DebugLog(
        None,
        evin_action_sdk::debug::EnvironmentTrace {
            env_sources,
            exported_vars_count,
            redacted_secrets_count: 0,
        }
        .into_record(),
    ));
}
