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

use crate::engine::PipelineCompletion;
#[cfg(unix)]
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
#[cfg(not(unix))]
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CleanupOutcome {
    Graceful,
    TimedOut,
    Forced,
}

pub(crate) async fn finish_session(
    session: &mut tokio::task::JoinHandle<PipelineCompletion>,
    signals: &mut SignalCoordinator,
) -> CleanupOutcome {
    finish_session_until(
        session,
        signals,
        tokio::time::Instant::now() + Duration::from_secs(2),
    )
    .await
}

pub(crate) async fn finish_session_until(
    session: &mut tokio::task::JoinHandle<PipelineCompletion>,
    signals: &mut SignalCoordinator,
    deadline: tokio::time::Instant,
) -> CleanupOutcome {
    tokio::select! {
        _ = &mut *session => CleanupOutcome::Graceful,
        _ = tokio::time::sleep_until(deadline) => {
            session.abort();
            let _ = (&mut *session).await;
            CleanupOutcome::TimedOut
        }
        _ = signals.next() => {
            session.abort();
            let _ = (&mut *session).await;
            CleanupOutcome::Forced
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestartCleanup {
    Continue,
    Terminate { forced: bool },
}

pub(crate) async fn finish_restart_session(
    session: &mut tokio::task::JoinHandle<PipelineCompletion>,
    summarize_cancelled_session: &AtomicBool,
    signals: &mut SignalCoordinator,
) -> RestartCleanup {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    tokio::select! {
        _ = &mut *session => RestartCleanup::Continue,
        _ = tokio::time::sleep_until(deadline) => {
            session.abort();
            let _ = (&mut *session).await;
            RestartCleanup::Continue
        }
        _ = signals.next() => {
            // This is the first signal: disable the pending restart, then use
            // the remaining grace window so a second signal can force cleanup.
            summarize_cancelled_session.store(true, Ordering::Relaxed);
            let outcome = finish_session_until(session, signals, deadline).await;
            RestartCleanup::Terminate { forced: outcome != CleanupOutcome::Graceful }
        }
    }
}

pub(crate) struct SignalCoordinator {
    #[cfg(unix)]
    pending: Arc<AtomicBool>,
    #[cfg(unix)]
    registrations: Vec<signal_hook::SigId>,
    #[cfg(not(unix))]
    task: tokio::task::JoinHandle<()>,
    #[cfg(not(unix))]
    receiver: mpsc::UnboundedReceiver<()>,
}

impl SignalCoordinator {
    pub(crate) fn install() -> Self {
        #[cfg(unix)]
        {
            use signal_hook::consts::signal::{SIGINT, SIGTERM};
            let pending = Arc::new(AtomicBool::new(false));
            let registrations = [SIGINT, SIGTERM]
                .into_iter()
                .map(|signal| {
                    signal_hook::flag::register(signal, pending.clone())
                        .expect("termination signal handler")
                })
                .collect();

            Self {
                pending,
                registrations,
            }
        }

        #[cfg(not(unix))]
        {
            let (sender, receiver) = mpsc::unbounded_channel();
            let signal_sender = sender.clone();
            let task = tokio::spawn(async move {
                while tokio::signal::ctrl_c().await.is_ok() {
                    if signal_sender.send(()).is_err() {
                        break;
                    }
                }
            });
            Self { task, receiver }
        }
    }

    pub(crate) async fn next(&mut self) {
        #[cfg(unix)]
        loop {
            if self.pending.swap(false, Ordering::SeqCst) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        #[cfg(not(unix))]
        let _ = self.receiver.recv().await;
    }

    #[cfg(all(test, unix))]
    pub(crate) fn inject(&self) {
        self.pending.store(true, Ordering::SeqCst);
    }
}

#[cfg(unix)]
impl Drop for SignalCoordinator {
    fn drop(&mut self) {
        for registration in self.registrations.drain(..) {
            signal_hook::low_level::unregister(registration);
        }
    }
}

#[cfg(not(unix))]
impl Drop for SignalCoordinator {
    fn drop(&mut self) {
        self.task.abort();
    }
}
