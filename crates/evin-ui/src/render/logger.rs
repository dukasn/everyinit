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
use std::fmt::Display;

pub struct Logger {
    layout: Layout,
    theme: Theme,
    backend: std::sync::Arc<dyn OutputBackend>,
}

impl Logger {
    pub fn new(layout: Layout, backend: std::sync::Arc<dyn OutputBackend>) -> Self {
        Self {
            layout,
            theme: Theme::default(),
            backend,
        }
    }

    fn preserves_ansi(&self) -> bool {
        self.backend.is_terminal() || std::env::var_os("EVIN_TAPE").is_some()
    }

    async fn write_line(&self, text: &str) {
        if self.preserves_ansi() {
            self.backend.write_line(text).await;
        } else {
            self.backend
                .write_line(console::strip_ansi_codes(text).as_ref())
                .await;
        }
    }

    async fn write_str(&self, text: &str) {
        if self.preserves_ansi() {
            self.backend.write_str(text).await;
        } else {
            self.backend
                .write_str(console::strip_ansi_codes(text).as_ref())
                .await;
        }
    }

    async fn print_prefix(&self, prefix: &str, msg: &str) {
        let wrapped = self.layout.wrap_lines(msg);
        let mut is_first = true;
        let frame = self
            .theme
            .frame_color
            .apply_to(SYMBOL_FRAME_BAR)
            .to_string();
        for line in wrapped {
            if is_first {
                self.write_line(&format!("{}  {}", prefix, line)).await;
                is_first = false;
            } else {
                self.write_line(&format!("{}  {}", frame, line)).await;
            }
        }
    }

    pub async fn header(&self, version: &str, mode: &str, os: &str, arch: &str, shell: &str) {
        let dim = console::Style::new().color256(244);
        let line1 = format!(
            "{} v{} · {} {}",
            dim.apply_to("evin:"),
            version,
            dim.apply_to("mode:"),
            mode
        );
        let line2 = format!(
            "{} {} ({}) · {} {}",
            dim.apply_to("system:"),
            os,
            arch,
            dim.apply_to("shell:"),
            shell
        );
        self.write_line(&line1).await;
        self.write_line(&format!("{}\n", line2)).await;
        self.backend.flush().await;
    }

    pub async fn workspace_header(&self, version: &str, workspace: &str) {
        let dim = console::Style::new().color256(244);
        self.write_line(&format!(
            "{} {} · {} {}",
            dim.apply_to("evin:"),
            version,
            dim.apply_to("workspace:"),
            workspace
        ))
        .await;
        self.write_line("").await;
        self.backend.flush().await;
    }

    pub async fn footer(&self, command: &str, flags: &[String]) {
        let dim = console::Style::new().color256(244);
        self.write_line("").await;
        self.write_line(&format!("{} {}", dim.apply_to("execution:"), command))
            .await;
        if !flags.is_empty() {
            self.write_line(&format!("{} {}", dim.apply_to("flags:"), flags.join(", ")))
                .await;
        }
        self.backend.flush().await;
    }

    pub async fn intro<T: Display>(&self, title: T) {
        let prefix = self.theme.frame_color.apply_to(SYMBOL_INTRO).to_string();
        self.print_prefix(&prefix, &title.to_string()).await;
        let frame = self
            .theme
            .frame_color
            .apply_to(SYMBOL_FRAME_BAR)
            .to_string();
        self.write_line(&frame).await;
        self.backend.flush().await;
    }

    pub async fn outro<T: Display>(&self, message: T) {
        let prefix = self.theme.frame_color.apply_to(SYMBOL_OUTRO).to_string();
        self.print_prefix(&prefix, &message.to_string()).await;
        self.backend.flush().await;
    }

    pub async fn outro_cancel<T: Display>(&self, message: T) {
        let prefix = self.theme.frame_color.apply_to(SYMBOL_OUTRO).to_string();
        self.print_prefix(
            &prefix,
            &self
                .theme
                .error_color
                .apply_to(message.to_string())
                .to_string(),
        )
        .await;
        self.backend.flush().await;
    }

    async fn emit(&self, symbol: &str, color: &console::Style, message: &str) {
        let prefix = color.apply_to(symbol).to_string();
        self.print_prefix(&prefix, message).await;
        self.bar().await;
    }

    pub async fn step<T: Display>(&self, message: T) {
        self.emit(SYMBOL_STEP, &self.theme.success_color, &message.to_string())
            .await;
    }

    pub async fn summary(&self, title: &str, items: &[String]) {
        for line in crate::render::summary_lines(self.layout, Mode::Normal, title, items) {
            self.write_line(&line).await;
        }
        self.backend.flush().await;
    }

    pub async fn info<T: Display>(&self, message: T) {
        self.emit(SYMBOL_INFO, &self.theme.info_color, &message.to_string())
            .await;
    }

    pub async fn warn<T: Display>(&self, message: T) {
        self.emit(SYMBOL_WARN, &self.theme.warn_color, &message.to_string())
            .await;
    }

    pub async fn error<T: Display>(&self, message: T) {
        self.emit(SYMBOL_ERROR, &self.theme.error_color, &message.to_string())
            .await;
    }

    pub async fn error_aborted_step<T: Display>(&self, message: T) {
        let dim_style = console::Style::new().color256(crate::constants::COLOR_DIM);
        let suffix = dim_style
            .apply_to(format!("{}  aborted", crate::constants::SYMBOL_SEQUENTIAL))
            .to_string();
        self.emit(
            SYMBOL_ERROR,
            &self.theme.error_color,
            &format!("{}  {}", message, suffix),
        )
        .await;
    }

    pub async fn success<T: Display>(&self, message: T) {
        self.emit(
            SYMBOL_SUCCESS,
            &self.theme.success_color,
            &message.to_string(),
        )
        .await;
    }

    pub async fn debug(&self, messages: &[String]) {
        if messages.is_empty() {
            return;
        }
        let debug_sym = self.theme.info_color.apply_to(SYMBOL_DEBUG).to_string();
        let debug_text = self.theme.info_color.apply_to("debug").to_string();
        let dim_style = console::Style::new().color256(crate::constants::COLOR_DIM);

        if !self.backend.is_terminal() {
            for (index, msg) in messages.iter().enumerate() {
                let styled_line = dim_style.apply_to(msg).to_string();
                if index == 0 {
                    self.write_line(&format!("{}   {}  {}", debug_sym, debug_text, styled_line))
                        .await;
                } else {
                    let frame = self
                        .theme
                        .frame_color
                        .apply_to(SYMBOL_FRAME_BAR)
                        .to_string();
                    self.write_line(&format!("{}         {}", frame, styled_line))
                        .await;
                }
            }
            self.bar().await;
            return;
        }

        let mut first = true;
        for msg in messages {
            // Reserve the width occupied by `~   debug  `. Explicit wrapping
            // keeps every continuation inside the pipeline gutter.
            let wrapped = self.layout.wrap_with_margin(msg, 8);
            for line in wrapped {
                let styled_line = dim_style.apply_to(line).to_string();
                if first {
                    self.write_line(&format!("{}   {}  {}", debug_sym, debug_text, styled_line))
                        .await;
                    first = false;
                } else {
                    let frame = self
                        .theme
                        .frame_color
                        .apply_to(SYMBOL_FRAME_BAR)
                        .to_string();
                    self.write_line(&format!("{}         {}", frame, styled_line))
                        .await;
                }
            }
        }
        self.bar().await;
    }

    pub async fn remark<T: Display>(&self, message: T) {
        let prefix = self.theme.remark_color.apply_to(SYMBOL_REMARK).to_string();
        self.write_line(&format!("{}   {}", prefix, message)).await;
        self.bar().await;
    }

    pub async fn daemon_started(&self, name: &str, duration_str: &str) {
        let daemon_sym = self.theme.success_color.apply_to(SYMBOL_DAEMON).to_string();
        let arrow = self
            .theme
            .frame_color
            .apply_to(SYMBOL_SEQUENTIAL)
            .to_string();
        let dim = console::Style::new().color256(COLOR_DIM);
        self.write_line(&format!(
            "{} {} {}  {}  {}",
            daemon_sym,
            dim.apply_to("daemon -"),
            name,
            arrow,
            dim.apply_to(duration_str)
        ))
        .await;
        self.bar().await;
    }

    pub async fn daemon_status_line(&self, frame_symbol: &str, name: &str, elapsed_str: &str) {
        let sym = self.theme.success_color.apply_to(frame_symbol).to_string();
        let arrow = self
            .theme
            .frame_color
            .apply_to(SYMBOL_SEQUENTIAL)
            .to_string();
        let dim = console::Style::new().color256(COLOR_DIM);
        // The transient status is cleared as one terminal row. Keep the fixed
        // lifecycle vocabulary and duration visible, cropping only the task
        // description when the terminal is narrow.
        let reserved_width = 16 + console::measure_text_width(elapsed_str);
        let name = self.layout.fit_single_line(name, reserved_width);
        let prefix = if self.backend.is_terminal() {
            "\r\x1b[2K"
        } else {
            ""
        };
        self.write_str(&format!(
            "{}{} {} {}  {}  {}",
            prefix,
            sym,
            dim.apply_to("daemon -"),
            name,
            arrow,
            dim.apply_to(elapsed_str)
        ))
        .await;
        self.backend.flush().await;
    }

    pub async fn daemon_stopped(&self, name: &str, duration_str: &str, failed: bool) {
        let prefix = if failed {
            self.theme.error_color.apply_to(SYMBOL_ERROR).to_string()
        } else {
            self.theme
                .success_color
                .apply_to(SYMBOL_SUCCESS)
                .to_string()
        };
        let arrow = self
            .theme
            .frame_color
            .apply_to(SYMBOL_SEQUENTIAL)
            .to_string();
        let dim = console::Style::new().color256(COLOR_DIM);
        self.write_line(&format!(
            "{} {} {}  {}  {}",
            prefix,
            dim.apply_to("daemon -"),
            name,
            arrow,
            dim.apply_to(duration_str)
        ))
        .await;
        self.bar().await;
    }

    pub async fn reloading(&self, name: &str, duration_str: &str) {
        let symbol = self
            .theme
            .success_color
            .apply_to(SYMBOL_SPINNER_RUNNING_2)
            .to_string();
        let arrow = self
            .theme
            .frame_color
            .apply_to(SYMBOL_SEQUENTIAL)
            .to_string();
        let dim = console::Style::new().color256(COLOR_DIM);
        self.write_line(&format!(
            "{} {} {}  {}  {}",
            symbol,
            dim.apply_to("reload -"),
            name,
            arrow,
            dim.apply_to(duration_str)
        ))
        .await;
        self.bar().await;
    }

    pub async fn bar(&self) {
        let frame = self
            .theme
            .frame_color
            .apply_to(SYMBOL_FRAME_BAR)
            .to_string();
        self.write_line(&frame).await;
        self.backend.flush().await;
    }

    pub async fn fenced(
        &self,
        title: &str,
        lines: &[String],
        is_error: bool,
        command: Option<&str>,
    ) {
        for line in self.fenced_lines(title, lines, is_error, command) {
            self.write_line(&line).await;
        }
        self.backend.flush().await;
    }

    pub(crate) fn fenced_lines(
        &self,
        title: &str,
        lines: &[String],
        is_error: bool,
        command: Option<&str>,
    ) -> Vec<String> {
        crate::render::fenced_lines(
            self.layout,
            &self.theme,
            title,
            lines,
            is_error.then_some(evin_diagnostic::Severity::Error),
            command,
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct RecordingBackend {
        lines: Mutex<Vec<String>>,
        terminal: bool,
    }

    #[async_trait]
    impl OutputBackend for RecordingBackend {
        async fn write_line(&self, text: &str) {
            self.lines.lock().unwrap().push(text.to_string());
        }

        async fn write_str(&self, text: &str) {
            self.lines.lock().unwrap().push(text.to_string());
        }

        async fn clear_line(&self) {}
        async fn clear_previous_lines(&self, _lines: usize) {}
        fn is_terminal(&self) -> bool {
            self.terminal
        }
        async fn flush(&self) {}
    }

    #[tokio::test]
    async fn debug_wraps_long_and_multiline_messages_inside_the_pipeline_gutter() {
        let backend = std::sync::Arc::new(RecordingBackend {
            terminal: true,
            ..Default::default()
        });
        let logger = Logger::new(Layout::with_width(48), backend.clone());
        logger
            .debug(&[format!(
                "[E_102] workspace_root=. config_origin={}\nwrite_permissions=writable",
                ".evin/a-very-long-configuration-directory/workspace.flow"
            )])
            .await;

        let lines = backend.lines.lock().unwrap();
        assert!(lines.len() > 3);
        assert!(lines[0].contains("debug"));
        assert!(
            lines[1..lines.len() - 1]
                .iter()
                .all(|line| console::strip_ansi_codes(line).starts_with("│         "))
        );
        assert!(
            lines
                .iter()
                .all(|line| console::measure_text_width(line) <= 48)
        );
    }

    #[tokio::test]
    async fn non_terminal_debug_keeps_each_record_on_one_physical_line() {
        let backend = std::sync::Arc::new(RecordingBackend::default());
        let logger = Logger::new(Layout::with_width(32), backend.clone());
        let record =
            "[E_102] workspace_root=. config_origin=.evin/a-very-long-directory/workspace.flow";
        logger.debug(&[record.to_string()]).await;

        let lines = backend.lines.lock().unwrap();
        assert_eq!(lines.len(), 2);
        assert!(console::strip_ansi_codes(&lines[0]).contains(record));
        assert!(lines.iter().all(|line| !line.contains('\u{1b}')));
    }

    #[tokio::test]
    async fn daemon_status_crops_long_descriptions_to_one_terminal_row() {
        let backend = std::sync::Arc::new(RecordingBackend {
            terminal: true,
            ..Default::default()
        });
        let logger = Logger::new(Layout::with_width(48), backend.clone());

        logger
            .daemon_status_line(
                "◆",
                "Serve an exceptionally long live-reloading documentation development server",
                "12.4s",
            )
            .await;

        let lines = backend.lines.lock().unwrap();
        assert_eq!(lines.len(), 1);
        let visible_line = lines[0].trim_start_matches("\r\x1b[2K");
        assert!(console::strip_ansi_codes(visible_line).contains("daemon -"));
        assert!(console::strip_ansi_codes(visible_line).contains("12.4s"));
        assert!(console::strip_ansi_codes(visible_line).contains('…'));
        assert!(console::measure_text_width(visible_line) <= 48);
    }

    #[tokio::test]
    async fn daemon_status_line_atomically_resets_and_clears_each_frame() {
        let backend = std::sync::Arc::new(RecordingBackend {
            terminal: true,
            ..Default::default()
        });
        let logger = Logger::new(Layout::default(), backend.clone());

        logger.daemon_status_line("◇", "cargo check", "0.1s").await;
        logger.daemon_status_line("◆", "cargo check", "0.2s").await;
        logger.daemon_status_line("◇", "cargo check", "0.3s").await;

        let lines = backend.lines.lock().unwrap();
        assert_eq!(lines.len(), 3);
        for line in lines.iter() {
            assert!(
                line.starts_with("\r\x1b[2K"),
                "Each frame must atomically start with carriage return and line clear escape sequence"
            );
        }
    }
}
