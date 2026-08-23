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

use crate::cache::PlanCache;
use crate::error::EshError;
use crate::planner::ir::ExecutionPlan;
use crate::runtime::context::ExecutionContext;
use crate::runtime::traits::{BoxFuture, Executable};
use std::path::Path;
use std::sync::Arc;

pub struct Engine {
    cache: PlanCache,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Self {
            cache: PlanCache::new(),
        }
    }

    #[inline(always)]
    pub fn compile(&self, script: &str) -> Result<Arc<ExecutionPlan>, EshError> {
        self.cache.get_or_compile(script)
    }

    pub fn eval_plan<'a>(
        &'a self,
        plan: &'a ExecutionPlan,
        ctx: &'a mut ExecutionContext,
    ) -> BoxFuture<'a, Result<i32, EshError>> {
        Box::pin(async move {
            let res = plan.execute(ctx).await?;
            Ok(res)
        })
    }

    pub fn eval_str<'a>(
        &'a self,
        script: &'a str,
        ctx: &'a mut ExecutionContext,
    ) -> BoxFuture<'a, Result<i32, EshError>> {
        Box::pin(async move {
            let plan = self.compile(script)?;
            let res = plan.execute(ctx).await?;
            Ok(res)
        })
    }

    pub fn eval_file<'a>(
        &'a self,
        path: &'a Path,
        ctx: &'a mut ExecutionContext,
    ) -> BoxFuture<'a, Result<i32, EshError>> {
        Box::pin(async move {
            let content = std::fs::read_to_string(path).map_err(|e| {
                crate::error::ExecError::NotFound(format!(
                    "Cannot read script file '{}': {}",
                    path.display(),
                    e
                ))
            })?;
            let plan = self.compile(&content)?;
            let res = plan.execute(ctx).await?;
            Ok(res)
        })
    }

    pub fn cache(&self) -> &PlanCache {
        &self.cache
    }
}
