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

use crate::ast::{DiagramKind, DocNode};

/// Serializes an AST slice into clean unformatted plain text.
pub fn render_plain(ast: &[DocNode]) -> String {
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
        DocNode::Heading { text, .. } => {
            out.push_str(text);
        }
        DocNode::Paragraph { children } => {
            for child in children {
                render_inline_plain(child, out);
            }
        }
        DocNode::CodeBlock { code, .. } => {
            out.push_str(code.trim_end());
        }
        DocNode::Tape { script, .. } => {
            out.push_str(script);
            out.push('\n');
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
                    out.push_str("- ");
                }
                for n in item_nodes {
                    render_node(n, out);
                }
                out.push('\n');
            }
        }
        DocNode::TaskList { items } => {
            for item in items {
                let mark = if item.checked { "[x] " } else { "[ ] " };
                out.push_str(mark);
                for n in &item.body {
                    render_node(n, out);
                }
                out.push('\n');
            }
        }
        DocNode::DefinitionList { items } => {
            for item in items {
                for t in &item.term {
                    render_inline_plain(t, out);
                }
                for c in &item.classifiers {
                    out.push_str(" : ");
                    for ci in c {
                        render_inline_plain(ci, out);
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
            for (idx, step) in steps.iter().enumerate() {
                let badge = step.badge.as_deref().unwrap_or("");
                if !badge.is_empty() {
                    out.push_str(badge);
                    out.push_str(". ");
                } else {
                    out.push_str(&format!("{}. ", idx + 1));
                }
                for t in &step.title {
                    render_inline_plain(t, out);
                }
                if let Some(status) = &step.status {
                    out.push_str(&format!(" [{}]", status));
                }
                out.push('\n');
                for n in &step.body {
                    render_node(n, out);
                }
                out.push('\n');
            }
        }
        DocNode::Table { headers, rows } => {
            for h in headers {
                for c in h {
                    render_inline_plain(c, out);
                    out.push('\t');
                }
                out.push('\n');
            }
            for row in rows {
                for cell in row {
                    for c in cell {
                        render_inline_plain(c, out);
                    }
                    out.push('\t');
                }
                out.push('\n');
            }
        }
        DocNode::Admonition { kind, children, .. } => {
            out.push_str(&format!("[{}]\n", kind.to_uppercase()));
            for c in children {
                render_node(c, out);
            }
        }
        DocNode::HorizontalRule => {
            out.push_str("----------------------------------------");
        }
        DocNode::Diagram {
            kind,
            code,
            title,
            caption,
            ..
        } => {
            let label = title.as_deref().unwrap_or(match kind {
                DiagramKind::Mermaid => "Diagram (Mermaid)",
                DiagramKind::Svgbob => "Architecture Diagram (Svgbob)",
                DiagramKind::Pikchr => "Technical Diagram (Pikchr)",
            });
            out.push_str(&format!("[{}]\n", label));
            for line in code.lines() {
                out.push_str("  ");
                out.push_str(line);
                out.push('\n');
            }
            if let Some(cap) = caption {
                out.push_str(&format!("  Caption: {}\n", cap));
            }
        }
        DocNode::MathBlock { latex: code } => {
            out.push_str(&crate::render::unicode_math::latex_to_unicode(code));
        }
        DocNode::Media { src, alt, .. } => {
            out.push_str(alt.as_deref().unwrap_or(src));
        }
        DocNode::RawHtml { content } => {
            out.push_str(content);
        }
        DocNode::LspSymbol {
            name,
            synopsis,
            summary,
            children,
            ..
        } => {
            if !synopsis.is_empty() {
                out.push_str(synopsis);
                out.push_str("\n\n");
            }
            if !summary.is_empty() {
                out.push_str(&format!("{}: {}\n\n", name, summary));
            }
            for (i, c) in children.iter().enumerate() {
                if i > 0 && !out.ends_with("\n\n") {
                    out.push('\n');
                }
                render_node(c, out);
            }
        }
        DocNode::Directive { body, .. } => {
            for b in body {
                render_node(b, out);
            }
        }
    }
}

fn render_inline_plain(inline: &crate::ast::InlineNode, out: &mut String) {
    match inline {
        crate::ast::InlineNode::Text { content } => out.push_str(content),
        crate::ast::InlineNode::Strong { children } => {
            for child in children {
                render_inline_plain(child, out);
            }
        }
        crate::ast::InlineNode::Emphasis { children } => {
            for child in children {
                render_inline_plain(child, out);
            }
        }
        crate::ast::InlineNode::Code { content } => out.push_str(content),
        crate::ast::InlineNode::Link { content, .. } => out.push_str(content),
        crate::ast::InlineNode::Image { alt, .. } => out.push_str(alt),
        crate::ast::InlineNode::Math { latex } => {
            out.push_str(&crate::render::unicode_math::latex_to_unicode(latex));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{DocNode, InlineNode};

    #[test]
    fn test_render_plain_with_math() {
        let ast = vec![
            DocNode::Paragraph {
                children: vec![
                    InlineNode::Text {
                        content: "The formula is ".to_string(),
                    },
                    InlineNode::Math {
                        latex: "E = mc^2".to_string(),
                    },
                    InlineNode::Text {
                        content: ".".to_string(),
                    },
                ],
            },
            DocNode::MathBlock {
                latex: r"\int_0^1 x^2 dx = \frac{1}{3}".to_string(),
            },
        ];

        let plain = render_plain(&ast);
        assert!(plain.contains("The formula is E = mc²."));
        assert!(plain.contains("∫₀¹ x² dx = ⅓"));
    }
}
