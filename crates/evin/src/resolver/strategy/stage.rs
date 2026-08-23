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

use super::TargetStrategy;
use evin_config::models::TaskObject;
use evin_diagnostic::Diagnostic;
use evin_pipeline::PipelineContext;

pub struct StageStrategy;

impl TargetStrategy for StageStrategy {
    fn context(&self) -> PipelineContext {
        PipelineContext::Stage
    }

    fn matches_pattern(&self, target: &str, _tasks: &[TaskObject]) -> bool {
        !target.is_empty()
            && target.chars().next().unwrap().is_ascii_uppercase()
            && target.chars().skip(1).all(|c| c.is_ascii_alphanumeric())
    }

    fn resolve(&self, target: &str, tasks: &[TaskObject]) -> Result<Vec<String>, Diagnostic> {
        let tasks_in_stage: Vec<String> = tasks
            .iter()
            .filter(|t| t.stage.as_deref() == Some(target))
            .map(|t| t.name.clone())
            .collect();

        if tasks_in_stage.is_empty() {
            let mut all_stages = std::collections::HashSet::new();
            for task in tasks {
                if let Some(s) = &task.stage {
                    all_stages.insert(s.clone());
                }
            }

            let mut diag = evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-invalid-stage-target").arg("stage", target),
            );

            if let Some(suggestion) =
                super::best_fuzzy_match(target, all_stages.iter().map(|s| s.as_str()))
            {
                let styled_suggestion = evin_ui::utils::suggestion(suggestion);
                diag = diag.with_hint(evin_locales::t!(
                    "suggestion-hint",
                    suggestion = &styled_suggestion
                ));
            }

            return Err(diag);
        }
        Ok(tasks_in_stage)
    }
}
