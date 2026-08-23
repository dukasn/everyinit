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

pub const CHAR_WORD: u8 = 1 << 0; // Alphanumeric, '_', '-', '.', '/', ':'
pub const CHAR_WHITESPACE: u8 = 1 << 1; // ' ', '\t', '\r'
pub const CHAR_TERMINATOR: u8 = 1 << 2; // '\n', ';'
pub const CHAR_OPERATOR: u8 = 1 << 3; // '|', '&', '>', '<'
pub const CHAR_DELIMITER: u8 = 1 << 4; // '(', ')', '{', '}'
pub const CHAR_SIGIL: u8 = 1 << 5; // '$'
pub const CHAR_QUOTE: u8 = 1 << 6; // '"', '\''

/// Static ASCII classification table loaded entirely in CPU L1 cache (256 bytes)
pub static ASCII_LUT: [u8; 256] = {
    let mut table = [0u8; 256];
    let mut i = 0usize;
    while i < 256 {
        let b = i as u8;
        if b.is_ascii_alphanumeric()
            || b == b'_'
            || b == b'-'
            || b == b'.'
            || b == b'/'
            || b == b':'
            || b == b'='
            || b == b','
            || b == b'%'
            || b == b'+'
            || b == b'@'
            || b == b'~'
            || b == b'*'
            || b == b'?'
            || b == b'['
            || b == b']'
        {
            table[i] |= CHAR_WORD;
        }
        if b == b' ' || b == b'\t' || b == b'\r' {
            table[i] |= CHAR_WHITESPACE;
        }
        if b == b'\n' || b == b';' {
            table[i] |= CHAR_TERMINATOR;
        }
        if b == b'|' || b == b'&' || b == b'>' || b == b'<' {
            table[i] |= CHAR_OPERATOR;
        }
        if b == b'(' || b == b')' || b == b'{' || b == b'}' {
            table[i] |= CHAR_DELIMITER;
        }
        if b == b'$' {
            table[i] |= CHAR_SIGIL;
        }
        if b == b'"' || b == b'\'' {
            table[i] |= CHAR_QUOTE;
        }
        i += 1;
    }
    table
};

#[inline(always)]
pub fn is_char_kind(byte: u8, kind: u8) -> bool {
    (ASCII_LUT[byte as usize] & kind) != 0
}

#[inline(always)]
pub fn is_word_byte(byte: u8) -> bool {
    is_char_kind(byte, CHAR_WORD)
}

#[inline(always)]
pub fn is_whitespace_byte(byte: u8) -> bool {
    is_char_kind(byte, CHAR_WHITESPACE)
}

#[inline(always)]
pub fn is_whitespace(byte: u8) -> bool {
    is_whitespace_byte(byte)
}

#[inline(always)]
pub fn is_ident_start(byte: u8) -> bool {
    (byte as char).is_ascii_alphabetic() || byte == b'_'
}

#[inline(always)]
pub fn is_ident_continue(byte: u8) -> bool {
    (byte as char).is_ascii_alphanumeric() || byte == b'_'
}

#[inline(always)]
pub fn is_digit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

#[inline(always)]
pub fn is_hex_digit(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}
