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

use evin_config::models::TaskObject;
use std::collections::BTreeMap;

pub async fn sync_actions(
    tasks: &[TaskObject],
    actions: &std::collections::HashMap<String, std::sync::Arc<dyn evin_action_sdk::Action>>,
    pipeline_mode: evin_action_sdk::PipelineMode,
) -> std::result::Result<Vec<evin_action_sdk::SyncResource>, evin_diagnostic::Diagnostic> {
    let mut configurations: BTreeMap<String, Vec<Option<evin_flow::SpannedValue>>> =
        BTreeMap::new();

    for task in tasks {
        for step in task.steps.as_deref().unwrap_or_default() {
            let Some(action_id_spanned) = &step.uses else {
                continue;
            };
            configurations
                .entry(action_id_spanned.as_str().to_string())
                .or_default()
                .push(step.with.clone());
        }
    }

    let context = evin_action_sdk::ActionSyncContext {
        workspace_root: std::env::current_dir().unwrap_or_else(|_| ".".into()),
        mode: pipeline_mode,
    };
    let mut resources = Vec::new();
    for (action_id, configs) in configurations {
        let action = actions
            .get(&action_id)
            .expect("registered action was checked while grouping");
        resources.extend(action.sync(&context, &configs).await.map_err(|error| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-sync-execution-failed")
                    .arg("error", error.to_string()),
            )
        })?);
    }
    Ok(resources)
}
