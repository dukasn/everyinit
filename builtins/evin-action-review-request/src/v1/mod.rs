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
pub use action::ReviewRequestAction;

use evin_action_sdk::{PipelineMode, RunnerContext};
mod config;
mod policy;
mod shell;
use config::{ReviewRequestConfig, Wanted, parse_flag};
use evin_flow::template::pipeline::resolve_templates;
use policy::{Decision, decide};
use shell::execute_command;
use std::collections::HashMap;

fn template_error(field: &str, error: impl std::fmt::Display) -> evin_diagnostic::IssueCode {
    evin_diagnostic::IssueCode::new("review-request-template-resolution-failed")
        .arg("field", field)
        .arg("error", error.to_string())
}

pub fn validate_config(
    with: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    ReviewRequestConfig::from_with(with.cloned()).map(|_| ())
}

// This entry point implements the SDK Action contract, whose error type is
// Diagnostic. Boxing here would be immediately undone by the adapter.
#[allow(clippy::result_large_err)]
pub async fn execute(
    ctx: &RunnerContext,
    with: Option<evin_flow::SpannedValue>,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let config: ReviewRequestConfig = ctx.load_config(with.as_ref()).map_err(|error| {
        error.into_diagnostic(
            evin_locales::t!("review-request-invalid-config-title"),
            None,
        )
    })?;
    config.validate().map_err(evin_locales::diagnostic)?;

    run(ctx, &config).await.map_err(evin_locales::diagnostic)
}

async fn run(
    ctx: &RunnerContext,
    config: &ReviewRequestConfig,
) -> Result<(), evin_diagnostic::IssueCode> {
    let resolve = |field: &str, value: &str| {
        resolve_templates(value, ctx).map_err(|error| template_error(field, error))
    };

    let wanted = match &config.wanted {
        Wanted::Flag(flag) => *flag,
        Wanted::Text(text) => parse_flag(&resolve("wanted", text)?)?,
    };
    let title = config
        .title
        .as_deref()
        .map(|value| resolve("title", value))
        .transpose()?;
    if wanted && title.as_deref().is_none_or(|t| t.trim().is_empty()) {
        return Err(evin_diagnostic::IssueCode::new(
            "review-request-title-resolved-empty",
        ));
    }

    let mut env = HashMap::new();
    env.insert(
        "EVIN_REVIEW_BRANCH".to_string(),
        resolve("branch", &config.branch)?,
    );
    env.insert(
        "EVIN_REVIEW_BASE".to_string(),
        resolve("base", &config.base)?,
    );
    if let Some(title) = &title {
        env.insert("EVIN_REVIEW_TITLE".to_string(), title.clone());
    }
    if let Some(path) = &config.body_path {
        let physical = ctx
            .fs
            .get_safe_physical_path(&resolve("body_path", path)?)
            .map_err(|error| {
                evin_diagnostic::IssueCode::new("review-request-body-path-unusable")
                    .arg("error", error.to_string())
            })?;
        env.insert(
            "EVIN_REVIEW_BODY_FILE".to_string(),
            physical.to_string_lossy().into_owned(),
        );
    }
    for (key, value) in &config.context {
        env.insert(
            format!("EVIN_REVIEW_CONTEXT_{}", key.to_ascii_uppercase()),
            resolve(&format!("context.{key}"), value)?,
        );
    }

    if ctx.mode == PipelineMode::DryRun {
        ctx.log.plan(evin_action_sdk::activity::ShellPlan {
            script: config.commands.find.clone(),
        });
        ctx.mark_unverified(evin_locales::t!("review-request-dry-run-skipped-lookup"));
        ctx.state.set_output("state", "none");
        return Ok(());
    }

    let found = execute_command(ctx, &config.commands.find, &env, false).await?;
    let id = found.lines().map(str::trim).find(|l| !l.is_empty());
    let decision = decide(wanted, id.is_some());

    let (name, command, state) = match decision {
        Decision::Create => ("create", config.commands.create.as_deref(), "created"),
        Decision::Update => ("update", config.commands.update.as_deref(), "updated"),
        Decision::Close => ("close", config.commands.close.as_deref(), "closed"),
        Decision::Nothing => ("", None, "none"),
    };

    if let Some(id) = id {
        env.insert("EVIN_REVIEW_ID".to_string(), id.to_string());
        ctx.state.set_output("id", id);
    }
    match command {
        Some(script) => {
            execute_command(ctx, script, &env, true).await?;
            ctx.state.set_output("state", state);
        }
        None => {
            if decision != Decision::Nothing && decision != Decision::Update {
                return Err(evin_diagnostic::IssueCode::new(
                    "review-request-command-required-for-state",
                )
                .arg("state", state)
                .arg("command", name));
            }
            ctx.state.set_output("state", "none");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
