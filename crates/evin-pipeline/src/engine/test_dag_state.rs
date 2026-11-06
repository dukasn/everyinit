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
use async_trait::async_trait;
use evin_action_sdk::{Action, RunnerContext};
use evin_config::models::{StepObject, TaskObject};
use std::collections::HashMap;
use tokio::sync::mpsc;

struct MockWriterAction;

#[async_trait]
impl Action for MockWriterAction {
    fn validate_config(
        &self,
        _with: Option<&evin_flow::SpannedValue>,
    ) -> Result<(), evin_flow::ConfigError> {
        Ok(())
    }

    async fn execute(
        &self,
        ctx: &RunnerContext,
        _with: Option<evin_flow::SpannedValue>,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        ctx.state.set_output("secret", "42");
        Ok(())
    }
}

struct MockReaderAction;

#[async_trait]
impl Action for MockReaderAction {
    fn validate_config(
        &self,
        _with: Option<&evin_flow::SpannedValue>,
    ) -> Result<(), evin_flow::ConfigError> {
        Ok(())
    }

    async fn execute(
        &self,
        ctx: &RunnerContext,
        _with: Option<evin_flow::SpannedValue>,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        let val = ctx.state.get_input("writer_task", "secret");
        if val == Some("42".to_string()) {
            Ok(())
        } else {
            Err("Failed to read secret".to_string().into())
        }
    }
}

#[tokio::test]
async fn test_executor_dag_state_passing() {
    let mut resolver = DagResolver::new();

    let task_a = TaskObject {
        name: "writer_task".to_string(),
        steps: Some(vec![StepObject {
            run: None,
            uses: Some(evin_flow::Spanned::new(
                "writer".to_string(),
                evin_flow::Span::default(),
            )),
            with: None,
        }]),
        ..Default::default()
    };

    let task_b = TaskObject {
        name: "reader_task".to_string(),
        needs: Some(vec!["writer_task".to_string()]),
        steps: Some(vec![StepObject {
            run: None,
            uses: Some(evin_flow::Spanned::new(
                "reader".to_string(),
                evin_flow::Span::default(),
            )),
            with: None,
        }]),
        ..Default::default()
    };

    resolver.add_task(task_a);
    resolver.add_task(task_b);

    let mut engine = PipelineEngine::new(
        false,
        evin_action_sdk::PipelineMode::Normal,
        HashMap::new(),
        vec![],
        None,
    );
    engine.register_action("writer", Box::new(MockWriterAction));
    engine.register_action("reader", Box::new(MockReaderAction));

    let (log_tx, _) = mpsc::unbounded_channel();
    let res = engine.execute(resolver, log_tx).await;

    assert!(
        res.error.is_none(),
        "PipelineEngine failed to pass state between tasks"
    );
}
