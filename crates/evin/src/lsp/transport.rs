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

use super::EvinLspServer;
use crate::commands::builtin_actions;
use evin_config::{WorkspaceSemanticAnalysis, WorkspaceSnapshot};
use evin_lsp::protocol::{read_message, write_message};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{self, BufReader};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const ANALYSIS_DEBOUNCE: Duration = Duration::from_millis(180);

struct RunTaskCompletion {
    request_id: Value,
    run_id: String,
    result: Result<RunTaskResult, String>,
}

struct RunTaskResult {
    success: bool,
    cancelled: bool,
    exit_code: Option<i32>,
}

#[derive(Debug, PartialEq)]
struct ExecuteCommand {
    request_id: Value,
    task: String,
    dry_run: bool,
    include_output: bool,
    parallel: bool,
    sequential: bool,
}

struct RunTaskRequest {
    root: PathBuf,
    task: String,
    dry_run: bool,
    include_output: bool,
    parallel: bool,
    sequential: bool,
    run_id: String,
    cancellation: CancellationToken,
    work_done_progress: bool,
    outgoing: mpsc::Sender<Vec<u8>>,
}

pub async fn run_lsp_server() -> io::Result<()> {
    let (incoming_tx, mut incoming_rx) = mpsc::channel::<String>(64);
    let reader = tokio::task::spawn_blocking(move || {
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin.lock());
        while let Some(message) = read_message(&mut reader)? {
            if incoming_tx.blocking_send(message).is_err() {
                break;
            }
        }
        Ok::<(), io::Error>(())
    });

    let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<Vec<u8>>(64);
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(frame) = outgoing_rx.recv().await {
            stdout.write_all(&frame).await?;
            stdout.flush().await?;
        }
        Ok::<(), io::Error>(())
    });

    let mut server = EvinLspServer {
        defer_analysis: true,
        ..EvinLspServer::default()
    };
    let mut analysis_deadline = None;
    let (analysis_tx, mut analysis_rx) =
        mpsc::channel::<(u64, Result<WorkspaceSemanticAnalysis, String>)>(4);
    let (run_tx, mut run_rx) = mpsc::channel::<RunTaskCompletion>(16);
    let mut running = HashMap::<String, CancellationToken>::new();
    loop {
        tokio::select! {
            biased;
            message = incoming_rx.recv() => {
                let Some(message) = message else { break; };
                let value = serde_json::from_str::<Value>(&message).unwrap_or(Value::Null);
                if let Some(request) = parse_execute_command(&value) {
                    let Some(ExecuteCommand { request_id, task, dry_run, include_output, parallel, sequential }) = request else {
                        send_rpc_error(&outgoing_tx, value.get("id").cloned().unwrap_or(Value::Null), -32602, "executeCommand requires a task name").await?;
                        continue;
                    };
                    let Some(root) = server.root.clone().filter(|_| server.initialized) else {
                        send_rpc_error(&outgoing_tx, request_id, -32002, "Server is not initialized").await?;
                        continue;
                    };
                    let run_id = crate::event_stream::new_run_id();
                    let cancellation = CancellationToken::new();
                    let request_key = request_id.to_string();
                    if running.contains_key(&request_key) {
                        send_rpc_error(&outgoing_tx, request_id, -32600, "Request id is already running").await?;
                        continue;
                    }
                    running.insert(request_key, cancellation.clone());
                    begin_run_progress(&outgoing_tx, server.work_done_progress, &run_id, &task).await?;
                    let work_done_progress = server.work_done_progress;
                    let outgoing = outgoing_tx.clone();
                    let completion_tx = run_tx.clone();
                    tokio::spawn(async move {
                        let result = relay_task_run(RunTaskRequest {
                            root,
                            task,
                            dry_run,
                            include_output,
                            parallel,
                            sequential,
                            run_id: run_id.clone(),
                            cancellation,
                            work_done_progress,
                            outgoing,
                        })
                        .await
                        .map_err(|error| error.to_string());
                        let _ = completion_tx.send(RunTaskCompletion { request_id, run_id, result }).await;
                    });
                    continue;
                }
                if value.get("method").and_then(Value::as_str) == Some("$/cancelRequest") {
                    if let Some(request_id) = value.pointer("/params/id")
                        && let Some(token) = running.get(&request_id.to_string())
                    {
                        token.cancel();
                    }
                    continue;
                }
                let mut frame = Vec::new();
                server.handle_message(&message, &mut frame)?;
                analysis_deadline = server
                    .analysis_pending
                    .then_some(tokio::time::Instant::now() + ANALYSIS_DEBOUNCE);
                send_frame(&outgoing_tx, frame).await?;
            }
            result = analysis_rx.recv() => {
                if let Some((generation, result)) = result {
                    match result {
                        Ok(analysis) => {
                            let mut frame = Vec::new();
                            server.apply_background_analysis(generation, analysis, &mut frame)?;
                            send_frame(&outgoing_tx, frame).await?;
                        }
                        Err(error) => {
                            eprintln!("evin: workspace analysis failed: {error}");
                            let mut frame = Vec::new();
                            server.fail_background_analysis(generation, &mut frame)?;
                            send_frame(&outgoing_tx, frame).await?;
                        }
                    }
                }
            }
            completion = run_rx.recv() => {
                if let Some(completion) = completion {
                    running.remove(&completion.request_id.to_string());
                    end_run_progress(&outgoing_tx, server.work_done_progress, &completion.run_id).await?;
                    match completion.result {
                        Ok(result) => {
                            let response = json!({
                                "jsonrpc": "2.0",
                                "id": completion.request_id,
                                "result": {
                                    "runId": completion.run_id,
                                    "success": result.success,
                                    "cancelled": result.cancelled,
                                    "exitCode": result.exit_code,
                                }
                            });
                            send_json(&outgoing_tx, response).await?;
                        }
                        Err(error) => send_rpc_error(&outgoing_tx, completion.request_id, -32603, &error).await?,
                    }
                }
            }
            _ = sleep_until_deadline(analysis_deadline), if analysis_deadline.is_some() => {
                if let Some((generation, source)) = server.take_analysis_job() {
                    let mut frame = Vec::new();
                    server.begin_analysis_progress(generation, &mut frame)?;
                    send_frame(&outgoing_tx, frame).await?;
                    let analysis_tx = analysis_tx.clone();
                    tokio::spawn(async move {
                        let result = tokio::task::spawn_blocking(move || {
                            let actions = builtin_actions();
                            WorkspaceSemanticAnalysis::build(
                                WorkspaceSnapshot::load_sync(&source),
                                &actions,
                            )
                        })
                        .await
                        .map_err(|error| error.to_string());
                        let _ = analysis_tx.send((generation, result)).await;
                    });
                }
                analysis_deadline = None;
            }
        }
    }

    drop(outgoing_tx);
    writer.await.map_err(io::Error::other)??;
    reader.await.map_err(io::Error::other)??;
    Ok(())
}

fn parse_execute_command(value: &Value) -> Option<Option<ExecuteCommand>> {
    if value.get("method").and_then(Value::as_str) != Some("workspace/executeCommand") {
        return None;
    }
    let id = value.get("id")?.clone();
    let params = value.get("params").unwrap_or(&Value::Null);
    let command = params.get("command").and_then(Value::as_str)?;
    if !matches!(
        command,
        "everyinit.lspRunTask" | "everyinit.lspRunTaskDryRun"
    ) {
        return None;
    }
    let arguments = params
        .get("arguments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let first = arguments.first().unwrap_or(&Value::Null);
    let task = first
        .as_str()
        .or_else(|| first.get("taskName").and_then(Value::as_str))
        .or_else(|| first.get("task").and_then(Value::as_str))
        .or_else(|| first.get("name").and_then(Value::as_str))
        .filter(|task| !task.trim().is_empty())
        .map(str::to_owned);
    let Some(task) = task else {
        return Some(None);
    };
    let dry_run = command == "everyinit.lspRunTaskDryRun"
        || first.get("dryRun").and_then(Value::as_bool) == Some(true);
    let include_output = first
        .get("includeOutput")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let parallel = first.get("parallel").and_then(Value::as_bool) == Some(true);
    let sequential = first.get("sequential").and_then(Value::as_bool) == Some(true);
    Some(Some(ExecuteCommand {
        request_id: id,
        task,
        dry_run,
        include_output,
        parallel,
        sequential,
    }))
}

async fn relay_task_run(request: RunTaskRequest) -> io::Result<RunTaskResult> {
    let RunTaskRequest {
        root,
        task,
        dry_run,
        include_output,
        parallel,
        sequential,
        run_id,
        cancellation,
        work_done_progress,
        outgoing,
    } = request;
    let executable = std::env::current_exe()?;
    let mut command = Command::new(executable);
    if dry_run {
        command.arg("--dry-run");
    }
    command
        .current_dir(root)
        .arg("run")
        .arg("--events")
        .arg("ndjson");
    if parallel {
        command.arg("--parallel");
    } else if sequential {
        command.arg("--sequential");
    }
    if include_output {
        command.arg("--events-output");
    }
    command
        .arg(&task)
        .env("EVIN_RUN_ID", &run_id)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("task process stdout was not piped"))?;
    let mut lines = AsyncBufReader::new(stdout).lines();
    let mut cancelled = false;
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line? else { break; };
                let Ok(event) = serde_json::from_str::<Value>(&line) else {
                    eprintln!("evin: ignored invalid child event line");
                    continue;
                };
                let kind = event.get("kind").and_then(Value::as_str).unwrap_or_default();
                send_json(&outgoing, json!({
                    "jsonrpc": "2.0",
                    "method": "evin/pipelineEvent",
                    "params": event,
                })).await?;
                if work_done_progress
                    && let Some(message) = progress_message(kind, event.get("payload").unwrap_or(&Value::Null))
                {
                    report_run_progress(&outgoing, &run_id, &message).await?;
                }
            }
            _ = cancellation.cancelled() => {
                cancelled = true;
                let _ = child.start_kill();
                break;
            }
        }
    }
    let status = child.wait().await?;
    Ok(RunTaskResult {
        success: !cancelled && status.success(),
        cancelled,
        exit_code: status.code(),
    })
}

fn progress_message(kind: &str, payload: &Value) -> Option<String> {
    match kind {
        "run_started" => Some("Run started".into()),
        "phase_started" => payload
            .get("phase")
            .and_then(Value::as_str)
            .map(str::to_owned),
        "task_started" => payload
            .get("name")
            .and_then(Value::as_str)
            .map(|name| format!("Running {name}")),
        "task_step_started" => Some(format!(
            "{}: step {}/{}",
            payload
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("task"),
            payload
                .get("step_number")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            payload
                .get("step_count")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
        )),
        "daemon_status" => Some(format!(
            "{}: {}",
            payload
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("daemon"),
            payload
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("status changed"),
        )),
        "run_completed" => Some(
            if payload.get("succeeded").and_then(Value::as_bool) == Some(true) {
                "Run completed"
            } else {
                "Run failed"
            }
            .into(),
        ),
        _ => None,
    }
}

async fn begin_run_progress(
    outgoing: &mpsc::Sender<Vec<u8>>,
    enabled: bool,
    run_id: &str,
    task: &str,
) -> io::Result<()> {
    if !enabled {
        return Ok(());
    }
    send_json(
        outgoing,
        json!({
            "jsonrpc": "2.0",
            "id": format!("evin-run-progress-create-{run_id}"),
            "method": "window/workDoneProgress/create",
            "params": { "token": format!("evin-run-{run_id}") },
        }),
    )
    .await?;
    send_json(
        outgoing,
        json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {
                "token": format!("evin-run-{run_id}"),
                "value": { "kind": "begin", "title": format!("Running {task}") }
            },
        }),
    )
    .await
}

async fn report_run_progress(
    outgoing: &mpsc::Sender<Vec<u8>>,
    run_id: &str,
    message: &str,
) -> io::Result<()> {
    send_json(
        outgoing,
        json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {
                "token": format!("evin-run-{run_id}"),
                "value": { "kind": "report", "message": message }
            },
        }),
    )
    .await
}

async fn end_run_progress(
    outgoing: &mpsc::Sender<Vec<u8>>,
    enabled: bool,
    run_id: &str,
) -> io::Result<()> {
    if !enabled {
        return Ok(());
    }
    send_json(
        outgoing,
        json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {
                "token": format!("evin-run-{run_id}"),
                "value": { "kind": "end" }
            },
        }),
    )
    .await
}

async fn send_rpc_error(
    outgoing: &mpsc::Sender<Vec<u8>>,
    id: Value,
    code: i64,
    message: &str,
) -> io::Result<()> {
    send_json(
        outgoing,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message },
        }),
    )
    .await
}

async fn send_json(outgoing: &mpsc::Sender<Vec<u8>>, value: Value) -> io::Result<()> {
    let mut frame = Vec::new();
    write_message(&mut frame, &value.to_string())?;
    send_frame(outgoing, frame).await
}

async fn sleep_until_deadline(deadline: Option<tokio::time::Instant>) {
    if let Some(deadline) = deadline {
        tokio::time::sleep_until(deadline).await;
    } else {
        std::future::pending().await
    }
}

async fn send_frame(outgoing: &mpsc::Sender<Vec<u8>>, frame: Vec<u8>) -> io::Result<()> {
    if frame.is_empty() {
        return Ok(());
    }
    outgoing
        .send(frame)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "LSP writer stopped"))
}

#[cfg(test)]
mod tests {
    use super::{ExecuteCommand, parse_execute_command};
    use serde_json::json;

    #[test]
    fn parses_task_run_options_from_execute_command() {
        let parsed = parse_execute_command(&json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "workspace/executeCommand",
            "params": {
                "command": "everyinit.lspRunTask",
                "arguments": [{
                    "taskName": "build",
                    "includeOutput": true,
                    "parallel": true
                }]
            }
        }));

        let Some(Some(ExecuteCommand {
            request_id,
            task,
            dry_run,
            include_output,
            parallel,
            sequential,
        })) = parsed
        else {
            panic!("expected an Evin task execution request");
        };
        assert_eq!(request_id, json!(7));
        assert_eq!(task, "build");
        assert!(!dry_run);
        assert!(include_output);
        assert!(parallel);
        assert!(!sequential);
    }

    #[test]
    fn ignores_other_execute_commands_and_rejects_missing_task_names() {
        assert!(
            parse_execute_command(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "workspace/executeCommand",
                "params": { "command": "other.command" }
            }))
            .is_none()
        );

        assert_eq!(
            parse_execute_command(&json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "workspace/executeCommand",
                "params": { "command": "everyinit.lspRunTask", "arguments": [] }
            })),
            Some(None)
        );
    }
}
