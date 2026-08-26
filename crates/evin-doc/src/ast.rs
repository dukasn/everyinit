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

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LspSymbolMeta {
    pub name: String,
    pub synopsis: String,
    pub summary: String,
    pub category: String,
    #[serde(default)]
    pub handler: String,
    pub hover_markdown: String,
    pub hover_plaintext: String,
    pub terminal_ansi: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LspCatalogExport {
    pub version: u32,
    pub generator: String,
    pub generated_at: String,
    pub symbols: BTreeMap<String, LspSymbolMeta>,
    #[serde(default)]
    pub keywords: BTreeMap<String, LspSymbolMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocSiteAst {
    pub version: String,
    pub title: Option<String>,
    #[serde(default = "default_reference_prefix")]
    pub reference_prefix: String,
    pub pages: Vec<DocPage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocPage {
    pub slug: String,
    pub source_path: String,
    pub format: DocFormat,
    pub metadata: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub reference: Option<FormalReference>,
    pub toc: Vec<TocEntry>,
    pub ast: Vec<DocNode>,
}

fn default_reference_prefix() -> String {
    "REF".to_string()
}

pub fn inferred_description(text: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= 160 {
        return normalized;
    }
    let mut description: String = normalized.chars().take(157).collect();
    description.push_str("...");
    description
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FormalReference {
    pub id: String,
    pub digits: String,
    pub number: u64,
    pub depends_on: Vec<String>,
    pub referenced_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocFormat {
    Markdown,
    ReStructuredText,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TocEntry {
    pub level: u8,
    pub text: String,
    pub anchor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum DocNode {
    Heading {
        level: u8,
        text: String,
        children: Vec<InlineNode>,
        anchor: String,
    },
    Paragraph {
        children: Vec<InlineNode>,
    },
    Media {
        kind: MediaKind,
        src: String,
        alt: Option<String>,
        title: Option<String>,
        poster: Option<String>,
    },
    RawHtml {
        content: String,
    },
    CodeBlock {
        language: Option<String>,
        title: Option<String>,
        code: String,
    },
    Tape {
        script: String,
        config: crate::tape::TapeConfig,
        /// Build-time rendering produced by a host action. Parsers always leave this empty.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rendered_svg: Option<String>,
    },
    Admonition {
        kind: String,
        title: Option<String>,
        children: Vec<DocNode>,
    },
    List {
        ordered: bool,
        numbering: NumberingKind,
        start: usize,
        items: Vec<Vec<DocNode>>,
    },
    TaskList {
        items: Vec<TaskListItem>,
    },
    DefinitionList {
        items: Vec<DefinitionListItem>,
    },
    OptionList {
        items: Vec<OptionListItem>,
    },
    FieldList {
        items: Vec<FieldListItem>,
    },
    Stepper {
        steps: Vec<StepItem>,
    },
    Table {
        headers: Vec<Vec<InlineNode>>,
        rows: Vec<Vec<Vec<InlineNode>>>,
    },
    Directive {
        name: String,
        args: Vec<String>,
        body: Vec<DocNode>,
    },
    Diagram {
        kind: DiagramKind,
        code: String,
        title: Option<String>,
        caption: Option<String>,
        alt: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rendered_mermaid: Option<RenderedMermaid>,
    },
    MathBlock {
        latex: String,
    },
    LspSymbol {
        name: String,
        kind: String,
        synopsis: String,
        summary: String,
        category: String,
        handler: String,
        children: Vec<DocNode>,
    },
    HorizontalRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagramKind {
    Mermaid,
    Svgbob,
    Pikchr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedMermaid {
    pub svg: String,
}

impl std::fmt::Display for DiagramKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagramKind::Mermaid => write!(f, "mermaid"),
            DiagramKind::Svgbob => write!(f, "svgbob"),
            DiagramKind::Pikchr => write!(f, "pikchr"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Video,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum InlineNode {
    Text {
        content: String,
    },
    Strong {
        children: Vec<InlineNode>,
    },
    Emphasis {
        children: Vec<InlineNode>,
    },
    Code {
        content: String,
    },
    Link {
        href: String,
        title: Option<String>,
        content: String,
        #[serde(default, skip_serializing_if = "is_false")]
        code: bool,
    },
    Image {
        src: String,
        alt: String,
        title: Option<String>,
    },
    Math {
        latex: String,
    },
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl InlineNode {
    pub fn plain_text(&self) -> String {
        match self {
            InlineNode::Text { content } => content.clone(),
            InlineNode::Strong { children } => children
                .iter()
                .map(InlineNode::plain_text)
                .collect::<Vec<_>>()
                .join(""),
            InlineNode::Emphasis { children } => children
                .iter()
                .map(InlineNode::plain_text)
                .collect::<Vec<_>>()
                .join(""),
            InlineNode::Code { content } => content.clone(),
            InlineNode::Link { content, .. } => content.clone(),
            InlineNode::Image { alt, .. } => alt.clone(),
            InlineNode::Math { latex } => latex.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum NumberingKind {
    #[default]
    Arabic, // 1, 2, 3
    LowerAlpha, // a, b, c
    UpperAlpha, // A, B, C
    LowerRoman, // i, ii, iii
    UpperRoman, // I, II, III
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskListItem {
    pub checked: bool,
    pub body: Vec<DocNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefinitionListItem {
    pub term: Vec<InlineNode>,
    pub classifiers: Vec<Vec<InlineNode>>,
    pub body: Vec<DocNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptionListItem {
    pub flags: Vec<String>,
    pub argument: Option<String>,
    pub body: Vec<DocNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldListItem {
    pub name: String,
    pub body: Vec<DocNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepItem {
    pub badge: Option<String>,
    pub title: Vec<InlineNode>,
    pub status: Option<String>,
    pub body: Vec<DocNode>,
}
