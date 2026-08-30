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

use async_trait::async_trait;
use console::{Style, StyledObject, Term};
use evin_diagnostic::Severity;
use std::fmt::Debug;
use tokio::io::AsyncWriteExt;

// --- Constants ---
pub const FALLBACK_WIDTH: usize = 80;
pub const MIN_CONTENT_WIDTH: usize = 20;
pub const FRAME_PREFIX_WIDTH: usize = 4;
pub const DIAGNOSTIC_GUTTER_RESERVE: usize = 12;

pub const SYMBOL_FRAME_BAR: &str = "│";
pub const SYMBOL_INTRO: &str = "╭";
pub const SYMBOL_OUTRO: &str = "╰";
pub const SYMBOL_STEP: &str = "◇";
pub const SYMBOL_DEBUG: &str = "~";
pub const SYMBOL_INFO: &str = "ℹ";
pub const SYMBOL_WARN: &str = "!";
pub const SYMBOL_ERROR: &str = "×";
pub const SYMBOL_SUCCESS: &str = "✓";
pub const SYMBOL_DAEMON: &str = "∞";
pub const SYMBOL_REMARK: &str = "·";
pub const SYMBOL_FENCED_TOP: &str = "╭─";
pub const SYMBOL_FENCED_BOTTOM: &str = "╰─";
pub const SYMBOL_FENCED_TASK_PREFIX: &str = "(task) ↩ ";
pub const SYMBOL_FENCED_CMD_PREFIX: &str = "(cmd) ↩ ";
pub const SYMBOL_FENCED_FILE_PREFIX: &str = "(file) ↩ ";
pub const SYMBOL_FENCED_HOOK_PREFIX: &str = "(hook) ↩ ";
pub const SYMBOL_FENCED_ACTION_PREFIX: &str = "(action) ↩ ";
pub const SYMBOL_FENCED_ERROR_PREFIX: &str = "(error) ↩ ";
pub const SYMBOL_FENCED_RESULT_PREFIX: &str = "(action) ↩ ";
pub const SYMBOL_PROMPT: &str = "?";
pub const SYMBOL_POINTER: &str = "›";
pub const SYMBOL_DISCOVERY_TARGET: &str = "»";
pub const SYMBOL_SELECT_POINTER: &str = "❯ ";
pub const SYMBOL_CHECKBOX_CHECKED: &str = "[x] ";
pub const SYMBOL_CHECKBOX_UNCHECKED: &str = "[ ] ";
pub const SYMBOL_PARALLEL: &str = "⇄";
pub const SYMBOL_SEQUENTIAL: &str = "→";
pub const SYMBOL_SPINNER_RUNNING_1: &str = "⟳";
pub const SYMBOL_SPINNER_RUNNING_2: &str = "↻";

pub const SPINNER_FRAMES: &[&str] = &["◇", "◆", "◇", "◆"];
pub const SPINNER_INTERVAL_MS: u64 = 80;

pub const COLOR_DIM: u8 = 244;

pub const UI_TICK_INTERVAL_MS: u64 = 50;
pub const UI_OUTPUT_MIN_DELAY_MS: u64 = 200;
pub const SPINNER_TEXT_MIN_LIFETIME_MS: u64 = 300;

// --- Modules ---
pub mod layout;
pub mod logger;
pub mod spinner;

pub(crate) use layout::Layout;
pub(crate) use logger::Logger;
pub(crate) use spinner::Spinner;

// --- Output Backend ---
#[async_trait]
pub trait OutputBackend: Send + Sync + Debug {
    async fn write_line(&self, text: &str);
    async fn write_str(&self, text: &str);
    async fn clear_line(&self);
    /// Clear the requested number of already-rendered terminal rows and return
    /// the cursor to the first cleared row. Implementations must be a no-op
    /// when their output is not an interactive terminal.
    async fn clear_previous_lines(&self, lines: usize);
    fn is_terminal(&self) -> bool;
    async fn flush(&self);
}

#[derive(Debug, Clone)]
pub struct StderrBackend;

#[async_trait]
impl OutputBackend for StderrBackend {
    async fn write_line(&self, text: &str) {
        let mut stderr = tokio::io::stderr();
        let _ = stderr.write_all(format!("{}\n", text).as_bytes()).await;
        let _ = stderr.flush().await;
    }

    async fn write_str(&self, text: &str) {
        let mut stderr = tokio::io::stderr();
        let _ = stderr.write_all(text.as_bytes()).await;
    }

    async fn clear_line(&self) {
        if !self.is_terminal() {
            return;
        }
        let mut stderr = tokio::io::stderr();
        let _ = stderr.write_all(b"\r\x1B[2K").await;
        let _ = stderr.flush().await;
    }

    async fn clear_previous_lines(&self, lines: usize) {
        if !self.is_terminal() || lines == 0 {
            return;
        }
        let mut stderr = tokio::io::stderr();
        for _ in 0..lines {
            let _ = stderr.write_all(b"\x1B[1A\x1B[2K\r").await;
        }
        let _ = stderr.flush().await;
    }

    fn is_terminal(&self) -> bool {
        console::Term::stderr().is_term()
    }

    async fn flush(&self) {
        let mut stderr = tokio::io::stderr();
        let _ = stderr.flush().await;
    }
}

// --- Mode ---
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    DryRun,
    Debug,
}

impl Mode {
    pub fn resolve(dry_run: bool, debug_enabled: bool) -> Self {
        if debug_enabled {
            Self::Debug
        } else if dry_run {
            Self::DryRun
        } else {
            Self::Normal
        }
    }

    pub fn format_title(self, title: &str) -> String {
        match self {
            Self::Normal => title.to_string(),
            Self::DryRun => format!(" DRY RUN ›{}", title),
            Self::Debug => format!(" DEBUG ›{}", title),
        }
    }

    pub fn title_style(self) -> Style {
        Style::new()
            .on_color256(self.background_color())
            .true_color(0, 0, 0)
            .bold()
    }

    pub fn accent_style(self) -> Style {
        match self {
            Self::Normal => Style::new().green(),
            Self::DryRun => Style::new().color256(75),
            Self::Debug => Style::new().color256(170),
        }
    }

    fn background_color(self) -> u8 {
        match self {
            Self::Normal => 120,
            Self::DryRun => 75,
            Self::Debug => 170,
        }
    }

    pub fn spinner_style(self) -> &'static str {
        match self {
            Self::Normal => "green",
            Self::DryRun => "75",
            Self::Debug => "170",
        }
    }
}

// --- Theme ---
pub struct Theme {
    pub frame_color: Style,
    pub error_color: Style,
    pub warn_color: Style,
    pub info_color: Style,
    pub success_color: Style,
    pub remark_color: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            frame_color: Style::new().color256(COLOR_DIM),
            error_color: Style::new().red(),
            warn_color: Style::new().yellow(),
            info_color: Style::new().blue(),
            success_color: Style::new().green(),
            remark_color: Style::new().color256(COLOR_DIM),
        }
    }
}

impl Theme {
    fn frame_style() -> Style {
        Style::new().color256(COLOR_DIM)
    }

    pub fn frame(symbol: &str) -> String {
        Self::frame_style().apply_to(symbol).to_string()
    }

    pub fn embed(lines: &[String]) -> Vec<String> {
        let bar = Self::frame(SYMBOL_FRAME_BAR);
        lines.iter().map(|line| format!("{bar}  {line}")).collect()
    }

    pub fn remark_symbol() -> String {
        Self::frame_style().apply_to(SYMBOL_REMARK).to_string()
    }

    pub fn severity_style(severity: Severity, text: &str) -> StyledObject<String> {
        let s = console::style(text.to_string());
        match severity {
            Severity::Error => s.red(),
            Severity::Warning => s.yellow(),
            Severity::Info => s.blue(),
        }
    }

    pub fn severity_icon(severity: Severity) -> &'static str {
        match severity {
            Severity::Error => SYMBOL_ERROR,
            Severity::Warning => SYMBOL_WARN,
            Severity::Info => SYMBOL_INFO,
        }
    }
}

// --- Formatting Utils ---
pub fn format_duration(secs: f64) -> String {
    human_format::Formatter::new()
        .with_scales(human_format::Scales::Time())
        .with_decimals(1)
        .format(secs)
        .replace(" ", "")
}

pub fn dim(value: impl ToString) -> String {
    console::style(value.to_string())
        .color256(COLOR_DIM)
        .to_string()
}

pub fn highlight(value: impl ToString) -> String {
    console::style(value.to_string()).cyan().to_string()
}

pub fn suggestion(value: impl ToString) -> String {
    console::style(value.to_string()).cyan().bold().to_string()
}

pub fn wrap_text(text: &str) -> String {
    Layout::current().wrap_content(text)
}

pub fn summary(mode: Mode, title: impl ToString, details: &[String]) {
    Term::stderr()
        .write_str(&summary_text(Layout::current(), mode, title, details))
        .ok();
}

pub fn summary_text(
    layout: Layout,
    mode: Mode,
    title: impl ToString,
    details: &[String],
) -> String {
    summary_lines(layout, mode, title, details).join("\n")
}

pub fn summary_lines(
    layout: Layout,
    mode: Mode,
    title: impl ToString,
    details: &[String],
) -> Vec<String> {
    let wrapped_title = layout.wrap_content(&title.to_string());
    let mut title_lines = wrapped_title.lines();
    let mut output = vec![format!(
        "{}  {}",
        mode.accent_style().apply_to(SYMBOL_STEP),
        title_lines.next().unwrap_or_default()
    )];
    output.extend(title_lines.map(|line| format!("{}  {line}", Theme::frame(SYMBOL_FRAME_BAR))));

    let frame = Theme::frame(SYMBOL_FRAME_BAR);
    if !details.is_empty() {
        output.push(format!("{frame}  "));
        for detail in details {
            for line in layout.wrap_content(detail).lines() {
                output.push(format!("{frame}    • {line}"));
            }
        }
    }

    output.push(Theme::frame(SYMBOL_FRAME_BAR));
    output
}

pub fn format_module_paths(modules: &[String]) -> String {
    use std::collections::HashMap;
    use std::path::Path;

    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    for m in modules {
        let path = Path::new(m);
        let parent = path
            .parent()
            .unwrap_or(Path::new(""))
            .to_string_lossy()
            .to_string();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        groups.entry(parent).or_default().push(name);
    }

    let mut formatted = Vec::new();
    for (parent, mut names) in groups {
        names.sort();
        if parent.is_empty() {
            for name in names {
                formatted.push(name);
            }
        } else if names.len() == 1 {
            formatted.push(format!("{}/{}", parent, names[0]));
        } else {
            formatted.push(format!("{}/{{{}}}", parent, names.join(", ")));
        }
    }
    formatted.sort();
    formatted.join(", ")
}
