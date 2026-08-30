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

pub struct TaskStrategy;

impl TargetStrategy for TaskStrategy {
    fn context(&self) -> PipelineContext {
        PipelineContext::Task
    }

    fn matches_pattern(&self, target: &str, _tasks: &[TaskObject]) -> bool {
        !target.is_empty()
            && target
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == ':')
    }

    fn resolve(&self, target: &str, tasks: &[TaskObject]) -> Result<Vec<String>, Diagnostic> {
        if tasks.iter().any(|t| t.name == target) {
            Ok(vec![target.to_string()])
        } else {
            let best_match = super::best_fuzzy_match(target, tasks.iter().map(|t| t.name.as_str()));

            let mut diag = evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-invalid-target").arg("target", target),
            );

            if let Some(suggestion) = best_match {
                let styled_suggestion = evin_ui::utils::suggestion(suggestion);
                diag = diag.with_hint(evin_locales::t!(
                    "suggestion-hint",
                    suggestion = &styled_suggestion
                ));
            }
            Err(diag)
        }
    }
}
