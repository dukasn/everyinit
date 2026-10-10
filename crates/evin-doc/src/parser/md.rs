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

use crate::ast::{
    DefinitionListItem, DiagramKind, DocNode, FieldListItem, InlineNode, MediaKind, NumberingKind,
    OptionListItem, StepItem, TaskListItem, TocEntry, inferred_description,
};
use crate::tape::parse_tape_fence_info;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

pub struct MarkdownParseResult {
    pub metadata: HashMap<String, serde_json::Value>,
    pub toc: Vec<TocEntry>,
    pub ast: Vec<DocNode>,
}

pub struct MarkdownParser;

impl MarkdownParser {
    fn parse_fence_info(info: &str) -> (Option<String>, Option<String>) {
        let info = info.trim();
        if info.is_empty() {
            return (None, None);
        }
        let language_end = info.find(char::is_whitespace).unwrap_or(info.len());
        let language = Some(info[..language_end].to_string());
        let raw_title = info[language_end..].trim();
        let title = (!raw_title.is_empty()).then(|| {
            if raw_title.len() >= 2
                && ((raw_title.starts_with('"') && raw_title.ends_with('"'))
                    || (raw_title.starts_with('\'') && raw_title.ends_with('\'')))
            {
                raw_title[1..raw_title.len() - 1].to_string()
            } else {
                raw_title.to_string()
            }
        });
        (language, title.filter(|title| !title.is_empty()))
    }

    pub fn parse(content: &str, extract_frontmatter: bool) -> MarkdownParseResult {
        let (mut metadata, markdown_body) = if extract_frontmatter {
            Self::extract_yaml_frontmatter(content)
        } else {
            (HashMap::new(), content)
        };

        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);
        options.insert(Options::ENABLE_MATH);

        let preprocessed = Self::preprocess_containers(markdown_body);
        let parser = Parser::new_ext(&preprocessed, options);

        let mut ast = Vec::new();
        let mut toc = Vec::new();

        let events: Vec<Event> = parser.collect();
        let mut idx = 0;

        while idx < events.len() {
            if let Some((node, next_idx)) = Self::parse_block(&events, idx, &mut toc) {
                ast.push(node);
                idx = next_idx;
            } else {
                idx += 1;
            }
        }

        Self::resolve_fallback_metadata(&mut metadata, &ast);

        MarkdownParseResult { metadata, toc, ast }
    }

    fn resolve_fallback_metadata(
        metadata: &mut HashMap<String, serde_json::Value>,
        ast: &[DocNode],
    ) {
        if !metadata.contains_key("title") {
            for node in ast {
                if let DocNode::Heading { text, .. } = node
                    && !text.trim().is_empty()
                {
                    metadata.insert("title".to_string(), serde_json::Value::String(text.clone()));
                    break;
                }
            }
        }

        if !metadata.contains_key("description") {
            for node in ast {
                if let DocNode::Paragraph { children } = node {
                    let text = children.iter().map(|c| c.plain_text()).collect::<String>();
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        let desc = inferred_description(trimmed);
                        metadata.insert("description".to_string(), serde_json::Value::String(desc));
                        break;
                    }
                }
            }
        }
    }

    fn extract_yaml_frontmatter(content: &str) -> (HashMap<String, serde_json::Value>, &str) {
        let trimmed = content.trim_start();
        if trimmed.starts_with("---")
            && let Some(end_idx) = trimmed[3..].find("\n---")
        {
            let yaml_str = &trimmed[3..3 + end_idx];
            let rest = &trimmed[3 + end_idx + 4..];

            if let Ok(value) = serde_yaml::from_str::<serde_json::Value>(yaml_str)
                && let Some(map) = value.as_object()
            {
                let mut meta = HashMap::new();
                for (k, v) in map {
                    meta.insert(k.clone(), v.clone());
                }
                return (meta, rest);
            }
        }
        (HashMap::new(), content)
    }

    fn parse_block<'a>(
        events: &'a [Event<'a>],
        mut start: usize,
        toc: &mut Vec<TocEntry>,
    ) -> Option<(DocNode, usize)> {
        match &events[start] {
            Event::DisplayMath(latex) => Some((
                DocNode::MathBlock {
                    latex: latex.to_string(),
                },
                start + 1,
            )),
            Event::Html(html) | Event::InlineHtml(html) => Some((
                Self::parse_video_html(html).unwrap_or_else(|| DocNode::RawHtml {
                    content: html.to_string(),
                }),
                start + 1,
            )),
            Event::Start(Tag::Heading { level, .. }) => {
                let lvl_num = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                let mut children = Vec::new();
                start += 1;
                while start < events.len() {
                    match &events[start] {
                        Event::End(TagEnd::Heading(_)) => {
                            start += 1;
                            break;
                        }
                        _ => {
                            if let Some((node, next_idx)) = Self::parse_inline(events, start) {
                                children.push(node);
                                start = next_idx;
                            } else {
                                start += 1;
                            }
                        }
                    }
                }
                let plain_text = children.iter().map(|c| c.plain_text()).collect::<String>();
                let anchor = Self::slugify(&plain_text);
                toc.push(TocEntry {
                    level: lvl_num,
                    text: plain_text.clone(),
                    anchor: anchor.clone(),
                });
                Some((
                    DocNode::Heading {
                        level: lvl_num,
                        text: plain_text,
                        children,
                        anchor,
                    },
                    start,
                ))
            }
            Event::Start(Tag::Paragraph) => {
                if start + 2 < events.len()
                    && let Event::DisplayMath(latex) = &events[start + 1]
                    && let Event::End(TagEnd::Paragraph) = &events[start + 2]
                {
                    return Some((
                        DocNode::MathBlock {
                            latex: latex.to_string(),
                        },
                        start + 3,
                    ));
                }

                let mut inlines = Vec::new();
                let mut html = String::new();
                start += 1;
                while start < events.len() {
                    match &events[start] {
                        Event::End(TagEnd::Paragraph) => {
                            start += 1;
                            break;
                        }
                        Event::Html(value) | Event::InlineHtml(value) => {
                            html.push_str(value);
                            inlines.push(InlineNode::Text {
                                content: value.to_string(),
                            });
                            start += 1;
                        }
                        _ => {
                            if let Some((inline, next)) = Self::parse_inline(events, start) {
                                inlines.push(inline);
                                start = next;
                            } else {
                                start += 1;
                            }
                        }
                    }
                }
                if let Some(video) = Self::parse_video_html(&html) {
                    Some((video, start))
                } else if let Some(field_list) = Self::try_parse_field_list(&inlines) {
                    Some((field_list, start))
                } else if let Some(option_list) = Self::try_parse_option_list(&inlines) {
                    Some((option_list, start))
                } else if let Some(def_list) = Self::try_parse_def_list(&inlines) {
                    Some((def_list, start))
                } else {
                    Some((DocNode::Paragraph { children: inlines }, start))
                }
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(l) if !l.is_empty() => {
                        Some(l.to_string())
                    }
                    _ => None,
                };
                let mut code = String::new();
                start += 1;
                while start < events.len() {
                    match &events[start] {
                        Event::End(TagEnd::CodeBlock) => {
                            start += 1;
                            break;
                        }
                        Event::Text(t) => code.push_str(t),
                        _ => {}
                    }
                    start += 1;
                }
                if let Some(l) = lang.as_deref()
                    && (l == "tape" || l.starts_with("tape "))
                {
                    let node = match parse_tape_fence_info(l) {
                        Ok(config) => DocNode::Tape {
                            script: code,
                            config,
                            rendered_svg: None,
                        },
                        Err(error) => DocNode::Directive {
                            name: "tape-error".to_string(),
                            args: vec![error],
                            body: Vec::new(),
                        },
                    };
                    Some((node, start))
                } else if let Some(l) = lang.as_deref()
                    && l.starts_with("component ")
                {
                    let parts: Vec<&str> = l.split_whitespace().collect();
                    let name = parts.get(1).copied().unwrap_or_default().to_string();
                    let args = parts
                        .iter()
                        .skip(2)
                        .map(|part| (*part).to_string())
                        .collect();
                    Some((
                        DocNode::Directive {
                            name,
                            args,
                            body: Self::parse(&code, false).ast,
                        },
                        start,
                    ))
                } else if let Some(l) = lang.as_deref()
                    && (l.starts_with("mermaid")
                        || l.starts_with("svgbob")
                        || l.starts_with("pikchr"))
                {
                    let (diagram_language, title) = Self::parse_fence_info(l);
                    let kind = match diagram_language.as_deref().unwrap_or("mermaid") {
                        "svgbob" => DiagramKind::Svgbob,
                        "pikchr" => DiagramKind::Pikchr,
                        _ => DiagramKind::Mermaid,
                    };

                    Some((
                        DocNode::Diagram {
                            kind,
                            code,
                            title,
                            caption: None,
                            alt: None,
                            rendered_mermaid: None,
                        },
                        start,
                    ))
                } else if lang.as_deref() == Some("steps") || lang.as_deref() == Some("stepper") {
                    Some((Self::parse_markdown_steps(&code), start))
                } else if lang.as_deref() == Some("math") {
                    Some((DocNode::MathBlock { latex: code }, start))
                } else if let Some(l) = lang.as_deref()
                    && (l.starts_with("lsp") || l.starts_with("symbol") || l.starts_with("keyword"))
                {
                    let parts: Vec<&str> = l.split_whitespace().collect();
                    let directive_name = parts.first().copied().unwrap_or("lsp");
                    let symbol_name = parts.get(1).copied().unwrap_or("").to_string();

                    let (header_part, body_part) = if let Some(sep) = code.find("\n---\n") {
                        (&code[..sep], &code[sep + 5..])
                    } else if let Some(sep) = code.find("\n---") {
                        (&code[..sep], &code[sep + 4..])
                    } else {
                        ("", code.as_str())
                    };

                    let mut meta_map: HashMap<String, String> = HashMap::new();
                    let mut target = None;
                    if !header_part.trim().is_empty()
                        && let Ok(val) = serde_yaml::from_str::<serde_json::Value>(header_part)
                        && let Some(obj) = val.as_object()
                    {
                        for (k, v) in obj {
                            if k.eq_ignore_ascii_case("target") {
                                target = v.as_array().map(|parts| {
                                    parts
                                        .iter()
                                        .filter_map(|part| part.as_str())
                                        .map(str::to_owned)
                                        .collect::<Vec<_>>()
                                });
                            } else if let Some(s) = v.as_str() {
                                meta_map.insert(k.to_ascii_lowercase(), s.to_string());
                            }
                        }
                    }

                    if meta_map.is_empty() {
                        for line in header_part.lines().chain(body_part.lines()) {
                            let trimmed = line.trim();
                            if let Some((k, v)) = trimmed.split_once(':') {
                                let k = k.trim().trim_start_matches(':').to_ascii_lowercase();
                                if [
                                    "name", "kind", "synopsis", "summary", "category", "handler",
                                    "binding", "language", "title",
                                ]
                                .contains(&k.as_str())
                                {
                                    meta_map.insert(k, v.trim().to_string());
                                }
                            }
                        }
                    }

                    let final_name =
                        if let Some(target) = target.as_ref().filter(|target| !target.is_empty()) {
                            target.join(".")
                        } else if !symbol_name.is_empty() {
                            symbol_name
                        } else {
                            meta_map.get("name").cloned().unwrap_or_default()
                        };

                    let kind = meta_map.get("kind").cloned().unwrap_or_else(|| {
                        if directive_name == "keyword" {
                            "keyword".to_string()
                        } else {
                            "builtin".to_string()
                        }
                    });
                    let synopsis = meta_map.get("synopsis").cloned().unwrap_or_default();
                    let summary = meta_map.get("summary").cloned().unwrap_or_default();
                    let category = meta_map
                        .get("category")
                        .cloned()
                        .unwrap_or_else(|| "general".to_string());
                    let binding = meta_map
                        .get("binding")
                        .cloned()
                        .or_else(|| meta_map.get("handler").cloned())
                        .unwrap_or_default();

                    let inner_ast = if body_part.trim().is_empty() {
                        Vec::new()
                    } else {
                        Self::parse(body_part, false).ast
                    };

                    Some((
                        DocNode::LspSymbol {
                            name: final_name,
                            target,
                            language: meta_map.get("language").cloned(),
                            title: meta_map.get("title").cloned(),
                            kind,
                            synopsis,
                            summary,
                            category,
                            binding,
                            children: inner_ast,
                        },
                        start,
                    ))
                } else {
                    let (language, title) = lang
                        .as_deref()
                        .map(Self::parse_fence_info)
                        .unwrap_or((None, None));
                    Some((
                        DocNode::CodeBlock {
                            language,
                            title,
                            code,
                        },
                        start,
                    ))
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                let mut children = Vec::new();
                start += 1;
                while start < events.len() {
                    match &events[start] {
                        Event::End(TagEnd::BlockQuote(_)) => {
                            start += 1;
                            break;
                        }
                        _ => {
                            if let Some((node, next)) = Self::parse_block(events, start, toc) {
                                children.push(node);
                                start = next;
                            } else {
                                start += 1;
                            }
                        }
                    }
                }
                // Check if this blockquote is an GFM admonition (e.g., [!NOTE], [!TIP], [!IMPORTANT], [!WARNING], [!CAUTION])
                let (kind, title) = Self::detect_and_strip_admonition(&mut children);
                if let Some(k) = kind {
                    Some((
                        DocNode::Admonition {
                            kind: k,
                            title,
                            children,
                        },
                        start,
                    ))
                } else {
                    Some((
                        DocNode::Admonition {
                            kind: "quote".to_string(),
                            title: None,
                            children,
                        },
                        start,
                    ))
                }
            }
            Event::Start(Tag::List(first_item_number)) => {
                let ordered = first_item_number.is_some();
                let start_idx = first_item_number.unwrap_or(1) as usize;
                let mut items = Vec::new();
                let mut task_items = Vec::new();
                let mut is_task_list = false;

                start += 1;
                while start < events.len() {
                    match &events[start] {
                        Event::End(TagEnd::List(_)) => {
                            start += 1;
                            break;
                        }
                        Event::Start(Tag::Item) => {
                            let mut item_nodes = Vec::new();
                            let mut inlines = Vec::new();
                            let mut task_checked = None;
                            start += 1;
                            while start < events.len() {
                                match &events[start] {
                                    Event::End(TagEnd::Item) => {
                                        if !inlines.is_empty() {
                                            item_nodes.push(DocNode::Paragraph {
                                                children: std::mem::take(&mut inlines),
                                            });
                                        }
                                        start += 1;
                                        break;
                                    }
                                    Event::TaskListMarker(checked) => {
                                        task_checked = Some(*checked);
                                        is_task_list = true;
                                        start += 1;
                                    }
                                    Event::SoftBreak | Event::HardBreak => {
                                        inlines.push(InlineNode::Text {
                                            content: "\n".to_string(),
                                        });
                                        start += 1;
                                    }
                                    _ => {
                                        if let Some((node, next)) =
                                            Self::parse_block(events, start, toc)
                                        {
                                            if !inlines.is_empty() {
                                                item_nodes.push(DocNode::Paragraph {
                                                    children: std::mem::take(&mut inlines),
                                                });
                                            }
                                            item_nodes.push(node);
                                            start = next;
                                        } else if let Some((inline, next)) =
                                            Self::parse_inline(events, start)
                                        {
                                            inlines.push(inline);
                                            start = next;
                                        } else {
                                            start += 1;
                                        }
                                    }
                                }
                            }
                            if !inlines.is_empty() {
                                item_nodes.push(DocNode::Paragraph { children: inlines });
                            }
                            if let Some(checked) = task_checked {
                                task_items.push(TaskListItem {
                                    checked,
                                    body: item_nodes.clone(),
                                });
                            }
                            items.push(item_nodes);
                        }
                        _ => start += 1,
                    }
                }

                if is_task_list {
                    Some((DocNode::TaskList { items: task_items }, start))
                } else {
                    Some((
                        DocNode::List {
                            ordered,
                            numbering: NumberingKind::Arabic,
                            start: start_idx,
                            items,
                        },
                        start,
                    ))
                }
            }
            Event::Start(Tag::Table(_)) => {
                let mut headers = Vec::new();
                let mut rows = Vec::new();
                let mut in_head = false;
                let mut current_row: Vec<Vec<InlineNode>> = Vec::new();
                let mut current_cell: Vec<InlineNode> = Vec::new();
                start += 1;
                while start < events.len() {
                    match &events[start] {
                        Event::End(TagEnd::Table) => {
                            start += 1;
                            break;
                        }
                        Event::Start(Tag::TableHead) => {
                            in_head = true;
                            start += 1;
                        }
                        Event::End(TagEnd::TableHead) => {
                            headers = std::mem::take(&mut current_row);
                            in_head = false;
                            start += 1;
                        }
                        Event::Start(Tag::TableRow) => {
                            current_row.clear();
                            start += 1;
                        }
                        Event::End(TagEnd::TableRow) => {
                            if !in_head {
                                rows.push(current_row.clone());
                            }
                            start += 1;
                        }
                        Event::Start(Tag::TableCell) => {
                            current_cell.clear();
                            start += 1;
                        }
                        Event::End(TagEnd::TableCell) => {
                            current_row.push(std::mem::take(&mut current_cell));
                            start += 1;
                        }
                        _ => {
                            if let Some((inline, next)) = Self::parse_inline(events, start) {
                                current_cell.push(inline);
                                start = next;
                            } else {
                                start += 1;
                            }
                        }
                    }
                }
                Some((DocNode::Table { headers, rows }, start))
            }
            Event::Rule => Some((DocNode::HorizontalRule, start + 1)),
            _ => None,
        }
    }

    fn parse_video_html(html: &str) -> Option<DocNode> {
        static VIDEO: OnceLock<Regex> = OnceLock::new();
        static ATTRIBUTE: OnceLock<Regex> = OnceLock::new();
        let video = VIDEO.get_or_init(|| {
            Regex::new(r#"(?is)^<video(?P<attributes>(?:\s+[a-zA-Z_:][-a-zA-Z0-9_:.]*(?:\s*=\s*(?:\"[^\"]*\"|'[^']*'|[^\s\"'=<>`]+))?)*)\s*></video>$"#).unwrap()
        });
        let attribute = ATTRIBUTE.get_or_init(|| {
            Regex::new(r#"(?x)\s+([a-zA-Z_:][-a-zA-Z0-9_:.]*)(?:\s*=\s*(?:\"([^\"]*)\"|'([^']*)'|([^\s\"'=<>`]+)))?"#).unwrap()
        });
        let attributes = video.captures(html.trim())?.name("attributes")?.as_str();
        let mut values = HashMap::new();
        let mut controls = false;

        for captures in attribute.captures_iter(attributes) {
            let name = captures.get(1)?.as_str().to_ascii_lowercase();
            let value = captures
                .get(2)
                .or_else(|| captures.get(3))
                .or_else(|| captures.get(4))
                .map(|value| value.as_str().to_string());
            match name.as_str() {
                "src" | "poster" | "aria-label"
                    if value.is_some() && !values.contains_key(&name) =>
                {
                    values.insert(name, value.unwrap());
                }
                "controls" if value.is_none() && !controls => controls = true,
                _ => return None,
            }
        }

        controls.then(|| DocNode::Media {
            kind: MediaKind::Video,
            src: values.remove("src").unwrap_or_default(),
            alt: values.remove("aria-label"),
            title: None,
            poster: values.remove("poster"),
        })
    }

    fn parse_inline<'a>(events: &'a [Event<'a>], start: usize) -> Option<(InlineNode, usize)> {
        match &events[start] {
            Event::Text(t) => Some((
                InlineNode::Text {
                    content: t.to_string(),
                },
                start + 1,
            )),
            Event::Code(c) => Some((
                InlineNode::Code {
                    content: c.to_string(),
                },
                start + 1,
            )),
            Event::Start(Tag::Strong) => {
                let mut children = Vec::new();
                let mut idx = start + 1;
                while idx < events.len() {
                    if matches!(&events[idx], Event::End(TagEnd::Strong)) {
                        idx += 1;
                        break;
                    }
                    if let Some((child, next)) = Self::parse_inline(events, idx) {
                        children.push(child);
                        idx = next;
                    } else {
                        idx += 1;
                    }
                }
                Some((InlineNode::Strong { children }, idx))
            }
            Event::Start(Tag::Emphasis) => {
                let mut children = Vec::new();
                let mut idx = start + 1;
                while idx < events.len() {
                    if matches!(&events[idx], Event::End(TagEnd::Emphasis)) {
                        idx += 1;
                        break;
                    }
                    if let Some((child, next)) = Self::parse_inline(events, idx) {
                        children.push(child);
                        idx = next;
                    } else {
                        idx += 1;
                    }
                }
                Some((InlineNode::Emphasis { children }, idx))
            }
            Event::Start(Tag::Link {
                dest_url, title, ..
            }) => {
                let href = dest_url.to_string();
                let title_opt = if title.is_empty() {
                    None
                } else {
                    Some(title.to_string())
                };
                let mut text = String::new();
                let mut code_events = 0usize;
                let mut non_code_events = 0usize;
                let mut idx = start + 1;
                while idx < events.len() {
                    match &events[idx] {
                        Event::End(TagEnd::Link) => {
                            idx += 1;
                            break;
                        }
                        Event::Code(code) => {
                            text.push_str(code);
                            code_events += 1;
                        }
                        Event::Text(t) => {
                            text.push_str(t);
                            non_code_events += 1;
                        }
                        _ => non_code_events += 1,
                    }
                    idx += 1;
                }
                Some((
                    InlineNode::Link {
                        href,
                        title: title_opt,
                        content: text,
                        code: code_events == 1 && non_code_events == 0,
                    },
                    idx,
                ))
            }
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
                let src = dest_url.to_string();
                let title_opt = if title.is_empty() {
                    None
                } else {
                    Some(title.to_string())
                };
                let mut alt = String::new();
                let mut idx = start + 1;
                while idx < events.len() {
                    match &events[idx] {
                        Event::End(TagEnd::Image) => {
                            idx += 1;
                            break;
                        }
                        Event::Text(t) => alt.push_str(t),
                        _ => {}
                    }
                    idx += 1;
                }
                Some((
                    InlineNode::Image {
                        src,
                        alt,
                        title: title_opt,
                    },
                    idx,
                ))
            }
            Event::InlineMath(latex) | Event::DisplayMath(latex) => Some((
                InlineNode::Math {
                    latex: latex.to_string(),
                },
                start + 1,
            )),
            Event::SoftBreak | Event::HardBreak => Some((
                InlineNode::Text {
                    content: "\n".to_string(),
                },
                start + 1,
            )),
            _ => None,
        }
    }

    fn preprocess_containers(content: &str) -> String {
        let mut out = String::with_capacity(content.len());
        let mut in_code_fence = false;
        let mut fence_char = '`';
        let mut fence_len = 0;
        let mut container_depth = 0;

        for line in content.lines() {
            let trimmed = line.trim_start();

            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                let current_fence_char = trimmed.chars().next().unwrap();
                let current_fence_len = trimmed
                    .chars()
                    .take_while(|&c| c == current_fence_char)
                    .count();

                if !in_code_fence {
                    in_code_fence = true;
                    fence_char = current_fence_char;
                    fence_len = current_fence_len;
                    out.push_str(line);
                    out.push('\n');
                    continue;
                } else if current_fence_char == fence_char && current_fence_len >= fence_len {
                    in_code_fence = false;
                    out.push_str(line);
                    out.push('\n');
                    continue;
                }
            }

            if !in_code_fence {
                if trimmed.starts_with("::: steps") || trimmed.starts_with("::: stepper") {
                    let indent = line.len() - trimmed.len();
                    out.push_str(&" ".repeat(indent));
                    out.push_str("````steps\n");
                    container_depth += 1;
                    continue;
                } else if trimmed == ":::" && container_depth > 0 {
                    let indent = line.len() - trimmed.len();
                    out.push_str(&" ".repeat(indent));
                    out.push_str("````\n");
                    container_depth -= 1;
                    continue;
                }
            }

            out.push_str(line);
            out.push('\n');
        }

        out
    }

    fn split_first_line_inlines(inlines: Vec<InlineNode>) -> (Vec<InlineNode>, Vec<InlineNode>) {
        let mut title_inlines = Vec::new();
        let mut body_inlines = Vec::new();
        let mut split_found = false;

        for inline in inlines {
            if split_found {
                body_inlines.push(inline);
                continue;
            }

            match inline {
                InlineNode::Text { content } => {
                    if let Some((before, after)) = content.split_once('\n') {
                        if !before.is_empty() {
                            title_inlines.push(InlineNode::Text {
                                content: before.to_string(),
                            });
                        }
                        let after_trimmed = after.trim_start();
                        if !after_trimmed.is_empty() {
                            body_inlines.push(InlineNode::Text {
                                content: after_trimmed.to_string(),
                            });
                        }
                        split_found = true;
                    } else {
                        title_inlines.push(InlineNode::Text { content });
                    }
                }
                other => {
                    title_inlines.push(other);
                }
            }
        }

        (title_inlines, body_inlines)
    }

    fn parse_markdown_steps(code: &str) -> DocNode {
        let parsed = Self::parse(code, false);
        let mut steps = Vec::new();

        for node in parsed.ast {
            if let DocNode::List { items, .. } = node {
                for (idx, item_nodes) in items.into_iter().enumerate() {
                    let default_num = (steps.len() + 1).to_string();
                    if item_nodes.is_empty() {
                        continue;
                    }
                    let mut item_iter = item_nodes.into_iter();
                    let first_node = item_iter.next().unwrap();
                    let mut body_nodes: Vec<DocNode> = item_iter.collect();

                    let (title, status, extra_body) = match first_node {
                        DocNode::Paragraph { children } => {
                            let (title_inlines, body_inlines) =
                                Self::split_first_line_inlines(children);
                            let (clean_title, s) =
                                Self::extract_markdown_step_status(title_inlines);
                            let extra = if !body_inlines.is_empty() {
                                Some(DocNode::Paragraph {
                                    children: body_inlines,
                                })
                            } else {
                                None
                            };
                            (clean_title, s, extra)
                        }
                        other => {
                            let title = vec![InlineNode::Text {
                                content: format!("Step {}", idx + 1),
                            }];
                            (title, None, Some(other))
                        }
                    };

                    if let Some(extra) = extra_body {
                        body_nodes.insert(0, extra);
                    }

                    steps.push(StepItem {
                        badge: Some(default_num),
                        title,
                        status,
                        body: body_nodes,
                    });
                }
            } else if let DocNode::Heading {
                level: _, children, ..
            } = node
            {
                let default_num = (steps.len() + 1).to_string();
                let (title, status) = Self::extract_markdown_step_status(children);
                steps.push(StepItem {
                    badge: Some(default_num),
                    title,
                    status,
                    body: Vec::new(),
                });
            } else if let Some(last_step) = steps.last_mut() {
                last_step.body.push(node);
            }
        }

        DocNode::Stepper { steps }
    }

    fn trim_trailing_chars_from_inlines(inlines: &mut Vec<InlineNode>, mut count: usize) {
        for inline in inlines.iter_mut().rev() {
            if count == 0 {
                break;
            }
            match inline {
                InlineNode::Text { content } => {
                    let char_count = content.chars().count();
                    if char_count <= count {
                        count -= char_count;
                        content.clear();
                    } else {
                        let keep = char_count - count;
                        *content = content.chars().take(keep).collect();
                        count = 0;
                    }
                }
                InlineNode::Strong { children } => {
                    Self::trim_trailing_chars_from_inlines(children, count);
                    count = 0;
                }
                _ => {}
            }
        }
        inlines.retain(|n| match n {
            InlineNode::Text { content } => !content.is_empty(),
            _ => true,
        });
    }

    fn extract_markdown_step_status(inlines: Vec<InlineNode>) -> (Vec<InlineNode>, Option<String>) {
        let plain = inlines.iter().map(|n| n.plain_text()).collect::<String>();
        let mut status = None;
        let trimmed = plain.trim_end();
        let mut trim_len = 0;

        if let Some(open) = trimmed.rfind('[') {
            if let Some(close) = trimmed[open..].find(']') {
                let candidate = trimmed[open + 1..open + close].trim().to_ascii_lowercase();
                if [
                    "done",
                    "active",
                    "in-progress",
                    "upcoming",
                    "completed",
                    "current",
                ]
                .contains(&candidate.as_str())
                {
                    let st = match candidate.as_str() {
                        "completed" => "done".to_string(),
                        "current" => "active".to_string(),
                        other => other.to_string(),
                    };
                    status = Some(st);
                    let start_idx = if open > 0 && plain.as_bytes()[open - 1] == b' ' {
                        open - 1
                    } else {
                        open
                    };
                    trim_len = plain.chars().count() - plain[..start_idx].chars().count();
                }
            }
        } else if let Some(open) = trimmed.rfind('(')
            && let Some(close) = trimmed[open..].find(')')
        {
            let candidate = trimmed[open + 1..open + close].trim().to_ascii_lowercase();
            if [
                "done",
                "active",
                "in-progress",
                "upcoming",
                "completed",
                "current",
            ]
            .contains(&candidate.as_str())
            {
                let st = match candidate.as_str() {
                    "completed" => "done".to_string(),
                    "current" => "active".to_string(),
                    other => other.to_string(),
                };
                status = Some(st);
                let start_idx = if open > 0 && plain.as_bytes()[open - 1] == b' ' {
                    open - 1
                } else {
                    open
                };
                trim_len = plain.chars().count() - plain[..start_idx].chars().count();
            }
        }

        let mut clean_inlines = inlines;
        if trim_len > 0 {
            Self::trim_trailing_chars_from_inlines(&mut clean_inlines, trim_len);
        }

        (clean_inlines, status)
    }

    pub fn parse_inlines_from_str(text: &str) -> Vec<InlineNode> {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_MATH);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        let parser = Parser::new_ext(text, options);
        let events: Vec<Event> = parser.collect();
        let mut inlines = Vec::new();
        let mut idx = 0;
        while idx < events.len() {
            match &events[idx] {
                Event::Start(Tag::Paragraph) => {
                    idx += 1;
                }
                Event::End(TagEnd::Paragraph) => {
                    idx += 1;
                }
                _ => {
                    if let Some((inline, next)) = Self::parse_inline(&events, idx) {
                        inlines.push(inline);
                        idx = next;
                    } else {
                        idx += 1;
                    }
                }
            }
        }
        inlines
    }

    fn try_parse_field_list(inlines: &[InlineNode]) -> Option<DocNode> {
        let plain = inlines.iter().map(|n| n.plain_text()).collect::<String>();
        let lines: Vec<&str> = plain
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();
        if lines.is_empty()
            || !lines
                .iter()
                .all(|l| l.starts_with(':') && l[1..].contains(':'))
        {
            return None;
        }

        let mut items = Vec::new();
        for line in lines {
            let colon_idx = line[1..].find(':')?;
            let key = line[1..1 + colon_idx].trim();
            let val = line[1 + colon_idx + 1..].trim();
            if key.is_empty() {
                return None;
            }
            let val_inlines = Self::parse_inlines_from_str(val);
            let body = if val_inlines.is_empty() {
                Vec::new()
            } else {
                vec![DocNode::Paragraph {
                    children: val_inlines,
                }]
            };
            items.push(FieldListItem {
                name: key.to_string(),
                body,
            });
        }

        if items.is_empty() {
            None
        } else {
            Some(DocNode::FieldList { items })
        }
    }

    fn try_parse_option_list(inlines: &[InlineNode]) -> Option<DocNode> {
        let plain = inlines.iter().map(|n| n.plain_text()).collect::<String>();
        let lines: Vec<&str> = plain
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();
        if lines.is_empty()
            || !lines.iter().all(|l| {
                l.starts_with('-')
                    && !l.starts_with("- ")
                    && !l.starts_with("-- ")
                    && !l.starts_with("- [")
            })
        {
            return None;
        }

        let mut items = Vec::new();
        for line in lines {
            let (synopsis_str, desc_str) = if let Some(sep_pos) = line.find("  ") {
                (line[..sep_pos].trim(), line[sep_pos..].trim())
            } else {
                (line.trim(), "")
            };

            let mut flags = Vec::new();
            let mut common_arg = None;

            for part in synopsis_str.split(',') {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                if let Some(eq_pos) = part.find('=') {
                    let flag = part[..eq_pos].trim().to_string();
                    let arg = part[eq_pos + 1..].trim().to_string();
                    if !flag.is_empty() {
                        flags.push(flag);
                    }
                    if !arg.is_empty() && common_arg.is_none() {
                        common_arg = Some(arg);
                    }
                } else if let Some(sp_pos) = part.find(' ') {
                    let flag = part[..sp_pos].trim().to_string();
                    let arg = part[sp_pos + 1..].trim().to_string();
                    if !flag.is_empty() {
                        flags.push(flag);
                    }
                    if !arg.is_empty() && common_arg.is_none() {
                        common_arg = Some(arg);
                    }
                } else {
                    flags.push(part.to_string());
                }
            }

            if flags.is_empty() || !flags.iter().all(|f| f.starts_with('-')) {
                return None;
            }

            let body_inlines = Self::parse_inlines_from_str(desc_str);
            let body = if body_inlines.is_empty() {
                Vec::new()
            } else {
                vec![DocNode::Paragraph {
                    children: body_inlines,
                }]
            };

            items.push(OptionListItem {
                flags,
                argument: common_arg,
                body,
            });
        }

        if items.is_empty() {
            None
        } else {
            Some(DocNode::OptionList { items })
        }
    }

    fn try_parse_def_list(inlines: &[InlineNode]) -> Option<DocNode> {
        if !inlines
            .iter()
            .any(|inline| matches!(inline, InlineNode::Text { content } if content.contains(" : ")))
        {
            return None;
        }

        let plain = inlines.iter().map(|n| n.plain_text()).collect::<String>();
        let lines: Vec<&str> = plain.lines().filter(|l| !l.trim().is_empty()).collect();
        if lines.len() < 2 {
            return None;
        }

        let mut items = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let header_line = lines[i].trim();
            if !header_line.contains(" : ")
                || header_line.starts_with('-')
                || header_line.starts_with('*')
                || header_line.starts_with(':')
            {
                return None;
            }
            let parts: Vec<&str> = header_line.split(" : ").collect();
            let term_raw = parts[0].trim();
            if term_raw.is_empty() {
                return None;
            }
            let term = Self::parse_inlines_from_str(term_raw);
            let mut classifiers = Vec::new();
            for c_raw in &parts[1..] {
                let c_trimmed = c_raw.trim();
                if !c_trimmed.is_empty() {
                    classifiers.push(Self::parse_inlines_from_str(c_trimmed));
                }
            }
            i += 1;
            let mut body_lines = Vec::new();
            while i < lines.len() {
                let cur = lines[i];
                if cur.contains(" : ") && !cur.starts_with("  ") && !cur.starts_with('\t') {
                    break;
                }
                body_lines.push(cur.trim());
                i += 1;
            }
            if body_lines.is_empty() {
                return None;
            }
            let body_text = body_lines.join("\n");
            let body_inlines = Self::parse_inlines_from_str(&body_text);
            let body = if body_inlines.is_empty() {
                Vec::new()
            } else {
                vec![DocNode::Paragraph {
                    children: body_inlines,
                }]
            };
            items.push(DefinitionListItem {
                term,
                classifiers,
                body,
            });
        }

        if items.is_empty() {
            None
        } else {
            Some(DocNode::DefinitionList { items })
        }
    }

    fn detect_and_strip_admonition(
        children: &mut Vec<DocNode>,
    ) -> (Option<String>, Option<String>) {
        if children.is_empty() {
            return (None, None);
        }

        let mut kind_found = None;
        let mut title_found = None;

        if let Some(DocNode::Paragraph { children: inlines }) = children.first_mut() {
            let plain = inlines.iter().map(|n| n.plain_text()).collect::<String>();
            let trimmed = plain.trim_start();
            if trimmed.starts_with("[!") && trimmed.contains(']') {
                let end_bracket = trimmed.find(']').unwrap();
                let candidate = trimmed[2..end_bracket].to_ascii_lowercase();
                if [
                    "note",
                    "tip",
                    "important",
                    "warning",
                    "caution",
                    "danger",
                    "hint",
                    "attention",
                    "error",
                    "seealso",
                    "spotlight",
                    "feature",
                ]
                .contains(&candidate.as_str())
                {
                    let full_tag = &trimmed[..=end_bracket];
                    kind_found = Some(candidate);

                    let rest_of_first_line = match trimmed[end_bracket + 1..].split_once('\n') {
                        Some((first_line, _)) => first_line.trim(),
                        None => trimmed[end_bracket + 1..].trim(),
                    };
                    if !rest_of_first_line.is_empty() {
                        title_found = Some(rest_of_first_line.to_string());
                    }

                    let chars_to_remove = if let Some(t) = &title_found {
                        full_tag.chars().count() + 1 + t.chars().count()
                    } else {
                        full_tag.chars().count()
                    };

                    let mut remaining_to_trim = chars_to_remove;
                    let mut remove_leading_newline = true;

                    for inline in inlines.iter_mut() {
                        if remaining_to_trim == 0 {
                            if remove_leading_newline && let InlineNode::Text { content } = inline {
                                *content =
                                    content.trim_start_matches('\n').trim_start().to_string();
                            }
                            break;
                        }
                        if let InlineNode::Text { content } = inline {
                            let count = content.chars().count();
                            if count <= remaining_to_trim {
                                remaining_to_trim -= count;
                                content.clear();
                            } else {
                                let rest: String =
                                    content.chars().skip(remaining_to_trim).collect();
                                *content = rest.trim_start_matches('\n').trim_start().to_string();
                                remaining_to_trim = 0;
                                remove_leading_newline = false;
                            }
                        }
                    }

                    inlines.retain(|n| match n {
                        InlineNode::Text { content } => !content.is_empty(),
                        _ => true,
                    });
                }
            }
        }

        if let Some(DocNode::Paragraph { children: inlines }) = children.first()
            && inlines.is_empty()
        {
            children.remove(0);
        }

        (kind_found, title_found)
    }

    fn slugify(text: &str) -> String {
        text.to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_markdown_lsp_block() {
        let md = r#"
# Heading

```lsp echo
kind: builtin
synopsis: echo [-n] [args...]
summary: Standard output engine
category: flow
handler: builtins::echo
---
Prints arguments to stdout.
```
"#;
        let res = MarkdownParser::parse(md, true);
        assert_eq!(res.ast.len(), 2);
        match &res.ast[1] {
            DocNode::LspSymbol {
                name,
                kind,
                synopsis,
                summary,
                category,
                binding,
                children,
                ..
            } => {
                assert_eq!(name, "echo");
                assert_eq!(kind, "builtin");
                assert_eq!(synopsis, "echo [-n] [args...]");
                assert_eq!(summary, "Standard output engine");
                assert_eq!(category, "flow");
                assert_eq!(binding, "builtins::echo");
                assert_eq!(children.len(), 1);
            }
            other => panic!("Expected LspSymbol node, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_markdown_generic_catalog_block() {
        let md = r#"
````lsp
target: [workspace, tasks]
language: flow
title: Task definitions
synopsis: "tasks:"
summary: Define tasks
---
Task documentation.
````
"#;
        let res = MarkdownParser::parse(md, true);
        match &res.ast[0] {
            DocNode::LspSymbol {
                name,
                target,
                language,
                title,
                binding,
                ..
            } => {
                assert_eq!(name, "workspace.tasks");
                assert_eq!(
                    target.as_deref(),
                    Some(["workspace".to_owned(), "tasks".to_owned()].as_slice())
                );
                assert_eq!(language.as_deref(), Some("flow"));
                assert_eq!(title.as_deref(), Some("Task definitions"));
                assert!(binding.is_empty());
            }
            other => panic!("Expected LspSymbol node, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_markdown_diagrams() {
        let md = r#"
```svgbob "Pipeline Flow"
+---+ ---> +---+
| A |      | B |
+---+      +---+
```

```mermaid "Auth Sequence"
sequenceDiagram
    Client->>Server: Hello
```
"#;
        let res = MarkdownParser::parse(md, true);
        assert_eq!(res.ast.len(), 2);
        match &res.ast[0] {
            DocNode::Diagram {
                kind, title, code, ..
            } => {
                assert_eq!(*kind, DiagramKind::Svgbob);
                assert_eq!(title.as_deref(), Some("Pipeline Flow"));
                assert!(code.contains("+---+ ---> +---+"));
            }
            other => panic!("Expected Svgbob Diagram node, found {:?}", other),
        }
        match &res.ast[1] {
            DocNode::Diagram {
                kind,
                title,
                code,
                rendered_mermaid,
                ..
            } => {
                assert_eq!(*kind, DiagramKind::Mermaid);
                assert_eq!(title.as_deref(), Some("Auth Sequence"));
                assert!(code.contains("sequenceDiagram"));
                assert!(rendered_mermaid.is_none());
            }
            other => panic!("Expected Mermaid Diagram node, found {:?}", other),
        }
    }

    #[test]
    fn parses_and_renders_a_titled_code_fence() {
        let md = "```rust \"Configuration loader\"\nfn load() {}\n```";
        let res = MarkdownParser::parse(md, false);

        assert!(matches!(
            res.ast.as_slice(),
            [DocNode::CodeBlock { language: Some(language), title: Some(title), code }]
                if language == "rust"
                    && title == "Configuration loader"
                    && code == "fn load() {}\n"
        ));
        assert_eq!(crate::render::render_markdown(&res.ast), md);
    }

    #[test]
    fn test_parse_markdown_math() {
        let md = r#"# Math Document

The mass-energy equivalence is $E = mc^2$.

$$
\int_0^1 x^2 dx = \frac{1}{3}
$$

```math
\sum_{i=1}^n i = \frac{n(n+1)}{2}
```
"#;
        let res = MarkdownParser::parse(md, true);
        assert_eq!(res.ast.len(), 4); // Heading, Paragraph with InlineMath, DisplayMath, MathBlock

        // 1. Paragraph with inline math
        match &res.ast[1] {
            DocNode::Paragraph { children } => {
                let has_inline_math = children
                    .iter()
                    .any(|c| matches!(c, InlineNode::Math { latex } if latex.contains("E = mc^2")));
                assert!(has_inline_math, "Expected InlineNode::Math in paragraph");
            }
            other => panic!("Expected Paragraph, found {:?}", other),
        }

        // 2. Display math $$...$$
        match &res.ast[2] {
            DocNode::MathBlock { latex } => {
                assert!(latex.contains(r"\int_0^1"));
            }
            other => panic!("Expected MathBlock, found {:?}", other),
        }

        // 3. Fenced ```math
        match &res.ast[3] {
            DocNode::MathBlock { latex } => {
                assert!(latex.contains(r"\sum_{i=1}^n"));
            }
            other => panic!("Expected MathBlock, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_markdown_tight_and_loose_lists() {
        let md = r#"
- Standard Physics: $E = mc^2$ and $F = G \frac{m_1 m_2}{r^2}$
- Trigonometric Identities: $\sin^2(\theta) + \cos^2(\theta) = 1$
- Complex Exponentials: $e^{i \theta} = \cos(\theta) + i \sin(\theta)$
- Greek Symbols: $\alpha, \beta, \gamma$
- Inequalities: $\lim_{x \to 0} \frac{\sin(x)}{x} = 1$

1. First ordered step
2. Second ordered step with `inline code`
"#;
        let res = MarkdownParser::parse(md, false);
        assert_eq!(res.ast.len(), 2);

        // 1. Unordered tight list with inline math
        match &res.ast[0] {
            DocNode::List { ordered, items, .. } => {
                assert!(!ordered);
                assert_eq!(items.len(), 5);
                // Verify first item contains text and math
                assert_eq!(items[0].len(), 1);
                match &items[0][0] {
                    DocNode::Paragraph { children } => {
                        assert!(children.iter().any(|c| matches!(c, InlineNode::Text { content } if content.contains("Standard Physics:"))));
                        assert!(children.iter().any(|c| matches!(c, InlineNode::Math { latex } if latex.contains("E = mc^2"))));
                    }
                    other => panic!("Expected Paragraph in list item, found {:?}", other),
                }
            }
            other => panic!("Expected List, found {:?}", other),
        }

        // 2. Ordered list
        match &res.ast[1] {
            DocNode::List {
                ordered,
                numbering,
                start,
                items,
            } => {
                assert!(ordered);
                assert_eq!(*numbering, NumberingKind::Arabic);
                assert_eq!(*start, 1);
                assert_eq!(items.len(), 2);
            }
            other => panic!("Expected ordered List, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_markdown_task_list() {
        let md = r#"
- [x] Finished task
- [ ] Unfinished task
- [X] Case-insensitive finished task
"#;
        let res = MarkdownParser::parse(md, false);
        assert_eq!(res.ast.len(), 1);

        match &res.ast[0] {
            DocNode::TaskList { items } => {
                assert_eq!(items.len(), 3);
                assert!(items[0].checked);
                assert!(!items[1].checked);
                assert!(items[2].checked);
            }
            other => panic!("Expected TaskList, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_markdown_arbitrary_nested_styles() {
        let md = "**Bold with *italic* and `code` and $E=mc^2$ and [Link](https://example.com)** and *Italic with **bold** and `code`*";
        let res = MarkdownParser::parse(md, false);
        assert_eq!(res.ast.len(), 1);

        match &res.ast[0] {
            DocNode::Paragraph { children } => {
                // 1. Strong with nested Emphasis, Code, Math, Link
                match &children[0] {
                    InlineNode::Strong { children: inner } => {
                        assert!(
                            inner
                                .iter()
                                .any(|c| matches!(c, InlineNode::Emphasis { .. }))
                        );
                        assert!(inner.iter().any(
                            |c| matches!(c, InlineNode::Code { content } if content == "code")
                        ));
                        assert!(inner.iter().any(
                            |c| matches!(c, InlineNode::Math { latex } if latex.contains("E=mc^2"))
                        ));
                        assert!(inner.iter().any(|c| matches!(c, InlineNode::Link { href, .. } if href == "https://example.com")));
                    }
                    other => panic!("Expected Strong, found {:?}", other),
                }

                // 2. Emphasis with nested Strong and Code
                match &children[2] {
                    InlineNode::Emphasis { children: inner } => {
                        assert!(inner.iter().any(|c| matches!(c, InlineNode::Strong { .. })));
                        assert!(inner.iter().any(
                            |c| matches!(c, InlineNode::Code { content } if content == "code")
                        ));
                    }
                    other => panic!("Expected Emphasis, found {:?}", other),
                }
            }
            other => panic!("Expected Paragraph, found {:?}", other),
        }
    }

    #[test]
    fn preserves_inline_code_link_labels_in_paragraphs_and_tables() {
        let md = "See [`evin/docs@v1`](/reference/actions/documentation-action).\n\n| Action | Purpose |\n| --- | --- |\n| [`evin/archive@v1`](/reference/actions/archive-action) | Package files |";
        let res = MarkdownParser::parse(md, false);

        let DocNode::Paragraph { children } = &res.ast[0] else {
            panic!("expected paragraph");
        };
        assert!(matches!(
            &children[1],
            InlineNode::Link { href, content, code: true, .. }
                if href == "/reference/actions/documentation-action"
                    && content == "evin/docs@v1"
        ));

        let DocNode::Table { rows, .. } = &res.ast[1] else {
            panic!("expected table");
        };
        assert!(matches!(
            &rows[0][0][0],
            InlineNode::Link { href, content, code: true, .. }
                if href == "/reference/actions/archive-action"
                    && content == "evin/archive@v1"
        ));

        let rendered = crate::render::render_markdown(&res.ast);
        assert!(rendered.contains("[`evin/docs@v1`](/reference/actions/documentation-action)"));
        assert!(rendered.contains("[`evin/archive@v1`](/reference/actions/archive-action)"));
    }

    #[test]
    fn classifier_syntax_inside_prose_remains_a_paragraph() {
        let md = "Definition-list headers use `term : classifier` followed by indented body text.\nField-list lines use `:name: value`. Option-list lines begin directly with a\nflag and separate their description with two spaces.";
        let res = MarkdownParser::parse(md, false);

        assert!(matches!(res.ast.as_slice(), [DocNode::Paragraph { .. }]));
    }

    #[test]
    fn test_parse_markdown_option_definition_and_field_lists() {
        let md = r#"
:Action: `evin/action-docs@v1`
:Inputs: `workspace.flow`, `docs/`
:Outputs: `dist/docs/`

-h, --help           Show help message.
-o <file>, --output  Target output file.

max_threads : integer : optional (default: 4)
    Maximum parallel DAG worker threads.
"#;
        let res = MarkdownParser::parse(md, false);
        assert_eq!(res.ast.len(), 3);

        // 1. FieldList
        match &res.ast[0] {
            DocNode::FieldList { items } => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0].name, "Action");
                assert_eq!(items[1].name, "Inputs");
                assert_eq!(items[2].name, "Outputs");
            }
            other => panic!("Expected FieldList, found {:?}", other),
        }

        // 2. OptionList
        match &res.ast[1] {
            DocNode::OptionList { items } => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].flags, vec!["-h", "--help"]);
                assert_eq!(items[1].flags, vec!["-o", "--output"]);
                assert_eq!(items[1].argument.as_deref(), Some("<file>"));
            }
            other => panic!("Expected OptionList, found {:?}", other),
        }

        // 3. DefinitionList
        match &res.ast[2] {
            DocNode::DefinitionList { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].classifiers.len(), 2);
            }
            other => panic!("Expected DefinitionList, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_markdown_steps_block() {
        let md = r#"
````steps
1. **Initialize Workspace** [done]
   Bootstrap with `evin init`.

   ```bash
   $ evin init --template action-docs
   ```

2. **Configure Pipeline** [active]
   Declare tasks in `workspace.flow`.
````
"#;
        let res = MarkdownParser::parse(md, false);
        assert_eq!(res.ast.len(), 1);
        match &res.ast[0] {
            DocNode::Stepper { steps } => {
                assert_eq!(steps.len(), 2);
                assert_eq!(steps[0].badge.as_deref(), Some("1"));
                assert_eq!(steps[0].status.as_deref(), Some("done"));
                assert_eq!(
                    steps[0]
                        .title
                        .iter()
                        .map(|n| n.plain_text())
                        .collect::<String>(),
                    "Initialize Workspace"
                );
                assert_eq!(steps[0].body.len(), 2); // Paragraph description + CodeBlock
                assert_eq!(steps[1].badge.as_deref(), Some("2"));
                assert_eq!(steps[1].status.as_deref(), Some("active"));
                assert_eq!(
                    steps[1]
                        .title
                        .iter()
                        .map(|n| n.plain_text())
                        .collect::<String>(),
                    "Configure Pipeline"
                );
            }
            other => panic!("Expected Stepper, found {:?}", other),
        }

        // Test ::: container syntax
        let container_md = r#"
::: steps
1. **Initialize Workspace** [done]
   Bootstrap with `evin init`.

   ```bash
   $ evin init --template action-docs
   ```

2. **Configure Pipeline** [active]
   Declare tasks in `workspace.flow`.
:::
"#;
        let container_res = MarkdownParser::parse(container_md, false);
        assert_eq!(container_res.ast.len(), 1);
        match &container_res.ast[0] {
            DocNode::Stepper { steps } => {
                assert_eq!(steps.len(), 2);
                assert_eq!(steps[0].badge.as_deref(), Some("1"));
                assert_eq!(steps[0].status.as_deref(), Some("done"));
                assert_eq!(steps[0].body.len(), 2);
            }
            other => panic!("Expected Stepper, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_markdown_tight_steps_with_descriptions() {
        let md = r#"
````steps
1. **Parse configuration and remote state** [done]
   Read `state` as JSON and require protocol version `v: 1`.
2. **Collect local release inputs** [active]
   Parse the changelog, invoke the local `git` executable, and scan artifact files.
````
"#;

        let res = MarkdownParser::parse(md, false);
        let DocNode::Stepper { steps } = &res.ast[0] else {
            panic!("Expected Stepper, found {:?}", res.ast[0]);
        };

        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].status.as_deref(), Some("done"));
        assert_eq!(steps[1].status.as_deref(), Some("active"));
        assert_eq!(
            steps[0]
                .title
                .iter()
                .map(InlineNode::plain_text)
                .collect::<String>(),
            "Parse configuration and remote state"
        );
        assert_eq!(steps[0].body.len(), 1);
        let DocNode::Paragraph { children } = &steps[0].body[0] else {
            panic!(
                "Expected description paragraph, found {:?}",
                steps[0].body[0]
            );
        };
        assert_eq!(
            children
                .iter()
                .map(InlineNode::plain_text)
                .collect::<String>(),
            "Read state as JSON and require protocol version v: 1."
        );
    }

    #[test]
    fn test_parse_markdown_tape_fence() {
        let parsed = MarkdownParser::parse(
            "```tape title=\"Demo\" rows=8 max-output=2MiB\ntype \"echo hi\"\nenter\nwait\n```",
            false,
        );
        match &parsed.ast[0] {
            DocNode::Tape {
                script,
                config,
                rendered_svg,
            } => {
                assert_eq!(script, "type \"echo hi\"\nenter\nwait\n");
                assert_eq!(config.title.as_deref(), Some("Demo"));
                assert_eq!(config.rows, 8);
                assert_eq!(config.max_output_bytes, 2 * 1024 * 1024);
                assert!(rendered_svg.is_none());
            }
            other => panic!("expected Tape, found {other:?}"),
        }
    }

    #[test]
    fn test_parse_markdown_component_fence() {
        let parsed = MarkdownParser::parse(
            "```component metrics-budget Landing budget\nA live metric panel.\n```",
            false,
        );
        let DocNode::Directive { name, args, body } = &parsed.ast[0] else {
            panic!("expected component directive, found {:?}", parsed.ast);
        };
        assert_eq!(name, "metrics-budget");
        assert_eq!(args, &["Landing", "budget"]);
        assert!(matches!(body.first(), Some(DocNode::Paragraph { .. })));
    }

    #[test]
    fn test_parse_markdown_feature_with_tape_aside() {
        let parsed = MarkdownParser::parse(
            "> [!FEATURE]\n>\n> ## Watch a workflow command take shape\n>\n> A description.\n>\n> - [Documentation](/guides/documentation-site)\n>\n> ```tape title=\"Inspect the workspace\" rows=8 cols=56\n> type \"pwd\"\n> enter\n> wait\n> ```",
            false,
        );
        let DocNode::Admonition { kind, children, .. } = &parsed.ast[0] else {
            panic!("expected FEATURE admonition, found {:?}", parsed.ast);
        };
        assert_eq!(kind, "feature");
        assert_eq!(children.len(), 4, "children: {children:?}");
        assert!(matches!(children[3], DocNode::Tape { .. }));
    }
}
