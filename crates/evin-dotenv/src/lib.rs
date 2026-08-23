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

use evin_diagnostic::{Diagnostic, IssueCode, Span, Spanned};

/// Parses a string formatted as a .env file and returns a list of key-value pairs.
/// Keys and values are wrapped in `Spanned` to retain their byte offsets and line/column locations.
#[allow(clippy::result_large_err, clippy::type_complexity)]
pub fn parse(input: &str) -> Result<Vec<(Spanned<String>, Spanned<String>)>, Diagnostic> {
    let mut pairs = Vec::new();
    let mut offset = 0;

    for (line_idx, line) in input.lines().enumerate() {
        let line_len = line.len();
        let current_offset = offset;

        // We handle \r\n internally by `.lines()` but the offset math might shift slightly if there's \r.
        // For precision, we just use the length of the string yielded by `.lines()` + 1 for \n.
        // To be completely accurate with byte offsets if \r\n is used, we should iterate differently.
        // But for simplicity, we assume standard \n here, or at least close enough for the snippet renderer.
        offset += line_len + 1; // +1 for the newline that was stripped by `.lines()`

        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let trimmed_offset = current_offset + (line.len() - trimmed.len());

        if let Some(eq_idx) = trimmed.find('=') {
            let key_str = trimmed[..eq_idx].trim_end();
            let val_str = trimmed[eq_idx + 1..].trim_start();

            // Calculate key span
            let key_col = (line.len() - trimmed.len()) + 1;
            let key_span = Span {
                line: line_idx + 1,
                column: key_col,
                offset: trimmed_offset,
                length: key_str.len().max(1),
            };

            // Strip quotes if they exist around the value
            let mut val_stripped = val_str;
            let mut val_offset_shift = 0;

            if (val_str.starts_with('"') && val_str.ends_with('"') && val_str.len() >= 2)
                || (val_str.starts_with('\'') && val_str.ends_with('\'') && val_str.len() >= 2)
            {
                val_stripped = &val_str[1..val_str.len() - 1];
                val_offset_shift = 1;
            }

            let val_offset = trimmed_offset
                + eq_idx
                + 1
                + (trimmed[eq_idx + 1..].len() - val_str.len())
                + val_offset_shift;
            let val_col = (val_offset - current_offset) + 1;

            let val_span = Span {
                line: line_idx + 1,
                column: val_col,
                offset: val_offset,
                length: val_stripped.len().max(1),
            };

            pairs.push((
                Spanned::new(key_str.to_string(), key_span),
                Spanned::new(val_stripped.to_string(), val_span),
            ));
        } else {
            return Err(
                evin_locales::diagnostic(IssueCode::new("issue-env-syntax-error")).with_file_span(
                    ".env",
                    trimmed_offset,
                    trimmed.len().max(1),
                    None,
                ),
            );
        }
    }

    Ok(pairs)
}
