// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 of the License.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

use crate::protocol::{Position, Range};

#[derive(Debug, Clone)]
pub struct SourcePositionIndex {
    line_starts: Vec<usize>,
    source: String,
}
impl SourcePositionIndex {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (offset, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(offset + 1);
            }
        }
        Self {
            line_starts,
            source: source.to_owned(),
        }
    }
    pub fn offset_to_position(&self, offset: usize) -> Position {
        let offset = offset.min(self.source.len());
        let line = match self.line_starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        };
        Position {
            line: line as u32,
            character: self.source[self.line_starts[line]..offset]
                .encode_utf16()
                .count() as u32,
        }
    }
    pub fn position_to_offset(&self, position: &Position) -> usize {
        let line = position.line as usize;
        if line >= self.line_starts.len() {
            return self.source.len();
        }
        let start = self.line_starts[line];
        let end = self
            .line_starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.source.len());
        let mut units = 0;
        let mut bytes = 0;
        for character in self.source[start..end].chars() {
            if units >= position.character as usize {
                break;
            }
            units += character.len_utf16();
            bytes += character.len_utf8();
        }
        start + bytes
    }
    pub fn span_to_range(&self, start: usize, end: usize) -> Range {
        Range {
            start: self.offset_to_position(start),
            end: self.offset_to_position(end.max(start)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SourcePositionIndex;

    #[test]
    fn utf16_positions_round_trip() {
        let index = SourcePositionIndex::new("a🦀\nb");
        let position = index.offset_to_position(5);
        assert_eq!((position.line, position.character), (0, 3));
        assert_eq!(index.position_to_offset(&position), 5);
    }
}
