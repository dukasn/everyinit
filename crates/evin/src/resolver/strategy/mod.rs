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

pub mod hook;
pub mod stage;
pub mod task;

pub use hook::HookStrategy;
pub use stage::StageStrategy;
pub use task::TaskStrategy;

use evin_config::models::TaskObject;
use evin_diagnostic::Diagnostic;
use evin_pipeline::PipelineContext;

/// Defines a strategy for parsing, matching, and resolving a user-provided target
/// into its corresponding logical execution context and dependency list.
pub trait TargetStrategy {
    /// Returns the pipeline context (e.g. Stage, Hook, Task) for this strategy
    fn context(&self) -> PipelineContext;

    /// Checks if the target strictly matches the pattern boundaries for this context
    fn matches_pattern(&self, target: &str, tasks: &[TaskObject]) -> bool;

    /// Resolves the target to a list of concrete task names, returning a Diagnostic on failure
    fn resolve(&self, target: &str, tasks: &[TaskObject]) -> Result<Vec<String>, Diagnostic>;
}

pub(crate) fn best_fuzzy_match<'a>(
    target: &str,
    candidates: impl Iterator<Item = &'a str>,
) -> Option<String> {
    let mut best_match = None;
    let mut best_dist = usize::MAX;

    for candidate in candidates {
        let dist = strsim::levenshtein(target, candidate);
        if dist < best_dist && dist <= 3 {
            best_dist = dist;
            best_match = Some(candidate.to_string());
        }
    }

    best_match
}
