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

pub fn push_escaped_html(out: &mut String, s: &str) {
    let bytes = s.as_bytes();
    let mut last = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let replacement = match b {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'"' => "&quot;",
            _ => continue,
        };
        if i > last {
            out.push_str(&s[last..i]);
        }
        out.push_str(replacement);
        last = i + 1;
    }
    if last < s.len() {
        out.push_str(&s[last..]);
    }
}
