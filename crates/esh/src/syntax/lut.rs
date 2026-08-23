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

use phf::phf_set;

pub use crate::lexer::lut::{
    ASCII_LUT, is_digit, is_hex_digit, is_ident_continue, is_ident_start, is_whitespace,
};

pub static KEYWORDS: phf::Set<&'static str> = phf_set! {
    "if", "then", "elif", "else", "fi", "for", "in", "while", "until", "do", "done", "case", "esac", "function", "select", "time",
};

pub static TEST_OPERATORS: phf::Set<&'static str> = phf_set! {
    "-eq", "-ne", "-lt", "-le", "-gt", "-ge",
    "-f", "-d", "-e", "-r", "-w", "-x", "-s", "-L", "-h", "-b", "-c", "-p", "-S",
    "-z", "-n", "=", "==", "!=", "<", ">",
    "-o", "-a", "!",
};
