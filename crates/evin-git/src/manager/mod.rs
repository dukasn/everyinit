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

use crate::env::HookEnvironment;
use crate::types::{GitHook, HookError};

const START_MARKER: &str = "# --- EVIN Start ---";
const END_MARKER: &str = "# --- EVIN End ---";

pub struct HookManager<E: HookEnvironment> {
    pub env: E,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookChange {
    Create,
    Update,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookPlan {
    pub hook: GitHook,
    pub content: String,
    pub change: HookChange,
}

impl<E: HookEnvironment> HookManager<E> {
    pub fn new(env: E) -> Self {
        Self { env }
    }

    pub async fn install_hook(&self, hook: &GitHook, payload: &str) -> Result<(), HookError> {
        let plan = self.plan_hook(hook, payload).await?;
        self.apply_plan(&plan).await?;
        Ok(())
    }

    pub async fn plan_hook(&self, hook: &GitHook, payload: &str) -> Result<HookPlan, HookError> {
        let existing_content = self.env.read_hook(hook).await?;
        let content = self.render_hook(existing_content.as_deref(), payload);
        let change = match existing_content {
            None => HookChange::Create,
            Some(existing) if existing == content && self.env.is_executable(hook).await? => {
                HookChange::Unchanged
            }
            Some(_) => HookChange::Update,
        };

        Ok(HookPlan {
            hook: hook.clone(),
            content,
            change,
        })
    }

    pub async fn apply_plan(&self, plan: &HookPlan) -> Result<(), HookError> {
        if plan.change != HookChange::Unchanged {
            self.env.write_hook(&plan.hook, &plan.content).await?;
            self.env.make_executable(&plan.hook).await?;
        }
        Ok(())
    }

    pub async fn prune_hook(&self, hook: &GitHook) -> Result<(), HookError> {
        if let Some(content) = self.env.read_hook(hook).await? {
            let new_content = self.prune_content(&content);
            self.env.write_hook(hook, &new_content).await?;
        }
        Ok(())
    }

    fn prune_content(&self, content: &str) -> String {
        let mut new_lines = Vec::new();
        let mut in_block = false;

        for line in content.lines() {
            if line == START_MARKER {
                in_block = true;
                continue;
            }
            if line == END_MARKER {
                in_block = false;
                continue;
            }

            if !in_block {
                new_lines.push(line);
            }
        }

        let mut out = new_lines.join("\n");
        if !new_lines.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }

    fn render_hook(&self, existing_content: Option<&str>, payload: &str) -> String {
        let formatted_payload = format!("\n{}\n{}\n{}\n", START_MARKER, payload, END_MARKER);
        match existing_content {
            Some(content) if content.contains(START_MARKER) => {
                self.prune_content(content) + &formatted_payload
            }
            Some(content) => {
                let mut content = content.to_string();
                if !content.ends_with('\n') {
                    content.push('\n');
                }
                content + &formatted_payload
            }
            None => format!("#!/usr/bin/env bash\n{}", formatted_payload),
        }
    }
}
