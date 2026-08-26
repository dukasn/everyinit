// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License, version 3.

//! Strict, offline Mermaid compilation for web documentation builds.

use crate::v1::ast::{DiagramKind, DocNode, DocSiteAst};
use evin_doc::ast::RenderedMermaid;
use merman::render::{
    HeadlessRenderer, HostThemeAppearance, HostThemeOutput, HostThemeProfile, HostThemeRoles,
    HostThemeRootBackground, sanitize_svg_id,
};

/// Compile every Mermaid diagram into deterministic light and dark SVGs.
///
/// This runs only in the web rendering path. Parsers and the LSP catalog keep
/// the source-only AST representation and do not need the rendering engine.
pub fn compile_site(site: &mut DocSiteAst) -> Result<(), String> {
    for page in &mut site.pages {
        let mut position = 0;
        compile_nodes(&mut page.ast, &page.slug, &page.source_path, &mut position)?;
    }
    Ok(())
}

fn compile_nodes(
    nodes: &mut [DocNode],
    page_slug: &str,
    source_path: &str,
    position: &mut usize,
) -> Result<(), String> {
    for node in nodes {
        match node {
            DocNode::Diagram {
                kind: DiagramKind::Mermaid,
                code,
                title,
                rendered_mermaid,
                ..
            } => {
                *position += 1;
                *rendered_mermaid = Some(compile_diagram(
                    code,
                    page_slug,
                    source_path,
                    title.as_deref(),
                    *position,
                )?);
            }
            DocNode::Admonition { children, .. }
            | DocNode::Directive { body: children, .. }
            | DocNode::LspSymbol { children, .. } => {
                compile_nodes(children, page_slug, source_path, position)?;
            }
            DocNode::List { items, .. } => {
                for item in items {
                    compile_nodes(item, page_slug, source_path, position)?;
                }
            }
            DocNode::TaskList { items } => {
                for item in items {
                    compile_nodes(&mut item.body, page_slug, source_path, position)?;
                }
            }
            DocNode::DefinitionList { items } => {
                for item in items {
                    compile_nodes(&mut item.body, page_slug, source_path, position)?;
                }
            }
            DocNode::OptionList { items } => {
                for item in items {
                    compile_nodes(&mut item.body, page_slug, source_path, position)?;
                }
            }
            DocNode::FieldList { items } => {
                for item in items {
                    compile_nodes(&mut item.body, page_slug, source_path, position)?;
                }
            }
            DocNode::Stepper { steps } => {
                for step in steps {
                    compile_nodes(&mut step.body, page_slug, source_path, position)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn compile_diagram(
    source: &str,
    page_slug: &str,
    source_path: &str,
    title: Option<&str>,
    position: usize,
) -> Result<RenderedMermaid, String> {
    let subject = title
        .filter(|title| !title.trim().is_empty())
        .map(|title| format!("Mermaid diagram '{title}'"))
        .unwrap_or_else(|| format!("Mermaid diagram #{position}"));
    let diagram_id = sanitize_svg_id(&format!("evin-{page_slug}-mermaid-{position}"));

    let profile = matter_profile();
    let svg = HeadlessRenderer::new()
        .with_strict_parsing()
        .with_vendored_text_measurer()
        .with_diagram_id(&diagram_id)
        .render_svg_with_host_theme_sync(source, &profile)
        .map_err(|error| format!("{source_path}: {subject} failed to render: {error}"))?
        .ok_or_else(|| format!("{source_path}: {subject} did not contain a supported diagram"))?;

    Ok(RenderedMermaid { svg })
}

pub fn matter_profile() -> HostThemeProfile {
    let mut output = HostThemeOutput::resvg_safe_editor();
    output.root_background = HostThemeRootBackground::None;

    HostThemeProfile::builder()
        .appearance(HostThemeAppearance::Light)
        .font_family("system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif")
        .font_size("14px")
        .roles(HostThemeRoles {
            canvas: None,
            surface: Some("var(--ui-diagram-node-bg, #F5EFE7)".to_string()),
            surface_alt: Some("var(--ui-color-surface-container, #ECE5DC)".to_string()),
            surface_muted: Some("var(--ui-color-surface-container-high, #E4DDD3)".to_string()),
            text: Some("var(--ui-diagram-node-text, #1D1B19)".to_string()),
            subtle_text: Some("var(--ui-color-on-surface-variant, #524B43)".to_string()),
            border: Some("var(--ui-diagram-node-border, #C5BCB0)".to_string()),
            line: Some("var(--ui-diagram-line, #48433E)".to_string()),
            edge_label_background: Some("var(--ui-diagram-bg, #EAE3D9)".to_string()),
            cluster_background: Some("var(--ui-diagram-cluster-bg, #DFD8CE)".to_string()),
            cluster_border: Some("var(--ui-diagram-cluster-border, #ABA193)".to_string()),
            note_background: Some("var(--ui-color-surface-container-lowest, #FFFBF7)".to_string()),
            note_border: Some("var(--ui-color-outline, #C0B7AA)".to_string()),
            note_text: Some("var(--ui-color-on-surface, #1D1B19)".to_string()),
            actor_background: Some("var(--ui-diagram-node-bg, #F5EFE7)".to_string()),
            actor_border: Some("var(--ui-diagram-node-border, #C5BCB0)".to_string()),
            actor_text: Some("var(--ui-diagram-node-text, #1D1B19)".to_string()),
            activation_background: Some("var(--ui-diagram-cluster-bg, #DFD8CE)".to_string()),
            activation_border: Some("var(--ui-diagram-cluster-border, #ABA193)".to_string()),
            error: Some("var(--ui-palette-red, #C50D33)".to_string()),
            warning: Some("var(--ui-palette-yellow, #C87B08)".to_string()),
            success: Some("var(--ui-palette-green, #358B22)".to_string()),
        })
        .series_palette([
            "var(--ui-diagram-git-0, #7B2ED6)",
            "var(--ui-diagram-git-1, #1656D8)",
            "var(--ui-diagram-git-2, #358B22)",
            "var(--ui-diagram-git-3, #E65500)",
            "var(--ui-palette-red, #C50D33)",
            "var(--ui-palette-cyan, #0E7F87)",
            "var(--ui-palette-yellow, #C87B08)",
        ])
        .output(output)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::ast::{DiagramKind, DocFormat, DocNode, DocPage, DocSiteAst};
    use std::collections::HashMap;

    #[test]
    fn compiles_with_stable_distinct_ids() {
        let first = compile_diagram(
            "flowchart LR\n  A --> B",
            "guide/start",
            "docs/guide/start.md",
            None,
            1,
        )
        .unwrap();
        let second = compile_diagram(
            "flowchart LR\n  A --> B",
            "guide/start",
            "docs/guide/start.md",
            None,
            1,
        )
        .unwrap();

        assert_eq!(first, second);
        assert!(first.svg.contains("id=\"evin-guide-start-mermaid-1\""));
    }

    #[test]
    fn compiles_sequence_diagrams() {
        let rendered = compile_diagram(
            "sequenceDiagram\n  Alice->>Bob: Hello",
            "sequence",
            "docs/sequence.md",
            Some("Greeting"),
            2,
        )
        .unwrap();
        assert!(rendered.svg.starts_with("<svg"));
    }

    #[test]
    fn compiles_with_matter_theme_tokens_and_transparent_background() {
        let rendered = compile_diagram(
            "flowchart LR\n  A[Start] --> B[End]",
            "flow",
            "docs/flow.md",
            Some("Flow"),
            1,
        )
        .unwrap();

        assert!(rendered.svg.starts_with("<svg"));
        assert!(rendered.svg.contains("var(--ui-diagram-node-bg, #F5EFE7)"));
        assert!(
            rendered
                .svg
                .contains("var(--ui-diagram-node-border, #C5BCB0)")
        );
        assert!(
            rendered
                .svg
                .contains("var(--ui-diagram-node-text, #1D1B19)")
        );
        assert!(rendered.svg.contains("var(--ui-diagram-line, #48433E)"));
    }

    #[test]
    fn rejects_malformed_and_missing_diagrams_with_source_context() {
        let malformed = compile_diagram(
            "flowchart LR\n  A --",
            "bad",
            "docs/bad.md",
            Some("Broken"),
            1,
        )
        .unwrap_err();
        assert!(malformed.contains("docs/bad.md"));
        assert!(malformed.contains("Broken"));

        let missing = compile_diagram("just words", "bad", "docs/bad.md", None, 3).unwrap_err();
        assert!(missing.contains("docs/bad.md"));
        assert!(missing.contains("#3"));
    }

    #[test]
    fn assigns_unique_positions_through_nested_components_without_changing_source() {
        let source = "flowchart LR\n  A --> B";
        let diagram = || DocNode::Diagram {
            kind: DiagramKind::Mermaid,
            code: source.to_string(),
            title: None,
            caption: None,
            alt: None,
            rendered_mermaid: None,
        };
        let mut site = DocSiteAst {
            version: "1".to_string(),
            title: None,
            reference_prefix: "REF".to_string(),
            pages: vec![DocPage {
                slug: "nested/page".to_string(),
                source_path: "docs/nested/page.md".to_string(),
                format: DocFormat::Markdown,
                metadata: HashMap::new(),
                reference: None,
                toc: vec![],
                ast: vec![
                    diagram(),
                    DocNode::Admonition {
                        kind: "note".to_string(),
                        title: None,
                        children: vec![diagram()],
                    },
                ],
            }],
        };

        compile_site(&mut site).unwrap();
        let DocNode::Diagram {
            code,
            rendered_mermaid: Some(first),
            ..
        } = &site.pages[0].ast[0]
        else {
            panic!("expected compiled first diagram");
        };
        let DocNode::Admonition { children, .. } = &site.pages[0].ast[1] else {
            panic!("expected nested component");
        };
        let DocNode::Diagram {
            rendered_mermaid: Some(second),
            ..
        } = &children[0]
        else {
            panic!("expected compiled nested diagram");
        };

        assert_eq!(code, source);
        assert!(first.svg.contains("mermaid-1"));
        assert!(second.svg.contains("mermaid-2"));
        assert_ne!(first.svg, second.svg);
    }
}
