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

use console::style;
use evin_diagnostic::{Diagnostic, FileLocation};
use evin_io::SourceCache;

use crate::render::{Layout, Theme, layout::crop_line};

pub struct SnippetRenderer<'a> {
    pub cache: Option<&'a SourceCache>,
    pub context_lines: usize,
    pub annotation_char: char,
}

impl<'a> Default for SnippetRenderer<'a> {
    fn default() -> Self {
        Self {
            cache: None,
            context_lines: 2,
            annotation_char: '·',
        }
    }
}

impl<'a> SnippetRenderer<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cache(mut self, cache: Option<&'a SourceCache>) -> Self {
        self.cache = cache;
        self
    }

    /// Render the snippet as a list of self-contained strings.
    /// The caller is responsible for embedding this within any outer frames.
    pub fn render(&self, diagnostic: &Diagnostic, message: &str) -> Option<Vec<String>> {
        self.render_with_layout(diagnostic, message, Layout::current())
    }

    pub(crate) fn render_with_layout(
        &self,
        diagnostic: &Diagnostic,
        _message: &str,
        layout: Layout,
    ) -> Option<Vec<String>> {
        let location = diagnostic.location.as_ref()?;

        let file_path = match location {
            FileLocation::ByteSpan { path, .. } => path,
            FileLocation::Path { path, .. } => path,
        };

        // Try to load the file — prefer the cache, fall back to a one-off read.
        let source_file = if let Some(cache) = self.cache {
            cache.load(file_path).ok()?
        } else {
            let content = std::fs::read_to_string(file_path).ok()?;
            let temp_cache = SourceCache::default();
            temp_cache.insert(file_path.clone(), content)
        };

        let (span_start, span_length, label) = match location {
            FileLocation::ByteSpan {
                offset,
                length,
                label,
                ..
            } => (*offset, *length, label.clone()),
            FileLocation::Path { line, column, .. } => {
                let l = line.unwrap_or(1);
                let col = column.unwrap_or(1);
                let range = source_file.line_range(l);
                let start = range.start
                    + byte_offset_for_column(
                        &source_file.text[range.clone()],
                        col.saturating_sub(1),
                    );
                (start, 1, None)
            }
        };

        let line_number = source_file.line_for_offset(span_start);
        let column = source_file.column_for_offset(span_start);

        // Auto-compute gutter width from the highest visible line number.
        let max_line = line_number + self.context_lines;
        // Format: " N │ "  — the number left-padded to `num_width`, then " │ " (3 chars)
        let num_width = max_line.to_string().len();

        let padding = " ".repeat(num_width + 1);

        let mut output = Vec::new();

        // ╭─[ file:line:col ]  ← header
        let header_text = format!(
            "{}:{}:{}",
            layout.snippet_source_name(file_path),
            line_number,
            column
        );
        output.push(format!(
            "{}{}{}{}",
            padding,
            style("╭─[ ").color256(244),
            style(&header_text).cyan(),
            style(" ]").color256(244)
        ));
        output.push(format!("{}{}", padding, style("│").color256(244)));

        // Helper closure: render one source line (1-indexed) as a gutter + content string
        let render_source_line = |l: usize| -> Option<String> {
            let range = source_file.line_range(l);
            // Guard against empty/past-EOF line ranges.
            if range.start >= source_file.text.len() && l > 1 {
                return None;
            }
            let text_slice = if range.start <= source_file.text.len() {
                let end = range.end.min(source_file.text.len());
                source_file.text[range.start..end].trim_end_matches(&['\n', '\r'][..])
            } else {
                ""
            };
            let (cropped, _, _) = crop_line(
                text_slice,
                0,
                text_slice.len(),
                layout.snippet_source_width(),
            );
            // " N │ content"
            let gutter = format!("{:>num_width$} │ ", l);
            Some(format!("{}{}", style(&gutter).color256(244), cropped))
        };

        for i in (1..=self.context_lines).rev() {
            if let Some(l) = line_number.checked_sub(i)
                && l > 0
                && let Some(s) = render_source_line(l)
            {
                output.push(s);
            }
        }

        // ── Target line ──────────────────────────────────────────────────────────
        let range = source_file.line_range(line_number);
        let end = range.end.min(source_file.text.len());
        let target_text = if range.start <= source_file.text.len() {
            source_file.text[range.start..end].trim_end_matches(&['\n', '\r'][..])
        } else {
            ""
        };
        let start_in_line = span_start.saturating_sub(range.start);
        let (cropped, cropped_start, cropped_length) = crop_line(
            target_text,
            start_in_line,
            span_length,
            layout.snippet_source_width(),
        );

        let before = &cropped[..cropped_start];
        let highlight =
            &cropped[cropped_start..(cropped_start + cropped_length).min(cropped.len())];
        let after = &cropped[(cropped_start + cropped_length).min(cropped.len())..];
        let colored_cropped = format!(
            "{}{}{}",
            before,
            Theme::severity_style(diagnostic.severity, highlight),
            after
        );

        let target_gutter = format!("{:>num_width$} │ ", line_number);
        output.push(format!(
            "{}{}",
            style(&target_gutter).color256(244),
            colored_cropped
        ));

        // ── Annotation rows ───────────────────────────────────────────────────────
        let ann_gutter = format!("{}{} ", padding, self.annotation_char);

        // Measure underline length using display-width so CJK characters work correctly.
        let underline_start = cropped_start;
        let underline_text =
            &cropped[cropped_start..(cropped_start + cropped_length).min(cropped.len())];
        let underline_len = console::measure_text_width(underline_text).max(1);

        let left_arm = if underline_len > 1 {
            underline_len.saturating_sub(1) / 2
        } else {
            0
        };

        // Label row: "╰── message"
        let lbl = label.unwrap_or_else(|| "here".to_string());
        let lbl_ellipsized = layout.snippet_label(&lbl);
        let tail = "╰──";

        let label_spaces = " ".repeat(underline_start + left_arm);

        output.push(format!(
            "{}{}{} {}",
            style(&ann_gutter).color256(244),
            label_spaces,
            Theme::severity_style(diagnostic.severity, tail),
            Theme::severity_style(diagnostic.severity, &lbl_ellipsized),
        ));

        // Context lines after the target.
        for i in 1..=self.context_lines {
            if let Some(s) = render_source_line(line_number + i) {
                output.push(s);
            }
        }

        // ╰ footer
        output.push(format!("{}{}", padding, style("╰─").color256(244)));

        Some(output)
    }
}

fn byte_offset_for_column(value: &str, column: usize) -> usize {
    value
        .char_indices()
        .nth(column)
        .map_or(value.len(), |(offset, _)| offset)
}
