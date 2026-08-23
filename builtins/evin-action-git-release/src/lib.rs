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

pub mod v1;

use evin_action_sdk::{Action, RunnerContext};

pub struct GitReleaseAction;

#[async_trait::async_trait]
impl Action for GitReleaseAction {
    fn validate_config(
        &self,
        with: Option<&evin_yaml::SpannedValue>,
    ) -> Result<(), evin_yaml::ConfigError> {
        v1::validate_config(with)
    }

    async fn execute(
        &self,
        ctx: &RunnerContext,
        with: Option<evin_yaml::SpannedValue>,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        v1::execute(ctx, with).await
    }
}
