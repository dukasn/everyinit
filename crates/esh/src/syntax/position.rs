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

use crate::lsp::protocol::{Position, Range};

/// Fast index that maps byte offsets to LSP UTF-16 `(line, character)` coordinates and vice versa.
#[derive(Debug, Clone)]
pub struct SourcePositionIndex {
    line_starts: Vec<usize>,
    source: String,
}

impl SourcePositionIndex {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            line_starts,
            source: source.to_string(),
        }
    }

    /// Convert a byte offset into an LSP UTF-16 Position.
    pub fn offset_to_position(&self, byte_offset: usize) -> Position {
        let offset = byte_offset.min(self.source.len());
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };

        let line_start = self.line_starts[line_idx];
        let line_slice = &self.source[line_start..offset];
        let utf16_col = line_slice.encode_utf16().count();

        Position {
            line: line_idx as u32,
            character: utf16_col as u32,
        }
    }

    /// Convert an LSP UTF-16 Position into a byte offset.
    pub fn position_to_offset(&self, pos: &Position) -> usize {
        let line_idx = pos.line as usize;
        if line_idx >= self.line_starts.len() {
            return self.source.len();
        }
        let line_start = self.line_starts[line_idx];
        let line_end = if line_idx + 1 < self.line_starts.len() {
            self.line_starts[line_idx + 1]
        } else {
            self.source.len()
        };

        let line_text = &self.source[line_start..line_end];
        let mut utf16_count = 0;
        let mut byte_idx = 0;

        for c in line_text.chars() {
            if utf16_count >= pos.character as usize {
                break;
            }
            utf16_count += c.len_utf16();
            byte_idx += c.len_utf8();
        }

        line_start + byte_idx
    }

    /// Convert a byte span range `[start, end]` into an LSP Range.
    pub fn span_to_range(&self, start: usize, end: usize) -> Range {
        Range {
            start: self.offset_to_position(start),
            end: self.offset_to_position(end.max(start)),
        }
    }
}
