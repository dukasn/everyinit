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

//! Target-neutral content validation helpers.

use crate::site::{DocIssue, DocSeverity};
use unicode_width::UnicodeWidthStr;

pub fn normalize_latex(latex: &str) -> String {
    latex
        .replace(r"\begin{cases}", r"\left\{ \begin{matrix}")
        .replace(r"\end{cases}", r"\end{matrix} \right.")
}

pub fn validate_math(
    source_path: &str,
    latex: &str,
    display: latex2mathml::DisplayStyle,
) -> Option<DocIssue> {
    latex2mathml::latex_to_mathml(&normalize_latex(latex), display)
        .err()
        .map(|error| {
            DocIssue::error(
                "invalid-math",
                format!("Invalid LaTeX math in '{source_path}': {error}"),
            )
            .with_source_path(source_path)
        })
}

pub fn validate_svgbob(source_path: &str, title: Option<&str>, code: &str) -> Vec<DocIssue> {
    let mut issues = Vec::new();
    let diagram_name = title.unwrap_or("Untitled Diagram");
    let mut in_box = false;
    let mut expected_width = None;
    let mut box_start_line = 0;
    for (index, raw_line) in code.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim_end();
        let is_border = line.starts_with('+')
            && line.ends_with('+')
            && line.len() > 1
            && line[1..line.len() - 1]
                .chars()
                .all(|character| matches!(character, '-' | '='));
        let is_body = line.starts_with('|') && line.ends_with('|') && line.len() > 1;
        if is_border || is_body {
            let width = UnicodeWidthStr::width(line);
            if !in_box {
                in_box = true;
                expected_width = Some(width);
                box_start_line = line_number;
            } else if expected_width != Some(width) {
                issues.push(warning("svgbob-box-mismatch", source_path, format!("Svgbob box column width mismatch in '{diagram_name}' at line {line_number}: expected width {}, found {width}. Box starts at line {box_start_line}.", expected_width.unwrap_or_default())));
            }
        } else {
            in_box = false;
            expected_width = None;
        }
        if is_body && line[1..line.len() - 1].matches('"').count() % 2 != 0 {
            issues.push(warning("svgbob-unpaired-quote", source_path, format!("Unpaired double quote in Svgbob diagram '{diagram_name}' at line {line_number}.")));
        }
    }
    issues
}

fn warning(code: &'static str, source_path: &str, message: String) -> DocIssue {
    DocIssue {
        code,
        severity: DocSeverity::Warning,
        source_path: Some(source_path.to_string()),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_piecewise_math() {
        assert!(normalize_latex(r"\begin{cases}x\end{cases}").contains(r"\begin{matrix}"));
    }
    #[test]
    fn reports_bad_svgbob_width() {
        assert_eq!(
            validate_svgbob("a.rst", None, "+---+\n| x |\n+----+").len(),
            1
        );
    }
}
