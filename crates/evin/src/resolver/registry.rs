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

use super::strategy::{HookStrategy, StageStrategy, TargetStrategy, TaskStrategy};
use evin_config::models::TaskObject;
use evin_diagnostic::Diagnostic;
use evin_pipeline::PipelineContext;
use std::collections::HashMap;

pub struct TargetRegistry {
    lut: HashMap<String, (PipelineContext, Vec<String>)>,
    tasks: Vec<TaskObject>,
}

impl TargetRegistry {
    pub fn build(tasks: Vec<TaskObject>) -> Self {
        let mut lut = HashMap::new();
        let strategies: Vec<Box<dyn TargetStrategy>> = vec![
            Box::new(HookStrategy),
            Box::new(StageStrategy),
            Box::new(TaskStrategy),
        ];

        // Pre-compute the LUT for all standard tasks, stages, and hooks
        for strategy in &strategies {
            let context = strategy.context();
            match context {
                PipelineContext::Stage => {
                    let mut unique_stages = std::collections::HashSet::new();
                    for t in &tasks {
                        if let Some(s) = &t.stage {
                            unique_stages.insert(s.clone());
                        }
                    }
                    for stage in unique_stages {
                        if let Ok(resolved) = strategy.resolve(&stage, &tasks) {
                            lut.insert(stage, (context, resolved));
                        }
                    }
                }
                PipelineContext::Hook => {
                    let mut unique_hooks = std::collections::HashSet::new();
                    for hook in evin_policy::KNOWN_HOOKS {
                        unique_hooks.insert(hook.to_string());
                    }
                    for t in &tasks {
                        if let Some(on) = &t.on
                            && let Some(hooks) = &on.hooks
                        {
                            for h in hooks {
                                unique_hooks.insert(h.clone());
                            }
                        }
                    }
                    for hook in unique_hooks {
                        if let Ok(resolved) = strategy.resolve(&hook, &tasks) {
                            lut.insert(hook, (context, resolved));
                        }
                    }
                }
                PipelineContext::Task => {
                    for t in &tasks {
                        if let Ok(resolved) = strategy.resolve(&t.name, &tasks) {
                            lut.insert(t.name.clone(), (context, resolved));
                        }
                    }
                }
                _ => {}
            }
        }

        Self { lut, tasks }
    }

    pub fn resolve(&self, target: &str) -> Result<(PipelineContext, Vec<String>), Diagnostic> {
        // O(1) fast path LUT lookup
        if let Some(resolved) = self.lut.get(target) {
            return Ok(resolved.clone());
        }

        // If not found in LUT, we run strategies to see if it matches any pattern strictly or we fallback to fuzzy matching
        let strategies: Vec<Box<dyn TargetStrategy>> = vec![
            Box::new(HookStrategy),
            Box::new(StageStrategy),
            Box::new(TaskStrategy),
        ];

        for strategy in strategies {
            if strategy.matches_pattern(target, &self.tasks) {
                let context = strategy.context();
                let targets = strategy.resolve(target, &self.tasks)?;
                return Ok((context, targets));
            }
        }

        // If no strict pattern matched, fallback to Task fuzzy matching to give a suggestion
        let task_strategy = TaskStrategy;
        let targets = task_strategy.resolve(target, &self.tasks)?;
        Ok((task_strategy.context(), targets))
    }
}
