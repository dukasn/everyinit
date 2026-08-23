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

use crate::dag::DagResolver;
use crate::engine::PipelineEngine;
use crate::runner::LogMessage;
use evin_config::models::TaskObject;

use std::collections::HashMap;
use tokio::sync::mpsc;

/// Ergonomic test harness for pipeline integration tests.
///
/// Reduces engine setup from ~30 lines to a simple builder chain:
/// ```ignore
/// let fixture = PipelineFixture::new()
///     .with_task("build", None, "echo built")
///     .with_task("test", Some(vec!["build"]), "echo tested");
/// let (report, _logs) = fixture.run().await;
/// assert!(report.error.is_none());
/// ```
pub struct PipelineFixture {
    engine: PipelineEngine,
    resolver: DagResolver,
}

impl PipelineFixture {
    /// Create a new fixture with a default no-CI, Normal mode engine.
    pub fn new() -> Self {
        Self {
            engine: PipelineEngine::new(
                false,
                evin_action_sdk::PipelineMode::Normal,
                HashMap::new(),
                vec![],
                None,
            ),
            resolver: DagResolver::new(),
        }
    }

    /// Add a shell script task to the pipeline under test.
    pub fn with_task(mut self, name: &str, needs: Option<Vec<&str>>, script: &str) -> Self {
        self.resolver.add_task(TaskObject {
            name: name.to_string(),
            needs: needs.map(|v| v.into_iter().map(str::to_string).collect()),
            run: Some(script.to_string()),
            ..Default::default()
        });
        self
    }

    /// Register a native action implementation for use in `steps:` tasks.
    pub fn with_action(mut self, name: &str, action: Box<dyn evin_action_sdk::Action>) -> Self {
        self.engine.register_action(name, action);
        self
    }

    /// Execute the pipeline and collect all log messages.
    pub async fn run(self) -> (crate::engine::PipelineReport, Vec<LogMessage>) {
        let (log_tx, mut log_rx) = mpsc::unbounded_channel::<LogMessage>();
        let report = self.engine.execute(self.resolver, log_tx).await;
        let mut logs = Vec::new();
        while let Ok(msg) = log_rx.try_recv() {
            logs.push(msg);
        }
        (report, logs)
    }
}

impl Default for PipelineFixture {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fixture_linear_success() {
        let (report, _) = PipelineFixture::new()
            .with_task("a", None, "exit 0")
            .with_task("b", Some(vec!["a"]), "exit 0")
            .run()
            .await;

        assert!(
            report.error.is_none(),
            "expected success: {:?}",
            report.error
        );
    }

    #[tokio::test]
    async fn test_fixture_failure_propagates() {
        let (report, _) = PipelineFixture::new()
            .with_task("fail", None, "exit 1")
            .run()
            .await;

        assert!(report.error.is_some(), "expected error but got none");
    }
}
