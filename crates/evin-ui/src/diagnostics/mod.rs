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

use console::{Term, style};
use evin_diagnostic::{Diagnostic, IssueCode, Severity};
use evin_io::SourceCache;

pub mod snippet;

use crate::render::{Layout, SYMBOL_ERROR, SYMBOL_INFO, SYMBOL_WARN, Theme};
use snippet::SnippetRenderer;

const BAR: &str = "│";

pub fn error(title: &str, message: &str, hint: Option<String>) {
    let mut diag = Diagnostic::new(title, IssueCode::custom("E_GENERIC"));
    if let Some(h) = hint {
        diag = diag.with_hint(h);
    }
    print_diagnostic(&diag, message, None);
}

pub fn print_diagnostic(diagnostic: &Diagnostic, message: &str, cache: Option<&SourceCache>) {
    let output = render_diagnostic(diagnostic, message, cache);
    let _ = Term::stderr().write_str(&output);
}

pub fn print_standalone_diagnostic(
    diagnostic: &Diagnostic,
    fallback_message: &str,
    cache: Option<&SourceCache>,
) {
    let output = render_standalone_diagnostic(diagnostic, fallback_message, cache);
    let _ = Term::stderr().write_str(&output);
}

pub fn render_standalone_diagnostic(
    diagnostic: &Diagnostic,
    fallback_message: &str,
    cache: Option<&SourceCache>,
) -> String {
    let mut lines = Vec::new();

    let (icon, label_key) = match diagnostic.severity {
        Severity::Error => (SYMBOL_ERROR, "error"),
        Severity::Warning => (SYMBOL_WARN, "warning"),
        Severity::Info => (SYMBOL_INFO, "info"),
    };

    let severity_styled = Theme::severity_style(diagnostic.severity, label_key).bold();
    let icon_styled = Theme::severity_style(diagnostic.severity, icon);

    let display_message = if diagnostic.title.is_empty() {
        fallback_message
    } else {
        &diagnostic.title
    };

    let header = format!(
        "{} {}[{}]: {}",
        icon_styled, severity_styled, diagnostic.code, display_message
    );
    lines.push(header);

    if diagnostic.location.is_some() {
        let snippet_renderer = SnippetRenderer::new().with_cache(cache);
        if let Some(snippet_lines) = snippet_renderer.render(diagnostic, fallback_message) {
            lines.extend(snippet_lines);
        }
    }

    if let Some(ref hint) = diagnostic.hint {
        lines.push(format!(
            "   \x1b[90m= \x1b[36mhint:\x1b[0m {}",
            style(hint).dim()
        ));
    }

    lines.push(String::new());
    lines.join("\n")
}

pub fn render_diagnostic(
    diagnostic: &Diagnostic,
    message: &str,
    cache: Option<&SourceCache>,
) -> String {
    let layout = Layout::current();
    let mut lines = Vec::new();

    let formatted_text_lines = diagnostic_text_lines(diagnostic, message, &layout);
    lines.extend(formatted_text_lines);

    if diagnostic.location.is_some() {
        let snippet_renderer = SnippetRenderer::new().with_cache(cache);
        if let Some(snippet_lines) = snippet_renderer.render(diagnostic, message) {
            lines.push(Theme::frame(BAR));
            lines.extend(snippet_lines);
        }
    }

    lines.push(Theme::frame(BAR));
    lines.push(String::new());
    lines.join("\n")
}

fn diagnostic_text_lines(
    diagnostic: &Diagnostic,
    fallback_message: &str,
    layout: &Layout,
) -> Vec<String> {
    let mut lines = Vec::new();

    let (icon, label_key) = match diagnostic.severity {
        Severity::Error => (SYMBOL_ERROR, "Error"),
        Severity::Warning => (SYMBOL_WARN, "Warning"),
        Severity::Info => (SYMBOL_INFO, "Info"),
    };

    let severity_styled = Theme::severity_style(diagnostic.severity, label_key).bold();
    let icon_styled = Theme::severity_style(diagnostic.severity, icon);

    let display_message = if diagnostic.title.is_empty() {
        fallback_message
    } else {
        &diagnostic.title
    };

    let full_title = format!(
        "{} {} [{}] {}",
        icon_styled, severity_styled, diagnostic.code, display_message
    );
    let wrapped_title = layout.wrap_content(&full_title);
    let mut title_lines = wrapped_title.lines();

    if let Some(first) = title_lines.next() {
        lines.push(format!("{}  {}", Theme::frame(BAR), first));
    }
    for line in title_lines {
        lines.push(format!("{}  {}", Theme::frame(BAR), line));
    }

    if let Some(ref hint) = diagnostic.hint {
        lines.push(Theme::frame(BAR));
        let hint_prefix = format!("{} ", Theme::remark_symbol());
        let wrapped_hint = layout.wrap_content(hint);
        let mut hint_lines = wrapped_hint.lines();

        if let Some(first) = hint_lines.next() {
            lines.push(format!(
                "{}  {}{}",
                Theme::frame(BAR),
                hint_prefix,
                style(first).dim()
            ));
        }
        for line in hint_lines {
            lines.push(format!("{}  {}", Theme::frame(BAR), style(line).dim()));
        }
    }

    lines
}
