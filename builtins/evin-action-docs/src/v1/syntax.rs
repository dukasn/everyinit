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

use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
        }
    }

    pub fn highlight(&self, language: Option<&str>, code: &str) -> String {
        let lang_token = match language {
            Some(l) => l.trim(),
            None => return crate::v1::renderer::HtmlRenderer::escape_html(code),
        };

        let normalized = match lang_token {
            "yml" => "yaml",
            "sh" | "bash" | "zsh" => "shell",
            "js" => "javascript",
            "ts" => "typescript",
            "rs" => "rust",
            "py" => "python",
            "md" => "markdown",
            other => other,
        };

        if matches!(normalized, "esh" | "evin-shell" | "esh-script") {
            return esh::syntax::highlight_html_classed(code, "syn-");
        }

        let syntax = match self
            .syntax_set
            .find_syntax_by_token(normalized)
            .or_else(|| self.syntax_set.find_syntax_by_extension(normalized))
        {
            Some(s) => s,
            None => return crate::v1::renderer::HtmlRenderer::escape_html(code),
        };

        let mut generator = ClassedHTMLGenerator::new_with_class_style(
            syntax,
            &self.syntax_set,
            ClassStyle::SpacedPrefixed { prefix: "syn-" },
        );

        for line in LinesWithEndings::from(code) {
            let _ = generator.parse_html_for_line_which_includes_newline(line);
        }

        generator.finalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syntax_highlighter_yaml_rust_and_esh() {
        let highlighter = SyntaxHighlighter::new();

        let yaml_code = "name: Evin Build\nversion: 1.0\n";
        let yaml_html = highlighter.highlight(Some("yaml"), yaml_code);
        assert!(
            yaml_html.contains("syn-"),
            "YAML should produce highlighted syn- tokens: {}",
            yaml_html
        );

        let rust_code = "fn main() {\n    println!(\"Hello\");\n}\n";
        let rust_html = highlighter.highlight(Some("rust"), rust_code);
        assert!(
            rust_html.contains("syn-"),
            "Rust should produce highlighted syn- tokens: {}",
            rust_html
        );

        let esh_code = "if read -c -p \"Deploy?\"; then\n    echo \"Deploying...\"\nfi\n";
        let esh_html = highlighter.highlight(Some("esh"), esh_code);
        assert!(
            esh_html.contains("syn-keyword"),
            "ESH should produce syn-keyword tokens: {}",
            esh_html
        );
        assert!(
            esh_html.contains("syn-string"),
            "ESH should produce syn-string tokens: {}",
            esh_html
        );
        assert!(
            esh_html.contains("syn-function"),
            "ESH should produce syn-function tokens: {}",
            esh_html
        );
    }
}
