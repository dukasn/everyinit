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

pub mod activity;
pub mod debug;
pub mod fs;
pub mod git;
pub mod http;
pub mod secrets;
use evin_yaml::template::pipeline::TemplateContext;
pub mod state;

pub mod dry_run {
    pub use evin_io::DryRunState;
}

use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineMode {
    Normal,
    DryRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskOutcome {
    Planned,
    Unverified,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone)]
pub enum LogMessage {
    Activity {
        task: String,
        record: activity::ActivityRecord,
    },
    RawOutput {
        task: String,
        stream: OutputStream,
        line: String,
    },
    PlannedOperation {
        task: String,
        message: activity::LocalizedText,
    },
    Progress {
        task: String,
        message: activity::LocalizedText,
    },
    Diagnostic {
        task: String,
        diagnostic: evin_diagnostic::Diagnostic,
    },
    DebugRecord {
        task: String,
        record: debug::DebugRecord,
    },
    TaskStarted {
        task_name: String,
        description: Option<String>,
        command: Option<String>,
    },
    DependencyUpdate {
        task_name: String,
        in_degree: usize,
    },
    TaskFinished(String, TaskOutcome, std::time::Duration),
    PipelineState(bool),
    DaemonStarted {
        task: String,
        startup_duration: std::time::Duration,
        status_message: activity::LocalizedText,
    },
    DaemonStatus {
        task: String,
        status_message: activity::LocalizedText,
    },
    DaemonStopped {
        task: String,
        duration: std::time::Duration,
        failed: bool,
    },
}

#[derive(Debug, Clone)]
pub struct ActionSyncContext {
    pub workspace_root: std::path::PathBuf,
    pub mode: PipelineMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncChange {
    Create,
    Update,
    Unchanged,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncResource {
    pub label: String,
    pub change: SyncChange,
}

/// The only action-authored output entry point.
///
/// Activity, plans, progress, and daemon statuses accept registry-generated
/// localized types; structural telemetry accepts registry-generated debug
/// types. Arbitrary prose cannot enter either protocol.
///
/// ```compile_fail
/// # fn invalid(log: &evin_action_sdk::Logger) {
/// log.info("uncategorized output is not supported");
/// # }
/// ```
///
/// ```compile_fail
/// let _ = evin_action_sdk::debug::DocsScan { scans: 1 };
/// ```
///
/// ```compile_fail
/// let _ = evin_action_sdk::activity::DocsReady { unexpected: 1 };
/// ```
///
/// ```compile_fail
/// # let pages = 1usize;
/// let _ = evin_action_sdk::debug::DocsScan {
///     scans: "one", pages, rst: 0, markdown: 0, references: 0, read_failures: 0,
/// };
/// ```
#[derive(Clone)]
pub struct Logger {
    task_name: String,
    tx: mpsc::UnboundedSender<LogMessage>,
    secrets: std::sync::Arc<secrets::SecretsProvider>,
}

impl Logger {
    pub fn new(
        task_name: String,
        tx: mpsc::UnboundedSender<LogMessage>,
        secrets: std::sync::Arc<secrets::SecretsProvider>,
    ) -> Self {
        Self {
            task_name,
            tx,
            secrets,
        }
    }

    fn protect_text(&self, text: activity::LocalizedText) -> activity::LocalizedText {
        text.with_redactor(self.secrets.clone())
    }

    pub fn activity<E: activity::ActivityEvent>(&self, event: E) {
        let mut record = event.into_record();
        record.text = self.protect_text(record.text);
        let _ = self.tx.send(LogMessage::Activity {
            task: self.task_name.clone(),
            record,
        });
    }

    #[doc(hidden)]
    pub fn raw_output(&self, stream: OutputStream, line: &str) {
        let _ = self.tx.send(LogMessage::RawOutput {
            task: self.task_name.clone(),
            stream,
            line: self.secrets.redact(line),
        });
    }

    pub fn plan<M: activity::LocalizedMessage>(&self, message: M) {
        let _ = self.tx.send(LogMessage::PlannedOperation {
            task: self.task_name.clone(),
            message: self.protect_text(message.localized_text()),
        });
    }

    pub fn daemon_started<M: activity::LocalizedMessage>(
        &self,
        startup_duration: std::time::Duration,
        status_message: M,
    ) {
        let _ = self.tx.send(LogMessage::DaemonStarted {
            task: self.task_name.clone(),
            startup_duration,
            status_message: self.protect_text(status_message.localized_text()),
        });
    }

    pub fn daemon_status<M: activity::LocalizedMessage>(&self, status_message: M) {
        let _ = self.tx.send(LogMessage::DaemonStatus {
            task: self.task_name.clone(),
            status_message: self.protect_text(status_message.localized_text()),
        });
    }

    pub fn daemon_stopped(&self, duration: std::time::Duration, failed: bool) {
        let _ = self.tx.send(LogMessage::DaemonStopped {
            task: self.task_name.clone(),
            duration,
            failed,
        });
    }

    pub fn debug_event<E: debug::StructuredDebugEvent>(&self, event: E) {
        let mut record = event.into_record();
        for (_, value) in &mut record.payload {
            if let debug::DebugValue::String(value) = value {
                *value = self.secrets.redact(value);
            }
            if let debug::DebugValue::List(values) = value {
                for value in values {
                    *value = self.secrets.redact(value);
                }
            }
        }
        let _ = self.tx.send(LogMessage::DebugRecord {
            task: self.task_name.clone(),
            record,
        });
    }

    pub fn progress<M: activity::LocalizedMessage>(&self, message: M) {
        let _ = self.tx.send(LogMessage::Progress {
            task: self.task_name.clone(),
            message: self.protect_text(message.localized_text()),
        });
    }

    pub fn diagnostic(&self, diagnostic: evin_diagnostic::Diagnostic) {
        let _ = self.tx.send(LogMessage::Diagnostic {
            task: self.task_name.clone(),
            diagnostic,
        });
    }
}

pub struct Environment {
    env: HashMap<String, String>,
}

impl Environment {
    pub fn new(env: HashMap<String, String>) -> Self {
        Self { env }
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.env.get(key)
    }
}

pub struct Values {
    values: HashMap<String, String>,
}

impl Values {
    pub fn new(values: HashMap<String, String>) -> Self {
        Self { values }
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.values.get(key)
    }
}

pub struct RunnerContext {
    pub workspace_name: Option<String>,
    pub task_name: String,
    pub is_ci_mode: bool,
    pub mode: PipelineMode,
    pub hook_payload: Vec<String>,
    pub cwd: Option<String>,
    pub log: Logger,
    pub env: Environment,
    pub values: Values,
    pub fs: fs::WorkspaceFs,
    pub state: state::DagState,
    pub git: std::sync::Arc<git::GitProvider>,
    pub secrets: std::sync::Arc<secrets::SecretsProvider>,
    pub dry_run: evin_io::DryRunState,
    pub gitrules: std::sync::Arc<evin_gitrules::GitRulesRoot>,
    pub http: std::sync::Arc<http::HttpClient>,
    pub cancel_token: Option<tokio_util::sync::CancellationToken>,
}

pub struct RunnerContextBuilder {
    task_name: String,
    log_tx: mpsc::UnboundedSender<LogMessage>,
    workspace_root: std::path::PathBuf,
    git: std::sync::Arc<git::GitProvider>,
    secrets: std::sync::Arc<secrets::SecretsProvider>,
    http: std::sync::Arc<http::HttpClient>,

    workspace_name: Option<String>,
    is_ci_mode: bool,
    mode: PipelineMode,
    hook_payload: Vec<String>,
    cwd: Option<String>,
    env: HashMap<String, String>,
    values: HashMap<String, String>,
    inputs: HashMap<String, HashMap<String, String>>,
    unverified_inputs: std::collections::HashSet<String>,
    dry_run: evin_io::DryRunState,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
}

impl RunnerContextBuilder {
    pub fn new(
        task_name: String,
        log_tx: mpsc::UnboundedSender<LogMessage>,
        workspace_root: std::path::PathBuf,
        git: std::sync::Arc<git::GitProvider>,
        secrets: std::sync::Arc<secrets::SecretsProvider>,
    ) -> Self {
        let http =
            std::sync::Arc::new(http::HttpClient::new().expect("Failed to initialize HTTP client"));
        Self {
            task_name,
            log_tx,
            workspace_root,
            git,
            secrets,
            http,
            workspace_name: None,
            is_ci_mode: false,
            mode: PipelineMode::Normal,
            hook_payload: Vec::new(),
            cwd: None,
            env: HashMap::new(),
            values: HashMap::new(),
            inputs: HashMap::new(),
            unverified_inputs: std::collections::HashSet::new(),
            dry_run: evin_io::DryRunState::default(),
            cancel_token: None,
        }
    }

    pub fn workspace_name(mut self, name: Option<String>) -> Self {
        self.workspace_name = name;
        self
    }

    pub fn is_ci_mode(mut self, ci: bool) -> Self {
        self.is_ci_mode = ci;
        self
    }

    pub fn mode(mut self, mode: PipelineMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn hook_payload(mut self, payload: Vec<String>) -> Self {
        self.hook_payload = payload;
        self
    }

    pub fn cwd(mut self, cwd: Option<String>) -> Self {
        self.cwd = cwd;
        self
    }

    pub fn env(mut self, env: HashMap<String, String>) -> Self {
        self.env = env;
        self
    }

    pub fn values(mut self, values: HashMap<String, String>) -> Self {
        self.values = values;
        self
    }

    pub fn inputs(mut self, inputs: HashMap<String, HashMap<String, String>>) -> Self {
        self.inputs = inputs;
        self
    }

    pub fn unverified_inputs(mut self, unverified: std::collections::HashSet<String>) -> Self {
        self.unverified_inputs = unverified;
        self
    }

    pub fn dry_run(mut self, dry_run: evin_io::DryRunState) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn cancel_token(
        mut self,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
    ) -> Self {
        self.cancel_token = cancel_token;
        self
    }

    pub fn build(self) -> RunnerContext {
        let mut ctx = RunnerContext::new_with_dry_run_and_values_and_http(
            self.workspace_name,
            self.task_name,
            self.is_ci_mode,
            self.mode,
            self.log_tx,
            self.hook_payload,
            self.cwd,
            self.env,
            self.values,
            self.workspace_root,
            self.inputs,
            self.unverified_inputs,
            self.git,
            self.secrets,
            self.http,
            self.dry_run,
        );
        ctx.cancel_token = self.cancel_token;
        ctx
    }
}

impl RunnerContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        workspace_name: Option<String>,
        task_name: String,
        is_ci_mode: bool,
        mode: PipelineMode,
        log_tx: mpsc::UnboundedSender<LogMessage>,
        hook_payload: Vec<String>,
        cwd: Option<String>,
        env: HashMap<String, String>,
        workspace_root: std::path::PathBuf,
        inputs: HashMap<String, HashMap<String, String>>,
        git: std::sync::Arc<git::GitProvider>,
        secrets: std::sync::Arc<secrets::SecretsProvider>,
    ) -> Self {
        Self::new_with_values(
            workspace_name,
            task_name,
            is_ci_mode,
            mode,
            log_tx,
            hook_payload,
            cwd,
            env,
            HashMap::new(),
            workspace_root,
            inputs,
            git,
            secrets,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_values(
        workspace_name: Option<String>,
        task_name: String,
        is_ci_mode: bool,
        mode: PipelineMode,
        log_tx: mpsc::UnboundedSender<LogMessage>,
        hook_payload: Vec<String>,
        cwd: Option<String>,
        env: HashMap<String, String>,
        values: HashMap<String, String>,
        workspace_root: std::path::PathBuf,
        inputs: HashMap<String, HashMap<String, String>>,
        git: std::sync::Arc<git::GitProvider>,
        secrets: std::sync::Arc<secrets::SecretsProvider>,
    ) -> Self {
        Self::new_with_dry_run_and_values(
            workspace_name,
            task_name,
            is_ci_mode,
            mode,
            log_tx,
            hook_payload,
            cwd,
            env,
            values,
            workspace_root,
            inputs,
            std::collections::HashSet::new(),
            git,
            secrets,
            evin_io::DryRunState::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_dry_run(
        workspace_name: Option<String>,
        task_name: String,
        is_ci_mode: bool,
        mode: PipelineMode,
        log_tx: mpsc::UnboundedSender<LogMessage>,
        hook_payload: Vec<String>,
        cwd: Option<String>,
        env: HashMap<String, String>,
        workspace_root: std::path::PathBuf,
        inputs: HashMap<String, HashMap<String, String>>,
        unverified_inputs: std::collections::HashSet<String>,
        git: std::sync::Arc<git::GitProvider>,
        secrets: std::sync::Arc<secrets::SecretsProvider>,
        dry_run: evin_io::DryRunState,
    ) -> Self {
        Self::new_with_dry_run_and_values(
            workspace_name,
            task_name,
            is_ci_mode,
            mode,
            log_tx,
            hook_payload,
            cwd,
            env,
            HashMap::new(),
            workspace_root,
            inputs,
            unverified_inputs,
            git,
            secrets,
            dry_run,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_dry_run_and_values(
        workspace_name: Option<String>,
        task_name: String,
        is_ci_mode: bool,
        mode: PipelineMode,
        log_tx: mpsc::UnboundedSender<LogMessage>,
        hook_payload: Vec<String>,
        cwd: Option<String>,
        env: HashMap<String, String>,
        values: HashMap<String, String>,
        workspace_root: std::path::PathBuf,
        inputs: HashMap<String, HashMap<String, String>>,
        unverified_inputs: std::collections::HashSet<String>,
        git: std::sync::Arc<git::GitProvider>,
        secrets: std::sync::Arc<secrets::SecretsProvider>,
        dry_run: evin_io::DryRunState,
    ) -> Self {
        Self::new_with_dry_run_and_values_and_http(
            workspace_name,
            task_name,
            is_ci_mode,
            mode,
            log_tx,
            hook_payload,
            cwd,
            env,
            values,
            workspace_root,
            inputs,
            unverified_inputs,
            git,
            secrets,
            std::sync::Arc::new(http::HttpClient::new().expect("Failed to initialize HTTP client")),
            dry_run,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_dry_run_and_values_and_http(
        workspace_name: Option<String>,
        task_name: String,
        is_ci_mode: bool,
        mode: PipelineMode,
        log_tx: mpsc::UnboundedSender<LogMessage>,
        hook_payload: Vec<String>,
        cwd: Option<String>,
        env: HashMap<String, String>,
        values: HashMap<String, String>,
        workspace_root: std::path::PathBuf,
        inputs: HashMap<String, HashMap<String, String>>,
        unverified_inputs: std::collections::HashSet<String>,
        git: std::sync::Arc<git::GitProvider>,
        secrets: std::sync::Arc<secrets::SecretsProvider>,
        http: std::sync::Arc<http::HttpClient>,
        dry_run: evin_io::DryRunState,
    ) -> Self {
        let log = Logger::new(task_name.clone(), log_tx, secrets.clone());
        let env_obj = Environment::new(env);
        let values_obj = Values::new(values);
        let dag_state = state::DagState::new(inputs, unverified_inputs);

        let task_cwd = cwd
            .clone()
            .map(|c| {
                let p = std::path::PathBuf::from(c);
                if p.is_absolute() {
                    p
                } else {
                    workspace_root.join(p)
                }
            })
            .unwrap_or_else(|| workspace_root.clone());
        let workspace_fs = fs::WorkspaceFs::new_with_dry_run(
            workspace_root,
            task_cwd,
            mode,
            log.clone(),
            dry_run.clone(),
        );

        Self {
            workspace_name,
            task_name,
            is_ci_mode,
            mode,
            hook_payload,
            cwd,
            log,
            env: env_obj,
            values: values_obj,
            fs: workspace_fs,
            state: dag_state,
            git,
            secrets,
            dry_run,
            gitrules: std::sync::Arc::new(evin_gitrules::GitRulesRoot::default()),
            http,
            cancel_token: None,
        }
    }

    pub fn git(&self) -> &git::GitProvider {
        &self.git
    }

    pub fn http(&self) -> &http::HttpClient {
        &self.http
    }

    pub fn mark_unverified(&self, reason: impl Into<String>) {
        self.dry_run.mark_unverified(&self.task_name, reason);
    }

    pub fn uncertainty(&self) -> Vec<String> {
        self.dry_run.uncertainty_for(&self.task_name)
    }

    /// Retrieves the globally resolved GitRules.
    /// Since the core engine always evaluates policies, this is guaranteed to return a valid schema.
    pub fn gitrules(&self) -> &evin_gitrules::GitRulesRoot {
        &self.gitrules
    }

    pub fn tag_format(&self) -> &str {
        self.gitrules
            .tags
            .as_ref()
            .and_then(|t| t.name_format.as_ref())
            .and_then(|f| f.iter().next())
            .map(|s| s.as_str())
            .unwrap_or("v{version}")
    }

    /// Safely gets whether signatures are required, defaulting to false
    pub fn require_signatures(&self) -> bool {
        self.gitrules
            .authorship
            .as_ref()
            .and_then(|a| a.policies.as_ref())
            .and_then(|p| p.require_signatures)
            .unwrap_or(false)
    }
    pub fn load_config<T: serde::de::DeserializeOwned>(
        &self,
        with: Option<&evin_yaml::SpannedValue>,
    ) -> Result<T, evin_yaml::ConfigError> {
        let evaluated_with = with.map(|v| evaluate_spanned_value(v.clone(), self));
        let (config, mut report) = match &evaluated_with {
            Some(value) => evin_yaml::de::from_spanned_value_with_report(value.clone())?,
            None => evin_yaml::de::from_spanned_value_with_report(
                evin_yaml::SpannedValue::Mapping(Default::default(), evin_yaml::Span::default()),
            )?,
        };

        emit_config_debug(&self.log, &mut report);

        Ok(config)
    }
}

fn emit_config_debug(logger: &Logger, report: &mut evin_yaml::ConfigReport) {
    report.overrides.sort();
    logger.debug_event(debug::ActionConfig {
        overrides: report.overrides.len(),
        defaults: report.defaults.len(),
        override_keys: report.overrides.clone(),
    });
}

/// The formal contract for all EVIN Native Actions.
#[async_trait]
pub trait Action: Send + Sync {
    /// Statically validates the `with:` payload provided in the YAML configuration.
    /// Every action must implement this contract, including actions that accept no options.
    fn validate_config(
        &self,
        with: Option<&evin_yaml::SpannedValue>,
    ) -> Result<(), evin_yaml::ConfigError>;

    async fn sync(
        &self,
        _ctx: &ActionSyncContext,
        _configs: &[Option<evin_yaml::SpannedValue>],
    ) -> Result<Vec<SyncResource>, evin_diagnostic::Diagnostic> {
        Ok(Vec::new())
    }

    /// Executes the native action.
    /// `ctx`: Provides access to the environment, logger, and the hook payload.
    /// `with`: The optional YAML arguments parsed from the `.evin/module.yaml` file.
    async fn execute(
        &self,
        ctx: &RunnerContext,
        with: Option<evin_yaml::SpannedValue>,
    ) -> Result<(), evin_diagnostic::Diagnostic>;
}

/// Helper method to deserialize the plugin's configuration from the `with` mapping.
pub fn parse_config<T: serde::de::DeserializeOwned>(
    with: Option<&evin_yaml::SpannedValue>,
) -> Result<T, evin_yaml::ConfigError> {
    match with {
        Some(value) => evin_yaml::de::from_spanned_value(value.clone()),
        None => evin_yaml::de::from_spanned_value(evin_yaml::SpannedValue::Mapping(
            Default::default(),
            evin_yaml::Span::default(),
        )),
    }
}

impl TemplateContext for RunnerContext {
    fn task_output(&self, task: &str, key: &str) -> Option<String> {
        // 1. Try the task name as written (handles fully-qualified "ns:task" and root tasks)
        if let Some(value) = self
            .state
            .inputs
            .get(task)
            .and_then(|o| o.get(key).cloned())
        {
            return Some(value);
        }

        // 2. Scope-chain fallback: if the current task has a namespace (e.g. "docs:build-docs")
        //    and the requested task name is bare (no ":"), try "ns:task" automatically.
        //    This mirrors how parse_tasks() auto-qualifies bare `needs:` references.
        if !task.contains(':')
            && let Some(ns) = self
                .task_name
                .split(':')
                .next()
                .filter(|_| self.task_name.contains(':'))
        {
            let qualified = format!("{}:{}", ns, task);
            if let Some(value) = self
                .state
                .inputs
                .get(&qualified)
                .and_then(|o| o.get(key).cloned())
            {
                return Some(value);
            }
        }

        // 3. Explicit workspace reference via workspace name prefix (e.g. ${evin-monorepo:build.out})
        if let Some(ws_name) = &self.workspace_name {
            let ws_prefix = format!("{}:", ws_name);
            if let Some(bare) = task.strip_prefix(&ws_prefix) {
                return self
                    .state
                    .inputs
                    .get(bare)
                    .and_then(|o| o.get(key).cloned());
            }
        }

        None
    }
    fn value(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }
    fn env(&self, key: &str) -> Option<String> {
        self.env.get(key).cloned()
    }
    fn is_unverified(&self, task: &str) -> bool {
        self.mode == PipelineMode::DryRun && self.state.input_is_unverified(task)
    }
    fn mark_unverified(&self, message: String) {
        self.mark_unverified(message);
    }
}

fn evaluate_spanned_value<C: TemplateContext>(
    val: evin_yaml::SpannedValue,
    ctx: &C,
) -> evin_yaml::SpannedValue {
    match val {
        evin_yaml::SpannedValue::String(s, span) => {
            if s.contains("${") {
                if let Ok(resolved) = evin_yaml::template::resolve_templates(&s, ctx) {
                    evin_yaml::SpannedValue::String(resolved, span)
                } else {
                    evin_yaml::SpannedValue::String(s, span)
                }
            } else {
                evin_yaml::SpannedValue::String(s, span)
            }
        }
        evin_yaml::SpannedValue::Sequence(seq, span) => {
            let new_seq = seq
                .into_iter()
                .map(|item| evaluate_spanned_value(item, ctx))
                .collect();
            evin_yaml::SpannedValue::Sequence(new_seq, span)
        }
        evin_yaml::SpannedValue::Mapping(map, span) => {
            let new_map = map
                .into_iter()
                .map(|(k, v)| {
                    let new_k = evaluate_spanned_value(k, ctx);
                    let new_v = evaluate_spanned_value(v, ctx);
                    (new_k, new_v)
                })
                .collect();
            evin_yaml::SpannedValue::Mapping(new_map, span)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_report_emits_one_coded_count_event_without_default_keys() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let logger = Logger::new(
            "build".to_string(),
            tx,
            std::sync::Arc::new(secrets::SecretsProvider::default()),
        );
        let mut report = evin_yaml::ConfigReport {
            overrides: vec!["title".to_string(), "mode".to_string()],
            defaults: vec!["port".to_string(), "theme".to_string()],
        };

        emit_config_debug(&logger, &mut report);

        let message = rx.try_recv().expect("one config debug event");
        assert!(rx.try_recv().is_err());
        let LogMessage::DebugRecord { record, .. } = message else {
            panic!("expected a structural debug event");
        };
        assert_eq!(record.code, "E_109");
        assert_eq!(record.aggregation, debug::DebugAggregation::Append);
        assert_eq!(
            record.payload,
            vec![
                ("overrides", debug::DebugValue::Unsigned(2)),
                ("defaults", debug::DebugValue::Unsigned(2)),
                (
                    "override_keys",
                    debug::DebugValue::List(vec!["mode".into(), "title".into()])
                ),
            ]
        );
        assert!(!format!("{:?}", record.payload).contains("port"));
        assert!(!format!("{:?}", record.payload).contains("theme"));
    }

    #[test]
    fn builtins_do_not_bypass_the_typed_output_protocols() {
        fn rust_files(path: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    rust_files(&path, files);
                } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                    files.push(path);
                }
            }
        }

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../builtins");
        let mut files = Vec::new();
        rust_files(&root, &mut files);
        for file in files {
            let source = std::fs::read_to_string(&file).unwrap();
            for forbidden in [
                ".info(",
                ".warn(",
                ".error(",
                ".debug(",
                ".structural_debug(",
                ".raw_output(",
            ] {
                assert!(
                    !source.contains(forbidden),
                    "{} bypasses the typed protocol with {forbidden}",
                    file.display()
                );
            }
        }
    }

    #[test]
    fn localized_activity_is_redacted_after_interpolation() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut secrets = secrets::SecretsProvider::default();
        secrets.register_secret("TOKEN", "secret-value");
        let logger = Logger::new("docs".into(), tx, std::sync::Arc::new(secrets));
        logger.activity(activity::DocsChanged {
            path: "prefix-secret-value-suffix".into(),
        });

        let LogMessage::Activity { record, .. } = rx.try_recv().unwrap() else {
            panic!("expected activity");
        };
        assert_eq!(record.text.args[0].1, "prefix-secret-value-suffix");
        let localized = evin_locales::render_message(record.text.key, &record.text.args);
        assert_eq!(record.text.redact_rendered(&localized), "prefix-***-suffix");
    }
}
