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

use crate::v1::ast::{DocNode, DocPage, InlineNode};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};

const MAGIC: &[u8; 4] = b"EVIS";
const FORMAT_VERSION: u16 = 1;

const FIELD_REFERENCE: u8 = 1 << 0;
const FIELD_TITLE: u8 = 1 << 1;
const FIELD_HEADING: u8 = 1 << 2;
const FIELD_BODY: u8 = 1 << 3;
const FIELD_ROUTE: u8 = 1 << 4;
const FIELD_CODE: u8 = 1 << 5;
const PROSE_OPTIONS: evin_doc::analysis::TextOptions = evin_doc::analysis::TextOptions {
    include_code: false,
    include_media_labels: true,
};

#[derive(Debug, Clone, Serialize)]
pub struct SearchArtifactRecord {
    pub version: String,
    pub locale: String,
    pub path: String,
    pub byte_length: u64,
    pub sha256: String,
}

#[derive(Debug, Clone)]
struct SearchRecord {
    page_id: String,
    route: String,
    page_title: String,
    section_title: String,
    anchor: String,
    reference_id: String,
    snippet: String,
    kind: u8,
}

struct SectionInput<'a> {
    page_id: &'a str,
    route: &'a str,
    page_title: &'a str,
    section_title: &'a str,
    anchor: &'a str,
    reference_id: &'a str,
    text: &'a str,
    code: &'a str,
}

#[derive(Debug, Default, Clone, Copy)]
struct Posting {
    fields: u8,
    frequency: u8,
}

#[derive(Debug, Default)]
struct SearchIndex {
    records: Vec<SearchRecord>,
    terms: BTreeMap<String, BTreeMap<u32, Posting>>,
}

pub fn build_binary(variants: &[(&str, &DocPage, String)]) -> Vec<u8> {
    let mut index = SearchIndex::default();
    for (route, page, page_id) in variants {
        if page
            .metadata
            .get("dev_only")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        {
            continue;
        }
        index.add_page(route, page, page_id);
    }
    index.serialize()
}

impl SearchIndex {
    fn add_page(&mut self, route: &str, page: &DocPage, page_id: &str) {
        let title = metadata_text(page, "title").unwrap_or_else(|| page.slug.clone());
        let reference_id = page
            .reference
            .as_ref()
            .map(|reference| reference.id.clone())
            .unwrap_or_default();
        let mut page_text = String::new();
        evin_doc::analysis::collect_text(&page.ast, &mut page_text, PROSE_OPTIONS);
        let mut page_code = String::new();
        collect_code(&page.ast, &mut page_code);
        let page_record = SearchRecord {
            page_id: page_id.to_string(),
            route: route.to_string(),
            page_title: title.clone(),
            section_title: title.clone(),
            anchor: String::new(),
            reference_id: reference_id.clone(),
            snippet: excerpt(&page_text, 180),
            kind: 0,
        };
        let page_terms = metadata_text(page, "keywords").unwrap_or_default();
        self.push_record(
            page_record,
            &title,
            &page_terms,
            &reference_id,
            &page_text,
            &page_code,
        );

        let mut section_title = String::new();
        let mut anchor = String::new();
        let mut section_text = String::new();
        let mut section_code = String::new();
        for node in &page.ast {
            if let DocNode::Heading {
                children,
                anchor: heading_anchor,
                ..
            } = node
            {
                if !section_title.is_empty() {
                    self.push_section(SectionInput {
                        page_id,
                        route,
                        page_title: &title,
                        section_title: &section_title,
                        anchor: &anchor,
                        reference_id: &reference_id,
                        text: &section_text,
                        code: &section_code,
                    });
                }
                section_title = inline_text(children);
                anchor = heading_anchor.clone();
                section_text.clear();
                section_code.clear();
            } else if !section_title.is_empty() {
                evin_doc::analysis::collect_text(
                    std::slice::from_ref(node),
                    &mut section_text,
                    PROSE_OPTIONS,
                );
                collect_code(std::slice::from_ref(node), &mut section_code);
            }
        }
        if !section_title.is_empty() {
            self.push_section(SectionInput {
                page_id,
                route,
                page_title: &title,
                section_title: &section_title,
                anchor: &anchor,
                reference_id: &reference_id,
                text: &section_text,
                code: &section_code,
            });
        }
    }

    fn push_section(&mut self, section: SectionInput<'_>) {
        let record = SearchRecord {
            page_id: section.page_id.to_string(),
            route: section.route.to_string(),
            page_title: section.page_title.to_string(),
            section_title: section.section_title.to_string(),
            anchor: section.anchor.to_string(),
            reference_id: section.reference_id.to_string(),
            snippet: excerpt(section.text, 180),
            kind: 1,
        };
        self.push_record(
            record,
            section.section_title,
            "",
            section.reference_id,
            section.text,
            section.code,
        );
    }

    fn push_record(
        &mut self,
        record: SearchRecord,
        title: &str,
        keywords: &str,
        reference_id: &str,
        body: &str,
        code: &str,
    ) {
        let document_id = self.records.len() as u32;
        let mut terms = HashMap::<String, Posting>::new();
        add_terms(&mut terms, title, FIELD_TITLE);
        add_terms(&mut terms, keywords, FIELD_TITLE);
        add_terms(&mut terms, &record.section_title, FIELD_HEADING);
        add_terms(&mut terms, &record.route, FIELD_ROUTE);
        add_terms(&mut terms, reference_id, FIELD_REFERENCE);
        add_terms(&mut terms, body, FIELD_BODY);
        add_terms(&mut terms, code, FIELD_CODE);
        for (term, posting) in terms {
            self.terms
                .entry(term)
                .or_default()
                .insert(document_id, posting);
        }
        self.records.push(record);
    }

    fn serialize(self) -> Vec<u8> {
        let mut unique_strings = BTreeSet::new();
        for record in &self.records {
            unique_strings.extend([
                record.page_id.as_str(),
                record.route.as_str(),
                record.page_title.as_str(),
                record.section_title.as_str(),
                record.anchor.as_str(),
                record.reference_id.as_str(),
                record.snippet.as_str(),
            ]);
        }
        unique_strings.extend(self.terms.keys().map(String::as_str));
        let strings = unique_strings
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let string_ids = strings
            .iter()
            .enumerate()
            .map(|(index, value)| (value.as_str(), index as u32))
            .collect::<HashMap<_, _>>();

        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&(self.records.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(self.terms.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(strings.len() as u32).to_le_bytes());

        for value in &strings {
            write_string(&mut bytes, value);
        }

        for record in &self.records {
            for value in [
                record.page_id.as_str(),
                record.route.as_str(),
                record.page_title.as_str(),
                record.section_title.as_str(),
                record.anchor.as_str(),
                record.reference_id.as_str(),
                record.snippet.as_str(),
            ] {
                write_varint(&mut bytes, string_ids[value]);
            }
            bytes.push(record.kind);
        }

        for (term, postings) in &self.terms {
            write_varint(&mut bytes, string_ids[term.as_str()]);
            write_varint(&mut bytes, postings.len() as u32);
            let mut previous = 0u32;
            for (index, (document_id, posting)) in postings.iter().enumerate() {
                let delta = if index == 0 {
                    *document_id
                } else {
                    *document_id - previous
                };
                write_varint(&mut bytes, delta);
                bytes.push(posting.fields);
                bytes.push(posting.frequency);
                previous = *document_id;
            }
        }
        bytes
    }
}

fn collect_code(nodes: &[DocNode], out: &mut String) {
    struct CodeCollector<'a>(&'a mut String);
    impl evin_doc::analysis::Visitor for CodeCollector<'_> {
        fn visit_node(&mut self, node: &DocNode) {
            let code = match node {
                DocNode::CodeBlock { code, .. } => Some(code.as_str()),
                DocNode::Tape { script, .. } => Some(script.as_str()),
                _ => None,
            };
            if let Some(code) = code {
                self.0.push_str(code);
                self.0.push(' ');
            }
        }
    }
    evin_doc::analysis::walk_nodes(nodes, &mut CodeCollector(out));
}

fn metadata_text(page: &DocPage, key: &str) -> Option<String> {
    let value = page.metadata.get(key)?;
    match value {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Array(values) => Some(
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => None,
    }
}

fn inline_text(inlines: &[InlineNode]) -> String {
    inlines
        .iter()
        .map(InlineNode::plain_text)
        .collect::<Vec<_>>()
        .join("")
}

fn excerpt(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let mut result = normalized
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    if let Some(space) = result.rfind(' ') {
        result.truncate(space);
    }
    result.push('…');
    result
}

fn add_terms(terms: &mut HashMap<String, Posting>, text: &str, field: u8) {
    for raw in text.split(|character: char| {
        !character.is_alphanumeric() && character != '-' && character != '_'
    }) {
        let term = raw.to_lowercase();
        if term.chars().count() < 2 {
            continue;
        }
        let posting = terms.entry(term).or_default();
        posting.fields |= field;
        posting.frequency = posting.frequency.saturating_add(1);
    }
}

fn write_string(bytes: &mut Vec<u8>, value: &str) {
    write_varint(bytes, value.len() as u32);
    bytes.extend_from_slice(value.as_bytes());
}

fn write_varint(bytes: &mut Vec<u8>, mut value: u32) {
    while value >= 0x80 {
        bytes.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::ast::{DocFormat, DocNode};
    use std::collections::HashMap;

    #[test]
    fn writes_a_deterministic_binary_index_with_content_and_anchors() {
        let page = DocPage {
            slug: "guide/install".into(),
            source_path: "guide/10.install.md".into(),
            format: DocFormat::Markdown,
            metadata: HashMap::from([
                (
                    "title".into(),
                    serde_json::Value::String("Install Evin".into()),
                ),
                (
                    "keywords".into(),
                    serde_json::Value::Array(vec![serde_json::Value::String("setup".into())]),
                ),
            ]),
            reference: None,
            toc: Vec::new(),
            ast: vec![
                DocNode::Heading {
                    level: 2,
                    text: "Install Evin".into(),
                    children: vec![InlineNode::Text {
                        content: "Install Evin".into(),
                    }],
                    anchor: "install-evin".into(),
                },
                DocNode::Paragraph {
                    children: vec![InlineNode::Text {
                        content: "Run cargo install evin.".into(),
                    }],
                },
            ],
        };
        let variants = [("/guide/install", &page, "page-1".to_string())];
        let first = build_binary(&variants);
        let second = build_binary(&variants);
        assert_eq!(first, second);
        assert_eq!(&first[..4], MAGIC);
        assert_eq!(u16::from_le_bytes([first[4], first[5]]), FORMAT_VERSION);
        assert!(first.len() > 100);
    }
}
