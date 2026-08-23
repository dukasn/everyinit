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

use crate::v1::{DocsConfig, debug::DocsDebugStats, parse_docs, render_docs, source_fingerprint};
use evin_diagnostic::{Diagnostic, IssueCode};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

pub struct DevServer;

type SourceFingerprint = Vec<(PathBuf, Option<std::time::SystemTime>, u64)>;

fn changed_source_label(
    before: &SourceFingerprint,
    after: &SourceFingerprint,
    source_dir: &str,
) -> String {
    let changed: BTreeSet<_> = after
        .iter()
        .filter(|entry| !before.contains(entry))
        .chain(before.iter().filter(|entry| !after.contains(entry)))
        .map(|entry| PathBuf::from(source_dir).join(&entry.0))
        .collect();
    match changed.len() {
        0 => source_dir.to_string(),
        1 => changed.into_iter().next().unwrap().display().to_string(),
        count => format!("{count} files in {source_dir}"),
    }
}

impl DevServer {
    pub(crate) async fn start(
        out_dir: PathBuf,
        workspace_root: PathBuf,
        config: DocsConfig,
        logger: evin_action_sdk::Logger,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
        start_instant: std::time::Instant,
        mut stats: DocsDebugStats,
    ) -> Result<(), Diagnostic> {
        let mut port = config.port;
        let listener = loop {
            match TcpListener::bind(format!("127.0.0.1:{port}")).await {
                Ok(listener) => break listener,
                Err(_) if port < config.port + 50 => port += 1,
                Err(_) => {
                    return Err(Diagnostic::new(
                        "Could not bind dev server to any available port",
                        IssueCode::new("issue-docs-server-bind-failed"),
                    ));
                }
            }
        };
        logger.daemon_started(
            start_instant.elapsed(),
            evin_action_sdk::activity::DocsServerRunning {
                url: format!("http://127.0.0.1:{port}/"),
            },
        );
        stats.emit_serve(&logger);

        let externally_cancelled = cancel_token.is_some();
        let shutdown = cancel_token.unwrap_or_default();
        let (reload_tx, _) = broadcast::channel::<()>(16);
        let reload_tx = Arc::new(reload_tx);
        let watcher_config = config.clone();
        let watcher_root = workspace_root.clone();
        let watcher_logger = logger.clone();
        let watcher_reload = reload_tx.clone();
        let watcher_cancel = shutdown.clone();
        let watcher_handle = tokio::spawn(async move {
            let mut fingerprint = source_fingerprint(&watcher_root, &watcher_config);
            watcher_logger.daemon_status(evin_action_sdk::activity::DocsWatching {});
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(500)) => {}
                    _ = watcher_cancel.cancelled() => break,
                }
                let next = source_fingerprint(&watcher_root, &watcher_config);
                if next == fingerprint {
                    continue;
                }
                let changed = changed_source_label(&fingerprint, &next, &watcher_config.source_dir);
                fingerprint = next;
                stats.changes += 1;
                stats.rebuilds += 1;
                evin_action_sdk::action_event!(
                    watcher_logger,
                    evin_action_sdk::activity::DocsChanged { path: changed }
                );
                watcher_logger.daemon_status(evin_action_sdk::activity::DocsRebuilding {});
                let rebuild_started = std::time::Instant::now();
                let rebuild =
                    parse_docs(&watcher_root, &watcher_config, &watcher_logger, &mut stats)
                        .and_then(|site_ast| {
                            render_docs(
                                &watcher_root,
                                &watcher_config,
                                &site_ast,
                                &watcher_logger,
                                &mut stats,
                            )
                        });
                stats.last_duration_ms = rebuild_started.elapsed().as_millis().max(1);
                match rebuild {
                    Ok(_) => {
                        stats.succeeded += 1;
                        stats.reloads += 1;
                        stats.emit_serve(&watcher_logger);
                        let elapsed = stats.last_duration_ms;
                        evin_action_sdk::action_event!(
                            watcher_logger,
                            evin_action_sdk::activity::DocsRebuildSucceeded {
                                duration_ms: elapsed,
                            }
                        );
                        let _ = watcher_reload.send(());
                        evin_action_sdk::action_event!(
                            watcher_logger,
                            evin_action_sdk::activity::DocsReloaded {}
                        );
                        watcher_logger
                            .daemon_status(evin_action_sdk::activity::DocsWatchingSucceeded {});
                    }
                    Err(error) => {
                        stats.failed += 1;
                        stats.emit_output(&watcher_logger);
                        stats.emit_serve(&watcher_logger);
                        evin_action_sdk::action_event!(
                            watcher_logger,
                            evin_action_sdk::activity::DocsRebuildFailed {
                                reason: error.title,
                            }
                        );
                        watcher_logger
                            .daemon_status(evin_action_sdk::activity::DocsWatchingFailed {});
                    }
                }
            }
        });

        let listener_cancel = shutdown.clone();
        let listener_reload = reload_tx.clone();
        let listener_handle = tokio::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            loop {
                let accepted = tokio::select! {
                    accepted = listener.accept() => accepted,
                    _ = listener_cancel.cancelled() => break,
                };
                let Ok((socket, _)) = accepted else {
                    continue;
                };
                connections.spawn(serve_connection(
                    socket,
                    out_dir.clone(),
                    listener_reload.subscribe(),
                    listener_cancel.clone(),
                ));
            }
            connections.abort_all();
            while connections.join_next().await.is_some() {}
        });

        if externally_cancelled {
            shutdown.cancelled().await;
        } else {
            let _ = tokio::signal::ctrl_c().await;
            shutdown.cancel();
        }
        let _ = tokio::join!(watcher_handle, listener_handle);
        logger.daemon_stopped(start_instant.elapsed(), false);
        Ok(())
    }
}

async fn serve_connection(
    mut socket: tokio::net::TcpStream,
    out_dir: PathBuf,
    mut reload_rx: broadcast::Receiver<()>,
    cancel_token: tokio_util::sync::CancellationToken,
) {
    let mut buffer = [0u8; 2048];
    let read = tokio::select! {
        read = socket.read(&mut buffer) => read,
        _ = cancel_token.cancelled() => return,
    };
    let Ok(read) = read else {
        return;
    };
    if read == 0 {
        return;
    }
    let request = String::from_utf8_lossy(&buffer[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    if path == "/__evin_reload" {
        let response = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\nevent: connected\ndata: {}\n\n";
        if socket.write_all(response.as_bytes()).await.is_err() {
            return;
        }
        loop {
            tokio::select! {
                event = reload_rx.recv() => if event.is_ok() { if socket.write_all(b"event: reload\ndata: {}\n\n").await.is_err() { break; } } else { break; },
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(15)) => if socket.write_all(b": keepalive\n\n").await.is_err() { break; },
                _ = cancel_token.cancelled() => break,
            }
        }
        return;
    }
    let clean_path = path.trim_start_matches('/');
    let direct = out_dir.join(clean_path);
    let file = if clean_path.is_empty() {
        out_dir.join("index.html")
    } else if direct.is_file() {
        direct
    } else {
        out_dir.join(clean_path).join("index.html")
    };
    let Ok(contents) = std::fs::read(&file) else {
        let _ = socket
            .write_all(b"HTTP/1.1 404 NOT FOUND\r\nContent-Length: 9\r\n\r\nNot Found")
            .await;
        return;
    };
    let mime = match file
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
    {
        "css" => "text/css",
        "js" => "application/javascript",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "jpg" | "jpeg" => "image/jpeg",
        "webmanifest" => "application/manifest+json",
        _ => "text/html",
    };
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        contents.len()
    );
    let _ = socket.write_all(header.as_bytes()).await;
    let _ = socket.write_all(&contents).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(path: &str, size: u64) -> (PathBuf, Option<std::time::SystemTime>, u64) {
        (PathBuf::from(path), None, size)
    }

    #[test]
    fn changed_source_label_reports_a_single_modified_path_once() {
        let before = vec![fingerprint("index.rst", 10)];
        let after = vec![fingerprint("index.rst", 20)];

        assert_eq!(
            changed_source_label(&before, &after, "docs"),
            "docs/index.rst"
        );
    }

    #[test]
    fn changed_source_label_summarizes_multiple_paths() {
        let before = vec![fingerprint("index.rst", 10)];
        let after = vec![fingerprint("guide.rst", 20), fingerprint("index.rst", 20)];

        assert_eq!(
            changed_source_label(&before, &after, "docs"),
            "2 files in docs"
        );
    }
}
