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

use crate::runner::{LogMessage, ShellRunner};
use crate::telemetry::TraceEvent;
use evin_action_sdk::{Action, TaskOutcome};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

pub struct TaskResult {
    pub task_name: String,
    pub result: Result<(), evin_diagnostic::IssueCode>,
    pub outcome: TaskOutcome,
    pub start_ts: u64,
    pub dur_us: u64,
    pub thread_id: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PipelineReport {
    pub planned: usize,
    pub unverified: usize,
    pub failed: usize,
    pub uncertainty: BTreeMap<String, Vec<String>>,
    pub telemetry: Vec<TraceEvent>,
    pub error: Option<evin_diagnostic::Diagnostic>,
    pub cancelled: bool,
}

pub struct EngineContext {
    pub log_tx: mpsc::UnboundedSender<LogMessage>,
    pub state_outputs: Arc<Mutex<HashMap<String, HashMap<String, String>>>>,
    pub task_outcomes: Arc<Mutex<HashMap<String, TaskOutcome>>>,
    pub runner: Arc<ShellRunner>,
    pub workspace_root: std::path::PathBuf,
    pub secrets: Arc<evin_action_sdk::secrets::SecretsProvider>,
    pub cancel_token: tokio_util::sync::CancellationToken,
}

pub struct PipelineConfig {
    pub is_ci_mode: bool,
    pub mode: evin_action_sdk::PipelineMode,
    pub workspace_env: HashMap<String, String>,
    pub global_env: Arc<HashMap<String, String>>,
    pub env_schema: Option<Arc<evin_config::environment::EnvironmentYaml>>,
    pub workspace_values: HashMap<String, String>,
    pub workspace_name: Option<String>,
    pub run_payload: Vec<String>,
    pub invocation_flags: HashMap<String, evin_action_sdk::InvocationFlags>,
    pub gitrules: Option<Arc<evin_gitrules::GitRulesRoot>>,
    pub forced_execution: Option<evin_config::workspace::ExecutionMode>,
    pub workspace_execution: Option<evin_config::workspace::ExecutionMode>,
}

pub struct PipelineExecutionState {
    pub actions: Arc<HashMap<String, Arc<dyn Action>>>,
    pub global_state: Arc<Mutex<HashMap<String, HashMap<String, String>>>>,
    pub task_outcomes: Arc<Mutex<HashMap<String, TaskOutcome>>>,
    pub dry_run: evin_action_sdk::fs::DryRunState,
}
