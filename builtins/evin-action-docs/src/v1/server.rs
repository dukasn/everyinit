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

use crate::v1::{DocsConfig, debug::DocsDebugStats, render_docs, source_fingerprint};
use evin_diagnostic::{Diagnostic, IssueCode};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

pub struct DevServer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServeHost {
    Loopback,
    AllInterfaces,
}

impl ServeHost {
    pub(crate) fn from_exposed(exposed: bool) -> Self {
        if exposed {
            Self::AllInterfaces
        } else {
            Self::Loopback
        }
    }

    fn address(self, port: u16) -> String {
        let host = match self {
            Self::Loopback => "127.0.0.1",
            Self::AllInterfaces => "0.0.0.0",
        };
        format!("{host}:{port}")
    }

    fn url(self, port: u16) -> String {
        format!("http://{}/", self.address(port))
    }

    pub(crate) fn network_url(self, port: u16) -> Option<String> {
        match self {
            Self::Loopback => None,
            Self::AllInterfaces => local_network_ip().map(|ip| format!("http://{ip}:{port}/")),
        }
    }
}

fn local_network_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    // Connect to an external address — no packets are sent; this just
    // causes the OS to select the preferred outbound interface.
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    let ip = addr.ip();
    if ip.is_loopback() { None } else { Some(ip) }
}

pub(crate) struct ServerBinding {
    listener: TcpListener,
    host: ServeHost,
    port: u16,
}

impl ServerBinding {
    pub(crate) fn url(&self) -> String {
        self.host.url(self.port)
    }

    pub(crate) fn network_url(&self) -> Option<String> {
        self.host.network_url(self.port)
    }
}

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
    pub(crate) async fn bind(
        host: ServeHost,
        first_port: u16,
    ) -> Result<ServerBinding, Diagnostic> {
        let mut port = first_port;
        let listener = loop {
            match TcpListener::bind(host.address(port)).await {
                Ok(listener) => break listener,
                Err(_) if port < first_port + 50 => port += 1,
                Err(_) => {
                    return Err(Diagnostic::new(
                        "Could not bind dev server to any available port",
                        IssueCode::new("issue-docs-server-bind-failed"),
                    ));
                }
            }
        };
        Ok(ServerBinding {
            listener,
            host,
            port,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn start(
        binding: ServerBinding,
        out_dir: PathBuf,
        workspace_root: PathBuf,
        config: DocsConfig,
        logger: evin_action_sdk::Logger,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
        start_instant: std::time::Instant,
        mut stats: DocsDebugStats,
    ) -> Result<(), Diagnostic> {
        let local_url = binding.url();
        let network_url = binding.network_url();
        logger.daemon_started(
            start_instant.elapsed(),
            evin_action_sdk::activity::DocsServerRunning {
                url: local_url.clone(),
            },
        );
        stats.port = binding.port;
        stats.network_url = network_url.clone();
        stats.emit_serve(&logger);
        let serve_ready_url = match &network_url {
            Some(lan) => format!("{local_url} · Network: {lan}"),
            None => local_url,
        };
        evin_action_sdk::action_event!(
            logger,
            evin_action_sdk::activity::DocsServeReady {
                url: serve_ready_url
            }
        );

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
                let changed = changed_source_label(
                    &fingerprint,
                    &next,
                    &watcher_config.source_dir.to_string_lossy(),
                );
                fingerprint = next;
                stats.changes += 1;
                stats.rebuilds += 1;
                evin_action_sdk::action_event!(
                    watcher_logger,
                    evin_action_sdk::activity::DocsChanged { path: changed }
                );
                watcher_logger.daemon_status(evin_action_sdk::activity::DocsRebuilding {});
                let rebuild_started = std::time::Instant::now();
                let rebuild = super::parse_configured_site(
                    &watcher_root,
                    &watcher_config,
                    &watcher_logger,
                    &mut stats,
                )
                .and_then(|site_ast| {
                    let output = render_docs(
                        &watcher_root,
                        &watcher_config,
                        &site_ast,
                        &watcher_logger,
                        &mut stats,
                    )?;
                    Ok(output)
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
                    accepted = binding.listener.accept() => accepted,
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
    let accept_gzip = request.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        lower.starts_with("accept-encoding:") && lower.contains("gzip")
    });
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
    let (mime, is_compressible) = resolve_mime(&file);
    let (body, is_gzipped) = if accept_gzip && is_compressible && contents.len() > 128 {
        use std::io::Write;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        if encoder.write_all(&contents).is_ok()
            && let Ok(compressed) = encoder.finish()
            && compressed.len() < contents.len()
        {
            (compressed, true)
        } else {
            (contents, false)
        }
    } else {
        (contents, false)
    };
    let header = if is_gzipped {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Encoding: gzip\r\nVary: Accept-Encoding\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
    } else {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nVary: Accept-Encoding\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
    };
    let _ = socket.write_all(header.as_bytes()).await;
    let _ = socket.write_all(&body).await;
}

fn resolve_mime(file: &std::path::Path) -> (&'static str, bool) {
    match file
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
    {
        "css" => ("text/css", true),
        "js" => ("application/javascript", true),
        "json" => ("application/json", true),
        "svg" => ("image/svg+xml", true),
        "html" | "htm" => ("text/html", true),
        "bin" => ("application/octet-stream", true),
        "png" => ("image/png", false),
        "avif" => ("image/avif", false),
        "ico" => ("image/x-icon", false),
        "jpg" | "jpeg" => ("image/jpeg", false),
        "woff2" => ("font/woff2", false),
        "woff" => ("font/woff", false),
        "webmanifest" => ("application/manifest+json", true),
        _ => ("text/html", true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(path: &str, size: u64) -> (PathBuf, Option<std::time::SystemTime>, u64) {
        (PathBuf::from(path), None, size)
    }

    #[test]
    fn mime_resolution_maps_bin_to_compressible_octet_stream() {
        let (mime, compressible) = resolve_mime(std::path::Path::new("assets/search-catalog.bin"));
        assert_eq!(mime, "application/octet-stream");
        assert!(compressible);

        let (mime, compressible) = resolve_mime(std::path::Path::new("assets/search-index.bin"));
        assert_eq!(mime, "application/octet-stream");
        assert!(compressible);
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
    fn serve_hosts_produce_the_bound_server_url() {
        assert_eq!(ServeHost::Loopback.address(3000), "127.0.0.1:3000");
        assert_eq!(ServeHost::Loopback.url(3001), "http://127.0.0.1:3001/");
        assert_eq!(ServeHost::AllInterfaces.address(3000), "0.0.0.0:3000");
        assert_eq!(ServeHost::AllInterfaces.url(3001), "http://0.0.0.0:3001/");
    }

    #[test]
    fn loopback_host_has_no_network_url() {
        assert_eq!(ServeHost::Loopback.network_url(3000), None);
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
