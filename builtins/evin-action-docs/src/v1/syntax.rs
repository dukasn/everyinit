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

use std::collections::BTreeMap;
use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::{SyntaxDefinition, SyntaxSet};
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

    pub fn load_grammars(paths: &[std::path::PathBuf]) -> Result<Self, String> {
        let mut builder = SyntaxSet::load_defaults_newlines().into_builder();
        for path in paths {
            let source = std::fs::read_to_string(path)
                .map_err(|error| format!("failed to read grammar '{}': {error}", path.display()))?;
            let fallback_name = path.file_stem().and_then(|value| value.to_str());
            let syntax = match path.extension().and_then(|value| value.to_str()) {
                Some("sublime-syntax") => {
                    SyntaxDefinition::load_from_str(&source, true, fallback_name)
                        .map_err(|error| format!("invalid grammar '{}': {error}", path.display()))?
                }
                Some("tmLanguage") => {
                    let converted =
                        tm_language_to_sublime(&source, fallback_name).map_err(|error| {
                            format!("invalid grammar '{}': {error}", path.display())
                        })?;
                    SyntaxDefinition::load_from_str(&converted, true, fallback_name)
                        .map_err(|error| format!("invalid grammar '{}': {error}", path.display()))?
                }
                _ => {
                    return Err(format!(
                        "custom grammar '{}' must use .sublime-syntax or .tmLanguage format",
                        path.display()
                    ));
                }
            };
            builder.add(syntax);
        }
        Ok(Self {
            syntax_set: builder.build(),
        })
    }

    pub fn highlight(&self, language: Option<&str>, code: &str) -> String {
        let Some(language) = language.map(str::trim).filter(|value| !value.is_empty()) else {
            return escape(code);
        };
        let normalized = match language {
            "yml" => "yaml",
            "sh" | "bash" | "zsh" => "shell",
            "js" => "javascript",
            "ts" => "typescript",
            "rs" => "rust",
            "py" => "python",
            "md" => "markdown",
            other => other,
        };
        let Some(syntax) = self
            .syntax_set
            .find_syntax_by_token(normalized)
            .or_else(|| self.syntax_set.find_syntax_by_extension(normalized))
        else {
            return escape(code);
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

fn escape(value: &str) -> String {
    crate::v1::renderer::HtmlRenderer::escape_html(value)
}

fn tm_language_to_sublime(source: &str, fallback_name: Option<&str>) -> Result<String, String> {
    let plist = plist::Value::from_reader(std::io::Cursor::new(source.as_bytes()))
        .map_err(|error| format!("could not parse TextMate property list: {error}"))?;
    let root = plist
        .as_dictionary()
        .ok_or_else(|| "TextMate grammar root must be a dictionary".to_string())?;
    let name = plist_string(root.get("name"))
        .or(fallback_name)
        .unwrap_or("Custom Grammar");
    let scope = plist_string(root.get("scopeName")).unwrap_or("source.custom");
    let extensions = plist_strings(root.get("fileTypes"));
    let mut contexts = BTreeMap::<String, Vec<String>>::new();
    let mut counter = 0usize;
    let main = convert_patterns(root.get("patterns"), &mut contexts, &mut counter)?;
    contexts.insert("main".to_string(), main);
    if let Some(repository) = root.get("repository").and_then(plist::Value::as_dictionary) {
        for (key, value) in repository {
            let patterns = value
                .as_dictionary()
                .and_then(|dict| dict.get("patterns"))
                .cloned()
                .unwrap_or_else(|| plist::Value::Array(vec![value.clone()]));
            let converted = convert_patterns(Some(&patterns), &mut contexts, &mut counter)?;
            contexts.insert(key.clone(), converted);
        }
    }

    let mut output = format!(
        "%YAML 1.2\n---\nname: {}\nfile_extensions: [{}]\nscope: {}\ncontexts:\n",
        yaml_quote(name),
        extensions
            .iter()
            .map(|extension| yaml_quote(extension))
            .collect::<Vec<_>>()
            .join(", "),
        yaml_quote(scope)
    );
    for (context, rules) in contexts {
        output.push_str(&format!("  {}:\n", yaml_quote(&context)));
        if rules.is_empty() {
            output.push_str("    - match: '(?!)'\n");
            output.push_str("      scope: comment\n");
        } else {
            for rule in rules {
                output.push_str("    ");
                output.push_str(&rule);
                output.push('\n');
            }
        }
    }
    Ok(output)
}

fn convert_patterns(
    patterns: Option<&plist::Value>,
    contexts: &mut BTreeMap<String, Vec<String>>,
    counter: &mut usize,
) -> Result<Vec<String>, String> {
    let Some(patterns) = patterns.and_then(plist::Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut rules = Vec::new();
    for pattern in patterns {
        let Some(pattern) = pattern.as_dictionary() else {
            continue;
        };
        if let Some(include) = plist_string(pattern.get("include")) {
            let include = include.strip_prefix('#').unwrap_or(include);
            rules.push(format!("- include: {}", yaml_quote(include)));
            continue;
        }
        if let Some(regex) = plist_string(pattern.get("match")) {
            rules.push(format!("- match: {}", yaml_quote(regex)));
            if let Some(scope) = plist_string(pattern.get("name")) {
                rules.push(format!("  scope: {}", yaml_quote(scope)));
            }
            continue;
        }
        let (Some(begin), Some(end)) = (
            plist_string(pattern.get("begin")),
            plist_string(pattern.get("end")),
        ) else {
            continue;
        };
        let child = format!("tm_context_{}", *counter);
        *counter += 1;
        rules.push(format!("- match: {}", yaml_quote(begin)));
        if let Some(scope) = plist_string(pattern.get("name")) {
            rules.push(format!("  scope: {}", yaml_quote(scope)));
        }
        rules.push(format!("  push: {}", yaml_quote(&child)));
        let mut child_rules = Vec::new();
        if let Some(scope) = plist_string(pattern.get("contentName")) {
            child_rules.push(format!("- meta_scope: {}", yaml_quote(scope)));
        }
        child_rules.push(format!("- match: {}", yaml_quote(end)));
        child_rules.push("  pop: true".to_string());
        child_rules.extend(convert_patterns(
            pattern.get("patterns"),
            contexts,
            counter,
        )?);
        contexts.insert(child, child_rules);
    }
    Ok(rules)
}

fn plist_string(value: Option<&plist::Value>) -> Option<&str> {
    value.and_then(plist::Value::as_string)
}

fn plist_strings(value: Option<&plist::Value>) -> Vec<&str> {
    value
        .and_then(plist::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(plist::Value::as_string)
        .collect()
}

fn yaml_quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_standard_grammars_and_unknown_language_fallback() {
        let highlighter = SyntaxHighlighter::new();
        let rust = highlighter.highlight(Some("rust"), "fn main() {}\n");
        assert!(rust.contains("syn-"));
        assert_eq!(
            highlighter.highlight(Some("flow"), "tasks: <x>"),
            "tasks: &lt;x&gt;"
        );
    }

    #[test]
    fn custom_sublime_and_textmate_grammars_are_loaded() {
        let directory = tempfile::tempdir().unwrap();
        let sublime = directory.path().join("sample.sublime-syntax");
        std::fs::write(
            &sublime,
            "%YAML 1.2\n---\nname: Sample DSL\nfile_extensions: [dsl]\nscope: source.dsl\ncontexts:\n  main:\n    - match: '\\bprint\\b'\n      scope: keyword.control.dsl\n",
        )
        .unwrap();
        let tm = directory.path().join("sample.tmLanguage");
        std::fs::write(
            &tm,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>name</key><string>Sample TM</string><key>scopeName</key><string>source.tm</string>
<key>fileTypes</key><array><string>tm</string></array><key>patterns</key><array>
<dict><key>match</key><string>\bprint\b</string><key>name</key><string>keyword.control.tm</string></dict>
</array></dict></plist>"#,
        )
        .unwrap();
        let highlighter = SyntaxHighlighter::load_grammars(&[sublime, tm]).unwrap();
        assert!(
            highlighter
                .highlight(Some("dsl"), "print hi")
                .contains("syn-keyword")
        );
        assert!(
            highlighter
                .highlight(Some("tm"), "print hi")
                .contains("syn-keyword")
        );
    }
}
