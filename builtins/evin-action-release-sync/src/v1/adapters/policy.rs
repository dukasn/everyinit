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

use evin_action_sdk::RunnerContext;
use evin_release_sync::{PolicyEvaluator, RemoteRelease};

pub struct ContextPolicyEvaluator<'a> {
    pub ctx: &'a RunnerContext,
    pub prune_orphans: bool,
}

impl<'a> PolicyEvaluator for ContextPolicyEvaluator<'a> {
    fn is_prerelease(&self, tag: &str) -> bool {
        let clean = tag.trim_start_matches('v');
        if let Ok(semver) = semver::Version::parse(clean) {
            return !semver.pre.is_empty();
        }
        tag.contains('-')
            || tag.contains("rc")
            || tag.contains("canary")
            || tag.contains("beta")
            || tag.contains("alpha")
    }

    fn is_draft(&self, _tag: &str) -> bool {
        false
    }

    fn should_prune_remote(&self, release: &RemoteRelease) -> bool {
        self.prune_orphans && release.is_draft
    }
}
