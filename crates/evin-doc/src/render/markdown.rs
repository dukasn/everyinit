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

use crate::ast::{DocNode, InlineNode};

/// Serializes an AST slice into standard GitHub-Flavored Markdown.
pub fn render_markdown(ast: &[DocNode]) -> String {
    let mut out = String::new();

    for (i, node) in ast.iter().enumerate() {
        if i > 0 && !out.ends_with("\n\n") {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }
        render_node(node, &mut out);
    }

    out.trim().to_string()
}

fn render_node(node: &DocNode, out: &mut String) {
    match node {
        DocNode::Heading {
            level, children, ..
        } => {
            let hashes = "#".repeat((*level as usize).clamp(1, 6));
            out.push_str(&hashes);
            out.push(' ');
            for child in children {
                render_inline(child, out);
            }
        }
        DocNode::Paragraph { children } => {
            for child in children {
                render_inline(child, out);
            }
        }
        DocNode::CodeBlock { language, code, .. } => {
            out.push_str("```");
            if let Some(lang) = language {
                out.push_str(lang);
            }
            out.push('\n');
            out.push_str(code.trim_end());
            out.push_str("\n```");
        }
        DocNode::Tape { script, config, .. } => {
            out.push_str("```tape");
            if let Some(title) = &config.title {
                out.push_str(" title=\"");
                out.push_str(title);
                out.push('"');
            }
            out.push('\n');
            out.push_str(script);
            out.push_str("\n```\n\n");
        }
        DocNode::List {
            ordered,
            start,
            items,
            ..
        } => {
            for (idx, item_nodes) in items.iter().enumerate() {
                if *ordered {
                    out.push_str(&format!("{}. ", start + idx));
                } else {
                    out.push_str("* ");
                }
                let mut item_str = String::new();
                for (j, n) in item_nodes.iter().enumerate() {
                    if j > 0 {
                        item_str.push(' ');
                    }
                    render_node(n, &mut item_str);
                }
                out.push_str(&item_str);
                out.push('\n');
            }
        }
        DocNode::TaskList { items } => {
            for item in items {
                let mark = if item.checked { "- [x] " } else { "- [ ] " };
                out.push_str(mark);
                let mut item_str = String::new();
                for (j, n) in item.body.iter().enumerate() {
                    if j > 0 {
                        item_str.push(' ');
                    }
                    render_node(n, &mut item_str);
                }
                out.push_str(&item_str);
                out.push('\n');
            }
        }
        DocNode::DefinitionList { items } => {
            for item in items {
                for t in &item.term {
                    render_inline(t, out);
                }
                for c in &item.classifiers {
                    out.push_str(" : ");
                    for ci in c {
                        render_inline(ci, out);
                    }
                }
                out.push('\n');
                for n in &item.body {
                    out.push_str("    ");
                    render_node(n, out);
                }
                out.push('\n');
            }
        }
        DocNode::OptionList { items } => {
            for item in items {
                out.push_str(&item.flags.join(", "));
                if let Some(arg) = &item.argument {
                    out.push(' ');
                    out.push_str(arg);
                }
                out.push_str("  ");
                for n in &item.body {
                    render_node(n, out);
                }
                out.push('\n');
            }
        }
        DocNode::FieldList { items } => {
            for item in items {
                out.push(':');
                out.push_str(&item.name);
                out.push_str(": ");
                for n in &item.body {
                    render_node(n, out);
                }
                out.push('\n');
            }
        }
        DocNode::Stepper { steps } => {
            out.push_str("```steps\n");
            for (idx, step) in steps.iter().enumerate() {
                out.push_str(&format!("{}. **", idx + 1));
                for t in &step.title {
                    render_inline(t, out);
                }
                out.push_str("**");
                if let Some(status) = &step.status {
                    out.push_str(&format!(" [{}]", status));
                }
                out.push('\n');
                for n in &step.body {
                    out.push_str("   ");
                    render_node(n, out);
                }
                out.push('\n');
            }
            out.push_str("```\n\n");
        }
        DocNode::Table { headers, rows } => {
            if !headers.is_empty() {
                out.push_str("| ");
                for h in headers {
                    let mut cell = String::new();
                    for c in h {
                        render_inline(c, &mut cell);
                    }
                    out.push_str(&cell);
                    out.push_str(" | ");
                }
                out.push('\n');

                out.push_str("| ");
                for _ in headers {
                    out.push_str("--- | ");
                }
                out.push('\n');
            }

            for row in rows {
                out.push_str("| ");
                for cell in row {
                    let mut cell_str = String::new();
                    for c in cell {
                        render_inline(c, &mut cell_str);
                    }
                    out.push_str(&cell_str);
                    out.push_str(" | ");
                }
                out.push('\n');
            }
        }
        DocNode::Admonition { kind, children, .. } => {
            out.push_str(&format!("> [!{}]\n", kind.to_uppercase()));
            let mut inner = String::new();
            for c in children {
                render_node(c, &mut inner);
            }
            for line in inner.lines() {
                out.push_str("> ");
                out.push_str(line);
                out.push('\n');
            }
        }
        DocNode::HorizontalRule => {
            out.push_str("---");
        }
        DocNode::Diagram {
            kind, code, title, ..
        } => {
            out.push_str("```");
            out.push_str(&kind.to_string());
            if let Some(t) = title {
                out.push_str(&format!(" \"{}\"", t));
            }
            out.push('\n');
            out.push_str(code.trim_end());
            out.push_str("\n```");
        }
        DocNode::MathBlock { latex } => {
            out.push_str("$$\n");
            out.push_str(latex.trim_end());
            out.push_str("\n$$");
        }
        DocNode::Media { src, alt, .. } => {
            out.push_str(&format!(
                "![{}]({})",
                alt.as_deref().unwrap_or_default(),
                src
            ));
        }
        DocNode::LspSymbol {
            name,
            synopsis,
            summary,
            children,
            ..
        } => {
            if !synopsis.is_empty() {
                out.push_str(&format!("```esh\n{}\n```\n\n", synopsis));
            }
            if !summary.is_empty() {
                out.push_str(&format!("**{}**: {}\n\n", name, summary));
            }
            for (i, c) in children.iter().enumerate() {
                if i > 0 && !out.ends_with("\n\n") {
                    out.push('\n');
                }
                render_node(c, out);
            }
        }
        DocNode::RawHtml { content } => {
            out.push_str(content);
        }
        DocNode::Directive { name, args, body } => {
            out.push_str(&format!(
                "<!-- directive: {} {} -->\n",
                name,
                args.join(" ")
            ));
            for b in body {
                render_node(b, out);
            }
        }
    }
}

fn render_inline(inline: &InlineNode, out: &mut String) {
    match inline {
        InlineNode::Text { content } => out.push_str(content),
        InlineNode::Strong { children } => {
            out.push_str("**");
            for child in children {
                render_inline(child, out);
            }
            out.push_str("**");
        }
        InlineNode::Emphasis { children } => {
            out.push('*');
            for child in children {
                render_inline(child, out);
            }
            out.push('*');
        }
        InlineNode::Code { content } => out.push_str(&format!("`{}`", content)),
        InlineNode::Link { href, content, .. } => out.push_str(&format!("[{}]({})", content, href)),
        InlineNode::Image { src, alt, .. } => out.push_str(&format!("![{}]({})", alt, src)),
        InlineNode::Math { latex } => out.push_str(&format!("${}$", latex)),
    }
}
