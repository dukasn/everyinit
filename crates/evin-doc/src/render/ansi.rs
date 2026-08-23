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

use crate::ast::{DiagramKind, DocNode, InlineNode};

/// Serializes an AST slice into ANSI colorized terminal text.
pub fn render_ansi(ast: &[DocNode]) -> String {
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
            let prefix = match level {
                1 => "\x1b[1;36m", // Bold Cyan
                2 => "\x1b[1;34m", // Bold Blue
                3 => "\x1b[1;33m", // Bold Yellow
                _ => "\x1b[1m",    // Bold
            };
            out.push_str(prefix);
            for child in children {
                render_inline(child, out);
            }
            out.push_str("\x1b[0m");
        }
        DocNode::Paragraph { children } => {
            for child in children {
                render_inline(child, out);
            }
        }
        DocNode::CodeBlock { code, .. } => {
            let formatted = code
                .lines()
                .map(|l| format!("\x1b[90m  │ \x1b[32m{}\x1b[0m", l))
                .collect::<Vec<_>>()
                .join("\n");
            out.push_str(&formatted);
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
                    out.push_str(&format!("\x1b[33m{}.\x1b[0m ", start + idx));
                } else {
                    out.push_str("\x1b[36m•\x1b[0m ");
                }
                for n in item_nodes {
                    render_node(n, out);
                }
                out.push('\n');
            }
        }
        DocNode::TaskList { items } => {
            for item in items {
                let mark = if item.checked {
                    "\x1b[32m[x]\x1b[0m "
                } else {
                    "\x1b[90m[ ]\x1b[0m "
                };
                out.push_str(mark);
                for n in &item.body {
                    render_node(n, out);
                }
                out.push('\n');
            }
        }
        DocNode::DefinitionList { items } => {
            for item in items {
                out.push_str("\x1b[1m");
                for t in &item.term {
                    render_inline(t, out);
                }
                out.push_str("\x1b[0m");
                for c in &item.classifiers {
                    out.push_str(" \x1b[90m:\x1b[0m \x1b[33m");
                    for ci in c {
                        render_inline(ci, out);
                    }
                    out.push_str("\x1b[0m");
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
                out.push_str("\x1b[36m");
                out.push_str(&item.flags.join(", "));
                out.push_str("\x1b[0m");
                if let Some(arg) = &item.argument {
                    out.push(' ');
                    out.push_str("\x1b[35m");
                    out.push_str(arg);
                    out.push_str("\x1b[0m");
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
                out.push_str("\x1b[1;34m:");
                out.push_str(&item.name);
                out.push_str(":\x1b[0m ");
                for n in &item.body {
                    render_node(n, out);
                }
                out.push('\n');
            }
        }
        DocNode::Stepper { steps } => {
            for (idx, step) in steps.iter().enumerate() {
                let badge = step.badge.as_deref().unwrap_or("");
                out.push_str("\x1b[1;32m");
                if !badge.is_empty() {
                    out.push_str(badge);
                    out.push_str(". ");
                } else {
                    out.push_str(&format!("{}. ", idx + 1));
                }
                out.push_str("\x1b[0m\x1b[1m");
                for t in &step.title {
                    render_inline(t, out);
                }
                out.push_str("\x1b[0m");
                if let Some(status) = &step.status {
                    out.push_str(&format!(" \x1b[33m[{}]\x1b[0m", status));
                }
                out.push('\n');
                for n in &step.body {
                    out.push_str("   ");
                    render_node(n, out);
                }
                out.push('\n');
            }
        }
        DocNode::Table { headers, rows } => {
            for h in headers {
                for c in h {
                    out.push_str("\x1b[1m");
                    render_inline(c, out);
                    out.push_str("\x1b[0m\t");
                }
                out.push('\n');
            }
            for row in rows {
                for cell in row {
                    for c in cell {
                        render_inline(c, out);
                    }
                    out.push('\t');
                }
                out.push('\n');
            }
        }
        DocNode::Admonition { kind, children, .. } => {
            let color = match kind.as_str() {
                "warning" => "\x1b[1;31m", // Bold Red
                "tip" => "\x1b[1;32m",     // Bold Green
                _ => "\x1b[1;34m",         // Bold Blue
            };
            out.push_str(&format!("{}▶ {}\x1b[0m\n", color, kind.to_uppercase()));
            for c in children {
                render_node(c, out);
            }
        }
        DocNode::HorizontalRule => {
            out.push_str("\x1b[90m────────────────────────────────────────\x1b[0m");
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
            out.push_str(&format!(
                "\x1b[1;36m┌─ [Diagram: {}] ───────────────────\x1b[0m\n",
                label
            ));
            for line in code.lines() {
                out.push_str(&format!("\x1b[90m│\x1b[0m  {}\n", line));
            }
            out.push_str("\x1b[1;36m└──────────────────────────────────────────────\x1b[0m\n");
            if let Some(cap) = caption {
                out.push_str(&format!("  \x1b[2mCaption: {}\x1b[0m\n", cap));
            }
        }
        DocNode::MathBlock { latex: code } => {
            let unicode = crate::render::unicode_math::latex_to_unicode(code);
            out.push_str("\x1b[35m  ");
            out.push_str(&unicode.replace('\n', "\n  "));
            out.push_str("\x1b[0m");
        }
        DocNode::Media { src, alt, .. } => {
            out.push_str(&format!(
                "\x1b[90m[Media: {}]\x1b[0m",
                alt.as_deref().unwrap_or(src)
            ));
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
                out.push_str(&format!("\x1b[1;36m{}\x1b[0m\n\n", synopsis));
            }
            if !summary.is_empty() {
                out.push_str(&format!("\x1b[1m{}\x1b[0m: {}\n\n", name, summary));
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

fn render_inline(inline: &InlineNode, out: &mut String) {
    match inline {
        InlineNode::Text { content } => out.push_str(content),
        InlineNode::Strong { children } => {
            out.push_str("\x1b[1m");
            for child in children {
                render_inline(child, out);
            }
            out.push_str("\x1b[0m");
        }
        InlineNode::Emphasis { children } => {
            out.push_str("\x1b[3m");
            for child in children {
                render_inline(child, out);
            }
            out.push_str("\x1b[0m");
        }
        InlineNode::Code { content } => out.push_str(&format!("\x1b[33m{}\x1b[0m", content)),
        InlineNode::Link { href, content, .. } => out.push_str(&format!(
            "\x1b[4;34m{}\x1b[0m (\x1b[90m{}\x1b[0m)",
            content, href
        )),
        InlineNode::Image { alt, .. } => out.push_str(&format!("\x1b[90m[Image: {}]\x1b[0m", alt)),
        InlineNode::Math { latex } => {
            let unicode = crate::render::unicode_math::latex_to_unicode(latex);
            out.push_str(&format!("\x1b[35m{}\x1b[0m", unicode));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{DocNode, InlineNode};

    #[test]
    fn test_render_ansi_with_math() {
        let ast = vec![
            DocNode::Paragraph {
                children: vec![
                    InlineNode::Text {
                        content: "Kinetic energy: ".to_string(),
                    },
                    InlineNode::Math {
                        latex: r"E_k = \frac{1}{2} m v^2".to_string(),
                    },
                ],
            },
            DocNode::MathBlock {
                latex: r"\sum_{i=1}^n i = \frac{n(n+1)}{2}".to_string(),
            },
        ];

        let ansi = render_ansi(&ast);
        assert!(ansi.contains("\x1b[35mEₖ = ½ m v²\x1b[0m"));
        assert!(ansi.contains("∑"));
    }
}
