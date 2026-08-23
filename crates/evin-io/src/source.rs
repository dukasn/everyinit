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

use std::ops::Range;
use std::sync::Arc;

pub struct SourceFile {
    pub name: Arc<str>,
    pub text: Arc<str>,
    pub line_offsets: Arc<[usize]>,
}

impl SourceFile {
    pub fn new(name: impl Into<Arc<str>>, text: impl Into<Arc<str>>) -> Self {
        let name = name.into();
        let text = text.into();
        let mut line_offsets = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_offsets.push(i + 1);
            }
        }
        Self {
            name,
            text,
            line_offsets: line_offsets.into(),
        }
    }

    pub fn line_for_offset(&self, offset: usize) -> usize {
        self.line_offsets.partition_point(|&x| x <= offset) // 1-based line number
    }

    pub fn column_for_offset(&self, offset: usize) -> usize {
        let line = self.line_for_offset(offset);
        let start = *self.line_offsets.get(line.saturating_sub(1)).unwrap_or(&0);
        let end = offset.min(self.text.len());
        self.text[start..end].chars().count() + 1
    }

    pub fn line_range(&self, line: usize) -> Range<usize> {
        let l = line.saturating_sub(1);
        let start = *self.line_offsets.get(l).unwrap_or(&self.text.len());
        let end = self
            .line_offsets
            .get(l + 1)
            .copied()
            .unwrap_or(self.text.len());
        start..end
    }
}
