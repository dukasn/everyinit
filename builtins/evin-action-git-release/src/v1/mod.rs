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

mod action;
pub use action::GitReleaseAction;

use evin_action_sdk::RunnerContext;
mod config;
mod push;
use config::{Enabled, GitReleaseConfig, StageMode};
use evin_flow::template::pipeline::resolve_templates;
use push::publish;

fn template_resolution_error(
    field: String,
    error: impl std::fmt::Display,
) -> evin_diagnostic::Diagnostic {
    evin_locales::diagnostic(
        evin_diagnostic::IssueCode::new("git-release-template-resolution-failed")
            .arg("field", field)
            .arg("error", error.to_string()),
    )
}

pub fn validate_config(
    with: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    GitReleaseConfig::from_with(with.cloned()).map(|_| ())
}

// This entry point implements the SDK Action contract, whose error type is
// Diagnostic. Boxing here would be immediately undone by the adapter.
#[allow(clippy::result_large_err)]
pub async fn execute(
    ctx: &RunnerContext,
    with: Option<evin_flow::SpannedValue>,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let config: GitReleaseConfig = ctx.load_config(with.as_ref()).map_err(|error| {
        error.into_diagnostic(evin_locales::t!("git-release-invalid-config-title"), None)
    })?;
    config.validate().map_err(evin_locales::diagnostic)?;

    let enabled = match &config.enabled {
        Enabled::Flag(flag) => *flag,
        Enabled::Text(text) => {
            let resolved = resolve_templates(text, ctx).map_err(|error| {
                template_resolution_error(evin_locales::t!("git-release-enabled-field"), error)
            })?;
            match resolved.trim().to_ascii_lowercase().as_str() {
                "true" => true,
                "false" => false,
                other => {
                    return Err(evin_locales::diagnostic(
                        evin_diagnostic::IssueCode::new("git-release-enabled-must-be-boolean")
                            .arg("value", other),
                    ));
                }
            }
        }
    };
    if !enabled {
        ctx.state.set_output("committed", "false");
        return Ok(());
    }

    let commit_msg_template = if let Some(m) = &config.message {
        m.clone()
    } else if let Some(m) = ctx
        .policy()
        .releases
        .as_ref()
        .and_then(|r| r.commit_message.clone())
    {
        m
    } else {
        return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
            "message-must-not-be-empty",
        )));
    };

    let commit_msg = resolve_templates(&commit_msg_template, ctx).map_err(|error| {
        template_resolution_error(evin_locales::t!("git-release-commit-message-field"), error)
    })?;

    if config.stage == StageMode::All {
        ctx.git()
            .stage_files(&["-A".to_string()])
            .await
            .map_err(evin_diagnostic::Diagnostic::from)?;
    }
    let has_staged_changes = ctx
        .git()
        .has_staged_changes()
        .await
        .map_err(evin_diagnostic::Diagnostic::from)?;
    let has_changes = has_staged_changes
        || (config.stage == StageMode::All
            && ctx.mode == evin_action_sdk::PipelineMode::DryRun
            && ctx
                .git()
                .is_dirty()
                .await
                .map_err(evin_diagnostic::Diagnostic::from)?);
    if !has_changes {
        return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
            "no-staged-changes",
        )));
    }

    ctx.git()
        .verify_identity()
        .await
        .map_err(evin_diagnostic::Diagnostic::from)?;

    let tag_name = if let Some(tag) = &config.tag {
        let resolved = resolve_templates(tag, ctx).map_err(|error| {
            template_resolution_error(evin_locales::t!("git-release-tag-field"), error)
        })?;
        ctx.git()
            .validate_tag_name(&resolved)
            .await
            .map_err(evin_diagnostic::Diagnostic::from)?;
        if ctx
            .git()
            .has_tag(&resolved)
            .await
            .map_err(evin_diagnostic::Diagnostic::from)?
        {
            return Err(evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("tag-already-exists").arg("tag", &resolved),
            ));
        }
        Some(resolved)
    } else {
        None
    };

    let require_signatures = ctx.require_signatures();

    ctx.git()
        .commit(&commit_msg, require_signatures)
        .await
        .map_err(evin_diagnostic::Diagnostic::from)?;

    let mut pushed_tag = None;
    if let Some(tag) = tag_name {
        let result = ctx.git().create_tag(&tag, &commit_msg).await;
        if let Err(e) = result {
            let commit_id = ctx
                .git()
                .get_last_commit_id()
                .await
                .unwrap_or_else(|_| "HEAD".to_string());
            return Err(evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("tag-recovery-failed")
                    .arg("commit_id", commit_id)
                    .arg("tag", &tag)
                    .arg("error", e.to_string()),
            ));
        }
        ctx.state.set_output("tag", &tag);
        ctx.log.debug_event(evin_action_sdk::debug::GitRelease {
            tag: tag.clone(),
            provider: "git".to_string(),
            draft: false,
        });
        pushed_tag = Some(tag);
    }

    ctx.state.set_output("committed", "true");

    if let Some(push) = &config.push {
        publish(ctx, push, pushed_tag.as_deref()).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests;
