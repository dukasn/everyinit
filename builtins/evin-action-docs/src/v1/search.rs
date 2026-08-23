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

use crate::v1::ast::{DocNode, DocPage, DocSiteAst, InlineNode};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const BIT_REF_EXACT: u8 = 1; // 1 << 0
pub const BIT_TITLE_EXACT: u8 = 2; // 1 << 1
pub const BIT_TITLE_PREFIX: u8 = 4; // 1 << 2
pub const BIT_CONTENT: u8 = 8; // 1 << 3

pub const KIND_PAGE: u8 = 0;
pub const KIND_CODE: u8 = 1;
pub const KIND_HEADING: u8 = 2;

/// Inverted search index with bitflag postings for compact payload and instant resolution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchIndexPayload {
    pub version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<String>,
    /// [slug, page_title, section_title, anchor, ref_id, snippet_idx, kind_flag]
    pub docs: Vec<(String, String, String, String, String, usize, u8)>,
    pub snippets: Vec<String>,
    /// term -> Vec<[doc_idx, match_flags, frequency]>
    pub index: BTreeMap<String, Vec<(usize, u8, u8)>>,
}

impl SearchIndexPayload {
    pub fn to_binary(&self) -> Vec<u8> {
        crate::v1::search_binary::BinarySearchIndexWriter::serialize(
            &self.docs,
            &self.snippets,
            &self.index,
        )
    }
}

pub struct RawDocSection {
    pub slug: String,
    pub page_title: String,
    pub title: String,
    pub anchor: String,
    pub ref_id: String,
    pub ref_digits: String,
    pub content: String,
    pub kind: u8,
}

impl RawDocSection {
    pub fn generate_excerpt(&self, max_len: usize) -> String {
        let clean = self
            .content
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if clean.len() <= max_len {
            clean
        } else {
            let slice = &clean[..max_len];
            if let Some(last_space) = slice.rfind(' ') {
                format!("{}...", &clean[..last_space])
            } else {
                format!("{slice}...")
            }
        }
    }
}

pub struct SearchIndexBuilder;

impl SearchIndexBuilder {
    pub fn extract_words(text: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut seen = HashSet::new();
        for word in text.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_') {
            let clean = word.trim().to_lowercase();
            if clean.len() >= 2 && seen.insert(clean.clone()) {
                words.push(clean);
            }
        }
        words
    }

    pub fn tokenize_section(raw: &RawDocSection) -> HashMap<String, (u8, u8)> {
        let mut terms_map: HashMap<String, (u8, u8)> = HashMap::new();

        // 1. Reference ID & Digits
        if !raw.ref_id.is_empty() {
            let clean_ref = raw.ref_id.to_lowercase();
            terms_map.entry(clean_ref).or_insert((0, 0)).0 |= BIT_REF_EXACT;
            terms_map
                .entry(raw.ref_id.to_lowercase())
                .or_insert((0, 0))
                .1 = 1;
        }
        if !raw.ref_digits.is_empty() {
            let clean_digits = raw.ref_digits.to_lowercase();
            terms_map.entry(clean_digits).or_insert((0, 0)).0 |= BIT_REF_EXACT;
            terms_map
                .entry(raw.ref_digits.to_lowercase())
                .or_insert((0, 0))
                .1 = 1;
        }

        // 2. Titles (Page title, Section title, Anchor)
        let mut title_words = Vec::new();
        for w in Self::extract_words(&raw.page_title) {
            title_words.push(w);
        }
        for w in Self::extract_words(&raw.title) {
            title_words.push(w);
        }
        if !raw.anchor.is_empty() {
            for w in Self::extract_words(&raw.anchor) {
                title_words.push(w);
            }
        }

        for tw in title_words {
            let entry = terms_map.entry(tw).or_insert((0, 0));
            entry.0 |= BIT_TITLE_EXACT;
            if entry.1 == 0 {
                entry.1 = 1;
            }
        }

        // 3. Body Content
        let content_words: Vec<String> = raw
            .content
            .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
            .map(|w| w.trim().to_lowercase())
            .filter(|w| w.len() >= 2)
            .collect();

        let mut freq_map = HashMap::new();
        for cw in content_words {
            *freq_map.entry(cw.clone()).or_insert(0u8) =
                freq_map.get(&cw).copied().unwrap_or(0).saturating_add(1);
        }

        for (term, freq) in freq_map {
            let entry = terms_map.entry(term).or_insert((0, 0));
            entry.0 |= BIT_CONTENT;
            entry.1 = entry.1.max(freq);
        }

        terms_map
    }

    pub fn build(site_ast: &DocSiteAst, prompts: Vec<String>) -> SearchIndexPayload {
        let mut raw_sections = Vec::new();

        for page in &site_ast.pages {
            let page_title = page
                .metadata
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or(&page.slug)
                .to_string();

            let ref_id = page
                .reference
                .as_ref()
                .map(|r| r.id.clone())
                .unwrap_or_default();
            let ref_digits = page
                .reference
                .as_ref()
                .map(|r| r.digits.clone())
                .unwrap_or_default();

            // Page-level entry
            let mut page_text = String::new();
            Self::collect_page_text(&page.ast, &mut page_text);

            raw_sections.push(RawDocSection {
                slug: page.slug.clone(),
                page_title: page_title.clone(),
                title: page_title.clone(),
                anchor: String::new(),
                ref_id: ref_id.clone(),
                ref_digits: ref_digits.clone(),
                content: page_text,
                kind: KIND_PAGE,
            });

            // Section-level entries split by headings
            Self::build_sections(page, &page_title, &ref_id, &ref_digits, &mut raw_sections);
        }

        let mut docs = Vec::with_capacity(raw_sections.len());
        let mut snippets = Vec::with_capacity(raw_sections.len());
        let mut index: BTreeMap<String, Vec<(usize, u8, u8)>> = BTreeMap::new();

        for (doc_id, section) in raw_sections.iter().enumerate() {
            let snippet_id = snippets.len();
            snippets.push(section.generate_excerpt(120));

            docs.push((
                section.slug.clone(),
                section.page_title.clone(),
                section.title.clone(),
                section.anchor.clone(),
                section.ref_id.clone(),
                snippet_id,
                section.kind,
            ));

            let terms_map = Self::tokenize_section(section);
            for (term, (flags, freq)) in terms_map {
                index.entry(term).or_default().push((doc_id, flags, freq));
            }
        }

        SearchIndexPayload {
            version: "2.0".to_string(),
            prompts,
            docs,
            snippets,
            index,
        }
    }

    fn build_sections(
        page: &DocPage,
        page_title: &str,
        ref_id: &str,
        ref_digits: &str,
        sections: &mut Vec<RawDocSection>,
    ) {
        let mut current_heading: Option<(String, String)> = None;
        let mut current_content = String::new();

        for node in &page.ast {
            match node {
                DocNode::Heading {
                    children, anchor, ..
                } => {
                    if let Some((title, anc)) = current_heading.take()
                        && !current_content.trim().is_empty()
                    {
                        sections.push(RawDocSection {
                            slug: page.slug.clone(),
                            page_title: page_title.to_string(),
                            title,
                            anchor: anc,
                            ref_id: ref_id.to_string(),
                            ref_digits: ref_digits.to_string(),
                            content: std::mem::take(&mut current_content),
                            kind: KIND_HEADING,
                        });
                    }

                    let heading_text = children
                        .iter()
                        .map(Self::extract_inline_text)
                        .collect::<String>();
                    current_heading = Some((heading_text, anchor.clone()));
                }
                _ => {
                    Self::append_node_text(node, &mut current_content);
                }
            }
        }

        if let Some((title, anc)) = current_heading
            && !current_content.trim().is_empty()
        {
            sections.push(RawDocSection {
                slug: page.slug.clone(),
                page_title: page_title.to_string(),
                title,
                anchor: anc,
                ref_id: ref_id.to_string(),
                ref_digits: ref_digits.to_string(),
                content: current_content,
                kind: KIND_HEADING,
            });
        }
    }

    fn collect_page_text(nodes: &[DocNode], out: &mut String) {
        evin_doc::analysis::collect_text(nodes, out, evin_doc::analysis::TextOptions::default());
    }

    fn append_node_text(node: &DocNode, out: &mut String) {
        match node {
            DocNode::Heading { children, .. } => {
                for c in children {
                    out.push_str(&Self::extract_inline_text(c));
                    out.push(' ');
                }
            }
            DocNode::Paragraph { children } => {
                for c in children {
                    out.push_str(&Self::extract_inline_text(c));
                    out.push(' ');
                }
            }
            DocNode::CodeBlock { code, .. } => {
                out.push_str(code);
                out.push(' ');
            }
            DocNode::Admonition {
                title, children, ..
            } => {
                if let Some(t) = title {
                    out.push_str(t);
                    out.push(' ');
                }
                Self::collect_page_text(children, out);
            }
            DocNode::List { items, .. } => {
                for item in items {
                    Self::collect_page_text(item, out);
                }
            }
            DocNode::TaskList { items } => {
                for item in items {
                    Self::collect_page_text(&item.body, out);
                }
            }
            DocNode::DefinitionList { items } => {
                for item in items {
                    for t in &item.term {
                        out.push_str(&Self::extract_inline_text(t));
                        out.push(' ');
                    }
                    for c in &item.classifiers {
                        for ci in c {
                            out.push_str(&Self::extract_inline_text(ci));
                            out.push(' ');
                        }
                    }
                    Self::collect_page_text(&item.body, out);
                }
            }
            DocNode::OptionList { items } => {
                for item in items {
                    for flag in &item.flags {
                        out.push_str(flag);
                        out.push(' ');
                    }
                    if let Some(arg) = &item.argument {
                        out.push_str(arg);
                        out.push(' ');
                    }
                    Self::collect_page_text(&item.body, out);
                }
            }
            DocNode::FieldList { items } => {
                for item in items {
                    out.push_str(&item.name);
                    out.push(' ');
                    Self::collect_page_text(&item.body, out);
                }
            }
            DocNode::Stepper { steps } => {
                for step in steps {
                    for t in &step.title {
                        out.push_str(&Self::extract_inline_text(t));
                        out.push(' ');
                    }
                    Self::collect_page_text(&step.body, out);
                }
            }
            DocNode::Table { headers, rows } => {
                for h in headers {
                    for cell in h {
                        out.push_str(&Self::extract_inline_text(cell));
                        out.push(' ');
                    }
                }
                for r in rows {
                    for cell in r {
                        for inl in cell {
                            out.push_str(&Self::extract_inline_text(inl));
                            out.push(' ');
                        }
                    }
                }
            }
            DocNode::Directive { body, .. } => {
                Self::collect_page_text(body, out);
            }
            DocNode::Media { alt, title, .. } => {
                if let Some(alt) = alt {
                    out.push_str(alt);
                    out.push(' ');
                }
                if let Some(title) = title {
                    out.push_str(title);
                    out.push(' ');
                }
            }
            _ => {}
        }
    }

    fn extract_inline_text(inline: &InlineNode) -> String {
        inline.plain_text()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::ast::{DocFormat, FormalReference};
    use std::collections::HashMap;

    #[test]
    fn formal_reference_data_is_included_in_search_entries() {
        let page = DocPage {
            slug: "reference/workspace-yaml".to_string(),
            source_path: "reference/workspace-yaml-4500.rst".to_string(),
            format: DocFormat::ReStructuredText,
            metadata: HashMap::from([(
                "title".to_string(),
                serde_json::Value::String("Workspace configuration".to_string()),
            )]),
            reference: Some(FormalReference {
                id: "REF-4500".to_string(),
                digits: "4500".to_string(),
                number: 4500,
                depends_on: vec!["REF-1000".to_string()],
                referenced_by: vec![],
            }),
            toc: vec![],
            ast: vec![DocNode::Paragraph {
                children: vec![InlineNode::Text {
                    content: "Configure a workspace".to_string(),
                }],
            }],
        };
        let payload = SearchIndexBuilder::build(
            &DocSiteAst {
                version: "1.0".to_string(),
                title: None,
                reference_prefix: "REF".to_string(),
                pages: vec![page],
            },
            vec!["Search the docs".to_string()],
        );
        assert_eq!(payload.docs.len(), 1);
        let doc = &payload.docs[0];
        assert_eq!(doc.0, "reference/workspace-yaml");
        assert_eq!(doc.1, "Workspace configuration");
        assert_eq!(doc.4, "REF-4500");
        assert_eq!(payload.prompts, ["Search the docs"]);
        assert!(payload.index.contains_key("ref-4500"));
        assert!(payload.index.contains_key("4500"));
        let postings = &payload.index["ref-4500"];
        assert_eq!(postings[0].0, 0); // doc_id 0
        assert_eq!(postings[0].1 & BIT_REF_EXACT, BIT_REF_EXACT);
    }
}
