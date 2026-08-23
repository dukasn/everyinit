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
use evin_git::env::HookEnvironment;
use evin_git::manager::{HookManager, HookPlan};
use evin_git::types::{GitHook, HookError};
use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookSummary {
    pub hook: GitHook,
    pub name: String,
    pub task_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidHookBinding {
    pub task_name: String,
    pub hook_name: String,
}

pub fn aggregate_hooks(
    tasks: &[TaskObject],
) -> std::result::Result<Vec<HookSummary>, InvalidHookBinding> {
    let mut hooks: BTreeMap<String, HookSummary> = BTreeMap::new();

    for task in tasks {
        let Some(task_hooks) = task.on.as_ref().and_then(|trigger| trigger.hooks.as_ref()) else {
            continue;
        };
        let mut seen_in_task = HashSet::new();

        for hook_name in task_hooks {
            if !seen_in_task.insert(hook_name.as_str()) {
                continue;
            }

            let hook = GitHook::from_str(hook_name).map_err(|_| InvalidHookBinding {
                task_name: task.name.clone(),
                hook_name: hook_name.clone(),
            })?;
            let canonical_name = hook.as_str().to_string();

            hooks
                .entry(canonical_name.clone())
                .and_modify(|summary| summary.task_count += 1)
                .or_insert(HookSummary {
                    hook,
                    name: canonical_name,
                    task_count: 1,
                });
        }
    }

    Ok(hooks.into_values().collect())
}

pub trait HookPlanner {
    fn plan(
        &self,
        hook: &GitHook,
        payload: &str,
    ) -> impl std::future::Future<Output = std::result::Result<HookPlan, HookError>> + Send;
    fn apply(
        &self,
        plan: &HookPlan,
    ) -> impl std::future::Future<Output = std::result::Result<(), HookError>> + Send;
}

impl<E: HookEnvironment + Send + Sync> HookPlanner for HookManager<E> {
    fn plan(
        &self,
        hook: &GitHook,
        payload: &str,
    ) -> impl std::future::Future<Output = std::result::Result<HookPlan, HookError>> + Send {
        self.plan_hook(hook, payload)
    }

    fn apply(
        &self,
        plan: &HookPlan,
    ) -> impl std::future::Future<Output = std::result::Result<(), HookError>> + Send {
        self.apply_plan(plan)
    }
}

pub async fn plan_hooks(
    planner: &impl HookPlanner,
    hooks: &[HookSummary],
    payload: &str,
) -> std::result::Result<Vec<HookPlan>, (String, HookError)> {
    let mut plans = Vec::new();
    for summary in hooks {
        plans.push(
            planner
                .plan(&summary.hook, payload)
                .await
                .map_err(|error| (summary.name.clone(), error))?,
        );
    }

    Ok(plans)
}

pub async fn apply_hook_plans(
    planner: &impl HookPlanner,
    plans: &[HookPlan],
) -> std::result::Result<(), (String, HookError)> {
    for plan in plans {
        planner
            .apply(plan)
            .await
            .map_err(|error| (plan.hook.as_str().to_string(), error))?;
    }
    Ok(())
}
