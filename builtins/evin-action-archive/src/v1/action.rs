// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

#![allow(clippy::result_large_err)]
use evin_action_sdk::{
    Action, ActionDescriptor, ActionFieldDescriptor, ActionFieldKind, ActionOutputDescriptor,
    RunnerContext,
};

const FIELDS: &[ActionFieldDescriptor] = &[
    ActionFieldDescriptor {
        path: "path",
        kind: ActionFieldKind::String,
        required: true,
        description: "Destination path for the tar.gz archive.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "include",
        kind: ActionFieldKind::List,
        required: true,
        description: "Input patterns for the archive.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "exclude",
        kind: ActionFieldKind::List,
        required: false,
        description: "Excluded input patterns.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "prefix",
        kind: ActionFieldKind::String,
        required: false,
        description: "Stored path prefix.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "layout",
        kind: ActionFieldKind::Enum(&["flat", "relative"]),
        required: false,
        description: "Stored path layout.",
        condition: None,
    },
    ActionFieldDescriptor {
        path: "compression_level",
        kind: ActionFieldKind::Integer,
        required: false,
        description: "Gzip compression level.",
        condition: None,
    },
];
const OUTPUTS: &[ActionOutputDescriptor] = &[
    ActionOutputDescriptor {
        name: "path",
        description: "Published archive path.",
    },
    ActionOutputDescriptor {
        name: "size",
        description: "Published archive byte size.",
    },
];
const DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Create a deterministic tar.gz archive.",
    fields: FIELDS,
    outputs: OUTPUTS,
};

pub struct ArchiveAction;

#[async_trait::async_trait]
impl Action for ArchiveAction {
    fn editor_descriptor(&self) -> Option<&'static ActionDescriptor> {
        Some(&DESCRIPTOR)
    }
    fn validate_config(
        &self,
        with: Option<&evin_flow::SpannedValue>,
    ) -> Result<(), evin_flow::ConfigError> {
        super::validate_archive_config(with)
    }
    async fn execute(
        &self,
        ctx: &RunnerContext,
        with: Option<evin_flow::SpannedValue>,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        super::execute_archive(ctx, with).await
    }
}
