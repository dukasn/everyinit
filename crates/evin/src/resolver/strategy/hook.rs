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
use std::collections::HashSet;

pub struct HookStrategy;

impl TargetStrategy for HookStrategy {
    fn context(&self) -> PipelineContext {
        PipelineContext::Hook
    }

    fn matches_pattern(&self, target: &str, tasks: &[TaskObject]) -> bool {
        let is_standard = evin_policy::KNOWN_HOOKS.contains(&target);
        if is_standard {
            return true;
        }

        tasks.iter().any(|task| {
            if let Some(on) = &task.on
                && let Some(hooks) = &on.hooks
            {
                return hooks.contains(&target.to_string());
            }
            false
        })
    }

    fn resolve(&self, target: &str, tasks: &[TaskObject]) -> Result<Vec<String>, Diagnostic> {
        let mut target_names = Vec::new();

        for task in tasks {
            if let Some(on) = &task.on
                && let Some(hooks) = &on.hooks
                && hooks.contains(&target.to_string())
            {
                target_names.push(task.name.clone());
            }
        }

        if target_names.is_empty() {
            let is_standard = evin_policy::KNOWN_HOOKS.contains(&target);

            let mut all_hooks = HashSet::new();
            for task in tasks {
                if let Some(on) = &task.on
                    && let Some(hooks) = &on.hooks
                {
                    for h in hooks {
                        all_hooks.insert(h.clone());
                    }
                }
            }

            if is_standard || all_hooks.contains(target) {
                return Ok(target_names);
            }

            let mut diag = evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-invalid-hook").arg("hook", target),
            );

            if let Some(suggestion) =
                super::best_fuzzy_match(target, all_hooks.iter().map(|s| s.as_str()))
            {
                let styled_suggestion = evin_ui::utils::suggestion(suggestion);
                diag = diag.with_hint(evin_locales::t!(
                    "suggestion-hint",
                    suggestion = &styled_suggestion
                ));
            }

            return Err(diag);
        }

        Ok(target_names)
    }
}
