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

use evin_action_sdk::Action;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::types::{PipelineConfig, PipelineExecutionState};

pub struct PipelineEngine {
    pub config: PipelineConfig,
    pub state: PipelineExecutionState,
}

impl Default for PipelineEngine {
    fn default() -> Self {
        Self::new(
            false,
            evin_action_sdk::PipelineMode::Normal,
            HashMap::new(),
            vec![],
            None,
        )
    }
}

impl PipelineEngine {
    pub fn new(
        is_ci_mode: bool,
        mode: evin_action_sdk::PipelineMode,
        workspace_env: HashMap<String, String>,
        run_payload: Vec<String>,
        workspace_name: Option<String>,
    ) -> Self {
        let config = PipelineConfig {
            is_ci_mode,
            mode,
            global_env: Arc::new(HashMap::new()),
            workspace_env,
            env_schema: None,
            workspace_values: HashMap::new(),
            workspace_name,
            run_payload,
            invocation_flags: HashMap::new(),
            gitrules: None,
            forced_execution: None,
            workspace_execution: None,
        };

        let state = PipelineExecutionState {
            actions: Arc::new(HashMap::new()),
            global_state: Arc::new(Mutex::new(HashMap::new())),
            task_outcomes: Arc::new(Mutex::new(HashMap::new())),
            dry_run: evin_action_sdk::fs::DryRunState::default(),
        };

        Self { config, state }
    }

    pub fn register_action(&mut self, name: &str, action: Box<dyn Action>) {
        if let Some(map) = Arc::get_mut(&mut self.state.actions) {
            map.insert(name.to_string(), action.into());
        }
    }

    pub fn register_action_arc(&mut self, name: String, action: Arc<dyn Action>) {
        if let Some(map) = Arc::get_mut(&mut self.state.actions) {
            map.insert(name, action);
        }
    }
}
