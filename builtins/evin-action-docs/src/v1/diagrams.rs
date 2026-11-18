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

use crate::v1::ast::{DiagramKind, DocNode};
use evin_diagnostic::{Diagnostic, IssueCode};
use merman::render::HeadlessRenderer;
use pikchr::{Pikchr, PikchrFlags};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn issue(code: &'static str, source: &str, message: impl Into<String>) -> Diagnostic {
    evin_locales::diagnostic(
        IssueCode::new(code)
            .arg("error", message.into())
            .arg("path", source),
    )
    .with_file_location(source, None, None)
}

fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn set_attribute(root: &str, name: &str, value: &str) -> String {
    let needle = format!(" {name}=\"");
    if let Some(start) = root.find(&needle) {
        let value_start = start + needle.len();
        if let Some(offset) = root[value_start..].find('"') {
            let end = value_start + offset + 1;
            return format!("{} {}=\"{}\"{}", &root[..start], name, value, &root[end..]);
        }
    }
    let mut updated = root.to_string();
    updated.insert_str(updated.len() - 1, &format!(" {name}=\"{value}\""));
    updated
}

fn attribute_value(root: &str, name: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!(" {name}={quote}");
        if let Some(start) = root.find(&needle) {
            let value_start = start + needle.len();
            if let Some(offset) = root[value_start..].find(quote) {
                return Some(root[value_start..value_start + offset].to_string());
            }
        }
    }
    None
}

fn normalize_svg_dimensions(root: &str) -> String {
    let view_box = attribute_value(root, "viewBox").or_else(|| {
        let width = attribute_value(root, "width")?;
        let height = attribute_value(root, "height")?;
        Some(format!("0 0 {width} {height}"))
    });
    let mut normalized = root.to_string();
    let Some(view_box) = view_box else {
        return normalized;
    };
    let dimensions = view_box
        .split(|character: char| character.is_ascii_whitespace() || character == ',')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if dimensions.len() != 4 {
        return normalized;
    }
    if attribute_value(&normalized, "viewBox").is_none() {
        normalized = set_attribute(&normalized, "viewBox", &view_box);
    }
    for (name, dimension) in [("width", dimensions[2]), ("height", dimensions[3])] {
        if attribute_value(&normalized, name).is_none_or(|value| value.ends_with('%')) {
            normalized = set_attribute(&normalized, name, dimension);
        }
    }
    normalized
}

fn decorate_svg(
    mut svg: String,
    id: &str,
    description: &str,
    kind: &DiagramKind,
) -> Result<String, String> {
    let root_start = svg
        .find("<svg")
        .ok_or_else(|| "renderer did not produce an SVG root element".to_string())?;
    let root_end = svg[root_start..]
        .find('>')
        .map(|offset| root_start + offset)
        .ok_or_else(|| "renderer produced an incomplete SVG root element".to_string())?;
    let root = normalize_svg_dimensions(&svg[root_start..=root_end]);
    let attributes = set_attribute(&root, "id", &xml_escape(id));
    let attributes = set_attribute(&attributes, "class", "diagram-svg");
    let attributes = set_attribute(&attributes, "data-diagram-kind", &kind.to_string());
    let title_id = format!("{id}-title");
    let attributes = set_attribute(&attributes, "role", "img");
    let attributes = set_attribute(&attributes, "aria-labelledby", &title_id);
    svg.replace_range(root_start..=root_end, &attributes);
    let title = format!(
        "<title id=\"{title_id}\">{}</title>",
        xml_escape(description)
    );
    let style = format!(
        "<style>.diagram-svg{{--diagram-ink:#17251d;--diagram-accent:#315b43;--diagram-surface:#e7eee3;--diagram-highlight:#d5f05b;--diagram-muted:#ccd8c9}}#{id}{{background-color:transparent!important}}#{id} .merman-foreignobject-fallback{{display:none!important}}#{id} text{{fill:var(--diagram-ink)!important;font-family:inherit!important;font-size:16px!important}}#{id} foreignObject div{{color:var(--diagram-ink)!important;font-family:inherit!important;font-size:16px!important}}#{id} path,#{id} line,#{id} polyline,#{id} polygon{{stroke:var(--diagram-ink)!important}}#{id} polygon,#{id} .filled{{fill:var(--diagram-ink)!important}}#{id} rect:not(.backdrop){{fill:var(--diagram-surface)!important;stroke:var(--diagram-accent)!important}}#{id} .backdrop{{fill:transparent!important;stroke:none!important}}#{id} .node polygon,#{id} .node circle,#{id} .cluster rect{{fill:var(--diagram-surface)!important;stroke:var(--diagram-accent)!important}}#{id} .edgePath path,#{id} .flowchart-link,#{id} marker path{{stroke:var(--diagram-ink)!important}}#{id} .edgeLabel,#{id} .labelBkg{{fill:var(--diagram-highlight)!important}}</style>"
    );
    let insert_at = svg
        .find("<svg")
        .and_then(|start| svg[start..].find('>').map(|offset| start + offset + 1))
        .ok_or_else(|| "renderer produced an incomplete SVG".to_string())?;
    svg.insert_str(insert_at, &title);
    let close = svg
        .rfind("</svg>")
        .ok_or_else(|| "renderer produced an incomplete SVG".to_string())?;
    svg.insert_str(close, &style);
    Ok(svg)
}

fn render_svg(
    kind: &DiagramKind,
    source: &str,
    id: &str,
    description: &str,
) -> Result<String, String> {
    let svg = match kind {
        DiagramKind::Mermaid => HeadlessRenderer::new()
            .with_diagram_id(id)
            .render_svg_readable_sync(source)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Mermaid source did not produce a diagram".to_string())?,
        DiagramKind::Svgbob => svgbob::to_svg_string_pretty(source),
        DiagramKind::Pikchr => Pikchr::render(source, Some("diagram-svg"), PikchrFlags::default())
            .map_err(|error| error.to_string())?
            .rendered()
            .to_string(),
    };
    decorate_svg(svg, id, description, kind)
}

struct DiagramBuilder<'a> {
    source_path: &'a str,
    page_id: &'a str,
    output: &'a Path,
    seen_ids: HashSet<String>,
    next_index: usize,
    failure: Option<Diagnostic>,
}

impl evin_doc::analysis::MutVisitor for DiagramBuilder<'_> {
    fn visit_node(&mut self, node: &mut DocNode) {
        if self.failure.is_some() {
            return;
        }
        let DocNode::Diagram {
            kind,
            code,
            title,
            caption,
            alt,
            id,
            svg_path,
            ..
        } = node
        else {
            return;
        };
        let generated_id;
        if id.is_none() {
            generated_id = format!("diagram-{}-{}", self.page_id, self.next_index);
            *id = Some(generated_id);
        }
        self.next_index += 1;
        let diagram_id = id.as_deref().unwrap_or_default();
        if !valid_id(diagram_id) {
            self.failure = Some(issue(
                "docs-diagram-invalid-id",
                self.source_path,
                format!(
                    "Diagram ID '{diagram_id}' must start with a letter and contain only letters, numbers, underscores, or hyphens."
                ),
            ));
            return;
        }
        if !self.seen_ids.insert(diagram_id.to_string()) {
            self.failure = Some(issue(
                "docs-diagram-duplicate-id",
                self.source_path,
                format!("Diagram ID '{diagram_id}' is duplicated on this page."),
            ));
            return;
        }
        let description = alt
            .as_deref()
            .or(caption.as_deref())
            .or(title.as_deref())
            .map(str::to_string)
            .unwrap_or_else(|| kind.to_string());
        match render_svg(kind, code, diagram_id, &description) {
            Ok(svg) => {
                let relative = PathBuf::from("diagrams")
                    .join(self.page_id)
                    .join(format!("{diagram_id}.svg"));
                let destination = self.output.join(&relative);
                if let Some(parent) = destination.parent()
                    && let Err(error) = std::fs::create_dir_all(parent)
                {
                    self.failure = Some(issue(
                        "docs-diagram-write-failed",
                        self.source_path,
                        format!("could not create {}: {error}", parent.display()),
                    ));
                    return;
                }
                if let Err(error) = std::fs::write(&destination, svg) {
                    self.failure = Some(issue(
                        "docs-diagram-write-failed",
                        self.source_path,
                        format!("could not write {}: {error}", destination.display()),
                    ));
                    return;
                }
                *svg_path = Some(relative.to_string_lossy().replace('\\', "/"));
            }
            Err(error) => {
                self.failure = Some(issue(
                    "docs-diagram-render-failed",
                    self.source_path,
                    format!("could not render {kind} diagram '{diagram_id}': {error}"),
                ));
            }
        }
    }
}

pub fn render_page_diagrams(
    ast: &mut [DocNode],
    page_id: &str,
    source_path: &str,
    output: &Path,
) -> Result<(), Diagnostic> {
    let mut builder = DiagramBuilder {
        source_path,
        page_id,
        output,
        seen_ids: HashSet::new(),
        next_index: 1,
        failure: None,
    };
    evin_doc::analysis::walk_nodes_mut(ast, &mut builder);
    builder.failure.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagram(kind: DiagramKind, code: &str, id: Option<&str>) -> DocNode {
        DocNode::Diagram {
            kind,
            code: code.to_string(),
            title: Some("Small example".to_string()),
            caption: None,
            alt: None,
            id: id.map(str::to_string),
            svg_path: None,
            rendered_mermaid: None,
        }
    }

    #[test]
    fn renders_existing_diagram_languages_to_accessible_svg_files() {
        let output = tempfile::tempdir().unwrap();
        let samples = [
            (DiagramKind::Mermaid, "flowchart TD\nA[Start] --> B[Done]"),
            (DiagramKind::Svgbob, "+---+\n| A |\n+---+"),
            (DiagramKind::Pikchr, "box \"Start\"; arrow; box \"Done\""),
        ];
        for (index, (kind, code)) in samples.into_iter().enumerate() {
            let page_id = format!("page-example-{index}");
            let mut ast = vec![diagram(kind, code, None)];
            render_page_diagrams(&mut ast, &page_id, "guide.md", output.path()).unwrap();
            let DocNode::Diagram { id, svg_path, .. } = &ast[0] else {
                unreachable!()
            };
            let id = id.as_deref().unwrap();
            assert_eq!(id, format!("diagram-{page_id}-1"));
            let svg =
                std::fs::read_to_string(output.path().join(svg_path.as_deref().unwrap())).unwrap();
            assert!(svg.starts_with("<svg"));
            assert!(svg.contains(&format!("id=\"{id}\"")));
            assert!(svg.contains(&format!("id=\"{id}-title\"")));
            assert!(svg.contains("--diagram-highlight"));
            if kind == DiagramKind::Mermaid {
                assert!(svg.contains(".merman-foreignobject-fallback{display:none!important}"));
            }
        }
    }

    #[test]
    fn rejects_invalid_and_duplicate_diagram_ids() {
        let output = tempfile::tempdir().unwrap();
        let mut invalid = vec![diagram(
            DiagramKind::Svgbob,
            "+---+\n| A |\n+---+",
            Some("bad id"),
        )];
        let error = render_page_diagrams(&mut invalid, "page-example", "guide.md", output.path())
            .unwrap_err();
        assert_eq!(error.code.key, "docs-diagram-invalid-id");

        let mut duplicate = vec![
            diagram(DiagramKind::Svgbob, "+---+\n| A |\n+---+", Some("same")),
            diagram(DiagramKind::Svgbob, "+---+\n| B |\n+---+", Some("same")),
        ];
        let error = render_page_diagrams(&mut duplicate, "page-example", "guide.md", output.path())
            .unwrap_err();
        assert_eq!(error.code.key, "docs-diagram-duplicate-id");
    }
}
