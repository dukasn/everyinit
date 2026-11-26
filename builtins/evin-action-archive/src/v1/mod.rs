// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

#![allow(clippy::result_large_err)]
mod action;
mod archiver;
pub mod config;
mod mapping;
mod strategies;
pub use action::ArchiveAction;

use archiver::Archiver;
use config::TarGzConfig;
use evin_action_sdk::{
    RunnerContext,
    packaging::{self, SourceEntry},
};
use evin_diagnostic::Diagnostic;
use mapping::resolve_and_map_files;
use serde::de::DeserializeOwned;
use strategies::targz::TarGzArchiver;

pub fn validate_archive_config(
    with: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    let config: TarGzConfig = parse_config(with)?;
    config.validate().map_err(to_config_error)
}

fn parse_config<T: DeserializeOwned>(
    with: Option<&evin_flow::SpannedValue>,
) -> Result<T, evin_flow::ConfigError> {
    evin_action_sdk::parse_config(with).map_err(|error| {
        error.map_message(|message| evin_locales::t!("invalid-config", error = message))
    })
}

fn to_config_error(diagnostic: Diagnostic) -> evin_flow::ConfigError {
    evin_flow::ConfigError {
        message: evin_locales::render_issue(&diagnostic.code),
        span: None,
    }
}

pub async fn execute_archive(
    ctx: &RunnerContext,
    with: Option<evin_flow::SpannedValue>,
) -> Result<(), Diagnostic> {
    let config: TarGzConfig = ctx
        .load_config(with.as_ref())
        .map_err(|error| error.into_diagnostic(evin_locales::t!("invalid-config-title"), None))?;
    config.validate()?;
    let sources = resolve_and_map_files(ctx, &config)?
        .into_iter()
        .map(|entry| SourceEntry {
            source: entry.source,
            destination: entry.destination.to_string_lossy().replace('\\', "/"),
            mode: None,
        })
        .collect::<Vec<_>>();
    let compression_level = config.compression_level;
    packaging::publish_artifact(
        ctx,
        config.path,
        "tar.gz".into(),
        config.include,
        sources,
        compression_level,
        move |output, context| TarGzArchiver { compression_level }.write_archive(output, context),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests;
