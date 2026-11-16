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

use super::*;

fn config(yaml: &str) -> Result<ReviewRequestConfig, evin_flow::ConfigError> {
    let with = evin_flow::parse_yaml(yaml).expect("valid YAML");
    ReviewRequestConfig::from_with(Some(with))
}

const BASE: &str = "branch: b\nbase: main\ncommands:\n  find: echo 1\n";

#[test]
fn template_resolution_errors_use_fluent_messages() {
    let issue = super::template_error("context.release", "unknown variable");
    assert_eq!(issue.key, "review-request-template-resolution-failed");
    assert_eq!(
        evin_locales::render_issue(&issue),
        evin_locales::t!(
            "review-request-template-resolution-failed",
            field = "context.release",
            error = "unknown variable"
        )
    );
}

#[tokio::test]
async fn creates_updates_and_closes() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path(), PipelineMode::Normal);
    std::fs::write(dir.path().join("body.md"), "body").unwrap();

    let yaml = |wanted: &str, found: &str| {
        format!(
            "branch: b\nbase: main\nwanted: \"{wanted}\"\ntitle: T\nbody_path: body.md\ncontext:\n  version: 1.2.3\ncommands:\n  find: {found}\n  create: echo \"$EVIN_REVIEW_TITLE $EVIN_REVIEW_CONTEXT_VERSION\" > created.txt\n  update: echo \"$EVIN_REVIEW_ID\" > updated.txt\n  close: echo \"$EVIN_REVIEW_ID\" > closed.txt\n"
        )
    };

    execute(&ctx, with(&yaml("true", "echo"))).await.unwrap();
    assert_eq!(output(&ctx, "state").as_deref(), Some("created"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("created.txt"))
            .unwrap()
            .trim(),
        "T 1.2.3"
    );

    execute(&ctx, with(&yaml("true", "echo 7"))).await.unwrap();
    assert_eq!(output(&ctx, "state").as_deref(), Some("updated"));
    assert_eq!(output(&ctx, "id").as_deref(), Some("7"));

    execute(&ctx, with(&yaml("false", "echo 7"))).await.unwrap();
    assert_eq!(output(&ctx, "state").as_deref(), Some("closed"));

    execute(&ctx, with(&yaml("false", "echo"))).await.unwrap();
    assert_eq!(output(&ctx, "state").as_deref(), Some("none"));
}

#[tokio::test]
async fn reports_failures_and_missing_commands() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path(), PipelineMode::Normal);
    let err = execute(
        &ctx,
        with("branch: b\nbase: m\ntitle: T\ncommands:\n  find: exit 3\n"),
    )
    .await
    .unwrap_err();
    let expected = evin_locales::t!(
        "review-request-command-failed",
        status = 3,
        command = "exit 3"
    );
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    assert_eq!(evin_locales::render_issue(&err.code), expected);

    let err = execute(
        &ctx,
        with("branch: b\nbase: m\ntitle: T\ncommands:\n  find: echo\n"),
    )
    .await
    .unwrap_err();
    let expected = evin_locales::t!(
        "review-request-command-required-for-state",
        state = "created",
        command = "create"
    );
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    assert_eq!(evin_locales::render_issue(&err.code), expected);
}

#[tokio::test]
async fn invalid_runtime_config_uses_localized_diagnostic_title() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path(), PipelineMode::Normal);
    let config = with("branch: b\nbase: main\ncommands:\n  find: echo\nunknown: true\n").unwrap();
    let config_error =
        evin_action_sdk::parse_config::<ReviewRequestConfig>(Some(&config)).unwrap_err();

    let error = execute(&ctx, Some(config)).await.unwrap_err();
    assert_eq!(error.code.key, "issue-yaml-parse-failed");
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("issue-yaml-parse-failed", error = config_error.message)
    );
    assert_eq!(
        error.title,
        evin_locales::t!("review-request-invalid-config-title")
    );
}

#[tokio::test]
async fn dry_run_executes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path(), PipelineMode::DryRun);
    execute(
        &ctx,
        with("branch: b\nbase: m\ntitle: T\ncommands:\n  find: echo > ran.txt\n"),
    )
    .await
    .unwrap();
    assert!(!dir.path().join("ran.txt").exists());
    assert_eq!(
        ctx.uncertainty(),
        [evin_locales::t!("review-request-dry-run-skipped-lookup")]
    );
}

fn context(dir: &std::path::Path, mode: PipelineMode) -> RunnerContext {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default());
    let log = evin_action_sdk::Logger::new("t".to_string(), tx.clone(), secrets.clone());
    let git = std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
        dir.to_path_buf(),
        mode,
        log,
    ));
    evin_action_sdk::RunnerContextBuilder::new("t".to_string(), tx, dir.to_path_buf(), git, secrets)
        .mode(mode)
        .build()
}

fn with(yaml: &str) -> Option<evin_flow::SpannedValue> {
    Some(evin_flow::parse_yaml(yaml).expect("valid YAML"))
}

fn output(ctx: &RunnerContext, key: &str) -> Option<String> {
    ctx.state.outputs.lock().unwrap().get(key).cloned()
}

mod config;
