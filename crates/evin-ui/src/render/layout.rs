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

use console::{Term, measure_text_width};
use textwrap::{Options, wrap};

use super::{DIAGNOSTIC_GUTTER_RESERVE, FALLBACK_WIDTH, FRAME_PREFIX_WIDTH, MIN_CONTENT_WIDTH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    width: usize,
}

#[derive(Debug, Clone)]
struct DisplayChar {
    byte_start: usize,
    byte_end: usize,
    width: usize,
}

impl Default for Layout {
    fn default() -> Self {
        Self::current()
    }
}

impl Layout {
    pub fn current() -> Self {
        let detected = Term::stderr()
            .size_checked()
            .map(|(_, width)| usize::from(width));
        Self::from_detected_width(detected)
    }

    pub fn from_detected_width(width: Option<usize>) -> Self {
        Self::with_width(width.unwrap_or(FALLBACK_WIDTH))
    }

    pub fn with_width(width: usize) -> Self {
        Self {
            width: width.max(MIN_CONTENT_WIDTH + FRAME_PREFIX_WIDTH),
        }
    }

    /// The available width after subtracting the border ("│  ") width (3 chars)
    pub fn inner_width(&self) -> usize {
        self.width.saturating_sub(3)
    }

    pub fn wrap_with_margin(&self, text: &str, margin: usize) -> Vec<String> {
        self.wrap_with_margin_options(text, margin, true)
    }

    /// Wraps between words while keeping long tokens, such as file URIs, intact.
    pub fn wrap_with_margin_preserving_words(&self, text: &str, margin: usize) -> Vec<String> {
        self.wrap_with_margin_options(text, margin, false)
    }

    fn wrap_with_margin_options(
        &self,
        text: &str,
        margin: usize,
        break_words: bool,
    ) -> Vec<String> {
        let width = self.inner_width().saturating_sub(margin);
        let options = Options::new(width)
            .break_words(break_words)
            .word_separator(textwrap::WordSeparator::AsciiSpace)
            .word_splitter(textwrap::WordSplitter::NoHyphenation)
            .subsequent_indent("");

        wrap(text, &options)
            .into_iter()
            .map(|s| s.into_owned())
            .collect()
    }

    /// Wraps text to fit inside the pipeline, returning multiple lines if needed.
    pub fn wrap_lines(&self, text: &str) -> Vec<String> {
        self.wrap_with_margin(text, 0)
    }

    pub fn content_width(self) -> usize {
        self.width
            .saturating_sub(FRAME_PREFIX_WIDTH)
            .max(MIN_CONTENT_WIDTH)
    }

    /// Fit dynamic text into a single terminal row while reserving room for
    /// fixed prefixes, status symbols, and timing information.
    pub fn fit_single_line(self, text: &str, reserved_width: usize) -> String {
        let available = self.width.saturating_sub(reserved_width).max(1);
        ellipsize_end(text, available)
    }

    pub fn snippet_source_width(self) -> usize {
        self.content_width()
            .saturating_sub(DIAGNOSTIC_GUTTER_RESERVE)
            .max(MIN_CONTENT_WIDTH)
    }

    pub fn snippet_source_name(self, name: &str) -> String {
        ellipsize_start(name, self.content_width().saturating_sub(12).max(8))
    }

    pub fn snippet_label(self, label: &str) -> String {
        ellipsize_end(label, (self.content_width() / 3).max(8))
    }

    pub fn wrap_content(self, text: &str) -> String {
        self.wrap(text, self.content_width())
    }

    pub fn wrap(self, text: &str, width: usize) -> String {
        let width = width.max(MIN_CONTENT_WIDTH);
        text.split('\n')
            .map(|line| {
                if line.is_empty() {
                    String::new()
                } else if measure_text_width(line) <= width {
                    line.to_string()
                } else {
                    textwrap::wrap(
                        line,
                        Options::new(width)
                            .break_words(true)
                            .word_splitter(textwrap::WordSplitter::NoHyphenation),
                    )
                    .into_iter()
                    .map(|line| line.into_owned())
                    .collect::<Vec<_>>()
                    .join("\n")
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn crop_line(
    line: &str,
    span_start: usize,
    span_length: usize,
    width: usize,
) -> (String, usize, usize) {
    if measure_text_width(line) <= width {
        return (
            line.to_string(),
            span_start.min(line.len()),
            span_length.min(line.len().saturating_sub(span_start)),
        );
    }

    let chars = display_chars(line);
    if chars.is_empty() {
        return (String::new(), 0, 0);
    }

    let span_end = span_start.saturating_add(span_length).min(line.len());
    let span_first = chars
        .iter()
        .position(|character| character.byte_end > span_start)
        .unwrap_or(chars.len() - 1);
    let mut span_last = chars
        .iter()
        .position(|character| character.byte_start >= span_end)
        .unwrap_or(chars.len());
    if span_last <= span_first {
        span_last = (span_first + 1).min(chars.len());
    }

    let visible_span_width = chars[span_first..span_last]
        .iter()
        .map(|character| character.width)
        .sum::<usize>();
    let mut window_start = span_first;
    let mut window_end = span_last;

    if visible_span_width + usize::from(window_start > 0) + usize::from(window_end < chars.len())
        > width
    {
        window_end = window_start;
        while window_end < chars.len() {
            let candidate_end = window_end + 1;
            if window_display_width(&chars, window_start, candidate_end) > width {
                break;
            }
            window_end = candidate_end;
        }
    } else {
        let mut left_context = 0;
        let mut right_context = 0;
        loop {
            let prefer_left = left_context <= right_context;
            let candidates = if prefer_left {
                [CropSide::Left, CropSide::Right]
            } else {
                [CropSide::Right, CropSide::Left]
            };
            let mut expanded = false;
            for side in candidates {
                let (candidate_start, candidate_end) = match side {
                    CropSide::Left if window_start > 0 => (window_start - 1, window_end),
                    CropSide::Right if window_end < chars.len() => (window_start, window_end + 1),
                    _ => continue,
                };
                if window_display_width(&chars, candidate_start, candidate_end) <= width {
                    match side {
                        CropSide::Left => {
                            window_start = candidate_start;
                            left_context += chars[window_start].width;
                        }
                        CropSide::Right => {
                            right_context += chars[window_end].width;
                            window_end = candidate_end;
                        }
                    }
                    expanded = true;
                    break;
                }
            }
            if !expanded {
                break;
            }
        }
    }

    while window_display_width(&chars, window_start, window_end) > width
        && window_end > window_start
    {
        window_end -= 1;
    }

    let left_ellipsis = window_start > 0;
    let right_ellipsis = window_end < chars.len();
    let content_start = chars[window_start].byte_start;
    let content_end = chars[window_end - 1].byte_end;
    let mut cropped = String::new();
    if left_ellipsis {
        cropped.push('…');
    }
    cropped.push_str(&line[content_start..content_end]);
    if right_ellipsis {
        cropped.push('…');
    }

    let visible_span_start = span_start.max(content_start).min(content_end);
    let visible_span_end = span_end.max(visible_span_start).min(content_end);
    let cropped_span_start =
        if left_ellipsis { '…'.len_utf8() } else { 0 } + visible_span_start - content_start;
    let cropped_span_length = visible_span_end.saturating_sub(visible_span_start);

    (cropped, cropped_span_start, cropped_span_length)
}

#[derive(Debug, Clone, Copy)]
enum CropSide {
    Left,
    Right,
}

fn window_display_width(chars: &[DisplayChar], start: usize, end: usize) -> usize {
    chars[start..end]
        .iter()
        .map(|character| character.width)
        .sum::<usize>()
        + usize::from(start > 0)
        + usize::from(end < chars.len())
}

fn display_chars(value: &str) -> Vec<DisplayChar> {
    value
        .char_indices()
        .map(|(byte_start, character)| DisplayChar {
            byte_start,
            byte_end: byte_start + character.len_utf8(),
            width: measure_text_width(&character.to_string()),
        })
        .collect()
}

fn ellipsize_start(value: &str, width: usize) -> String {
    if measure_text_width(value) <= width {
        return value.to_string();
    }
    let chars = display_chars(value);
    let mut start = chars.len();
    let mut used = 1;
    while start > 0 && used + chars[start - 1].width <= width {
        start -= 1;
        used += chars[start].width;
    }
    format!("…{}", &value[chars[start].byte_start..])
}

fn ellipsize_end(value: &str, width: usize) -> String {
    if measure_text_width(value) <= width {
        return value.to_string();
    }
    if width <= 1 {
        return "…".to_string();
    }
    let chars = display_chars(value);
    let mut end = 0;
    let mut used = 1;
    while end < chars.len() && used + chars[end].width <= width {
        used += chars[end].width;
        end += 1;
    }
    format!("{}…", &value[..chars[end - 1].byte_end])
}
