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

use serde::{Deserialize, Serialize};
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminalColor {
    Default,
    Command,
    Builtin,
    Keyword,
    Flag,
    Variable,
    String,
    Number,
    Operator,
    Punctuation,
    Comment,
    Ansi(u8),
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UnderlineStyle {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellAttributes {
    pub foreground: TerminalColor,
    pub background: TerminalColor,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub underline_style: UnderlineStyle,
    pub underline_color: Option<TerminalColor>,
    pub invert: bool,
    pub strikethrough: bool,
    pub overline: bool,
    pub hidden: bool,
}

impl CellAttributes {
    pub const MASK_BOLD: u8 = 1 << 0; // 0x01
    pub const MASK_DIM: u8 = 1 << 1; // 0x02
    pub const MASK_ITALIC: u8 = 1 << 2; // 0x04
    pub const MASK_UNDERLINE: u8 = 1 << 3; // 0x08
    pub const MASK_INVERT: u8 = 1 << 4; // 0x10
    pub const MASK_STRIKETHROUGH: u8 = 1 << 5; // 0x20
    pub const MASK_OVERLINE: u8 = 1 << 6; // 0x40
    pub const MASK_HIDDEN: u8 = 1 << 7; // 0x80

    #[inline]
    pub fn modifier_mask(&self) -> u8 {
        (self.bold as u8 * Self::MASK_BOLD)
            | (self.dim as u8 * Self::MASK_DIM)
            | (self.italic as u8 * Self::MASK_ITALIC)
            | (self.underline as u8 * Self::MASK_UNDERLINE)
            | (self.invert as u8 * Self::MASK_INVERT)
            | (self.strikethrough as u8 * Self::MASK_STRIKETHROUGH)
            | (self.overline as u8 * Self::MASK_OVERLINE)
            | (self.hidden as u8 * Self::MASK_HIDDEN)
    }

    #[inline]
    pub fn effective_colors(&self) -> (TerminalColor, TerminalColor) {
        if self.invert {
            (self.background.clone(), self.foreground.clone())
        } else {
            (self.foreground.clone(), self.background.clone())
        }
    }
}

impl Default for CellAttributes {
    fn default() -> Self {
        Self {
            foreground: TerminalColor::Default,
            background: TerminalColor::Default,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            underline_style: UnderlineStyle::None,
            underline_color: None,
            invert: false,
            strikethrough: false,
            overline: false,
            hidden: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalCell {
    pub ch: char,
    pub attributes: CellAttributes,
    pub continuation: bool,
}

impl Default for TerminalCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            attributes: CellAttributes::default(),
            continuation: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalFrame {
    pub timestamp_ms: u32,
    pub rows: u16,
    pub cols: u16,
    pub cells: Vec<TerminalCell>,
    pub cursor_row: u16,
    pub cursor_col: u16,
}

#[derive(Debug, Clone)]
pub struct VirtualScreen {
    rows: u16,
    cols: u16,
    cells: Vec<TerminalCell>,
    cursor_row: u16,
    cursor_col: u16,
    attributes: CellAttributes,
}

impl VirtualScreen {
    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            rows,
            cols,
            cells: vec![TerminalCell::default(); usize::from(rows) * usize::from(cols)],
            cursor_row: 0,
            cursor_col: 0,
            attributes: CellAttributes::default(),
        }
    }

    pub fn write_str(&mut self, value: &str) {
        let bytes = value.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'[') {
                let mut end = index + 2;
                while end < bytes.len() && !(0x40..=0x7e).contains(&bytes[end]) {
                    end += 1;
                }
                if let Some(&final_byte) = bytes.get(end) {
                    self.apply_csi(&String::from_utf8_lossy(&bytes[index + 2..end]), final_byte);
                    index = end + 1;
                    continue;
                }
            }
            let remainder = &value[index..];
            let Some(character) = remainder.chars().next() else {
                break;
            };
            self.write_char(character);
            index += character.len_utf8();
        }
    }

    pub fn write_char(&mut self, character: char) {
        match character {
            '\n' => self.line_feed(),
            '\r' => self.cursor_col = 0,
            '\u{8}' => self.cursor_col = self.cursor_col.saturating_sub(1),
            '\t' => {
                let next = ((self.cursor_col / 8) + 1) * 8;
                self.cursor_col = next.min(self.cols.saturating_sub(1));
            }
            character if character.is_control() => {}
            character => self.put_character(character),
        }
    }

    /// Writes a source character with a semantic foreground. PTY output keeps
    /// its ANSI attributes; only the tape executor applies syntax categories.
    pub fn write_styled_char(&mut self, character: char, foreground: TerminalColor) {
        let previous = self.attributes.foreground.clone();
        self.attributes.foreground = foreground;
        self.write_char(character);
        self.attributes.foreground = previous;
    }

    pub fn capture(&self, timestamp_ms: u32) -> TerminalFrame {
        TerminalFrame {
            timestamp_ms,
            rows: self.rows,
            cols: self.cols,
            cells: self.cells.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        }
    }

    pub fn plain_text(&self) -> String {
        let mut output = String::new();
        for row in 0..self.rows {
            let start = usize::from(row) * usize::from(self.cols);
            let line = self.cells[start..start + usize::from(self.cols)]
                .iter()
                .filter(|cell| !cell.continuation)
                .map(|cell| cell.ch)
                .collect::<String>();
            output.push_str(line.trim_end());
            if row + 1 < self.rows {
                output.push('\n');
            }
        }
        output
    }

    fn put_character(&mut self, character: char) {
        let width = UnicodeWidthChar::width(character).unwrap_or(0).min(2) as u16;
        if width == 0 {
            return;
        }
        if self.cursor_col + width > self.cols {
            self.line_feed();
        }
        let index = self.index(self.cursor_row, self.cursor_col);
        self.cells[index] = TerminalCell {
            ch: character,
            attributes: self.attributes.clone(),
            continuation: false,
        };
        if width == 2 && self.cursor_col + 1 < self.cols {
            let continuation = self.index(self.cursor_row, self.cursor_col + 1);
            self.cells[continuation] = TerminalCell {
                ch: ' ',
                attributes: self.attributes.clone(),
                continuation: true,
            };
        }
        self.cursor_col += width;
        if self.cursor_col >= self.cols {
            self.line_feed();
        }
    }

    fn line_feed(&mut self) {
        self.cursor_col = 0;
        if self.cursor_row + 1 >= self.rows {
            self.cells.drain(0..usize::from(self.cols));
            self.cells
                .extend(std::iter::repeat_with(TerminalCell::default).take(usize::from(self.cols)));
        } else {
            self.cursor_row += 1;
        }
    }

    fn apply_csi(&mut self, params: &str, final_byte: u8) {
        if final_byte == b'm' {
            self.apply_sgr_params(params);
            return;
        }

        let values: Vec<u16> = if params.is_empty() {
            vec![0]
        } else {
            params
                .split(';')
                .map(|value| value.parse().unwrap_or(0))
                .collect()
        };
        let amount = values.first().copied().unwrap_or(0).max(1);
        match final_byte {
            b'A' => self.cursor_row = self.cursor_row.saturating_sub(amount),
            b'B' => self.cursor_row = (self.cursor_row + amount).min(self.rows.saturating_sub(1)),
            b'C' => self.cursor_col = (self.cursor_col + amount).min(self.cols.saturating_sub(1)),
            b'D' => self.cursor_col = self.cursor_col.saturating_sub(amount),
            b'H' | b'f' => {
                self.cursor_row = values
                    .first()
                    .copied()
                    .unwrap_or(1)
                    .saturating_sub(1)
                    .min(self.rows.saturating_sub(1));
                self.cursor_col = values
                    .get(1)
                    .copied()
                    .unwrap_or(1)
                    .saturating_sub(1)
                    .min(self.cols.saturating_sub(1));
            }
            b'J' => self.erase_display(values.first().copied().unwrap_or(0)),
            b'K' => self.erase_line(values.first().copied().unwrap_or(0)),
            _ => {}
        }
    }

    fn erase_display(&mut self, mode: u16) {
        match mode {
            2 | 3 => self.cells.fill(TerminalCell::default()),
            1 => {
                for row in 0..=self.cursor_row {
                    let end = if row == self.cursor_row {
                        self.cursor_col
                    } else {
                        self.cols.saturating_sub(1)
                    };
                    self.clear_range(row, 0, end);
                }
            }
            _ => {
                self.clear_range(
                    self.cursor_row,
                    self.cursor_col,
                    self.cols.saturating_sub(1),
                );
                for row in self.cursor_row.saturating_add(1)..self.rows {
                    self.clear_range(row, 0, self.cols.saturating_sub(1));
                }
            }
        }
    }

    fn erase_line(&mut self, mode: u16) {
        match mode {
            1 => self.clear_range(self.cursor_row, 0, self.cursor_col),
            2 => self.clear_range(self.cursor_row, 0, self.cols.saturating_sub(1)),
            _ => self.clear_range(
                self.cursor_row,
                self.cursor_col,
                self.cols.saturating_sub(1),
            ),
        }
    }

    fn clear_range(&mut self, row: u16, start: u16, end: u16) {
        for col in start..=end {
            let index = self.index(row, col);
            self.cells[index] = TerminalCell::default();
        }
    }

    fn apply_sgr_params(&mut self, params: &str) {
        if params.is_empty() {
            self.attributes = CellAttributes::default();
            return;
        }
        let args: Vec<&str> = params.split(';').collect();
        let mut i = 0;
        while i < args.len() {
            let arg = args[i];
            if arg.contains(':') {
                let sub_parts: Vec<&str> = arg.split(':').collect();
                let code: u16 = sub_parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
                match code {
                    0 => self.attributes = CellAttributes::default(),
                    4 => {
                        let style = sub_parts
                            .get(1)
                            .and_then(|s| s.parse::<u16>().ok())
                            .unwrap_or(1);
                        match style {
                            0 => {
                                self.attributes.underline = false;
                                self.attributes.underline_style = UnderlineStyle::None;
                            }
                            1 => {
                                self.attributes.underline = true;
                                self.attributes.underline_style = UnderlineStyle::Single;
                            }
                            2 => {
                                self.attributes.underline = true;
                                self.attributes.underline_style = UnderlineStyle::Double;
                            }
                            3 => {
                                self.attributes.underline = true;
                                self.attributes.underline_style = UnderlineStyle::Curly;
                            }
                            4 => {
                                self.attributes.underline = true;
                                self.attributes.underline_style = UnderlineStyle::Dotted;
                            }
                            5 => {
                                self.attributes.underline = true;
                                self.attributes.underline_style = UnderlineStyle::Dashed;
                            }
                            _ => {
                                self.attributes.underline = true;
                                self.attributes.underline_style = UnderlineStyle::Single;
                            }
                        }
                    }
                    38 | 48 | 58 => {
                        if let Some(color) = parse_colon_color(&sub_parts[1..]) {
                            match code {
                                38 => self.attributes.foreground = color,
                                48 => self.attributes.background = color,
                                58 => self.attributes.underline_color = Some(color),
                                _ => {}
                            }
                        }
                    }
                    _ => self.apply_single_sgr(code),
                }
                i += 1;
                continue;
            }

            let code: u16 = arg.parse().unwrap_or(0);
            match code {
                38 | 48 | 58 => {
                    if let Some((color, consumed)) = parse_semicolon_color(&args[i + 1..]) {
                        match code {
                            38 => self.attributes.foreground = color,
                            48 => self.attributes.background = color,
                            58 => self.attributes.underline_color = Some(color),
                            _ => {}
                        }
                        i += consumed + 1;
                        continue;
                    } else {
                        self.apply_single_sgr(code);
                    }
                }
                _ => self.apply_single_sgr(code),
            }
            i += 1;
        }
    }

    fn apply_single_sgr(&mut self, code: u16) {
        match code {
            0 => self.attributes = CellAttributes::default(),
            1 => self.attributes.bold = true,
            2 => self.attributes.dim = true,
            3 => self.attributes.italic = true,
            4 => {
                self.attributes.underline = true;
                self.attributes.underline_style = UnderlineStyle::Single;
            }
            7 => self.attributes.invert = true,
            8 => self.attributes.hidden = true,
            9 => self.attributes.strikethrough = true,
            21 => {
                self.attributes.underline = true;
                self.attributes.underline_style = UnderlineStyle::Double;
            }
            22 => {
                self.attributes.bold = false;
                self.attributes.dim = false;
            }
            23 => self.attributes.italic = false,
            24 => {
                self.attributes.underline = false;
                self.attributes.underline_style = UnderlineStyle::None;
            }
            27 => self.attributes.invert = false,
            28 => self.attributes.hidden = false,
            29 => self.attributes.strikethrough = false,
            30..=37 => self.attributes.foreground = TerminalColor::Ansi((code - 30) as u8),
            39 => self.attributes.foreground = TerminalColor::Default,
            40..=47 => self.attributes.background = TerminalColor::Ansi((code - 40) as u8),
            49 => self.attributes.background = TerminalColor::Default,
            53 => self.attributes.overline = true,
            55 => self.attributes.overline = false,
            59 => self.attributes.underline_color = None,
            90..=97 => self.attributes.foreground = TerminalColor::Ansi((code - 90 + 8) as u8),
            100..=107 => self.attributes.background = TerminalColor::Ansi((code - 100 + 8) as u8),
            _ => {}
        }
    }

    fn index(&self, row: u16, col: u16) -> usize {
        usize::from(row) * usize::from(self.cols) + usize::from(col)
    }
}

fn parse_colon_color(sub_parts: &[&str]) -> Option<TerminalColor> {
    if sub_parts.is_empty() {
        return None;
    }
    match sub_parts[0] {
        "5" => sub_parts
            .get(1)
            .and_then(|s| s.parse::<u8>().ok())
            .map(TerminalColor::Indexed),
        "2" => {
            if sub_parts.len() >= 4 {
                let offset = if sub_parts.get(1).is_some_and(|s| s.is_empty()) {
                    2
                } else {
                    1
                };
                let r = sub_parts.get(offset).and_then(|s| s.parse::<u8>().ok())?;
                let g = sub_parts
                    .get(offset + 1)
                    .and_then(|s| s.parse::<u8>().ok())?;
                let b = sub_parts
                    .get(offset + 2)
                    .and_then(|s| s.parse::<u8>().ok())?;
                Some(TerminalColor::Rgb(r, g, b))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn parse_semicolon_color(args: &[&str]) -> Option<(TerminalColor, usize)> {
    if args.is_empty() {
        return None;
    }
    match args[0] {
        "5" => {
            let index = args.get(1).and_then(|s| s.parse::<u8>().ok())?;
            Some((TerminalColor::Indexed(index), 2))
        }
        "2" => {
            if args.len() >= 4 {
                let r = args.get(1).and_then(|s| s.parse::<u8>().ok())?;
                let g = args.get(2).and_then(|s| s.parse::<u8>().ok())?;
                let b = args.get(3).and_then(|s| s.parse::<u8>().ok())?;
                Some((TerminalColor::Rgb(r, g, b), 4))
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_unicode_width_and_ansi_attributes() {
        let mut screen = VirtualScreen::new(2, 6);
        screen.write_str("a界\x1b[31mb\x1b[0m");
        let frame = screen.capture(0);
        assert_eq!(frame.cells[0].ch, 'a');
        assert_eq!(frame.cells[1].ch, '界');
        assert!(frame.cells[2].continuation);
        assert_eq!(frame.cells[3].attributes.foreground, TerminalColor::Ansi(1));
    }

    #[test]
    fn handles_cursor_erase_scrolling_and_extended_colors() {
        let mut screen = VirtualScreen::new(2, 4);
        screen.write_str("abcd\nefgh\x1b[1;2H!\x1b[K\x1b[38;5;196mr\x1b[38;2;1;2;3mg");
        let frame = screen.capture(0);
        assert_eq!(frame.cells[1].ch, '!');
        assert_eq!(frame.cells[2].ch, 'r');
        assert_eq!(
            frame.cells[2].attributes.foreground,
            TerminalColor::Indexed(196)
        );
        assert_eq!(
            frame.cells[3].attributes.foreground,
            TerminalColor::Rgb(1, 2, 3)
        );
    }

    #[test]
    fn preserves_executor_supplied_semantic_tokens() {
        let mut screen = VirtualScreen::new(1, 8);
        screen.write_styled_char('c', TerminalColor::Command);
        screen.write_styled_char('d', TerminalColor::Builtin);
        screen.write_styled_char(' ', TerminalColor::Default);
        screen.write_styled_char('|', TerminalColor::Operator);
        let frame = screen.capture(0);
        assert_eq!(frame.cells[0].attributes.foreground, TerminalColor::Command);
        assert_eq!(frame.cells[1].attributes.foreground, TerminalColor::Builtin);
        assert_eq!(frame.cells[2].attributes.foreground, TerminalColor::Default);
        assert_eq!(
            frame.cells[3].attributes.foreground,
            TerminalColor::Operator
        );
    }

    #[test]
    fn parses_comprehensive_sgr_modifiers_and_colon_colors() {
        let mut screen = VirtualScreen::new(1, 8);
        // Test invert (7), strikethrough (9), overline (53), hidden (8), curly underline (4:3) and colon truecolor (38:2::255:128:0)
        screen.write_str("\x1b[7;9;53;8;4:3;38:2::255:128:0mX\x1b[0m");
        let frame = screen.capture(0);
        let cell = &frame.cells[0];
        assert_eq!(cell.ch, 'X');
        assert!(cell.attributes.invert);
        assert!(cell.attributes.strikethrough);
        assert!(cell.attributes.overline);
        assert!(cell.attributes.hidden);
        assert!(cell.attributes.underline);
        assert_eq!(cell.attributes.underline_style, UnderlineStyle::Curly);
        assert_eq!(cell.attributes.foreground, TerminalColor::Rgb(255, 128, 0));

        // Test double underline (21) and underline color (58:2::10:20:30)
        screen.write_str("\x1b[21;58:2::10:20:30mY\x1b[0m");
        let frame = screen.capture(0);
        let cell_y = &frame.cells[1];
        assert_eq!(cell_y.ch, 'Y');
        assert_eq!(cell_y.attributes.underline_style, UnderlineStyle::Double);
        assert_eq!(
            cell_y.attributes.underline_color,
            Some(TerminalColor::Rgb(10, 20, 30))
        );
    }
}
