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
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;
use tokio::time::Duration;

pub struct Spinner {
    state: Arc<Mutex<SpinnerState>>,
    join_handle: Option<JoinHandle<()>>,
    backend: std::sync::Arc<dyn OutputBackend>,
    layout: Layout,
    output_lock: Arc<tokio::sync::Mutex<()>>,
}

struct SpinnerState {
    message: String,
    running: bool,
    frame_index: usize,
    cleared: bool,
    start_time: Option<std::time::Instant>,
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new(Layout::current())
    }
}

impl Spinner {
    pub fn new(layout: Layout) -> Self {
        Self::with_backend(layout, std::sync::Arc::new(StderrBackend))
    }

    pub fn with_backend(layout: Layout, backend: std::sync::Arc<dyn OutputBackend>) -> Self {
        let state = Arc::new(Mutex::new(SpinnerState {
            message: String::new(),
            running: false,
            frame_index: 0,
            cleared: false,
            start_time: None,
        }));

        Self {
            state,
            join_handle: None,
            backend,
            layout,
            output_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub fn is_running(&self) -> bool {
        if let Ok(state) = self.state.lock() {
            state.running
        } else {
            false
        }
    }

    pub fn start(&mut self, message: &str) {
        self.start_with_timer(message, std::time::Instant::now());
    }

    pub fn start_with_timer(&mut self, message: &str, start_time: std::time::Instant) {
        // A watch restart can start a new session on the same presenter.
        // Never leave the previous animation loop attached to shared state.
        if let Some(handle) = self.join_handle.take() {
            handle.abort();
        }

        {
            let mut state = self.state.lock().unwrap();
            state.message = message.to_string();
            state.running = true;
            state.cleared = false;
            state.start_time = Some(start_time);
        }

        if !self.backend.is_terminal() {
            return;
        }

        let _ = console::Term::stderr().hide_cursor();

        let state_clone = self.state.clone();
        let backend_clone = self.backend.clone();
        let output_lock = self.output_lock.clone();
        let layout = self.layout;
        self.join_handle = Some(tokio::spawn(async move {
            let theme = Theme::default();

            loop {
                tokio::time::sleep(Duration::from_millis(80)).await;

                let mut state_msg = String::new();
                let mut cleared = false;
                let mut frame_index = 0;
                let mut start_time = None;

                {
                    let mut state = state_clone.lock().unwrap();
                    if !state.running {
                        break;
                    }

                    if state.cleared {
                        cleared = true;
                    } else {
                        state_msg = state.message.clone();
                        frame_index = state.frame_index;
                        start_time = state.start_time;
                        state.frame_index += 1;
                    }
                }

                if cleared {
                    continue;
                }

                let frame = crate::constants::SPINNER_FRAMES
                    [frame_index % crate::constants::SPINNER_FRAMES.len()];

                let spinner_prefix = theme.success_color.apply_to(frame).to_string();
                let mut time_suffix = String::new();
                if let Some(start) = start_time {
                    let secs = start.elapsed().as_secs_f64();
                    let s = crate::utils::format_duration(secs);
                    let running_symbol = if frame_index.is_multiple_of(2) {
                        "⟳"
                    } else {
                        "↻"
                    };
                    time_suffix = format!(
                        "  {}  {}",
                        crate::utils::dim(running_symbol),
                        crate::utils::dim(s)
                    );
                }

                // Live output must occupy exactly one physical row because
                // redraws clear exactly one row. Reserve the symbol and timer
                // widths, then crop long task descriptions with an ellipsis.
                let reserved_width = 3 + console::measure_text_width(&time_suffix);
                let msg = layout.fit_single_line(&state_msg, reserved_width);

                let _output_guard = output_lock.lock().await;
                {
                    let state = state_clone.lock().unwrap();
                    if state.cleared || !state.running {
                        continue;
                    }
                }
                backend_clone.clear_line().await;
                backend_clone
                    .write_str(&format!("{}  {}{}", spinner_prefix, msg, time_suffix))
                    .await;
                backend_clone.flush().await;
            }
        }));
    }

    pub fn set_message(&self, message: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.message = message.to_string();
            state.cleared = false;
        }
    }

    pub fn set_message_with_timer(&self, message: &str, start_time: std::time::Instant) {
        if let Ok(mut state) = self.state.lock() {
            state.message = message.to_string();
            state.cleared = false;
            state.start_time = Some(start_time);
        }
    }

    pub async fn clear(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.cleared = true;
        }
        let _output_guard = self.output_lock.lock().await;
        self.backend.clear_line().await;
        self.backend.flush().await;
    }

    pub fn unclear(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.cleared = false;
        }
    }

    pub async fn stop(&mut self) {
        {
            if let Ok(mut state) = self.state.lock() {
                state.running = false;
            }
        }

        if let Some(handle) = self.join_handle.take() {
            let _ = handle.await;
        }

        if self.backend.is_terminal() {
            let _ = console::Term::stderr().show_cursor();
        }
        self.backend.clear_line().await;
        self.backend.flush().await;
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            state.running = false;
        }
        if let Some(handle) = self.join_handle.take() {
            handle.abort();
        }
        if self.backend.is_terminal() {
            let _ = console::Term::stderr().show_cursor();
        }
        let backend = self.backend.clone();
        tokio::spawn(async move {
            backend.clear_line().await;
        });
    }
}
