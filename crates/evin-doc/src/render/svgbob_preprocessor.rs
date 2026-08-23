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

/// Sanitizes ASCII box diagram lines before passing them to Svgbob.
///
/// Automatically protects unquoted paths ('/'), arithmetic ('$(('),
/// and backslashes ('\') without altering outer box column alignment.
///
/// Invariants:
/// 1. Idempotent: Lines already wrapped in `"..."` are never modified.
/// 2. Width Preserving: Preserves exact character column length so box borders remain aligned.
/// 3. Zero Padding Guard: If a box row has insufficient whitespace padding to inject quotes (< 2 spaces),
///    the line is left untouched.
/// 4. Arrow Transparency: Solitary vertical connector lines (`|`, `v`) are left untouched.
pub fn sanitize_svgbob_code(raw_code: &str) -> String {
    let mut sanitized_lines = Vec::new();

    for line in raw_code.lines() {
        let trimmed_end = line.trim_end();

        // Target full rectangular box rows: starts with '|' and ends with '|' (len > 2)
        if trimmed_end.starts_with('|') && trimmed_end.ends_with('|') && trimmed_end.len() > 2 {
            let inner = &trimmed_end[1..trimmed_end.len() - 1];
            let inner_trimmed = inner.trim();

            let is_already_quoted = inner_trimmed.starts_with('"') && inner_trimmed.ends_with('"');
            let has_dangerous_chars = inner.contains("//")
                || inner.contains('\\')
                || inner.contains("$(( ")
                || inner.contains("$(((")
                || (inner.matches('"').count() % 2 != 0);

            if !is_already_quoted && has_dangerous_chars {
                let clean_text = inner_trimmed.replace('"', "'");
                let quoted_text = format!("\"{}\"", clean_text);

                // Check zero padding guard: need at least 2 chars of padding to absorb quotes
                if inner.len() >= clean_text.len() + 2 {
                    let leading_spaces = inner.len() - inner.trim_start().len();
                    let remaining_width = inner.len().saturating_sub(leading_spaces);
                    let formatted_inner = format!(
                        "{:indent$}{:width$}",
                        "",
                        quoted_text,
                        indent = leading_spaces,
                        width = remaining_width
                    );

                    if formatted_inner.len() == inner.len() {
                        sanitized_lines.push(format!("|{}|", formatted_inner));
                        continue;
                    }
                }
            }
        }

        sanitized_lines.push(line.to_string());
    }

    sanitized_lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_slashes_and_backslashes() {
        let raw = "\
+---------------------------------------------------------------+
|  Stage 1: Parameter & Variable Expansion                      |
|           ($VAR, ${VAR:-def}, ${VAR//pat/rep}, ${#VAR})       |
+---------------------------------------------------------------+";

        let sanitized = sanitize_svgbob_code(raw);
        assert!(sanitized.contains("\"($VAR, ${VAR:-def}, ${VAR//pat/rep}, ${#VAR})\""));
        for line in sanitized.lines() {
            if line.starts_with('+') || line.starts_with('|') {
                assert_eq!(line.len(), 65, "Line length mismatch: {}", line);
            }
        }
    }

    #[test]
    fn test_sanitize_idempotency() {
        let raw = "\
+---------------------------------------------------------------+
|  \"Stage 1: Parameter & Variable Expansion\"                    |
|  \"         ($VAR, ${VAR:-def}, ${VAR//pat/rep}, ${#VAR})\"     |
+---------------------------------------------------------------+";

        let sanitized = sanitize_svgbob_code(raw);
        assert_eq!(raw, sanitized);
    }

    #[test]
    fn test_sanitize_preserves_arrows_and_connectors() {
        let raw = "\
+-------------------------------+
|             Box A             |
+-------------------------------+
                |
                v
+-------------------------------+
|             Box B             |
+-------------------------------+";

        let sanitized = sanitize_svgbob_code(raw);
        assert_eq!(raw, sanitized);
    }

    #[test]
    fn test_zero_padding_guard() {
        // Tight box with no whitespace padding for quotes
        let raw = "|tight//box|";
        let sanitized = sanitize_svgbob_code(raw);
        assert_eq!(raw, sanitized);
    }
}
