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

const FLOW_DIRECTIVES: &[&str] = &["on", "watch", "steps"];
const FLOW_PATH_FIELDS: &[&str] = &[
    "asset_dir",
    "cwd",
    "exclude",
    "extensions_dir",
    "favicon",
    "files",
    "icon_dir",
    "include",
    "layout_dir",
    "logo",
    "modules",
    "out",
    "output_dir",
    "path",
    "paths",
    "rules",
    "source_dir",
    "watch",
];
const FLOW_ENUM_FIELDS: &[&str] = &[
    "bump",
    "channel",
    "execution",
    "format",
    "hooks",
    "presets",
    "scope",
    "stage",
    "stages",
    "targets",
];

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
        if normalized == "flow" {
            return highlight_flow(code);
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

fn highlight_flow(code: &str) -> String {
    let mut html = String::with_capacity(code.len() + code.len() / 3);
    let mut in_tasks = false;
    let mut task_indent = 0;
    let mut list_field: Option<(String, usize)> = None;

    for line in LinesWithEndings::from(code) {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let newline = if line.ends_with('\n') { "\n" } else { "" };
        let indent = content.len() - content.trim_start_matches([' ', '\t']).len();
        let body = &content[indent..];

        if body.is_empty() {
            html.push_str(newline);
            continue;
        }
        if body.starts_with("#!evin/") {
            push_flow_span(&mut html, "pragma", body);
            html.push_str(newline);
            continue;
        }
        if body.starts_with('#') {
            push_flow_span(&mut html, "comment", body);
            html.push_str(newline);
            continue;
        }

        let (list_prefix, key_start) = if body.starts_with("- ") {
            ("- ", 2)
        } else {
            ("", 0)
        };
        let rest = &body[key_start..];
        let key_end = rest.find(':');
        if let Some(key_end) = key_end {
            let key_text = &rest[..key_end];
            let key = key_text.trim();
            let is_simple_key = !key.is_empty()
                && key
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'));
            if is_simple_key {
                if indent == 0 {
                    in_tasks = key == "tasks";
                    task_indent = indent + 2;
                }
                let role = if indent == 0 {
                    "root"
                } else if in_tasks && indent == task_indent && list_prefix.is_empty() {
                    "task"
                } else if FLOW_DIRECTIVES.contains(&key) {
                    "directive"
                } else {
                    "property"
                };

                html.push_str(&escape(&content[..indent]));
                html.push_str(&escape(list_prefix));
                push_flow_span(&mut html, role, key_text);
                html.push(':');
                let value = &rest[key_end + 1..];
                push_flow_value(&mut html, value, key);
                let trimmed_value = value.trim();
                if trimmed_value.is_empty() {
                    list_field = Some((key.to_owned(), indent));
                } else if list_field
                    .as_ref()
                    .is_some_and(|(_, parent_indent)| indent <= *parent_indent)
                {
                    list_field = None;
                }
                html.push_str(newline);
                continue;
            }
        }

        html.push_str(&escape(&content[..indent]));
        if let Some((key, parent_indent)) = list_field.as_ref()
            && indent > *parent_indent
            && let Some(item) = body.strip_prefix("- ")
        {
            html.push_str("- ");
            push_flow_scalar(&mut html, item, key);
        } else {
            html.push_str(&escape(body));
        }
        html.push_str(newline);
    }

    html
}

fn push_flow_value(html: &mut String, value: &str, key: &str) {
    let mut plain_end = 0;
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'#' && (index == 0 || bytes[index - 1].is_ascii_whitespace()) {
            push_flow_scalar(html, &value[plain_end..index], key);
            push_flow_span(html, "comment", &value[index..]);
            return;
        }
        if bytes[index] == b'"' || bytes[index] == b'\'' {
            push_flow_scalar(html, &value[plain_end..index], key);
            let quote = bytes[index];
            let start = index;
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                    continue;
                }
                if bytes[index] == quote {
                    index += 1;
                    break;
                }
                index += 1;
            }
            push_flow_interpolated(html, &value[start..index], flow_value_role(key));
            plain_end = index;
            continue;
        }
        if value[index..].starts_with("${")
            && let Some(end) = value[index..].find('}')
        {
            push_flow_scalar(html, &value[plain_end..index], key);
            let end = index + end + 1;
            push_flow_span(html, "variable", &value[index..end]);
            index = end;
            plain_end = index;
            continue;
        }
        let ch = value[index..].chars().next().expect("valid UTF-8 boundary");
        index += ch.len_utf8();
    }
    push_flow_scalar(html, &value[plain_end..], key);
}

fn push_flow_scalar(html: &mut String, value: &str, key: &str) {
    if value.is_empty() {
        return;
    }
    let role = flow_value_role(key);
    if role == "value" {
        html.push_str(&escape(value));
    } else {
        push_flow_span(html, role, value);
    }
}

fn push_flow_interpolated(html: &mut String, value: &str, role: &str) {
    let mut cursor = 0;
    while let Some(offset) = value[cursor..].find("${") {
        let start = cursor + offset;
        if let Some(end_offset) = value[start..].find('}') {
            let end = start + end_offset + 1;
            push_flow_span(html, role, &value[cursor..start]);
            push_flow_span(html, "variable", &value[start..end]);
            cursor = end;
        } else {
            break;
        }
    }
    push_flow_span(html, role, &value[cursor..]);
}

fn flow_value_role(key: &str) -> &'static str {
    if key == "uses" {
        "action"
    } else if FLOW_PATH_FIELDS.contains(&key) {
        "path"
    } else if FLOW_ENUM_FIELDS.contains(&key) {
        "enum"
    } else {
        "string"
    }
}

fn push_flow_span(html: &mut String, role: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    html.push_str("<span class=\"syn-flow-");
    html.push_str(role);
    html.push_str("\">");
    html.push_str(&escape(value));
    html.push_str("</span>");
}

fn escape(value: &str) -> String {
    crate::v1::renderer::HtmlRenderer::escape_html(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syntax_highlighter_flow_semantics_yaml_rust_and_esh() {
        let highlighter = SyntaxHighlighter::new();

        let yaml_code = "name: Evin Build\nversion: 1.0\n";
        let yaml_html = highlighter.highlight(Some("yaml"), yaml_code);
        assert!(
            yaml_html.contains("syn-"),
            "YAML should produce highlighted syn- tokens: {}",
            yaml_html
        );

        let flow_code = "#!evin/v1\nworkspace:\n  name: example\n";
        let flow_html = highlighter.highlight(Some("flow"), flow_code);
        assert!(
            flow_html.contains("syn-flow-pragma"),
            "Flow pragma should receive its own syntax role: {}",
            flow_html
        );
        assert!(flow_html.contains("syn-flow-root"));
        assert!(flow_html.contains("syn-flow-property"));
        assert!(flow_html.contains("example"));

        let semantic_sample = "#!evin/v1\ntasks:\n  lint:\n    stage: Check\n    steps:\n      - uses: evin/archive@v1\n        with:\n          package: ${package_name}\n";
        let semantic_html = highlighter.highlight(Some("flow"), semantic_sample);
        for role in ["task", "enum", "directive", "action", "variable"] {
            assert!(
                semantic_html.contains(&format!("syn-flow-{role}")),
                "Flow sample should contain {role} coloring: {semantic_html}"
            );
        }
        let escaped_html =
            highlighter.highlight(Some("flow"), "tasks:\n  build:\n    run: <script>\n");
        assert!(escaped_html.contains("&lt;script&gt;"));

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
