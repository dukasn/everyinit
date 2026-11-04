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

use evin_action_sdk::{OutputStream, PipelineMode, RunnerContext};
use evin_yaml::template::pipeline::{resolve_templates, validate_template_syntax};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Wanted {
    Flag(bool),
    Text(String),
}

impl Default for Wanted {
    fn default() -> Self {
        Self::Flag(true)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Commands {
    find: String,
    create: Option<String>,
    update: Option<String>,
    close: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewRequestConfig {
    branch: String,
    base: String,
    #[serde(default)]
    wanted: Wanted,
    title: Option<String>,
    body_path: Option<String>,
    commands: Commands,
    #[serde(default)]
    context: BTreeMap<String, String>,
}

impl ReviewRequestConfig {
    fn from_with(with: Option<evin_yaml::SpannedValue>) -> Result<Self, evin_yaml::ConfigError> {
        let config: Self = evin_action_sdk::parse_config(with.as_ref())?;
        config
            .validate()
            .map_err(|message| evin_yaml::ConfigError {
                message,
                span: None,
            })?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        let required = [
            ("branch", &self.branch),
            ("base", &self.base),
            ("commands.find", &self.commands.find),
        ];
        for (name, value) in required {
            if value.trim().is_empty() {
                return Err(format!("{name} must not be empty"));
            }
        }
        let optional = [
            ("title", self.title.as_ref()),
            ("body_path", self.body_path.as_ref()),
            ("commands.create", self.commands.create.as_ref()),
            ("commands.update", self.commands.update.as_ref()),
            ("commands.close", self.commands.close.as_ref()),
        ];
        for (name, value) in optional {
            if value.is_some_and(|v| v.trim().is_empty()) {
                return Err(format!("{name} must not be empty when provided"));
            }
        }
        // Wanted can only be checked up front when it is a literal.
        let wanted_literal = match &self.wanted {
            Wanted::Flag(flag) => Some(*flag),
            Wanted::Text(text) => {
                validate_template_syntax(text).map_err(|e| e.to_string())?;
                parse_flag(text).ok()
            }
        };
        if wanted_literal != Some(false) && self.title.is_none() {
            return Err("title is required when a review request is wanted".to_string());
        }
        for template in [Some(&self.branch), Some(&self.base), self.title.as_ref()]
            .into_iter()
            .chain([self.body_path.as_ref()])
            .flatten()
            .chain(self.context.values())
        {
            validate_template_syntax(template).map_err(|e| e.to_string())?;
        }
        if let Some(path) = &self.body_path
            && (path.starts_with('/') || path.split(['/', '\\']).any(|part| part == ".."))
        {
            return Err("body_path must stay inside the workspace".to_string());
        }
        for key in self.context.keys() {
            if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(format!(
                    "context key '{key}' may only contain letters, digits, and underscores"
                ));
            }
        }
        Ok(())
    }
}

fn parse_flag(text: &str) -> Result<bool, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("wanted must be true or false, got '{other}'")),
    }
}

pub fn validate_config(
    with: Option<&evin_yaml::SpannedValue>,
) -> Result<(), evin_yaml::ConfigError> {
    ReviewRequestConfig::from_with(with.cloned()).map(|_| ())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Create,
    Update,
    Close,
    Nothing,
}

fn decide(wanted: bool, existing: bool) -> Decision {
    match (wanted, existing) {
        (true, true) => Decision::Update,
        (true, false) => Decision::Create,
        (false, true) => Decision::Close,
        (false, false) => Decision::Nothing,
    }
}

// This entry point implements the SDK Action contract, whose error type is
// Diagnostic. Boxing here would be immediately undone by the adapter.
#[allow(clippy::result_large_err)]
pub async fn execute(
    ctx: &RunnerContext,
    with: Option<evin_yaml::SpannedValue>,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let config: ReviewRequestConfig = ctx
        .load_config(with.as_ref())
        .map_err(|e| evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(e.to_string())))?;
    config
        .validate()
        .map_err(|e| evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(e)))?;

    run(ctx, &config)
        .await
        .map_err(|e| evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(e)))
}

async fn run(ctx: &RunnerContext, config: &ReviewRequestConfig) -> Result<(), String> {
    let resolve = |value: &str| resolve_templates(value, ctx).map_err(|e| e.to_string());

    let wanted = match &config.wanted {
        Wanted::Flag(flag) => *flag,
        Wanted::Text(text) => parse_flag(&resolve(text)?)?,
    };
    let title = config.title.as_deref().map(resolve).transpose()?;
    if wanted && title.as_deref().is_none_or(|t| t.trim().is_empty()) {
        return Err("the review request title resolved to an empty string".to_string());
    }

    let mut env = HashMap::new();
    env.insert("EVIN_REVIEW_BRANCH".to_string(), resolve(&config.branch)?);
    env.insert("EVIN_REVIEW_BASE".to_string(), resolve(&config.base)?);
    if let Some(title) = &title {
        env.insert("EVIN_REVIEW_TITLE".to_string(), title.clone());
    }
    if let Some(path) = &config.body_path {
        let physical = ctx
            .fs
            .get_safe_physical_path(&resolve(path)?)
            .map_err(|e| format!("body_path is not usable: {e}"))?;
        env.insert(
            "EVIN_REVIEW_BODY_FILE".to_string(),
            physical.to_string_lossy().into_owned(),
        );
    }
    for (key, value) in &config.context {
        env.insert(
            format!("EVIN_REVIEW_CONTEXT_{}", key.to_ascii_uppercase()),
            resolve(value)?,
        );
    }

    if ctx.mode == PipelineMode::DryRun {
        ctx.log.plan(evin_action_sdk::activity::ShellPlan {
            script: config.commands.find.clone(),
        });
        ctx.mark_unverified("The existing review request was not looked up in a dry run");
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
                return Err(format!(
                    "a review request must be {state} but commands.{name} is not configured"
                ));
            }
            ctx.state.set_output("state", "none");
        }
    }
    Ok(())
}

/// Runs a script through ESH and returns its stdout.
async fn execute_command(
    ctx: &RunnerContext,
    script: &str,
    env: &HashMap<String, String>,
    show_stdout: bool,
) -> Result<String, String> {
    let mut esh_ctx = match &ctx.cancel_token {
        Some(token) => esh::ExecutionContext::new().with_token(token.clone()),
        None => esh::ExecutionContext::new(),
    };
    let _ = esh_ctx.set_cwd(ctx.fs.task_cwd());
    for (key, value) in ctx.env.iter().chain(env.iter()) {
        esh_ctx.set_env(key.clone(), value.clone());
    }
    esh_ctx.enable_capture();

    let status = esh::Engine::new().eval_str(script, &mut esh_ctx).await;
    let stdout = esh_ctx.get_captured_stdout().unwrap_or_default();
    let stderr = esh_ctx.get_captured_stderr().unwrap_or_default();
    if show_stdout {
        ctx.log.process_output(OutputStream::Stdout, &stdout);
    }
    ctx.log.process_output(OutputStream::Stderr, &stderr);

    let code = match status {
        Ok(code) => code,
        Err(esh::EshError::Exec(e)) => e.exit_code(),
        Err(_) => 2,
    };
    if code != 0 {
        return Err(format!(
            "command failed with exit status {code}: {}",
            script.lines().next().unwrap_or(script)
        ));
    }
    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(yaml: &str) -> Result<ReviewRequestConfig, evin_yaml::ConfigError> {
        let with = evin_yaml::parse_yaml(yaml).expect("valid YAML");
        ReviewRequestConfig::from_with(Some(with))
    }

    const BASE: &str = "branch: b\nbase: main\ncommands:\n  find: echo 1\n";

    #[test]
    fn decision_table() {
        assert_eq!(decide(true, false), Decision::Create);
        assert_eq!(decide(true, true), Decision::Update);
        assert_eq!(decide(false, true), Decision::Close);
        assert_eq!(decide(false, false), Decision::Nothing);
    }

    #[test]
    fn parses_flags() {
        assert_eq!(parse_flag("true"), Ok(true));
        assert_eq!(parse_flag(" False "), Ok(false));
        assert!(parse_flag("yes").is_err());
    }

    #[test]
    fn validates_config() {
        assert!(config(&format!("{BASE}title: t")).is_ok());
        assert!(config(&format!("{BASE}wanted: false")).is_ok());
        assert!(
            config(&format!(
                "{BASE}wanted: \"${{release:bump.changed}}\"\ntitle: t"
            ))
            .is_ok()
        );
        for extra in [
            "",
            "title: t\nbody_path: ../x.md",
            "title: t\nbody_path: /x.md",
            "title: t\nunknown: 1",
            "title: t\ncontext:\n  bad-key: v",
            "title: ' '",
        ] {
            assert!(config(&format!("{BASE}{extra}")).is_err(), "{extra}");
        }
        assert!(config("branch: b\nbase: main\ntitle: t").is_err());
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
        evin_action_sdk::RunnerContextBuilder::new(
            "t".to_string(),
            tx,
            dir.to_path_buf(),
            git,
            secrets,
        )
        .mode(mode)
        .build()
    }

    fn with(yaml: &str) -> Option<evin_yaml::SpannedValue> {
        Some(evin_yaml::parse_yaml(yaml).expect("valid YAML"))
    }

    fn output(ctx: &RunnerContext, key: &str) -> Option<String> {
        ctx.state.outputs.lock().unwrap().get(key).cloned()
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
        assert!(err.to_string().contains("exit status 3"), "{err}");

        let err = execute(
            &ctx,
            with("branch: b\nbase: m\ntitle: T\ncommands:\n  find: echo\n"),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("commands.create"), "{err}");
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
    }
}
