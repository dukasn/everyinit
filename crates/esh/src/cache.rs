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

use crate::error::EshError;
use crate::planner::compiler::compile_script;
use crate::planner::ir::ExecutionPlan;
use parking_lot::RwLock;
use rustc_hash::FxBuildHasher;
use std::collections::HashMap;
use std::sync::Arc;

pub struct PlanCache {
    entries: RwLock<HashMap<u64, Arc<ExecutionPlan>, FxBuildHasher>>,
}

impl Default for PlanCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanCache {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::with_hasher(FxBuildHasher)),
        }
    }

    #[inline(always)]
    pub fn get_or_compile(&self, script: &str) -> Result<Arc<ExecutionPlan>, EshError> {
        let hash = xxhash_rust::xxh3::xxh3_64(script.as_bytes());

        // Fast read lock check
        {
            let guard = self.entries.read();
            if let Some(plan) = guard.get(&hash) {
                return Ok(Arc::clone(plan));
            }
        }

        // Cache miss: Compile through frontend
        let plan = Arc::new(compile_script(script)?);
        self.entries.write().insert(hash, Arc::clone(&plan));
        Ok(plan)
    }

    pub fn clear(&self) {
        self.entries.write().clear();
    }

    pub fn len(&self) -> usize {
        self.entries.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }
}
