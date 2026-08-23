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
use crate::tape::TapeConfig;
use std::collections::HashMap;

pub struct RstParseResult {
    pub metadata: HashMap<String, serde_json::Value>,
    pub toc: Vec<TocEntry>,
    pub ast: Vec<DocNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RstListKind {
    Bullet,
    Numbered(NumberingKind, usize),
    Auto,
    Task(bool),
}

impl RstListKind {
    pub fn is_ordered(&self) -> bool {
        matches!(self, RstListKind::Numbered(_, _) | RstListKind::Auto)
    }

    pub fn matches_series(&self, other: &Self) -> bool {
        match (self, other) {
            (RstListKind::Bullet, RstListKind::Bullet) => true,
            (RstListKind::Auto, RstListKind::Auto) => true,
            (RstListKind::Numbered(k1, _), RstListKind::Numbered(k2, _)) => k1 == k2,
            (RstListKind::Task(_), RstListKind::Task(_)) => true,
            _ => false,
        }
    }

    pub fn numbering_and_start(&self) -> (NumberingKind, usize) {
        match self {
            RstListKind::Numbered(kind, start) => (*kind, *start),
            RstListKind::Auto => (NumberingKind::Arabic, 1),
            _ => (NumberingKind::Arabic, 1),
        }
    }
}

pub struct RstParser;

impl RstParser {
    pub fn parse(content: &str) -> RstParseResult {
        Self::parse_with_heading_hierarchy(content, false)
    }

    pub fn parse_start_page(content: &str) -> RstParseResult {
        Self::parse_with_heading_hierarchy(content, true)
    }

    fn parse_with_heading_hierarchy(
        content: &str,
        normalize_heading_hierarchy: bool,
    ) -> RstParseResult {
        let mut metadata = HashMap::new();
        let mut toc = Vec::new();
        let mut ast = Vec::new();
        let mut heading_styles = Vec::new();

        let lines: Vec<&str> = content.lines().collect();
        let mut idx = 0;

        while idx < lines.len() {
            let line = lines[idx];

            // 1. Field lists (:author: Du or :Action: `...`)
            if let Some((_, _)) = Self::detect_rst_field_marker(line) {
                let mut field_items = Vec::new();
                let base_indent = line.len() - line.trim_start().len();

                while idx < lines.len() {
                    let mut peek_idx = idx;
                    while peek_idx < lines.len() && lines[peek_idx].trim().is_empty() {
                        peek_idx += 1;
                    }
                    if peek_idx < lines.len()
                        && Self::detect_rst_field_marker(lines[peek_idx]).is_some()
                        && lines[peek_idx].len() - lines[peek_idx].trim_start().len() == base_indent
                    {
                        idx = peek_idx;
                    }

                    let cur_line = lines[idx];
                    if let Some((cur_name, cur_val)) = Self::detect_rst_field_marker(cur_line) {
                        let cur_indent = cur_line.len() - cur_line.trim_start().len();
                        if cur_indent != base_indent {
                            break;
                        }

                        // Also populate metadata
                        metadata.insert(
                            cur_name.to_lowercase(),
                            serde_json::Value::String(cur_val.to_string()),
                        );

                        let mut body_lines = if !cur_val.is_empty() {
                            vec![cur_val.to_string()]
                        } else {
                            Vec::new()
                        };
                        idx += 1;

                        while idx < lines.len() {
                            let next_line = lines[idx];
                            if next_line.trim().is_empty() {
                                let mut peek_idx = idx + 1;
                                let mut has_continuation = false;
                                while peek_idx < lines.len() {
                                    if lines[peek_idx].trim().is_empty() {
                                        peek_idx += 1;
                                        continue;
                                    }
                                    let peek_indent =
                                        lines[peek_idx].len() - lines[peek_idx].trim_start().len();
                                    if peek_indent > base_indent {
                                        has_continuation = true;
                                    }
                                    break;
                                }
                                if has_continuation {
                                    body_lines.push(String::new());
                                    idx += 1;
                                    continue;
                                } else {
                                    break;
                                }
                            }
                            let next_indent = next_line.len() - next_line.trim_start().len();
                            if next_indent <= base_indent {
                                break;
                            }
                            body_lines.push(next_line.trim_start().to_string());
                            idx += 1;
                        }

                        let body_text = body_lines.join("\n");
                        let parsed_body = Self::parse(&body_text).ast;
                        let body = if parsed_body.is_empty() && !body_text.trim().is_empty() {
                            vec![DocNode::Paragraph {
                                children: Self::parse_rst_inlines(&body_text),
                            }]
                        } else {
                            parsed_body
                        };

                        field_items.push(FieldListItem {
                            name: cur_name,
                            body,
                        });
                    } else {
                        break;
                    }
                }

                ast.push(DocNode::FieldList { items: field_items });
                continue;
            }

            // 2. Header metadata is typed YAML, matching Markdown frontmatter.
            if line.trim() == ".. metadata::" {
                let is_header = ast.is_empty();
                let body = Self::parse_metadata_body(&lines, &mut idx);
                if !is_header {
                    metadata.insert(
                        "__evin_metadata_error".to_string(),
                        serde_json::Value::String(
                            "the `metadata` directive must appear before page content".to_string(),
                        ),
                    );
                } else {
                    match serde_yaml::from_str::<serde_json::Value>(&body) {
                        Ok(serde_json::Value::Object(values)) => metadata.extend(values),
                        Ok(_) => {
                            metadata.insert(
                                "__evin_metadata_error".to_string(),
                                serde_json::Value::String(
                                    "the `metadata` directive must contain a YAML mapping"
                                        .to_string(),
                                ),
                            );
                        }
                        Err(error) => {
                            metadata.insert(
                                "__evin_metadata_error".to_string(),
                                serde_json::Value::String(format!(
                                    "invalid YAML in the `metadata` directive: {error}"
                                )),
                            );
                        }
                    }
                }
                continue;
            }

            // 3. Directives (e.g. .. note:: or .. code-block:: rust or .. contents::)
            if line.trim().starts_with(".. ")
                && line.contains("::")
                && let Some(node) = Self::parse_directive(&lines, &mut idx)
            {
                ast.push(node);
                continue;
            }

            // Standalone transition divider (e.g. ---, ---- or ==== or ****)
            let trimmed = line.trim();
            if trimmed.len() >= 3
                && (trimmed.chars().all(|c| c == '-')
                    || trimmed.chars().all(|c| c == '=')
                    || trimmed.chars().all(|c| c == '*'))
                && (idx + 1 >= lines.len() || lines[idx + 1].trim().is_empty())
            {
                ast.push(DocNode::HorizontalRule);
                idx += 1;
                continue;
            }

            // 3. Overline + Underline Headings (e.g. ===\nTitle\n===)
            if idx + 2 < lines.len()
                && Self::is_rst_heading_underline(lines[idx].trim())
                && !lines[idx + 1].trim().is_empty()
                && Self::is_rst_heading_underline(lines[idx + 2].trim())
            {
                let raw_text = lines[idx + 1].trim();
                let children = Self::parse_rst_inlines(raw_text);
                let plain_text = children.iter().map(|c| c.plain_text()).collect::<String>();
                let anchor = Self::slugify(&plain_text);
                let level = if normalize_heading_hierarchy {
                    Self::heading_level(
                        lines[idx].trim().chars().next().unwrap_or('='),
                        &mut heading_styles,
                    )
                } else {
                    1
                };
                toc.push(TocEntry {
                    level,
                    text: plain_text.clone(),
                    anchor: anchor.clone(),
                });
                ast.push(DocNode::Heading {
                    level,
                    text: plain_text,
                    children,
                    anchor,
                });
                idx += 3;
                continue;
            }

            // 4. Underline-only Headings (e.g. Title\n=======)
            if idx + 1 < lines.len()
                && !lines[idx].trim().is_empty()
                && !Self::is_rst_heading_underline(lines[idx].trim())
                && Self::is_rst_heading_underline(lines[idx + 1].trim())
                && lines[idx + 1].trim().len() >= lines[idx].trim().len().saturating_sub(2)
            {
                let underline_char = lines[idx + 1].trim().chars().next().unwrap_or('=');
                let level = if normalize_heading_hierarchy {
                    Self::heading_level(underline_char, &mut heading_styles)
                } else {
                    Self::legacy_heading_level(underline_char)
                };
                let raw_text = lines[idx].trim();
                let children = Self::parse_rst_inlines(raw_text);
                let plain_text = children.iter().map(|c| c.plain_text()).collect::<String>();
                let anchor = Self::slugify(&plain_text);

                toc.push(TocEntry {
                    level,
                    text: plain_text.clone(),
                    anchor: anchor.clone(),
                });
                ast.push(DocNode::Heading {
                    level,
                    text: plain_text,
                    children,
                    anchor,
                });
                idx += 2;
                continue;
            }

            // 5. CLI Option Lists (-h, --help  Show help)
            if let Some((_, _, _)) = Self::detect_rst_option_marker(line) {
                let mut option_items = Vec::new();
                let base_indent = line.len() - line.trim_start().len();

                while idx < lines.len() {
                    let mut peek_idx = idx;
                    while peek_idx < lines.len() && lines[peek_idx].trim().is_empty() {
                        peek_idx += 1;
                    }
                    if peek_idx < lines.len()
                        && Self::detect_rst_option_marker(lines[peek_idx]).is_some()
                        && lines[peek_idx].len() - lines[peek_idx].trim_start().len() == base_indent
                    {
                        idx = peek_idx;
                    }

                    let cur_line = lines[idx];
                    if let Some((cur_flags, cur_arg, cur_desc)) =
                        Self::detect_rst_option_marker(cur_line)
                    {
                        let cur_indent = cur_line.len() - cur_line.trim_start().len();
                        if cur_indent != base_indent {
                            break;
                        }

                        let mut body_lines = if !cur_desc.is_empty() {
                            vec![cur_desc.to_string()]
                        } else {
                            Vec::new()
                        };
                        idx += 1;

                        while idx < lines.len() {
                            let next_line = lines[idx];
                            if next_line.trim().is_empty() {
                                let mut peek_idx = idx + 1;
                                let mut has_continuation = false;
                                while peek_idx < lines.len() {
                                    if lines[peek_idx].trim().is_empty() {
                                        peek_idx += 1;
                                        continue;
                                    }
                                    let peek_indent =
                                        lines[peek_idx].len() - lines[peek_idx].trim_start().len();
                                    if peek_indent > base_indent {
                                        has_continuation = true;
                                    }
                                    break;
                                }
                                if has_continuation {
                                    body_lines.push(String::new());
                                    idx += 1;
                                    continue;
                                } else {
                                    break;
                                }
                            }
                            let next_indent = next_line.len() - next_line.trim_start().len();
                            if next_indent <= base_indent {
                                break;
                            }
                            body_lines.push(next_line.trim_start().to_string());
                            idx += 1;
                        }

                        let body_text = body_lines.join("\n");
                        let parsed_body = Self::parse(&body_text).ast;
                        let body = if parsed_body.is_empty() && !body_text.trim().is_empty() {
                            vec![DocNode::Paragraph {
                                children: Self::parse_rst_inlines(&body_text),
                            }]
                        } else {
                            parsed_body
                        };

                        option_items.push(OptionListItem {
                            flags: cur_flags,
                            argument: cur_arg,
                            body,
                        });
                    } else {
                        break;
                    }
                }

                ast.push(DocNode::OptionList {
                    items: option_items,
                });
                continue;
            }

            // 6. Definition Lists (term : classifier\n    body)
            if idx + 1 < lines.len()
                && !lines[idx + 1].trim().is_empty()
                && lines[idx + 1].len() - lines[idx + 1].trim_start().len() >= 2
                && let Some((_, _)) = Self::detect_rst_definition_term(line)
            {
                let mut def_items = Vec::new();
                let base_indent = line.len() - line.trim_start().len();

                while idx < lines.len() {
                    let mut peek_idx = idx;
                    while peek_idx < lines.len() && lines[peek_idx].trim().is_empty() {
                        peek_idx += 1;
                    }
                    if peek_idx < lines.len()
                        && peek_idx + 1 < lines.len()
                        && !lines[peek_idx + 1].trim().is_empty()
                        && lines[peek_idx + 1].len() - lines[peek_idx + 1].trim_start().len() >= 2
                        && Self::detect_rst_definition_term(lines[peek_idx]).is_some()
                        && lines[peek_idx].len() - lines[peek_idx].trim_start().len() == base_indent
                    {
                        idx = peek_idx;
                    }

                    let cur_line = lines[idx];
                    if let Some((cur_term, cur_classifiers)) =
                        Self::detect_rst_definition_term(cur_line)
                    {
                        let cur_indent = cur_line.len() - cur_line.trim_start().len();
                        if cur_indent != base_indent {
                            break;
                        }

                        idx += 1;
                        let mut body_lines = Vec::new();

                        while idx < lines.len() {
                            let next_line = lines[idx];
                            if next_line.trim().is_empty() {
                                let mut peek_idx = idx + 1;
                                let mut has_continuation = false;
                                while peek_idx < lines.len() {
                                    if lines[peek_idx].trim().is_empty() {
                                        peek_idx += 1;
                                        continue;
                                    }
                                    let peek_indent =
                                        lines[peek_idx].len() - lines[peek_idx].trim_start().len();
                                    if peek_indent > base_indent {
                                        has_continuation = true;
                                    }
                                    break;
                                }
                                if has_continuation {
                                    body_lines.push(String::new());
                                    idx += 1;
                                    continue;
                                } else {
                                    break;
                                }
                            }
                            let next_indent = next_line.len() - next_line.trim_start().len();
                            if next_indent <= base_indent {
                                break;
                            }
                            body_lines.push(next_line.trim_start().to_string());
                            idx += 1;
                        }

                        let body_text = body_lines.join("\n");
                        let parsed_body = Self::parse(&body_text).ast;
                        let body = if parsed_body.is_empty() && !body_text.trim().is_empty() {
                            vec![DocNode::Paragraph {
                                children: Self::parse_rst_inlines(&body_text),
                            }]
                        } else {
                            parsed_body
                        };

                        def_items.push(DefinitionListItem {
                            term: cur_term,
                            classifiers: cur_classifiers,
                            body,
                        });
                    } else {
                        break;
                    }
                }

                ast.push(DocNode::DefinitionList { items: def_items });
                continue;
            }

            // 7. Lists (Bullet, Ordered, Auto, Task)
            if let Some((kind, base_indent, _, _)) = Self::detect_rst_list_marker(line) {
                let is_task = matches!(kind, RstListKind::Task(_));
                let ordered = kind.is_ordered();
                let (numbering, start) = kind.numbering_and_start();

                let mut items: Vec<Vec<DocNode>> = Vec::new();
                let mut task_items: Vec<TaskListItem> = Vec::new();

                while idx < lines.len() {
                    let mut peek_idx = idx;
                    while peek_idx < lines.len() && lines[peek_idx].trim().is_empty() {
                        peek_idx += 1;
                    }
                    if peek_idx < lines.len()
                        && let Some((cur_kind, cur_indent, _, _)) =
                            Self::detect_rst_list_marker(lines[peek_idx])
                        && cur_indent == base_indent
                        && kind.matches_series(&cur_kind)
                    {
                        idx = peek_idx;
                    }

                    let cur_line = lines[idx];
                    if let Some((cur_kind, cur_indent, cur_marker_len, first_text)) =
                        Self::detect_rst_list_marker(cur_line)
                    {
                        if cur_indent != base_indent {
                            break;
                        }
                        if is_task && !matches!(cur_kind, RstListKind::Task(_)) {
                            break;
                        }
                        if !is_task && cur_kind.is_ordered() != ordered {
                            break;
                        }

                        let content_indent = cur_indent + cur_marker_len;
                        let mut item_lines = vec![first_text.to_string()];
                        idx += 1;

                        while idx < lines.len() {
                            let next_line = lines[idx];
                            if next_line.trim().is_empty() {
                                let mut peek_idx = idx + 1;
                                let mut has_continuation = false;
                                while peek_idx < lines.len() {
                                    if lines[peek_idx].trim().is_empty() {
                                        peek_idx += 1;
                                        continue;
                                    }
                                    let peek_trimmed = lines[peek_idx].trim_start();
                                    let peek_indent = lines[peek_idx].len() - peek_trimmed.len();
                                    if peek_indent >= content_indent {
                                        has_continuation = true;
                                    }
                                    break;
                                }

                                if has_continuation {
                                    item_lines.push(String::new());
                                    idx += 1;
                                    continue;
                                } else {
                                    break;
                                }
                            }

                            let next_trimmed = next_line.trim_start();
                            let next_indent = next_line.len() - next_trimmed.len();

                            if next_indent <= base_indent {
                                break;
                            }

                            let stripped_line = if next_indent >= content_indent {
                                &next_line[content_indent..]
                            } else {
                                next_trimmed
                            };

                            item_lines.push(stripped_line.to_string());
                            idx += 1;
                        }

                        let item_body = item_lines.join("\n");
                        let item_res = Self::parse(&item_body);
                        let item_nodes = if item_res.ast.is_empty() {
                            let full_text = item_lines.join(" ");
                            if !full_text.trim().is_empty() {
                                vec![DocNode::Paragraph {
                                    children: Self::parse_rst_inlines(&full_text),
                                }]
                            } else {
                                Vec::new()
                            }
                        } else {
                            item_res.ast
                        };

                        if let RstListKind::Task(checked) = cur_kind {
                            task_items.push(TaskListItem {
                                checked,
                                body: item_nodes.clone(),
                            });
                        }
                        items.push(item_nodes);
                    } else {
                        break;
                    }
                }

                if is_task {
                    ast.push(DocNode::TaskList { items: task_items });
                } else {
                    ast.push(DocNode::List {
                        ordered,
                        numbering,
                        start,
                        items,
                    });
                }
                continue;
            }

            // 8. Paragraph
            if !line.trim().is_empty() {
                let mut p_lines = vec![line.trim()];
                idx += 1;
                while idx < lines.len()
                    && !lines[idx].trim().is_empty()
                    && !lines[idx].trim().starts_with(".. ")
                    && !Self::is_rst_heading_underline(lines[idx].trim())
                    && Self::detect_rst_field_marker(lines[idx]).is_none()
                    && Self::detect_rst_option_marker(lines[idx]).is_none()
                    && Self::detect_rst_list_marker(lines[idx]).is_none()
                    && !(idx + 1 < lines.len()
                        && lines[idx + 1].len() - lines[idx + 1].trim_start().len() >= 2
                        && Self::detect_rst_definition_term(lines[idx]).is_some())
                {
                    p_lines.push(lines[idx].trim());
                    idx += 1;
                }
                let full_text = p_lines.join(" ");
                ast.push(DocNode::Paragraph {
                    children: Self::parse_rst_inlines(&full_text),
                });
                continue;
            }

            idx += 1;
        }

        Self::resolve_fallback_metadata(&mut metadata, &ast);

        RstParseResult { metadata, toc, ast }
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

    fn strip_common_indentation(lines: &[&str]) -> String {
        let mut min_indent: Option<usize> = None;
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let indent = line.chars().take_while(|c| c.is_whitespace()).count();
            min_indent = Some(min_indent.map_or(indent, |m| m.min(indent)));
        }

        let indent_len = min_indent.unwrap_or(0);
        let mut result = Vec::new();
        for line in lines {
            if line.trim().is_empty() {
                result.push("");
            } else {
                let stripped = if line.len() >= indent_len {
                    &line[indent_len..]
                } else {
                    line.trim_start()
                };
                result.push(stripped);
            }
        }
        result.join("\n").trim_matches('\n').to_string()
    }

    fn parse_metadata_body(lines: &[&str], idx: &mut usize) -> String {
        *idx += 1;
        let mut body = Vec::new();
        while *idx < lines.len() {
            let line = lines[*idx];
            if line.trim().is_empty() {
                body.push("");
                *idx += 1;
            } else if line.chars().next().is_some_and(|ch| ch.is_whitespace()) {
                body.push(line);
                *idx += 1;
            } else {
                break;
            }
        }
        Self::strip_common_indentation(&body)
    }

    fn parse_directive(lines: &[&str], idx: &mut usize) -> Option<DocNode> {
        let line = lines[*idx].trim();
        let directive_content = line.strip_prefix(".. ")?;
        let mut parts = directive_content.splitn(2, "::");
        let name = parts.next()?.trim().to_string();
        let arg_str = parts.next()?.trim();
        let args = if arg_str.is_empty() {
            vec![]
        } else {
            vec![arg_str.to_string()]
        };

        *idx += 1;

        // Gather indented directive body
        let mut raw_body_lines = Vec::new();
        let mut trimmed_body_lines = Vec::new();
        while *idx < lines.len() {
            let next_line = lines[*idx];
            if next_line.trim().is_empty() {
                raw_body_lines.push("");
                trimmed_body_lines.push("");
                *idx += 1;
            } else if next_line.chars().next().is_some_and(|c| c.is_whitespace()) {
                raw_body_lines.push(next_line);
                trimmed_body_lines.push(next_line.trim());
                *idx += 1;
            } else {
                break;
            }
        }

        let body_str = Self::strip_common_indentation(&raw_body_lines);
        let options = Self::directive_options(&trimmed_body_lines);

        if name == "image" || name == "video" {
            Some(DocNode::Media {
                kind: if name == "image" {
                    MediaKind::Image
                } else {
                    MediaKind::Video
                },
                src: args.first().cloned().unwrap_or_default(),
                alt: options.get("alt").cloned(),
                title: options.get("title").cloned(),
                poster: options.get("poster").cloned(),
            })
        } else if name == "mermaid" || name == "svgbob" || name == "pikchr" {
            let kind = match name.as_str() {
                "svgbob" => DiagramKind::Svgbob,
                "pikchr" => DiagramKind::Pikchr,
                _ => DiagramKind::Mermaid,
            };
            let title = if !args.is_empty() {
                Some(args.join(" "))
            } else {
                options.get("title").cloned()
            };
            let caption = options.get("caption").cloned();
            let alt = options.get("alt").cloned();

            let non_option_lines: Vec<&str> = trimmed_body_lines
                .iter()
                .cloned()
                .filter(|l| !l.trim().starts_with(':'))
                .collect();
            let clean_body = non_option_lines.join("\n");

            Some(DocNode::Diagram {
                kind,
                code: clean_body,
                title,
                caption,
                alt,
            })
        } else if name == "tape" {
            let mut tape_options = options;
            if !args.is_empty() {
                tape_options.insert("title".to_string(), args.join(" "));
            }
            match TapeConfig::from_attributes(&tape_options) {
                Ok(config) => Some(DocNode::Tape {
                    script: body_str,
                    config,
                    rendered_svg: None,
                }),
                Err(error) => Some(DocNode::Directive {
                    name: "tape-error".to_string(),
                    args: vec![error],
                    body: Vec::new(),
                }),
            }
        } else if name == "math" {
            Some(DocNode::MathBlock { latex: body_str })
        } else if name == "code-block" || name == "code" {
            let lang = args.first().map(|s| s.to_ascii_lowercase());
            if let Some(l) = lang.as_deref()
                && (l == "mermaid" || l == "svgbob" || l == "pikchr")
            {
                let kind = match l {
                    "svgbob" => DiagramKind::Svgbob,
                    "pikchr" => DiagramKind::Pikchr,
                    _ => DiagramKind::Mermaid,
                };
                let title = if args.len() > 1 {
                    Some(args[1..].join(" "))
                } else {
                    options.get("title").cloned()
                };
                let caption = options.get("caption").cloned();
                let alt = options.get("alt").cloned();
                let non_option_lines: Vec<&str> = trimmed_body_lines
                    .iter()
                    .cloned()
                    .filter(|l| !l.trim().starts_with(':'))
                    .collect();
                let clean_body = non_option_lines.join("\n");
                Some(DocNode::Diagram {
                    kind,
                    code: clean_body,
                    title,
                    caption,
                    alt,
                })
            } else {
                Some(DocNode::CodeBlock {
                    language: args.first().cloned(),
                    title: None,
                    code: body_str,
                })
            }
        } else if name == "list-table" {
            let mut raw_rows: Vec<Vec<String>> = Vec::new();
            let mut current_row: Vec<String> = Vec::new();
            let mut current_cell: String = String::new();
            let has_header = trimmed_body_lines
                .iter()
                .any(|l| l.contains(":header-rows: 1"));

            for bline in &trimmed_body_lines {
                let trimmed = bline.trim();
                if trimmed.starts_with(":header-rows:") || trimmed.starts_with(":widths:") {
                    continue;
                }
                if trimmed.starts_with("* - ") {
                    if !current_cell.is_empty() {
                        current_row.push(current_cell.trim().to_string());
                        current_cell.clear();
                    }
                    if !current_row.is_empty() {
                        raw_rows.push(current_row.clone());
                        current_row.clear();
                    }
                    current_cell.push_str(trimmed.trim_start_matches("* - "));
                } else if trimmed.starts_with("- ") {
                    if !current_cell.is_empty() {
                        current_row.push(current_cell.trim().to_string());
                        current_cell.clear();
                    }
                    current_cell.push_str(trimmed.trim_start_matches("- "));
                } else if !trimmed.is_empty() {
                    if !current_cell.is_empty() {
                        current_cell.push(' ');
                    }
                    current_cell.push_str(trimmed);
                }
            }
            if !current_cell.is_empty() {
                current_row.push(current_cell.trim().to_string());
            }
            if !current_row.is_empty() {
                raw_rows.push(current_row);
            }

            let mut headers = Vec::new();
            if has_header && !raw_rows.is_empty() {
                let raw_header = raw_rows.remove(0);
                headers = raw_header
                    .into_iter()
                    .map(|cell| Self::parse_rst_inlines(&cell))
                    .collect();
            }

            let rows = raw_rows
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|cell| Self::parse_rst_inlines(&cell))
                        .collect()
                })
                .collect();

            Some(DocNode::Table { headers, rows })
        } else if name == "toctree" {
            let mut caption = None;
            let mut entries = Vec::new();
            for bline in &trimmed_body_lines {
                let trimmed = bline.trim();
                if trimmed.starts_with(":caption:") {
                    caption = Some(trimmed.trim_start_matches(":caption:").trim().to_string());
                } else if !trimmed.starts_with(':') && !trimmed.is_empty() {
                    let page_slug = trimmed.trim_end_matches(".rst").trim_end_matches(".md");
                    entries.push((page_slug.to_string(), trimmed.to_string()));
                }
            }
            let mut directive_args = args;
            if let Some(cap) = caption {
                directive_args.push(cap);
            }
            Some(DocNode::Directive {
                name: "toctree".to_string(),
                args: directive_args,
                body: entries
                    .into_iter()
                    .map(|(slug, text)| DocNode::Paragraph {
                        children: vec![InlineNode::Link {
                            href: format!("{}/index.html", slug),
                            title: None,
                            content: text,
                        }],
                    })
                    .collect(),
            })
        } else if name == "contents" {
            let clean_lines: Vec<&str> = trimmed_body_lines
                .iter()
                .cloned()
                .filter(|l| !l.trim().starts_with(':'))
                .collect();
            let clean_body = clean_lines.join("\n").trim().to_string();
            Some(DocNode::Directive {
                name: "contents".to_string(),
                args,
                body: if clean_body.is_empty() {
                    vec![]
                } else {
                    vec![DocNode::Paragraph {
                        children: Self::parse_rst_inlines(&clean_body),
                    }]
                },
            })
        } else if name == "note"
            || name == "warning"
            || name == "tip"
            || name == "important"
            || name == "caution"
            || name == "attention"
            || name == "danger"
            || name == "error"
            || name == "hint"
            || name == "seealso"
            || name == "feature"
            || name == "admonition"
        {
            let (title, actual_body) = if name == "admonition" {
                (args.first().cloned(), body_str)
            } else {
                let explicit_title = if !args.is_empty() {
                    Some(args.join(" "))
                } else {
                    None
                };
                (explicit_title, body_str)
            };

            let parsed_children = if actual_body.trim().is_empty() {
                Vec::new()
            } else {
                let parsed = Self::parse(&actual_body);
                if !parsed.ast.is_empty() {
                    parsed.ast
                } else {
                    vec![DocNode::Paragraph {
                        children: Self::parse_rst_inlines(&actual_body),
                    }]
                }
            };

            Some(DocNode::Admonition {
                kind: name,
                title,
                children: parsed_children,
            })
        } else if name == "lsp" || name == "symbol" || name == "keyword" {
            let symbol_name = args.first().cloned().unwrap_or_default();
            let options = Self::directive_options(&trimmed_body_lines);
            let kind = options.get("kind").cloned().unwrap_or_else(|| {
                if name == "keyword" {
                    "keyword".to_string()
                } else {
                    "builtin".to_string()
                }
            });
            let synopsis = options.get("synopsis").cloned().unwrap_or_default();
            let summary = options.get("summary").cloned().unwrap_or_default();
            let category = options
                .get("category")
                .cloned()
                .unwrap_or_else(|| "general".to_string());
            let handler = options.get("handler").cloned().unwrap_or_default();

            let mut non_option_lines = Vec::new();
            let mut past_options = false;
            for b_line in body_str.lines() {
                let trimmed = b_line.trim();
                if !past_options {
                    if trimmed.starts_with(':') || trimmed.is_empty() {
                        continue;
                    }
                    past_options = true;
                }
                non_option_lines.push(b_line);
            }
            let non_option_body = Self::strip_common_indentation(&non_option_lines);
            let inner_ast = if non_option_body.trim().is_empty() {
                Vec::new()
            } else {
                Self::parse(&non_option_body).ast
            };

            Some(DocNode::LspSymbol {
                name: symbol_name,
                kind,
                synopsis,
                summary,
                category,
                handler,
                children: inner_ast,
            })
        } else if name == "steps" || name == "stepper" {
            Some(Self::parse_rst_steps(&body_str))
        } else {
            Some(DocNode::Directive {
                name,
                args,
                body: vec![DocNode::Paragraph {
                    children: Self::parse_rst_inlines(&body_str),
                }],
            })
        }
    }

    fn parse_rst_steps(body_str: &str) -> DocNode {
        let lines: Vec<&str> = body_str.lines().collect();
        let mut steps = Vec::new();
        let mut idx = 0;

        while idx < lines.len() {
            let line = lines[idx];
            let trimmed = line.trim();
            if trimmed.is_empty() {
                idx += 1;
                continue;
            }

            // Case A: .. step:: Title
            if trimmed.starts_with(".. step::") {
                let title_raw = trimmed.strip_prefix(".. step::").unwrap().trim();
                let title = Self::parse_rst_inlines(title_raw);
                idx += 1;

                let mut step_body_lines = Vec::new();
                let mut options = HashMap::new();
                let mut past_options = false;

                while idx < lines.len() {
                    let next_line = lines[idx];
                    let next_trimmed = next_line.trim();
                    if next_trimmed.is_empty() {
                        step_body_lines.push("");
                        idx += 1;
                    } else if next_line.starts_with("  ") || next_line.starts_with('\t') {
                        if !past_options
                            && next_trimmed.starts_with(':')
                            && let Some(opt_rest) = next_trimmed.strip_prefix(':')
                            && let Some((k, v)) = opt_rest.split_once(':')
                        {
                            options.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
                            idx += 1;
                            continue;
                        }
                        past_options = true;
                        step_body_lines.push(next_line);
                        idx += 1;
                    } else {
                        break;
                    }
                }

                let badge = options.get("badge").cloned();
                let status = options.get("status").cloned();
                let clean_body = Self::strip_common_indentation(&step_body_lines);
                let body = if clean_body.trim().is_empty() {
                    Vec::new()
                } else {
                    Self::parse(&clean_body).ast
                };

                steps.push(StepItem {
                    badge,
                    title,
                    status,
                    body,
                });
            } else if let Some((_, _, _, item_text)) = Self::detect_rst_list_marker(line) {
                // Case B: Numbered item inside steps
                let (badge, title, status) =
                    Self::extract_step_title_and_status(item_text, steps.len() + 1);
                idx += 1;

                let mut step_body_lines = Vec::new();
                while idx < lines.len() {
                    let next_line = lines[idx];
                    let next_trimmed = next_line.trim();
                    if next_trimmed.is_empty() {
                        step_body_lines.push("");
                        idx += 1;
                    } else if (next_line.starts_with("  ") || next_line.starts_with('\t'))
                        && Self::detect_rst_list_marker(next_line).is_none()
                    {
                        step_body_lines.push(next_line);
                        idx += 1;
                    } else {
                        break;
                    }
                }

                let clean_body = Self::strip_common_indentation(&step_body_lines);
                let body = if clean_body.trim().is_empty() {
                    Vec::new()
                } else {
                    Self::parse(&clean_body).ast
                };

                steps.push(StepItem {
                    badge: Some(badge),
                    title,
                    status,
                    body,
                });
            } else {
                idx += 1;
            }
        }

        DocNode::Stepper { steps }
    }

    fn extract_step_title_and_status(
        text: &str,
        default_num: usize,
    ) -> (String, Vec<InlineNode>, Option<String>) {
        let mut status = None;
        let mut clean_text = text.trim();

        if let Some(open) = clean_text.rfind('[') {
            if let Some(close) = clean_text[open..].find(']') {
                let candidate = clean_text[open + 1..open + close]
                    .trim()
                    .to_ascii_lowercase();
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
                    clean_text = clean_text[..open].trim();
                }
            }
        } else if let Some(open) = clean_text.rfind('(')
            && let Some(close) = clean_text[open..].find(')')
        {
            let candidate = clean_text[open + 1..open + close]
                .trim()
                .to_ascii_lowercase();
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
                clean_text = clean_text[..open].trim();
            }
        }

        let inlines = Self::parse_rst_inlines(clean_text);
        (default_num.to_string(), inlines, status)
    }

    fn directive_options(lines: &[&str]) -> HashMap<String, String> {
        lines
            .iter()
            .filter_map(|line| {
                let option = line.strip_prefix(':')?;
                let (key, value) = option.split_once(':')?;
                let value = value.trim();
                (!value.is_empty()).then(|| (key.trim().to_ascii_lowercase(), value.to_string()))
            })
            .collect()
    }

    fn is_rst_heading_underline(s: &str) -> bool {
        if s.len() < 2 {
            return false;
        }
        let first = s.chars().next().unwrap();
        matches!(first, '=' | '-' | '~' | '`' | '#' | '*' | '^') && s.chars().all(|c| c == first)
    }

    fn heading_level(style: char, styles: &mut Vec<char>) -> u8 {
        if let Some(index) = styles.iter().position(|candidate| *candidate == style) {
            (index + 1).min(6) as u8
        } else {
            styles.push(style);
            styles.len().min(6) as u8
        }
    }

    fn legacy_heading_level(style: char) -> u8 {
        match style {
            '=' => 2,
            '-' => 3,
            '~' => 4,
            '`' => 5,
            _ => 6,
        }
    }

    pub fn parse_rst_inlines(text: &str) -> Vec<InlineNode> {
        let mut inlines = Vec::new();
        let mut buffer = String::new();
        let mut i = 0;
        let len = text.len();

        while i < len {
            // 0. Inline Math role: :math:`latex`
            if text[i..].starts_with(":math:`")
                && let Some(end_offset) = text[i + 7..].find('`')
            {
                if !buffer.is_empty() {
                    inlines.push(InlineNode::Text {
                        content: std::mem::take(&mut buffer),
                    });
                }
                let latex = &text[i + 7..i + 7 + end_offset];
                inlines.push(InlineNode::Math {
                    latex: latex.trim().to_string(),
                });
                i += 7 + end_offset + 1;
                continue;
            }

            // 0b. Inline Code role: :code:`code`
            if text[i..].starts_with(":code:`")
                && let Some(end_offset) = text[i + 7..].find('`')
            {
                if !buffer.is_empty() {
                    inlines.push(InlineNode::Text {
                        content: std::mem::take(&mut buffer),
                    });
                }
                let code_content = &text[i + 7..i + 7 + end_offset];
                inlines.push(InlineNode::Code {
                    content: code_content.to_string(),
                });
                i += 7 + end_offset + 1;
                continue;
            }

            // 0c. Inline Doc role: :doc:`route` or :doc:`title <route>`
            if text[i..].starts_with(":doc:`")
                && let Some(end_offset) = text[i + 6..].find('`')
            {
                if !buffer.is_empty() {
                    inlines.push(InlineNode::Text {
                        content: std::mem::take(&mut buffer),
                    });
                }
                let target = &text[i + 6..i + 6 + end_offset];
                if let Some(open_angle) = target.rfind(" <")
                    && let Some(href) = target[open_angle + 2..].strip_suffix('>')
                {
                    inlines.push(InlineNode::Link {
                        href: href.trim().to_string(),
                        title: None,
                        content: target[..open_angle].trim().to_string(),
                    });
                } else {
                    inlines.push(InlineNode::Link {
                        href: target.trim().to_string(),
                        title: None,
                        content: target.trim().to_string(),
                    });
                }
                i += 6 + end_offset + 1;
                continue;
            }

            // 0d. Inline Ref role: :ref:`anchor` or :ref:`title <anchor>`
            if text[i..].starts_with(":ref:`")
                && let Some(end_offset) = text[i + 6..].find('`')
            {
                if !buffer.is_empty() {
                    inlines.push(InlineNode::Text {
                        content: std::mem::take(&mut buffer),
                    });
                }
                let target = &text[i + 6..i + 6 + end_offset];
                if let Some(open_angle) = target.rfind(" <")
                    && let Some(anchor) = target[open_angle + 2..].strip_suffix('>')
                {
                    inlines.push(InlineNode::Link {
                        href: format!("#{}", anchor.trim()),
                        title: None,
                        content: target[..open_angle].trim().to_string(),
                    });
                } else {
                    inlines.push(InlineNode::Link {
                        href: format!("#{}", target.trim()),
                        title: None,
                        content: target.trim().to_string(),
                    });
                }
                i += 6 + end_offset + 1;
                continue;
            }

            // 1. Standard named link: `Link title <route>`_
            if text[i..].starts_with('`')
                && !text[i..].starts_with("``")
                && let Some(end_offset) = text[i + 1..].find("`_")
            {
                let link_text = &text[i + 1..i + 1 + end_offset];
                if let Some(open_angle) = link_text.rfind(" <")
                    && let Some(href) = link_text[open_angle + 2..].strip_suffix('>')
                {
                    let label = link_text[..open_angle].trim();
                    if !label.is_empty() && !href.trim().is_empty() {
                        if !buffer.is_empty() {
                            inlines.push(InlineNode::Text {
                                content: std::mem::take(&mut buffer),
                            });
                        }
                        inlines.push(InlineNode::Link {
                            href: href.trim().to_string(),
                            title: None,
                            content: label.to_string(),
                        });
                        i += end_offset + 3;
                        continue;
                    }
                }
            }

            // 2. Double backticks ``code``
            if text[i..].starts_with("``")
                && let Some(end_offset) = text[i + 2..].find("``")
            {
                if !buffer.is_empty() {
                    inlines.push(InlineNode::Text {
                        content: std::mem::take(&mut buffer),
                    });
                }
                let code_content = &text[i + 2..i + 2 + end_offset];
                inlines.push(InlineNode::Code {
                    content: code_content.to_string(),
                });
                i += 2 + end_offset + 2;
                continue;
            }

            // 3. Double asterisks **bold**
            if text[i..].starts_with("**")
                && let Some(end_offset) = Self::find_rst_closing_strong(&text[i + 2..])
            {
                if !buffer.is_empty() {
                    inlines.push(InlineNode::Text {
                        content: std::mem::take(&mut buffer),
                    });
                }
                let bold_content = &text[i + 2..i + 2 + end_offset];
                let children = Self::parse_rst_inlines(bold_content);
                inlines.push(InlineNode::Strong { children });
                i += 2 + end_offset + 2;
                continue;
            }

            // 4. Single asterisk *italic*
            if text[i..].starts_with('*')
                && !text[i..].starts_with("**")
                && let Some(end_offset) = Self::find_rst_closing_emphasis(&text[i + 1..])
            {
                if !buffer.is_empty() {
                    inlines.push(InlineNode::Text {
                        content: std::mem::take(&mut buffer),
                    });
                }
                let italic_content = &text[i + 1..i + 1 + end_offset];
                let children = Self::parse_rst_inlines(italic_content);
                inlines.push(InlineNode::Emphasis { children });
                i += 1 + end_offset + 1;
                continue;
            }

            // Accumulate plain text character
            let ch = text[i..].chars().next().unwrap_or(' ');
            buffer.push(ch);
            i += ch.len_utf8();
        }

        if !buffer.is_empty() {
            inlines.push(InlineNode::Text { content: buffer });
        }

        inlines
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

    fn find_rst_closing_strong(s: &str) -> Option<usize> {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if s[i..].starts_with("``")
                && let Some(end) = s[i + 2..].find("``")
            {
                i += 2 + end + 2;
                continue;
            }
            if s[i..].starts_with("**") {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    fn find_rst_closing_emphasis(s: &str) -> Option<usize> {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if s[i..].starts_with("``")
                && let Some(end) = s[i + 2..].find("``")
            {
                i += 2 + end + 2;
                continue;
            }
            if s[i..].starts_with("**") {
                // Skip bold span inside italic
                if let Some(end) = Self::find_rst_closing_strong(&s[i + 2..]) {
                    i += 2 + end + 2;
                    continue;
                } else {
                    i += 2;
                    continue;
                }
            }
            if bytes[i] == b'*' {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    pub fn detect_rst_list_marker(line: &str) -> Option<(RstListKind, usize, usize, &str)> {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            return None;
        }
        let indent = line.len() - trimmed.len();

        // 0. Task list markers: '- [ ] ', '- [x] ', '- [X] ', '* [ ] ', '* [x] '
        if trimmed.starts_with("- [ ] ") || trimmed.starts_with("* [ ] ") {
            return Some((
                RstListKind::Task(false),
                indent,
                6,
                trimmed[6..].trim_start(),
            ));
        }
        if trimmed.starts_with("- [x] ")
            || trimmed.starts_with("- [X] ")
            || trimmed.starts_with("* [x] ")
            || trimmed.starts_with("* [X] ")
        {
            return Some((
                RstListKind::Task(true),
                indent,
                6,
                trimmed[6..].trim_start(),
            ));
        }

        // 1. Bullet list markers: '*', '-', '+'
        if trimmed.starts_with("* ") || trimmed.starts_with("- ") || trimmed.starts_with("+ ") {
            return Some((RstListKind::Bullet, indent, 2, trimmed[2..].trim_start()));
        }

        // 2. Auto-enumerator '#.'
        if let Some(content) = trimmed.strip_prefix("#. ") {
            return Some((RstListKind::Auto, indent, 3, content.trim_start()));
        }

        // 3. Numbered with paren: '(1) ', '(a) ', '(i) ', '1) ', 'a) ', 'i) '
        if let Some(paren_pos) = trimmed.find(") ") {
            let prefix = &trimmed[..paren_pos];
            if let Some(inner) = prefix.strip_prefix('(') {
                if let Some((kind, start)) = Self::parse_enumerator_number(inner) {
                    return Some((
                        RstListKind::Numbered(kind, start),
                        indent,
                        paren_pos + 2,
                        trimmed[paren_pos + 2..].trim_start(),
                    ));
                }
            } else if let Some((kind, start)) = Self::parse_enumerator_number(prefix) {
                return Some((
                    RstListKind::Numbered(kind, start),
                    indent,
                    paren_pos + 2,
                    trimmed[paren_pos + 2..].trim_start(),
                ));
            }
        }

        // 4. Numbered with dot: '1. ', 'a. ', 'i. '
        if let Some(dot_pos) = trimmed.find(". ") {
            let prefix = &trimmed[..dot_pos];
            if let Some((kind, start)) = Self::parse_enumerator_number(prefix) {
                return Some((
                    RstListKind::Numbered(kind, start),
                    indent,
                    dot_pos + 2,
                    trimmed[dot_pos + 2..].trim_start(),
                ));
            }
        }

        None
    }

    pub fn parse_enumerator_number(prefix: &str) -> Option<(NumberingKind, usize)> {
        if prefix.is_empty() {
            return None;
        }
        if prefix.chars().all(|c| c.is_ascii_digit())
            && let Ok(val) = prefix.parse::<usize>()
        {
            return Some((NumberingKind::Arabic, val));
        }
        if prefix.len() == 1 {
            let c = prefix.chars().next().unwrap();
            if c.is_ascii_lowercase() {
                let val = (c as usize) - ('a' as usize) + 1;
                if c == 'i' || c == 'v' || c == 'x' {
                    return Some((
                        NumberingKind::LowerRoman,
                        Self::roman_to_usize(prefix).unwrap_or(val),
                    ));
                }
                return Some((NumberingKind::LowerAlpha, val));
            } else if c.is_ascii_uppercase() {
                let val = (c as usize) - ('A' as usize) + 1;
                if c == 'I' || c == 'V' || c == 'X' {
                    return Some((
                        NumberingKind::UpperRoman,
                        Self::roman_to_usize(prefix).unwrap_or(val),
                    ));
                }
                return Some((NumberingKind::UpperAlpha, val));
            }
        }
        if Self::is_roman_numeral(prefix)
            && let Some(val) = Self::roman_to_usize(prefix)
        {
            if prefix.chars().all(|c| c.is_ascii_uppercase()) {
                return Some((NumberingKind::UpperRoman, val));
            } else {
                return Some((NumberingKind::LowerRoman, val));
            }
        }
        None
    }

    pub fn roman_to_usize(s: &str) -> Option<usize> {
        let mut total = 0;
        let mut prev_val = 0;
        for c in s.chars().rev() {
            let val = match c.to_ascii_uppercase() {
                'I' => 1,
                'V' => 5,
                'X' => 10,
                'L' => 50,
                'C' => 100,
                'D' => 500,
                'M' => 1000,
                _ => return None,
            };
            if val < prev_val {
                total -= val;
            } else {
                total += val;
                prev_val = val;
            }
        }
        if total > 0 { Some(total) } else { None }
    }

    fn is_roman_numeral(s: &str) -> bool {
        if s.is_empty() || s.len() > 10 {
            return false;
        }
        if !s.chars().all(|c| {
            matches!(
                c,
                'i' | 'v' | 'x' | 'l' | 'c' | 'd' | 'm' | 'I' | 'V' | 'X' | 'L' | 'C' | 'D' | 'M'
            )
        }) {
            return false;
        }
        Self::roman_to_usize(s).is_some()
    }

    pub fn detect_rst_option_marker(line: &str) -> Option<(Vec<String>, Option<String>, &str)> {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('-') {
            return None;
        }
        if trimmed.starts_with("- ") || trimmed.starts_with("-- ") || trimmed.starts_with("- [") {
            return None;
        }
        if trimmed.chars().all(|c| c == '-') {
            return None;
        }

        let (synopsis_str, desc_str) = if let Some(sep_pos) = trimmed.find("  ") {
            let syn = trimmed[..sep_pos].trim();
            let desc = trimmed[sep_pos..].trim();
            (syn, desc)
        } else {
            (trimmed.trim(), "")
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

        Some((flags, common_arg, desc_str))
    }

    pub fn detect_rst_definition_term(
        line: &str,
    ) -> Option<(Vec<InlineNode>, Vec<Vec<InlineNode>>)> {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with(".. ")
            || trimmed.starts_with(':')
            || trimmed.starts_with('-')
            || trimmed.starts_with('*')
            || trimmed.starts_with('+')
        {
            return None;
        }
        if Self::is_rst_heading_underline(trimmed) {
            return None;
        }
        if Self::detect_rst_list_marker(line).is_some() {
            return None;
        }

        let parts: Vec<&str> = trimmed.split(" : ").collect();
        let term_raw = parts[0].trim();
        if term_raw.is_empty() {
            return None;
        }

        let term = Self::parse_rst_inlines(term_raw);
        let mut classifiers = Vec::new();
        for c_raw in &parts[1..] {
            let c_trimmed = c_raw.trim();
            if !c_trimmed.is_empty() {
                classifiers.push(Self::parse_rst_inlines(c_trimmed));
            }
        }

        Some((term, classifiers))
    }

    pub fn detect_rst_field_marker(line: &str) -> Option<(String, &str)> {
        let trimmed = line.trim_start();
        if trimmed.starts_with(':')
            && !trimmed.starts_with("::")
            && let Some(colon_idx) = trimmed[1..].find(':')
        {
            let name = trimmed[1..1 + colon_idx].trim();
            let rest = trimmed[1 + colon_idx + 1..].trim();
            if !name.is_empty() && !name.contains('`') && !name.contains("::") {
                return Some((name.to_string(), rest));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lsp_directive() {
        let rst = r#"
.. lsp:: echo
   :kind: builtin
   :synopsis: echo [-n] [args...]
   :summary: Standard output engine
   :category: flow
   :handler: builtins::echo

   Prints arguments to stdout.
"#;
        let res = RstParser::parse(rst);
        assert_eq!(res.ast.len(), 1);
        match &res.ast[0] {
            DocNode::LspSymbol {
                name,
                kind,
                synopsis,
                summary,
                category,
                handler,
                children,
            } => {
                assert_eq!(name, "echo");
                assert_eq!(kind, "builtin");
                assert_eq!(synopsis, "echo [-n] [args...]");
                assert_eq!(summary, "Standard output engine");
                assert_eq!(category, "flow");
                assert_eq!(handler, "builtins::echo");
                assert_eq!(children.len(), 1);
            }
            other => panic!("Expected LspSymbol node, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_lsp_directive_with_nested_code_block() {
        let rst = r#"
.. lsp:: cd
   :kind: builtin
   :synopsis: cd [directory]
   :summary: Change current working directory
   :category: fs
   :handler: builtins::cd

   Changes the current working directory.

   .. code-block:: esh

      cd ./src
      cd -
"#;
        let res = RstParser::parse(rst);
        assert_eq!(res.ast.len(), 1);
        match &res.ast[0] {
            DocNode::LspSymbol {
                name,
                category,
                children,
                ..
            } => {
                assert_eq!(name, "cd");
                assert_eq!(category, "fs");
                assert_eq!(children.len(), 2);
                match &children[1] {
                    DocNode::CodeBlock { language, code, .. } => {
                        assert_eq!(language.as_deref(), Some("esh"));
                        assert!(code.contains("cd ./src"));
                        assert!(code.contains("cd -"));
                    }
                    other => panic!("Expected CodeBlock child, found {:?}", other),
                }
            }
            other => panic!("Expected LspSymbol node, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_three_dash_divider() {
        let rst = "Some prose\n\n---\n\nMore prose";
        let res = RstParser::parse(rst);
        assert_eq!(res.ast.len(), 3);
        assert!(matches!(res.ast[1], DocNode::HorizontalRule));
    }

    #[test]
    fn test_parse_diagram_directives() {
        let rst = r#"
.. svgbob:: Pipeline Flow
   :caption: Figure 1: DAG

   +---+ ---> +---+
   | A |      | B |
   +---+      +---+

.. mermaid:: Auth Sequence
   :caption: Figure 2: Auth

   sequenceDiagram
       Client->>Server: Hello
"#;
        let res = RstParser::parse(rst);
        assert_eq!(res.ast.len(), 2);
        match &res.ast[0] {
            DocNode::Diagram {
                kind,
                title,
                caption,
                code,
                ..
            } => {
                assert_eq!(*kind, DiagramKind::Svgbob);
                assert_eq!(title.as_deref(), Some("Pipeline Flow"));
                assert_eq!(caption.as_deref(), Some("Figure 1: DAG"));
                assert!(code.contains("+---+ ---> +---+"));
            }
            other => panic!("Expected Svgbob Diagram node, found {:?}", other),
        }
        match &res.ast[1] {
            DocNode::Diagram {
                kind,
                title,
                caption,
                code,
                ..
            } => {
                assert_eq!(*kind, DiagramKind::Mermaid);
                assert_eq!(title.as_deref(), Some("Auth Sequence"));
                assert_eq!(caption.as_deref(), Some("Figure 2: Auth"));
                assert!(code.contains("sequenceDiagram"));
            }
            other => panic!("Expected Mermaid Diagram node, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_rst_math() {
        let rst = r#"
The mass-energy equivalence is :math:`E = mc^2`.

.. math::

   \int_0^1 x^2 dx = \frac{1}{3}
"#;
        let res = RstParser::parse(rst);
        assert_eq!(res.ast.len(), 2);

        // 1. Paragraph with :math:`...`
        match &res.ast[0] {
            DocNode::Paragraph { children } => {
                let has_inline_math = children
                    .iter()
                    .any(|c| matches!(c, InlineNode::Math { latex } if latex.contains("E = mc^2")));
                assert!(has_inline_math, "Expected InlineNode::Math in paragraph");
            }
            other => panic!("Expected Paragraph, found {:?}", other),
        }

        // 2. Directive .. math::
        match &res.ast[1] {
            DocNode::MathBlock { latex } => {
                assert!(latex.contains(r"\int_0^1"));
            }
            other => panic!("Expected MathBlock, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_rst_lists() {
        let rst = r#"
* Bullet item 1
  with multi-line continuation
* Bullet item 2
+ Plus bullet item 3

1. Numbered item 1
2. Numbered item 2

#. Auto item 1
#. Auto item 2
"#;
        let res = RstParser::parse(rst);
        assert_eq!(res.ast.len(), 3);

        // 1. Bullet list
        match &res.ast[0] {
            DocNode::List { ordered, items, .. } => {
                assert!(!ordered);
                assert_eq!(items.len(), 3);
                match &items[0][0] {
                    DocNode::Paragraph { children } => {
                        assert!(children.iter().any(|c| matches!(c, InlineNode::Text { content } if content.contains("with multi-line continuation"))));
                    }
                    other => panic!("Expected Paragraph, found {:?}", other),
                }
            }
            other => panic!("Expected bullet List, found {:?}", other),
        }

        // 2. Numbered list
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

        // 3. Auto-enumerated list
        match &res.ast[2] {
            DocNode::List { ordered, items, .. } => {
                assert!(ordered);
                assert_eq!(items.len(), 2);
            }
            other => panic!("Expected auto ordered List, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_rst_list_with_nested_blocks() {
        let rst = r#"
1. **Single-File Components (``.evin``)**:
   A modern single-file format that unifies markup:

   .. code-block:: html

      <template>
        <div>Hello</div>
      </template>

2. **Co-located Triads (``.html``, ``.scss``, ``.js``)**:
   * **``[component].html``**: Template markup
   * **``[component].scss``**: Component styling
"#;
        let res = RstParser::parse(rst);
        assert_eq!(res.ast.len(), 1);

        match &res.ast[0] {
            DocNode::List { ordered, items, .. } => {
                assert!(ordered);
                assert_eq!(items.len(), 2);

                // Item 1: Paragraph + CodeBlock
                let item1 = &items[0];
                assert_eq!(item1.len(), 2);
                assert!(matches!(&item1[0], DocNode::Paragraph { .. }));
                match &item1[1] {
                    DocNode::CodeBlock { language, code, .. } => {
                        assert_eq!(language.as_deref(), Some("html"));
                        assert!(code.contains("<template>"));
                    }
                    other => panic!("Expected CodeBlock child, found {:?}", other),
                }

                // Item 2: Paragraph + nested List
                let item2 = &items[1];
                assert_eq!(item2.len(), 2);
                assert!(matches!(&item2[0], DocNode::Paragraph { .. }));
                match &item2[1] {
                    DocNode::List {
                        ordered: nested_ordered,
                        items: nested_items,
                        ..
                    } => {
                        assert!(!nested_ordered);
                        assert_eq!(nested_items.len(), 2);
                    }
                    other => panic!("Expected nested List child, found {:?}", other),
                }
            }
            other => panic!("Expected ordered List, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_rst_cli_option_list() {
        let rst = r#"
-h, --help           Show this help message and exit.
-o <file>, --output  Direct output to a specific file.
-j, --jobs=<N>       Run parallel compilation threads.
"#;
        let res = RstParser::parse(rst);
        assert_eq!(res.ast.len(), 1);
        match &res.ast[0] {
            DocNode::OptionList { items } => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0].flags, vec!["-h", "--help"]);
                assert_eq!(items[0].argument, None);
                assert_eq!(items[1].flags, vec!["-o", "--output"]);
                assert_eq!(items[1].argument, Some("<file>".to_string()));
                assert_eq!(items[2].flags, vec!["-j", "--jobs"]);
                assert_eq!(items[2].argument, Some("<N>".to_string()));
            }
            other => panic!("Expected OptionList, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_rst_definition_list_with_classifiers() {
        let rst = r#"
max_threads : integer : optional (default: 4)
    The maximum number of parallel DAG workers allocated to the task.

timeout : duration (e.g. 30s, 5m)
    Hard execution cutoff threshold before SIGKILL is dispatched.
"#;
        let res = RstParser::parse(rst);
        assert_eq!(res.ast.len(), 1);
        match &res.ast[0] {
            DocNode::DefinitionList { items } => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].term[0].plain_text(), "max_threads");
                assert_eq!(items[0].classifiers.len(), 2);
                assert_eq!(items[0].classifiers[0][0].plain_text(), "integer");
                assert_eq!(
                    items[0].classifiers[1][0].plain_text(),
                    "optional (default: 4)"
                );
                assert_eq!(items[1].term[0].plain_text(), "timeout");
                assert_eq!(items[1].classifiers.len(), 1);
                assert_eq!(
                    items[1].classifiers[0][0].plain_text(),
                    "duration (e.g. 30s, 5m)"
                );
            }
            other => panic!("Expected DefinitionList, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_rst_field_list() {
        let rst = r#"
:Action: ``evin/action-docs@v1``
:Inputs: ``workspace.yaml``, ``docs/``
:Outputs: ``dist/docs/`` (Static Site)
"#;
        let res = RstParser::parse(rst);
        assert_eq!(res.ast.len(), 1);
        match &res.ast[0] {
            DocNode::FieldList { items } => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0].name, "Action");
                assert_eq!(items[1].name, "Inputs");
                assert_eq!(items[2].name, "Outputs");
            }
            other => panic!("Expected FieldList, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_rst_roman_and_alpha_ordered_lists() {
        let rst = r#"
(i) First roman item
(ii) Second roman item
(iii) Third roman item
"#;
        let res = RstParser::parse(rst);
        assert_eq!(res.ast.len(), 1);
        match &res.ast[0] {
            DocNode::List {
                ordered,
                numbering,
                start,
                items,
            } => {
                assert!(ordered);
                assert_eq!(*numbering, NumberingKind::LowerRoman);
                assert_eq!(*start, 1);
                assert_eq!(items.len(), 3);
            }
            other => panic!("Expected Roman List, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_rst_task_list() {
        let rst = r#"
- [x] Implement AST tree-shaking for CSS & JS
- [x] Native IntersectionObserver ScrollSpy
- [ ] Air-gapped single-file export mode
"#;
        let res = RstParser::parse(rst);
        assert_eq!(res.ast.len(), 1);
        match &res.ast[0] {
            DocNode::TaskList { items } => {
                assert_eq!(items.len(), 3);
                assert!(items[0].checked);
                assert!(items[1].checked);
                assert!(!items[2].checked);
            }
            other => panic!("Expected TaskList, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_rst_nested_inlines_and_code_inside_strong() {
        let rst = "**Single-File Components (``.evin``)** and **Co-located Triads (``.html``, ``.scss``, ``.js``)** with **``[component].html``** and :code:`inline_role`.";
        let res = RstParser::parse(rst);
        assert_eq!(res.ast.len(), 1);

        match &res.ast[0] {
            DocNode::Paragraph { children } => {
                // Item 0: Strong { children: [Text("Single-File Components ("), Code(".evin"), Text(")")] }
                match &children[0] {
                    InlineNode::Strong {
                        children: strong_children,
                    } => {
                        assert_eq!(strong_children.len(), 3);
                        assert_eq!(
                            strong_children[0],
                            InlineNode::Text {
                                content: "Single-File Components (".to_string()
                            }
                        );
                        assert_eq!(
                            strong_children[1],
                            InlineNode::Code {
                                content: ".evin".to_string()
                            }
                        );
                        assert_eq!(
                            strong_children[2],
                            InlineNode::Text {
                                content: ")".to_string()
                            }
                        );
                    }
                    other => panic!("Expected Strong, found {:?}", other),
                }

                // Item 2: Strong { children: [Text("Co-located Triads ("), Code(".html"), Text(", "), Code(".scss"), Text(", "), Code(".js"), Text(")")] }
                match &children[2] {
                    InlineNode::Strong {
                        children: strong_children,
                    } => {
                        assert_eq!(strong_children.len(), 7);
                        assert_eq!(
                            strong_children[1],
                            InlineNode::Code {
                                content: ".html".to_string()
                            }
                        );
                        assert_eq!(
                            strong_children[3],
                            InlineNode::Code {
                                content: ".scss".to_string()
                            }
                        );
                        assert_eq!(
                            strong_children[5],
                            InlineNode::Code {
                                content: ".js".to_string()
                            }
                        );
                    }
                    other => panic!("Expected Strong, found {:?}", other),
                }

                // Item 4: Strong { children: [Code("[component].html")] }
                match &children[4] {
                    InlineNode::Strong {
                        children: strong_children,
                    } => {
                        assert_eq!(strong_children.len(), 1);
                        assert_eq!(
                            strong_children[0],
                            InlineNode::Code {
                                content: "[component].html".to_string()
                            }
                        );
                    }
                    other => panic!("Expected Strong, found {:?}", other),
                }

                // Item 6: Code { content: "inline_role" }
                match &children[6] {
                    InlineNode::Code { content } => {
                        assert_eq!(content, "inline_role");
                    }
                    other => panic!("Expected Code, found {:?}", other),
                }
            }
            other => panic!("Expected Paragraph, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_rst_arbitrary_nested_styles_and_roles() {
        let rst = "**Bold with *italic* and :math:`E=mc^2` and ``code``** and *Italic with **bold** and `Link <https://example.com>`_* and :doc:`Docs </guides/site>` and :ref:`section-target`.";
        let res = RstParser::parse(rst);
        assert_eq!(res.ast.len(), 1);

        match &res.ast[0] {
            DocNode::Paragraph { children } => {
                // 1. Strong with nested Emphasis, Math, and Code
                match &children[0] {
                    InlineNode::Strong { children: inner } => {
                        assert!(
                            inner
                                .iter()
                                .any(|c| matches!(c, InlineNode::Emphasis { .. }))
                        );
                        assert!(inner.iter().any(
                            |c| matches!(c, InlineNode::Math { latex } if latex.contains("E=mc^2"))
                        ));
                        assert!(inner.iter().any(
                            |c| matches!(c, InlineNode::Code { content } if content == "code")
                        ));
                    }
                    other => panic!("Expected Strong, found {:?}", other),
                }

                // 2. Emphasis with nested Strong and Link
                match &children[2] {
                    InlineNode::Emphasis { children: inner } => {
                        assert!(inner.iter().any(|c| matches!(c, InlineNode::Strong { .. })));
                        assert!(inner.iter().any(|c| matches!(c, InlineNode::Link { href, content, .. } if href == "https://example.com" && content == "Link")));
                    }
                    other => panic!("Expected Emphasis, found {:?}", other),
                }

                // 3. :doc: role parsed as Link
                match &children[4] {
                    InlineNode::Link { href, content, .. } => {
                        assert_eq!(href, "/guides/site");
                        assert_eq!(content, "Docs");
                    }
                    other => panic!("Expected Link from :doc:, found {:?}", other),
                }

                // 4. :ref: role parsed as Link
                match &children[6] {
                    InlineNode::Link { href, content, .. } => {
                        assert_eq!(href, "#section-target");
                        assert_eq!(content, "section-target");
                    }
                    other => panic!("Expected Link from :ref:, found {:?}", other),
                }
            }
            other => panic!("Expected Paragraph, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_rst_stepper_directive() {
        let rst = r#"
.. steps::

   .. step:: Initialize Workspace
      :status: done
      :badge: 1

      Bootstrap the workspace with ``evin init``.

   .. step:: Configure Pipeline
      :status: active
      :badge: 2

      Declare tasks in ``workspace.yaml``.
"#;
        let res = RstParser::parse(rst);
        assert_eq!(res.ast.len(), 1);
        match &res.ast[0] {
            DocNode::Stepper { steps } => {
                assert_eq!(steps.len(), 2);
                assert_eq!(steps[0].badge.as_deref(), Some("1"));
                assert_eq!(steps[0].status.as_deref(), Some("done"));
                assert_eq!(steps[1].badge.as_deref(), Some("2"));
                assert_eq!(steps[1].status.as_deref(), Some("active"));
            }
            other => panic!("Expected Stepper, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_rst_admonition_with_nested_list() {
        let rst = r#"
.. tip::
   Essential verification checklist before deployment:

   - [x] Run ``cargo test --workspace``
   - [x] Audit static documentation assets
   - [ ] Run Playwright end-to-end browser suite
"#;
        let res = RstParser::parse(rst);
        assert_eq!(res.ast.len(), 1);
        match &res.ast[0] {
            DocNode::Admonition { kind, children, .. } => {
                assert_eq!(kind, "tip");
                assert_eq!(children.len(), 2);
                match &children[0] {
                    DocNode::Paragraph { children } => {
                        assert_eq!(
                            children[0].plain_text(),
                            "Essential verification checklist before deployment:"
                        );
                    }
                    other => panic!("Expected Paragraph, found {:?}", other),
                }
                match &children[1] {
                    DocNode::TaskList { items } => {
                        assert_eq!(items.len(), 3);
                        assert!(items[0].checked);
                        assert!(items[1].checked);
                        assert!(!items[2].checked);
                    }
                    other => panic!("Expected TaskList, found {:?}", other),
                }
            }
            other => panic!("Expected Admonition, found {:?}", other),
        }
    }

    #[test]
    fn test_parse_rst_tape_directive() {
        let parsed = RstParser::parse(
            ".. tape:: Demo\n   :rows: 8\n   :max_frames: 99\n\n   type \"echo hi\"\n   enter\n   wait\n",
        );
        match &parsed.ast[0] {
            DocNode::Tape {
                script,
                config,
                rendered_svg,
            } => {
                assert_eq!(config.title.as_deref(), Some("Demo"));
                assert_eq!(config.rows, 8);
                assert_eq!(config.max_frames, 99);
                assert!(script.contains("type \"echo hi\""));
                assert!(rendered_svg.is_none());
            }
            other => panic!("expected Tape, found {other:?}"),
        }
    }

    #[test]
    fn test_parse_rst_feature_directive_as_an_admonition() {
        let parsed = RstParser::parse(".. feature::\n\n   Feature copy.\n");
        match &parsed.ast[0] {
            DocNode::Admonition { kind, children, .. } => {
                assert_eq!(kind, "feature");
                assert_eq!(children.len(), 1);
            }
            other => panic!("expected Feature admonition, found {other:?}"),
        }
    }
}
