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
use std::collections::HashMap;
#[tokio::test]
async fn test_shell_runner_success() {
    let runner = ShellRunner::new();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let ctx = RunnerContext::new(
        None,
        "test_success".to_string(),
        true,
        evin_action_sdk::PipelineMode::Normal,
        tx,
        vec![],
        None,
        std::collections::HashMap::new(),
        std::path::PathBuf::from("."),
        std::collections::HashMap::new(),
        std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
            std::path::PathBuf::from("."),
            evin_action_sdk::PipelineMode::Normal,
            evin_action_sdk::Logger::new(
                "test".to_string(),
                tokio::sync::mpsc::unbounded_channel().0,
                std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
            ),
        )),
        std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
    );
    let mut env = HashMap::new();
    env.insert("TEST_VAR".to_string(), "hello".to_string());

    #[cfg(unix)]
    let script = "exit 0";
    #[cfg(windows)]
    let script = "exit 0";

    let res = runner.run(&ctx, script, env, None).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn test_shell_runner_failure() {
    let runner = ShellRunner::new();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let ctx = RunnerContext::new(
        None,
        "test_fail".to_string(),
        true,
        evin_action_sdk::PipelineMode::Normal,
        tx,
        vec![],
        None,
        std::collections::HashMap::new(),
        std::path::PathBuf::from("."),
        std::collections::HashMap::new(),
        std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
            std::path::PathBuf::from("."),
            evin_action_sdk::PipelineMode::Normal,
            evin_action_sdk::Logger::new(
                "test".to_string(),
                tokio::sync::mpsc::unbounded_channel().0,
                std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
            ),
        )),
        std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
    );

    #[cfg(unix)]
    let script = "exit 1";
    #[cfg(windows)]
    let script = "exit 1";

    let res = runner.run(&ctx, script, HashMap::new(), None).await;
    assert!(res.is_err());
    let error = res.unwrap_err();
    assert_eq!(
        error,
        evin_diagnostic::IssueCode::new("issue-task-exit-failed")
            .arg("task_name", "test_fail")
            .arg("status", "1")
    );
}

#[tokio::test]
async fn test_shell_runner_context_buffer() {
    let runner = ShellRunner::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let ctx = RunnerContext::new(
        None,
        "test_buf".to_string(),
        false,
        evin_action_sdk::PipelineMode::Normal,
        tx,
        vec![],
        None,
        std::collections::HashMap::new(),
        std::path::PathBuf::from("."),
        std::collections::HashMap::new(),
        std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
            std::path::PathBuf::from("."),
            evin_action_sdk::PipelineMode::Normal,
            evin_action_sdk::Logger::new(
                "test".to_string(),
                tokio::sync::mpsc::unbounded_channel().0,
                std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
            ),
        )),
        std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
    ); // Not CI mode, shouldn't print directly

    #[cfg(unix)]
    let script = "echo 'EVIN Rocks'";
    #[cfg(windows)]
    let script = "echo 'EVIN Rocks'";

    let _ = runner.run(&ctx, script, HashMap::new(), None).await;

    let mut output = String::new();
    while let Ok(msg) = rx.try_recv() {
        if let LogMessage::RawOutput { line, .. } = msg {
            output.push_str(&line);
            output.push('\n');
        }
    }

    assert!(output.contains("EVIN Rocks"));
}

#[tokio::test]
async fn dry_run_shell_emits_plan_without_running_script() {
    let runner = ShellRunner::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let root = std::env::temp_dir().join(format!("evin-shell-dry-run-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let marker = root.join("marker.txt");
    let ctx = RunnerContext::new(
        None,
        "dry_run".to_string(),
        false,
        evin_action_sdk::PipelineMode::DryRun,
        tx,
        vec![],
        Some(root.to_string_lossy().to_string()),
        HashMap::new(),
        root.clone(),
        HashMap::new(),
        std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
            root.clone(),
            evin_action_sdk::PipelineMode::DryRun,
            evin_action_sdk::Logger::new(
                "dry_run".to_string(),
                tokio::sync::mpsc::unbounded_channel().0,
                std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
            ),
        )),
        std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
    );

    runner
        .run(&ctx, "touch marker.txt", HashMap::new(), None)
        .await
        .unwrap();

    assert!(!marker.exists());
    assert!(matches!(
        rx.try_recv().unwrap(),
        LogMessage::PlannedOperation { message, .. }
            if message.key == "plan-shell-command"
                && message.args == [("script", "touch marker.txt".to_string())]
    ));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn test_shell_runner_redaction() {
    let runner = ShellRunner::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // Create a temporary .env file to load secrets into the context.
    let temp_dir = std::env::temp_dir().join(format!("evin-redact-test-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let env_path = temp_dir.join(".env");
    std::fs::write(&env_path, "SUPER_SECRET=my_top_secret_token_12345").unwrap();

    let secrets_provider = evin_action_sdk::secrets::SecretsProvider::new(&temp_dir);

    let ctx = RunnerContext::new(
        None,
        "test_redact".to_string(),
        false,
        evin_action_sdk::PipelineMode::Normal,
        tx,
        vec![],
        None,
        std::collections::HashMap::new(),
        temp_dir.clone(),
        std::collections::HashMap::new(),
        std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
            temp_dir.clone(),
            evin_action_sdk::PipelineMode::Normal,
            evin_action_sdk::Logger::new(
                "test".to_string(),
                tokio::sync::mpsc::unbounded_channel().0,
                std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
            ),
        )),
        std::sync::Arc::new(secrets_provider),
    );

    let script = "set -x; echo 'Connecting with token my_top_secret_token_12345...'; printf 'stderr token my_top_secret_token_12345...\\n' >&2";

    let _ = runner.run(&ctx, script, HashMap::new(), None).await;

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut reported_redactions = None;
    while let Ok(msg) = rx.try_recv() {
        match msg {
            LogMessage::RawOutput { line, stream, .. } => {
                let output = match stream {
                    evin_action_sdk::OutputStream::Stdout => &mut stdout,
                    evin_action_sdk::OutputStream::Stderr => &mut stderr,
                };
                output.push_str(&line);
                output.push('\n');
            }
            LogMessage::DebugRecord { record, .. } if record.code == "E_605" => {
                reported_redactions =
                    record
                        .payload
                        .iter()
                        .find_map(|(key, value)| match (key, value) {
                            (key, evin_action_sdk::debug::DebugValue::Unsigned(n))
                                if *key == "redaction_count" =>
                            {
                                Some(*n as usize)
                            }
                            _ => None,
                        });
            }
            _ => {}
        }
    }

    assert!(stdout.contains("Connecting with token ***..."), "{stdout}");
    assert!(!stdout.contains("my_top_secret_token_12345"), "{stdout}");
    assert!(stderr.contains("stderr token ***..."), "{stderr}");
    assert!(!stderr.contains("my_top_secret_token_12345"), "{stderr}");
    let redacted_lines = stdout
        .lines()
        .chain(stderr.lines())
        .filter(|line| line.contains("***"))
        .count();
    assert_eq!(reported_redactions, Some(redacted_lines));
    let _ = std::fs::remove_dir_all(temp_dir);
}

#[tokio::test]
async fn esh_state_outputs_feed_dag_state_and_redact_dynamic_secret_outputs() {
    let runner = ShellRunner::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let temp_dir = tempfile::tempdir().unwrap();
    let mut secrets = evin_action_sdk::secrets::SecretsProvider::default();
    secrets.register_secret("SOURCE_TOKEN", "source-secret-value");
    let secrets = std::sync::Arc::new(secrets);
    let logger = evin_action_sdk::Logger::new("state-output".into(), tx.clone(), secrets.clone());
    let ctx = RunnerContext::new(
        None,
        "state-output".into(),
        false,
        evin_action_sdk::PipelineMode::Normal,
        tx,
        vec![],
        Some(temp_dir.path().to_string_lossy().into_owned()),
        HashMap::new(),
        temp_dir.path().to_path_buf(),
        HashMap::new(),
        std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
            temp_dir.path().to_path_buf(),
            evin_action_sdk::PipelineMode::Normal,
            logger,
        )),
        secrets,
    );

    let script = "printf 'API_TOKEN=prefix-source-secret-value-suffix\\nversion=1.2.3\\n' >> \"$EVIN_STATE\"\necho \"token: prefix-source-secret-value-suffix\"";
    runner
        .run(&ctx, script, HashMap::new(), None)
        .await
        .unwrap();

    let outputs = ctx.state.outputs.lock().unwrap();
    assert_eq!(outputs.get("version").map(String::as_str), Some("1.2.3"));
    assert_eq!(
        outputs.get("API_TOKEN").map(String::as_str),
        Some("prefix-source-secret-value-suffix")
    );
    drop(outputs);

    let mut stdout = Vec::new();
    let mut state_keys = None;
    let mut redaction_count = None;
    while let Ok(message) = rx.try_recv() {
        match message {
            LogMessage::RawOutput {
                line,
                stream: evin_action_sdk::OutputStream::Stdout,
                ..
            } => stdout.push(line),
            LogMessage::DebugRecord { record, .. } if record.code == "E_604" => {
                state_keys = record
                    .payload
                    .iter()
                    .find_map(|(key, value)| match (key, value) {
                        (key, evin_action_sdk::debug::DebugValue::List(keys)) if *key == "keys" => {
                            Some(keys.clone())
                        }
                        _ => None,
                    });
            }
            LogMessage::DebugRecord { record, .. } if record.code == "E_605" => {
                redaction_count =
                    record
                        .payload
                        .iter()
                        .find_map(|(key, value)| match (key, value) {
                            (key, evin_action_sdk::debug::DebugValue::Unsigned(count))
                                if *key == "redaction_count" =>
                            {
                                Some(*count)
                            }
                            _ => None,
                        });
            }
            _ => {}
        }
    }
    assert!(stdout.iter().any(|line| line == "token: ***"));
    assert_eq!(state_keys, Some(vec!["API_TOKEN".into(), "version".into()]));
    assert_eq!(redaction_count, Some(1));
}
