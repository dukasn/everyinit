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

//! Exhaustive, allocation-free AST traversal and text extraction utilities.

use crate::ast::{DocNode, InlineNode};

pub trait Visitor {
    fn visit_node(&mut self, _node: &DocNode) {}
    fn visit_inline(&mut self, _inline: &InlineNode) {}
}

pub fn walk_nodes(nodes: &[DocNode], visitor: &mut impl Visitor) {
    for node in nodes {
        visitor.visit_node(node);
        match node {
            DocNode::Heading { children, .. } | DocNode::Paragraph { children } => {
                walk_inlines(children, visitor)
            }
            DocNode::Admonition { children, .. }
            | DocNode::Directive { body: children, .. }
            | DocNode::LspSymbol { children, .. } => walk_nodes(children, visitor),
            DocNode::List { items, .. } => {
                for item in items {
                    walk_nodes(item, visitor);
                }
            }
            DocNode::TaskList { items } => {
                for item in items {
                    walk_nodes(&item.body, visitor);
                }
            }
            DocNode::DefinitionList { items } => {
                for item in items {
                    walk_inlines(&item.term, visitor);
                    for classifiers in &item.classifiers {
                        walk_inlines(classifiers, visitor);
                    }
                    walk_nodes(&item.body, visitor);
                }
            }
            DocNode::OptionList { items } => {
                for item in items {
                    walk_nodes(&item.body, visitor);
                }
            }
            DocNode::FieldList { items } => {
                for item in items {
                    walk_nodes(&item.body, visitor);
                }
            }
            DocNode::Stepper { steps } => {
                for step in steps {
                    walk_inlines(&step.title, visitor);
                    walk_nodes(&step.body, visitor);
                }
            }
            DocNode::Table { headers, rows } => {
                for header in headers {
                    walk_inlines(header, visitor);
                }
                for row in rows {
                    for cell in row {
                        walk_inlines(cell, visitor);
                    }
                }
            }
            DocNode::Media { .. }
            | DocNode::RawHtml { .. }
            | DocNode::CodeBlock { .. }
            | DocNode::Tape { .. }
            | DocNode::Diagram { .. }
            | DocNode::MathBlock { .. }
            | DocNode::HorizontalRule => {}
        }
    }
}

pub fn walk_inlines(inlines: &[InlineNode], visitor: &mut impl Visitor) {
    for inline in inlines {
        visitor.visit_inline(inline);
        if let InlineNode::Strong { children } | InlineNode::Emphasis { children } = inline {
            walk_inlines(children, visitor);
        }
    }
}

pub trait MutVisitor {
    fn visit_node(&mut self, _node: &mut DocNode) {}
    fn visit_inline(&mut self, _inline: &mut InlineNode) {}
}

pub fn walk_nodes_mut(nodes: &mut [DocNode], visitor: &mut impl MutVisitor) {
    for node in nodes {
        visitor.visit_node(node);
        match node {
            DocNode::Heading { children, .. } | DocNode::Paragraph { children } => {
                walk_inlines_mut(children, visitor)
            }
            DocNode::Admonition { children, .. }
            | DocNode::Directive { body: children, .. }
            | DocNode::LspSymbol { children, .. } => walk_nodes_mut(children, visitor),
            DocNode::List { items, .. } => {
                for item in items {
                    walk_nodes_mut(item, visitor);
                }
            }
            DocNode::TaskList { items } => {
                for item in items {
                    walk_nodes_mut(&mut item.body, visitor);
                }
            }
            DocNode::DefinitionList { items } => {
                for item in items {
                    walk_inlines_mut(&mut item.term, visitor);
                    for classifiers in &mut item.classifiers {
                        walk_inlines_mut(classifiers, visitor);
                    }
                    walk_nodes_mut(&mut item.body, visitor);
                }
            }
            DocNode::OptionList { items } => {
                for item in items {
                    walk_nodes_mut(&mut item.body, visitor);
                }
            }
            DocNode::FieldList { items } => {
                for item in items {
                    walk_nodes_mut(&mut item.body, visitor);
                }
            }
            DocNode::Stepper { steps } => {
                for step in steps {
                    walk_inlines_mut(&mut step.title, visitor);
                    walk_nodes_mut(&mut step.body, visitor);
                }
            }
            DocNode::Table { headers, rows } => {
                for header in headers {
                    walk_inlines_mut(header, visitor);
                }
                for row in rows {
                    for cell in row {
                        walk_inlines_mut(cell, visitor);
                    }
                }
            }
            DocNode::Media { .. }
            | DocNode::RawHtml { .. }
            | DocNode::CodeBlock { .. }
            | DocNode::Tape { .. }
            | DocNode::Diagram { .. }
            | DocNode::MathBlock { .. }
            | DocNode::HorizontalRule => {}
        }
    }
}

pub fn walk_inlines_mut(inlines: &mut [InlineNode], visitor: &mut impl MutVisitor) {
    for inline in inlines {
        visitor.visit_inline(inline);
        if let InlineNode::Strong { children } | InlineNode::Emphasis { children } = inline {
            walk_inlines_mut(children, visitor);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TextOptions {
    pub include_code: bool,
    pub include_media_labels: bool,
}

impl Default for TextOptions {
    fn default() -> Self {
        Self {
            include_code: true,
            include_media_labels: true,
        }
    }
}

pub fn collect_text(nodes: &[DocNode], out: &mut String, options: TextOptions) {
    struct Collector<'a> {
        out: &'a mut String,
        options: TextOptions,
    }
    impl Visitor for Collector<'_> {
        fn visit_node(&mut self, node: &DocNode) {
            match node {
                DocNode::CodeBlock { code, .. } if self.options.include_code => {
                    append(self.out, code)
                }
                DocNode::Tape { script, .. } if self.options.include_code => {
                    append(self.out, script)
                }
                DocNode::Admonition {
                    title: Some(title), ..
                } => append(self.out, title),
                DocNode::Media { alt, title, .. } if self.options.include_media_labels => {
                    if let Some(alt) = alt {
                        append(self.out, alt);
                    }
                    if let Some(title) = title {
                        append(self.out, title);
                    }
                }
                DocNode::OptionList { items } => {
                    for item in items {
                        for flag in &item.flags {
                            append(self.out, flag);
                        }
                        if let Some(argument) = &item.argument {
                            append(self.out, argument);
                        }
                    }
                }
                DocNode::FieldList { items } => {
                    for item in items {
                        append(self.out, &item.name);
                    }
                }
                _ => {}
            }
        }
        fn visit_inline(&mut self, inline: &InlineNode) {
            match inline {
                InlineNode::Text { content } | InlineNode::Code { content } => {
                    append(self.out, content)
                }
                InlineNode::Link { content, .. } => append(self.out, content),
                InlineNode::Image { alt, .. } => append(self.out, alt),
                InlineNode::Math { latex } => append(self.out, latex),
                InlineNode::Strong { .. } | InlineNode::Emphasis { .. } => {}
            }
        }
    }
    walk_nodes(nodes, &mut Collector { out, options });
}

fn append(out: &mut String, value: &str) {
    out.push_str(value);
    out.push(' ');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{DocNode, InlineNode};
    #[test]
    fn text_collection_visits_nested_inline_content() {
        let nodes = vec![DocNode::Paragraph {
            children: vec![InlineNode::Strong {
                children: vec![InlineNode::Text {
                    content: "nested".into(),
                }],
            }],
        }];
        let mut text = String::new();
        collect_text(&nodes, &mut text, TextOptions::default());
        assert_eq!(text, "nested ");
    }
}
