// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

//! Versioned, machine-readable projection of pipeline events.

use crate::events::PipelineEvent;
use evin_action_sdk::OutputStream as InternalOutputStream;
use evin_diagnostic::{Diagnostic, FileLocation, Severity};
use serde::Serialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{Notify, mpsc};

pub const PUBLIC_EVENT_SCHEMA_VERSION: u16 = 1;
const DEFAULT_OUTPUT_BUFFER: usize = 256;

#[derive(Debug, Clone, Serialize)]
pub struct PublicEventEnvelope {
    pub schema_version: u16,
    pub run_id: String,
    pub seq: u64,
    pub timestamp: String,
    #[serde(flatten)]
    pub event: PublicEvent,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum PublicEvent {
    RunStarted {
        workspace: Option<String>,
        tasks: Vec<String>,
    },
    RunCompleted {
        duration_ms: u64,
        succeeded: bool,
    },
    PhaseStarted {
        phase: String,
    },
    PhaseCompleted {
        phase: String,
        duration_ms: u64,
    },
    PhaseFailed {
        phase: String,
        duration_ms: u64,
        diagnostics: Vec<PublicDiagnostic>,
    },
    TaskStarted {
        name: String,
        description: Option<String>,
    },
    TaskStepStarted {
        name: String,
        step_number: usize,
        step_count: usize,
    },
    TaskOutput {
        name: String,
        stream: PublicOutputStream,
        line: String,
    },
    TaskEnded {
        name: String,
        duration_ms: u64,
        failed: bool,
        unverified: bool,
        cancelled: bool,
    },
    Diagnostic {
        task: Option<String>,
        diagnostic: PublicDiagnostic,
    },
    DaemonStarted {
        name: String,
        startup_duration_ms: u64,
    },
    DaemonStatus {
        name: String,
        status: String,
    },
    DaemonStopped {
        name: String,
        duration_ms: u64,
        failed: bool,
    },
    OutputDropped {
        count: u64,
    },
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicOutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicDiagnostic {
    pub code: Option<String>,
    pub severity: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<PublicLocation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicLocation {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_offset: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_length: Option<usize>,
}

/// Projects internal lifecycle and task events into the stable external schema.
/// Presentation-only events and verbose/debug records intentionally have no mapping.
pub fn project_event(event: &PipelineEvent) -> Option<PublicEvent> {
    match event {
        PipelineEvent::PhaseStarted(phase) => Some(PublicEvent::PhaseStarted {
            phase: stable_phase_name(phase),
        }),
        PipelineEvent::PhaseCompleted(phase, duration) => Some(PublicEvent::PhaseCompleted {
            phase: stable_phase_name(phase),
            duration_ms: duration_ms(*duration),
        }),
        PipelineEvent::PhaseFailed(phase, duration, diagnostics) => {
            Some(PublicEvent::PhaseFailed {
                phase: stable_phase_name(phase),
                duration_ms: duration_ms(*duration),
                diagnostics: diagnostics.iter().map(project_diagnostic).collect(),
            })
        }
        PipelineEvent::TaskOutputStarted {
            name, description, ..
        } => Some(PublicEvent::TaskStarted {
            name: name.clone(),
            description: description.clone(),
        }),
        PipelineEvent::TaskStepStarted {
            name,
            step_number,
            step_count,
            ..
        } => Some(PublicEvent::TaskStepStarted {
            name: name.clone(),
            step_number: *step_number,
            step_count: *step_count,
        }),
        PipelineEvent::TaskOutputEnded {
            name,
            duration,
            failed,
            unverified,
            cancelled,
            ..
        } => Some(PublicEvent::TaskEnded {
            name: name.clone(),
            duration_ms: duration_ms(*duration),
            failed: *failed,
            unverified: *unverified,
            cancelled: *cancelled,
        }),
        PipelineEvent::RawOutput { task, stream, line } => Some(PublicEvent::TaskOutput {
            name: task.clone(),
            stream: match stream {
                InternalOutputStream::Stdout => PublicOutputStream::Stdout,
                InternalOutputStream::Stderr => PublicOutputStream::Stderr,
            },
            line: line.clone(),
        }),
        PipelineEvent::Diagnostic { task, diagnostic } => Some(PublicEvent::Diagnostic {
            task: Some(task.clone()),
            diagnostic: project_diagnostic(diagnostic),
        }),
        PipelineEvent::TaskDaemonStarted {
            name,
            startup_duration,
            ..
        } => Some(PublicEvent::DaemonStarted {
            name: name.clone(),
            startup_duration_ms: duration_ms(*startup_duration),
        }),
        PipelineEvent::TaskDaemonStatus {
            name,
            status_message,
        } => Some(PublicEvent::DaemonStatus {
            name: name.clone(),
            status: status_message.redact_rendered(&evin_locales::render_message(
                status_message.key,
                &status_message.args,
            )),
        }),
        PipelineEvent::TaskDaemonStopped {
            name,
            duration,
            failed,
        } => Some(PublicEvent::DaemonStopped {
            name: name.clone(),
            duration_ms: duration_ms(*duration),
            failed: *failed,
        }),
        _ => None,
    }
}

fn stable_phase_name(phase: &str) -> String {
    phase
        .strip_prefix("ui.phase.")
        .unwrap_or(phase)
        .replace('_', "-")
}

fn duration_ms(duration: std::time::Duration) -> u64 {
    duration.as_millis().min(u64::MAX as u128) as u64
}

fn project_diagnostic(diagnostic: &Diagnostic) -> PublicDiagnostic {
    let location = diagnostic.location.as_ref().map(|location| match location {
        FileLocation::Path { path, line, column } => PublicLocation {
            path: path.clone(),
            line: *line,
            column: *column,
            byte_offset: None,
            byte_length: None,
        },
        FileLocation::ByteSpan {
            path,
            offset,
            length,
            ..
        } => PublicLocation {
            path: path.clone(),
            line: None,
            column: None,
            byte_offset: Some(*offset),
            byte_length: Some(*length),
        },
    });
    PublicDiagnostic {
        code: Some(diagnostic.code.to_string()),
        severity: match diagnostic.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
        .to_owned(),
        message: diagnostic.title.clone(),
        location,
    }
}

#[derive(Clone)]
pub struct PublicEventPublisher {
    lifecycle_tx: mpsc::UnboundedSender<PublicEvent>,
    output_tx: Option<mpsc::Sender<PublicEvent>>,
    dropped_output: Arc<AtomicU64>,
    dropped_notify: Arc<Notify>,
}

pub struct PublicEventSubscriber {
    run_id: String,
    lifecycle_rx: mpsc::UnboundedReceiver<PublicEvent>,
    output_rx: Option<mpsc::Receiver<PublicEvent>>,
    dropped_output: Arc<AtomicU64>,
    dropped_notify: Arc<Notify>,
    lifecycle_open: bool,
    output_open: bool,
    next_seq: u64,
    pending: Option<PublicEvent>,
}

pub fn public_event_channel(
    run_id: String,
    include_output: bool,
) -> (PublicEventPublisher, PublicEventSubscriber) {
    let (lifecycle_tx, lifecycle_rx) = mpsc::unbounded_channel();
    let (output_tx, output_rx) = if include_output {
        let (tx, rx) = mpsc::channel(DEFAULT_OUTPUT_BUFFER);
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };
    let dropped_output = Arc::new(AtomicU64::new(0));
    let dropped_notify = Arc::new(Notify::new());
    (
        PublicEventPublisher {
            lifecycle_tx,
            output_tx,
            dropped_output: dropped_output.clone(),
            dropped_notify: dropped_notify.clone(),
        },
        PublicEventSubscriber {
            run_id,
            lifecycle_rx,
            output_rx,
            dropped_output,
            dropped_notify,
            lifecycle_open: true,
            output_open: include_output,
            next_seq: 0,
            pending: None,
        },
    )
}

impl PublicEventPublisher {
    pub fn publish_internal(&self, event: &PipelineEvent) {
        if let Some(event) = project_event(event) {
            self.publish(event);
        }
    }

    pub fn publish(&self, event: PublicEvent) {
        if matches!(event, PublicEvent::TaskOutput { .. }) {
            let Some(output_tx) = &self.output_tx else {
                return;
            };
            match output_tx.try_send(event) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    self.dropped_output.fetch_add(1, Ordering::Relaxed);
                    self.dropped_notify.notify_one();
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {}
            }
        } else {
            let _ = self.lifecycle_tx.send(event);
        }
    }
}

impl PublicEventSubscriber {
    pub async fn recv(&mut self) -> Option<PublicEventEnvelope> {
        loop {
            if let Some(event) = self.pending.take() {
                if let Some(dropped) = self.take_dropped_output() {
                    self.pending = Some(event);
                    return Some(self.envelope(PublicEvent::OutputDropped { count: dropped }));
                }
                return Some(self.envelope(event));
            }

            if let Some(dropped) = self.take_dropped_output() {
                return Some(self.envelope(PublicEvent::OutputDropped { count: dropped }));
            }
            if !self.lifecycle_open && !self.output_open {
                return None;
            }

            let next = tokio::select! {
                biased;
                event = self.lifecycle_rx.recv(), if self.lifecycle_open => {
                    match event {
                        Some(event) => Some(event),
                        None => { self.lifecycle_open = false; continue; }
                    }
                }
                event = receive_output(&mut self.output_rx), if self.output_open => {
                    match event {
                        Some(event) => Some(event),
                        None => { self.output_open = false; continue; }
                    }
                }
                _ = self.dropped_notify.notified() => continue,
            };
            if let Some(event) = next {
                if let Some(dropped) = self.take_dropped_output() {
                    self.pending = Some(event);
                    return Some(self.envelope(PublicEvent::OutputDropped { count: dropped }));
                }
                return Some(self.envelope(event));
            }
        }
    }

    fn take_dropped_output(&self) -> Option<u64> {
        match self.dropped_output.swap(0, Ordering::AcqRel) {
            0 => None,
            count => Some(count),
        }
    }

    fn envelope(&mut self, event: PublicEvent) -> PublicEventEnvelope {
        self.next_seq = self.next_seq.saturating_add(1);
        PublicEventEnvelope {
            schema_version: PUBLIC_EVENT_SCHEMA_VERSION,
            run_id: self.run_id.clone(),
            seq: self.next_seq,
            timestamp: chrono::Utc::now().to_rfc3339(),
            event,
        }
    }
}

async fn receive_output(
    output_rx: &mut Option<mpsc::Receiver<PublicEvent>>,
) -> Option<PublicEvent> {
    match output_rx {
        Some(receiver) => receiver.recv().await,
        None => std::future::pending().await,
    }
}

pub async fn dispatch_pipeline_events(
    mut event_rx: mpsc::UnboundedReceiver<PipelineEvent>,
    ui_tx: mpsc::UnboundedSender<PipelineEvent>,
    publisher: Option<PublicEventPublisher>,
) {
    while let Some(event) = event_rx.recv().await {
        if let Some(publisher) = &publisher {
            publisher.publish_internal(&event);
        }
        let _ = ui_tx.send(event);
    }
}

pub async fn write_ndjson(mut subscriber: PublicEventSubscriber) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;

    let mut stdout = tokio::io::stdout();
    while let Some(envelope) = subscriber.recv().await {
        let mut line = serde_json::to_vec(&envelope).map_err(std::io::Error::other)?;
        line.push(b'\n');
        stdout.write_all(&line).await?;
        stdout.flush().await?;
    }
    Ok(())
}

pub fn new_run_id() -> String {
    use std::sync::atomic::AtomicU64;
    use std::time::{SystemTime, UNIX_EPOCH};
    if let Some(run_id) = std::env::var_os("EVIN_RUN_ID")
        .and_then(|value| value.into_string().ok())
        .filter(|value| !value.trim().is_empty())
    {
        return run_id;
    }
    static NEXT_RUN_ID: AtomicU64 = AtomicU64::new(1);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = NEXT_RUN_ID.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}-{:x}", std::process::id(), timestamp, sequence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn projected_events_use_the_versioned_kind_payload_shape() {
        let event = project_event(&PipelineEvent::PhaseStarted(
            "ui.phase.resolving_configuration".into(),
        ))
        .unwrap();
        let envelope = PublicEventEnvelope {
            schema_version: PUBLIC_EVENT_SCHEMA_VERSION,
            run_id: "run-1".into(),
            seq: 1,
            timestamp: "2026-11-16T00:00:00Z".into(),
            event,
        };
        let value = serde_json::to_value(envelope).unwrap();
        assert_eq!(value["kind"], json!("phase_started"));
        assert_eq!(value["payload"]["phase"], json!("resolving-configuration"));
        assert_eq!(value["schema_version"], json!(1));
    }

    #[tokio::test]
    async fn output_is_opt_in_and_dropped_lines_are_reported_in_sequence() {
        let (publisher, mut subscriber) = public_event_channel("run-1".into(), true);
        for index in 0..(DEFAULT_OUTPUT_BUFFER + 8) {
            publisher.publish(PublicEvent::TaskOutput {
                name: "build".into(),
                stream: PublicOutputStream::Stdout,
                line: index.to_string(),
            });
        }
        publisher.publish(PublicEvent::PhaseStarted {
            phase: "build".into(),
        });
        drop(publisher);

        let mut events = Vec::new();
        while let Some(envelope) = subscriber.recv().await {
            events.push(envelope);
        }
        assert_eq!(events.last().unwrap().seq as usize, events.len());
        assert!(
            events
                .iter()
                .any(|envelope| matches!(envelope.event, PublicEvent::OutputDropped { count: 8 }))
        );
        assert!(
            events
                .iter()
                .any(|envelope| matches!(envelope.event, PublicEvent::PhaseStarted { .. }))
        );

        let (publisher, mut subscriber) = public_event_channel("run-2".into(), false);
        publisher.publish(PublicEvent::TaskOutput {
            name: "build".into(),
            stream: PublicOutputStream::Stdout,
            line: "hidden".into(),
        });
        publisher.publish(PublicEvent::PhaseStarted {
            phase: "build".into(),
        });
        drop(publisher);
        let event = subscriber.recv().await.unwrap();
        assert!(matches!(event.event, PublicEvent::PhaseStarted { .. }));
        assert!(subscriber.recv().await.is_none());
    }
}
