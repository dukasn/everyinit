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

use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Evaluates dynamic template variable truthiness.
/// Returns `false` for `None`, empty strings, `"0"`, and `"false"`.
#[inline]
pub fn is_truthy(val: Option<&str>) -> bool {
    match val {
        None => false,
        Some(s) => {
            let trimmed = s.trim();
            !trimmed.is_empty() && trimmed != "0" && trimmed != "false"
        }
    }
}

/// Escapes HTML special characters into standard entities.
pub fn html_escape(input: &str, out: &mut String) {
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
}

/// Abstract Syntax Tree (AST) node for compiled component templates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateToken {
    /// Static HTML text fragment
    Literal(String),
    /// Standard dynamic variable: {{var}}
    EscapedVariable(String),
    /// Raw unescaped dynamic HTML: {{{var}}}
    RawVariable(String),
    /// Conditional template block: {{#if condition}} ... {{/if}}
    Conditional {
        condition: String,
        body: Vec<TemplateToken>,
    },
}

impl TemplateToken {
    /// Recursively evaluates template tokens against a variable dictionary.
    pub fn render(tokens: &[TemplateToken], vars: &[(&str, &str)], out: &mut String) {
        for token in tokens {
            match token {
                TemplateToken::Literal(lit) => out.push_str(lit),
                TemplateToken::EscapedVariable(key) => {
                    if let Some((_, val)) = vars.iter().find(|(k, _)| k == key) {
                        out.push_str(val);
                    }
                }
                TemplateToken::RawVariable(key) => {
                    if let Some((_, val)) = vars.iter().find(|(k, _)| k == key) {
                        out.push_str(val);
                    }
                }
                TemplateToken::Conditional { condition, body } => {
                    let val = vars.iter().find(|(k, _)| k == condition).map(|(_, v)| *v);
                    if is_truthy(val) {
                        Self::render(body, vars, out);
                    }
                }
            }
        }
    }
}

/// Parses HTML template string into a structured `Vec<TemplateToken>` AST.
pub fn parse_tokens(html: &str) -> Vec<TemplateToken> {
    let (tokens, _) = parse_tokens_recursive(html, None);
    tokens
}

fn parse_tokens_recursive<'a>(
    mut rest: &'a str,
    until_tag: Option<&str>,
) -> (Vec<TemplateToken>, &'a str) {
    let mut tokens = Vec::new();

    while let Some(start) = rest.find("{{") {
        if start > 0 {
            tokens.push(TemplateToken::Literal(rest[..start].to_string()));
        }
        rest = &rest[start + 2..];

        // Check for raw {{{var}}}
        if rest.starts_with('{') {
            rest = &rest[1..];
            if let Some(end) = rest.find("}}}") {
                let var_name = rest[..end].trim().to_string();
                tokens.push(TemplateToken::RawVariable(var_name));
                rest = &rest[end + 3..];
            } else {
                tokens.push(TemplateToken::Literal(format!("{{{{{{{rest}")));
                rest = "";
                break;
            }
            continue;
        }

        // Check for conditional {{#if cond}}
        if rest.starts_with("#if") {
            rest = &rest[3..];
            if let Some(end) = rest.find("}}") {
                let condition = rest[..end].trim().to_string();
                rest = &rest[end + 2..];
                let (body_tokens, remaining) = parse_tokens_recursive(rest, Some("/if"));
                tokens.push(TemplateToken::Conditional {
                    condition,
                    body: body_tokens,
                });
                rest = remaining;
            } else {
                tokens.push(TemplateToken::Literal(format!("{{{{#if{rest}")));
                rest = "";
                break;
            }
            continue;
        }

        // Check for end of conditional {{/if}}
        if rest.starts_with("/if") {
            if let Some(end) = rest.find("}}") {
                rest = &rest[end + 2..];
                if until_tag == Some("/if") {
                    return (tokens, rest);
                }
            }
            continue;
        }

        // Standard {{var}}
        if let Some(end) = rest.find("}}") {
            let var_name = rest[..end].trim().to_string();
            tokens.push(TemplateToken::EscapedVariable(var_name));
            rest = &rest[end + 2..];
        } else {
            tokens.push(TemplateToken::Literal(format!("{{{{{rest}")));
            rest = "";
            break;
        }
    }

    if !rest.is_empty() {
        tokens.push(TemplateToken::Literal(rest.to_string()));
        rest = "";
    }

    (tokens, rest)
}

/// Extracted blocks from a Single-File Component (`.evin` or multi-block `.html`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SfcBlocks {
    pub template: Option<String>,
    pub style: Option<String>,
    pub style_scoped: bool,
    pub script: Option<String>,
}

/// Compile-time lexer for Single-File Components.
pub struct SfcParser;

impl SfcParser {
    pub fn is_sfc(content: &str) -> bool {
        content.trim_start().starts_with("<template")
    }

    pub fn parse(content: &str) -> Result<SfcBlocks, String> {
        let mut template = None;
        let mut style = None;
        let mut style_scoped = false;
        let mut script = None;

        let mut rest = content;
        while let Some(start_tag) = rest.find('<') {
            rest = &rest[start_tag..];
            if rest.starts_with("<template") {
                let (inner, after) = Self::extract_block(rest, "template")?;
                template = Some(inner.to_string());
                rest = after;
            } else if rest.starts_with("<style") {
                let header_end = rest
                    .find('>')
                    .ok_or_else(|| "Malformed <style> tag".to_string())?;
                let header = &rest[..header_end];
                style_scoped = header.contains("scoped");
                let (inner, after) = Self::extract_block(rest, "style")?;
                style = Some(inner.to_string());
                rest = after;
            } else if rest.starts_with("<script") {
                let (inner, after) = Self::extract_block(rest, "script")?;
                script = Some(inner.to_string());
                rest = after;
            } else {
                rest = &rest[1..];
            }
        }

        Ok(SfcBlocks {
            template,
            style,
            style_scoped,
            script,
        })
    }

    fn extract_block<'a>(input: &'a str, tag: &str) -> Result<(&'a str, &'a str), String> {
        let open_end = input
            .find('>')
            .ok_or_else(|| format!("Malformed <{tag}> tag"))?;
        let close_tag = format!("</{tag}>");
        let close_start = input
            .find(&close_tag)
            .ok_or_else(|| format!("Unclosed <{tag}> tag"))?;
        let content = input[open_end + 1..close_start].trim();
        let after = &input[close_start + close_tag.len()..];
        Ok((content, after))
    }
}

/// Wraps component SCSS in modern `@scope` rules targeting `[data-c-<name>]`.
pub fn transform_scoped_scss(component_name: &str, scss: &str) -> String {
    format!("@scope ([data-c-{}]) {{\n{}\n}}", component_name, scss)
}

/// Injects `data-c-<name>` scope attribute into the root element of a template.
pub fn inject_scope_attribute_to_template(component_name: &str, template_html: &str) -> String {
    let trimmed = template_html.trim_start();
    if let Some(tag_open) = trimmed.find('<')
        && let Some(space_or_close) =
            trimmed[tag_open..].find(|c: char| c.is_whitespace() || c == '>')
    {
        let insert_pos = tag_open + space_or_close;
        let mut result = String::with_capacity(template_html.len() + component_name.len() + 16);
        result.push_str(&trimmed[..insert_pos]);
        result.push_str(&format!(" data-c-{}", component_name));
        result.push_str(&trimmed[insert_pos..]);
        return result;
    }
    template_html.to_string()
}

#[derive(Debug, Clone, Default)]
pub struct CompiledComponent {
    pub tokens: Vec<TemplateToken>,
    pub js: Option<String>,
    pub scss: Option<String>,
}

pub struct ComponentRegistry {
    components: HashMap<String, CompiledComponent>,
}

impl ComponentRegistry {
    pub fn empty() -> Self {
        Self {
            components: HashMap::new(),
        }
    }

    pub fn load_from_layout(
        layouts_dir: &include_dir::Dir,
        theme: &str,
        custom_path: Option<&Path>,
    ) -> Self {
        let mut raw_components: HashMap<String, (String, Option<String>, Option<String>)> =
            HashMap::new();

        let mut load_file_content = |stem: &str, ext: &str, content: &str| {
            if ext == "evin" || (ext == "html" && SfcParser::is_sfc(content)) {
                if let Ok(blocks) = SfcParser::parse(content) {
                    let entry = raw_components.entry(stem.to_string()).or_default();
                    if let Some(mut tmpl) = blocks.template {
                        if blocks.style_scoped {
                            tmpl = inject_scope_attribute_to_template(stem, &tmpl);
                        }
                        entry.0 = tmpl;
                    }
                    if let Some(script) = blocks.script {
                        entry.1 = Some(script);
                    }
                    if let Some(style) = blocks.style {
                        if blocks.style_scoped {
                            entry.2 = Some(transform_scoped_scss(stem, &style));
                        } else {
                            entry.2 = Some(style);
                        }
                    }
                }
            } else {
                let entry = raw_components.entry(stem.to_string()).or_default();
                match ext {
                    "html" => entry.0 = content.to_string(),
                    "js" => entry.1 = Some(content.to_string()),
                    "scss" | "css" => entry.2 = Some(content.to_string()),
                    _ => {}
                }
            }
        };

        // Helper to load templates from embedded directory
        let mut load_embedded = |dir: &include_dir::Dir| {
            for file in dir.files() {
                if let Some(stem) = file.path().file_stem().and_then(|s| s.to_str())
                    && let Some(ext) = file.path().extension().and_then(|s| s.to_str())
                    && let Some(content) = file.contents_utf8()
                {
                    load_file_content(stem, ext, content);
                }
            }
        };

        // 1. Load matter embedded component templates
        if let Some(matter_dir) = layouts_dir.get_dir("matter/templates") {
            load_embedded(matter_dir);
        }

        // 2. Load theme-specific embedded templates if different from matter
        if theme != "matter"
            && let Some(theme_dir) = layouts_dir.get_dir(format!("{}/templates", theme))
        {
            load_embedded(theme_dir);
        }

        // 3. Load custom workspace templates if custom path exists
        if let Some(path) = custom_path {
            let custom_templates_dir = path.join("templates");
            if custom_templates_dir.is_dir()
                && let Ok(entries) = std::fs::read_dir(custom_templates_dir)
            {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file()
                        && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
                        && let Some(ext) = p.extension().and_then(|s| s.to_str())
                        && let Ok(content) = std::fs::read_to_string(&p)
                    {
                        load_file_content(stem, ext, &content);
                    }
                }
            }
        }

        let mut components = HashMap::new();
        for (name, (html, js, scss)) in raw_components {
            let tokens = parse_tokens(&html);
            components.insert(name, CompiledComponent { tokens, js, scss });
        }

        Self { components }
    }

    pub fn render_to(
        &self,
        template_name: &str,
        vars: &[(&str, &str)],
        used_tracker: &mut HashSet<String>,
        out: &mut String,
    ) {
        let component = match self.components.get(template_name) {
            Some(c) => c,
            None => return,
        };

        used_tracker.insert(template_name.to_string());
        TemplateToken::render(&component.tokens, vars, out);
    }

    pub fn render(
        &self,
        template_name: &str,
        vars: &[(&str, &str)],
        used_tracker: &mut HashSet<String>,
    ) -> String {
        let mut output = String::new();
        self.render_to(template_name, vars, used_tracker, &mut output);
        output
    }

    pub fn collect_js(&self, used: &HashSet<String>) -> String {
        let mut js = String::new();
        let mut sorted_used: Vec<_> = used.iter().collect();
        sorted_used.sort();
        for name in sorted_used {
            if let Some(asset) = self.components.get(name.as_str())
                && let Some(ref j) = asset.js
            {
                js.push_str(j);
                js.push('\n');
            }
        }
        js
    }

    pub fn collect_css(&self, used: &HashSet<String>) -> String {
        let mut css = String::new();
        let mut sorted_used: Vec<_> = used.iter().collect();
        sorted_used.sort();
        for name in sorted_used {
            if let Some(asset) = self.components.get(name.as_str())
                && let Some(ref s) = asset.scss
            {
                css.push_str(s);
                css.push('\n');
            }
        }
        css
    }

    pub fn has_component(&self, template_name: &str) -> bool {
        self.components.contains_key(template_name)
    }

    pub fn list_components(&self) -> Vec<&str> {
        self.components.keys().map(|s| s.as_str()).collect()
    }

    pub fn get_placeholders(&self, template_name: &str) -> Vec<String> {
        let mut vars = Vec::new();
        if let Some(component) = self.components.get(template_name) {
            collect_token_placeholders(&component.tokens, &mut vars);
        }
        vars
    }

    pub fn validate_registered_templates(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        let expected_vars: &[(&str, &[&str])] = &[
            ("heading", &["level", "anchor", "content"]),
            ("paragraph", &["content"]),
            (
                "code_block",
                &["data_language", "display_language", "extension", "code"],
            ),
            ("callout", &["title", "content"]),
            ("list", &["tag", "items"]),
            ("table", &["header_html", "rows_html"]),
            ("toc_block", &["title", "items"]),
        ];

        for &(component_name, required_keys) in expected_vars {
            if let Some(_comp) = self.components.get(component_name) {
                let actual = self.get_placeholders(component_name);
                for &req in required_keys {
                    if !actual.iter().any(|a| a == req) {
                        warnings.push(format!(
                            "Component template '{}' is missing expected variable placeholder '{{{{{}}}}}'",
                            component_name, req
                        ));
                    }
                }
            } else {
                warnings.push(format!(
                    "Required component template '{}' is not registered",
                    component_name
                ));
            }
        }

        warnings
    }
}

fn collect_token_placeholders(tokens: &[TemplateToken], out: &mut Vec<String>) {
    for token in tokens {
        match token {
            TemplateToken::Literal(_) => {}
            TemplateToken::EscapedVariable(v) | TemplateToken::RawVariable(v) => {
                if !out.contains(v) {
                    out.push(v.clone());
                }
            }
            TemplateToken::Conditional { condition, body } => {
                if !out.contains(condition) {
                    out.push(condition.clone());
                }
                collect_token_placeholders(body, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_tokenization_and_placeholders() {
        let html = "<div><h1>{{title}}</h1><p>{{content}}</p></div>";
        let tokens = parse_tokens(html);
        assert_eq!(tokens.len(), 5);

        let mut comp_map = HashMap::new();
        comp_map.insert(
            "test_card".to_string(),
            CompiledComponent {
                tokens,
                js: Some("console.log('test');".to_string()),
                scss: Some(".test-card { color: red; }".to_string()),
            },
        );
        let registry = ComponentRegistry {
            components: comp_map,
        };

        assert!(registry.has_component("test_card"));
        assert!(!registry.has_component("non_existent"));
        assert_eq!(
            registry.get_placeholders("test_card"),
            vec!["title", "content"]
        );

        let mut used = HashSet::new();
        used.insert("test_card".to_string());
        assert_eq!(registry.collect_js(&used), "console.log('test');\n");
        assert_eq!(registry.collect_css(&used), ".test-card { color: red; }\n");

        let empty = HashSet::new();
        assert_eq!(registry.collect_js(&empty), "");
        assert_eq!(registry.collect_css(&empty), "");
    }

    #[test]
    fn test_sfc_parser_blocks() {
        let sfc = r#"
<template>
  <aside class="evin-callout">
    {{#if title}}
    <h4 class="callout-title">{{title}}</h4>
    {{/if}}
    <div class="callout-body">{{{content}}}</div>
  </aside>
</template>

<style scoped>
  .evin-callout {
    padding: 1rem;
  }
</style>

<script>
  window.EvinActions['callout:toggle'] = () => {};
</script>
"#;
        assert!(SfcParser::is_sfc(sfc));
        let blocks = SfcParser::parse(sfc).expect("Valid SFC blocks");

        assert!(blocks.template.is_some());
        assert!(
            blocks
                .template
                .as_ref()
                .unwrap()
                .contains("<aside class=\"evin-callout\">")
        );
        assert!(blocks.style.is_some());
        assert!(blocks.style.as_ref().unwrap().contains(".evin-callout {"));
        assert!(blocks.style_scoped);
        assert!(blocks.script.is_some());
        assert!(
            blocks
                .script
                .as_ref()
                .unwrap()
                .contains("window.EvinActions['callout:toggle']")
        );
    }

    #[test]
    fn nested_html_template_does_not_turn_a_component_into_an_sfc() {
        let html = "<figure><template class=\"copy-payload\">{{script}}</template></figure>";
        assert!(!SfcParser::is_sfc(html));
    }

    #[test]
    fn test_template_conditional_and_raw_slots() {
        let tmpl = "<div>{{#if header}}<h1>{{{header}}}</h1>{{/if}}<p>{{body}}</p></div>";
        let tokens = parse_tokens(tmpl);

        let vars_truthy = [
            ("header", "<span>Raw Header</span>"),
            ("body", "Simple Body"),
        ];
        let mut out = String::new();
        TemplateToken::render(&tokens, &vars_truthy, &mut out);
        assert_eq!(
            out,
            "<div><h1><span>Raw Header</span></h1><p>Simple Body</p></div>"
        );

        let vars_falsy = [("header", ""), ("body", "Simple Body")];
        let mut out2 = String::new();
        TemplateToken::render(&tokens, &vars_falsy, &mut out2);
        assert_eq!(out2, "<div><p>Simple Body</p></div>");
    }

    #[test]
    fn test_scoped_css_injection_and_transform() {
        let tmpl = "<aside class=\"callout\">Content</aside>";
        let injected = inject_scope_attribute_to_template("callout", tmpl);
        assert_eq!(
            injected,
            "<aside data-c-callout class=\"callout\">Content</aside>"
        );

        let scss = ".callout { color: blue; }";
        let transformed = transform_scoped_scss("callout", scss);
        assert_eq!(
            transformed,
            "@scope ([data-c-callout]) {\n.callout { color: blue; }\n}"
        );
    }
}
