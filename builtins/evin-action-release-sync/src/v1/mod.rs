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

pub mod adapter;
mod adapters;
pub mod config;

mod action;
pub use action::ReleaseSyncAction;

use adapter::{
    CommandGitStateProvider, ContextPolicyEvaluator, FsArtifactInspector, FsNotesExtractor,
    FsPlanSink, FsRemoteStateSource,
};
use config::ReleaseSyncConfig;
use evin_action_sdk::RunnerContext;
use evin_release_sync::Reconciler;

fn protocol_error_diagnostic(
    error: evin_release_sync::ProtocolError,
) -> evin_diagnostic::Diagnostic {
    match error {
        evin_release_sync::ProtocolError::UnsupportedSchemaVersion(version) => {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("release-sync-schema-version-unsupported")
                    .arg("version", version.to_string()),
            )
        }
        evin_release_sync::ProtocolError::SchemaValidation(error) => evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("release-sync-schema-validation-failed")
                .arg("error", error),
        ),
        evin_release_sync::ProtocolError::StateIngestion(error) => evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("release-sync-state-ingestion-failed")
                .arg("error", error),
        ),
        evin_release_sync::ProtocolError::GitInspection(error) => evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("release-sync-git-inspection-failed")
                .arg("error", error),
        ),
        evin_release_sync::ProtocolError::NotesExtraction(error) => evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("release-sync-notes-extraction-failed")
                .arg("error", error),
        ),
        evin_release_sync::ProtocolError::ArtifactInspection(error) => evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("release-sync-artifact-inspection-failed")
                .arg("error", error),
        ),
        evin_release_sync::ProtocolError::PlanWrite(error) => evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("release-sync-plan-operation-failed")
                .arg("error", error),
        ),
        evin_release_sync::ProtocolError::Io(error) => evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("release-sync-io-failed").arg("error", error),
        ),
    }
}

pub fn validate_config(
    with: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    ReleaseSyncConfig::from_with(with).map(|_| ())
}

#[allow(clippy::result_large_err)]
pub async fn execute(
    ctx: &RunnerContext,
    with: Option<evin_flow::SpannedValue>,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let config = ReleaseSyncConfig::from_with(with.as_ref()).map_err(|error| {
        error.into_diagnostic(evin_locales::t!("release-sync-invalid-config-title"), None)
    })?;

    let root = ctx.fs.workspace_root();
    let state_source = FsRemoteStateSource {
        workspace_root: root,
        state_path: &config.state,
    };
    let notes_extractor = FsNotesExtractor::new(root, &config.changelog);
    let git_provider = CommandGitStateProvider {
        workspace_root: root,
        tag: config.tag.as_deref(),
        title: config.title.as_deref(),
    };
    let artifact_inspector = FsArtifactInspector {
        workspace_root: root,
        artifacts_dir: &config.artifacts_dir,
    };
    let policy_evaluator = ContextPolicyEvaluator {
        ctx,
        prune_orphans: config.prune_orphans,
    };
    let mut plan_sink = FsPlanSink::new(root, &config);

    let mut reconciler = Reconciler::new(
        &state_source,
        &notes_extractor,
        &git_provider,
        &artifact_inspector,
        &policy_evaluator,
        &mut plan_sink,
    );

    let plan = reconciler.reconcile().map_err(protocol_error_diagnostic)?;

    ctx.state.set_output("plan", &config.plan_out);
    ctx.state
        .set_output("delta_count", &plan.delta_count.to_string());
    ctx.state
        .set_output("reconciled", &plan.reconciled.to_string());

    Ok(())
}

#[cfg(test)]
mod tests;
