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

use crate::v1::assets::ResolvedAssets;
use crate::v1::ast::{DocNode, DocSiteAst, InlineNode};
use crate::v1::reference::looks_like_alias;
use evin_doc::analysis::{Visitor, walk_nodes};
use evin_doc::{SiteIndex, SourceResolver};
use std::path::Path;
use url::Url;

pub struct LinkValidator;

#[derive(Debug, Default)]
pub struct LinkAuditReport {
    pub diagnostics: Vec<evin_diagnostic::Diagnostic>,
    pub links_checked: usize,
    pub anchors_indexed: usize,
}

struct PageLinkChecker<'a> {
    site_ast: &'a DocSiteAst,
    index: &'a SiteIndex,
    current_slug: &'a str,
    source_path: &'a str,
    assets: &'a ResolvedAssets,
    report: &'a mut LinkAuditReport,
}

impl Visitor for PageLinkChecker<'_> {
    fn visit_node(&mut self, node: &DocNode) {
        match node {
            DocNode::Media {
                kind, src, poster, ..
            } => {
                LinkValidator::check_media_url(
                    self.current_slug,
                    "source",
                    src,
                    *kind == crate::v1::ast::MediaKind::Image,
                    self.assets,
                    &mut self.report.diagnostics,
                );
                if let Some(poster) = poster {
                    LinkValidator::check_media_url(
                        self.current_slug,
                        "poster",
                        poster,
                        false,
                        self.assets,
                        &mut self.report.diagnostics,
                    );
                }
            }
            DocNode::Diagram {
                kind, code, title, ..
            } if *kind == evin_doc::ast::DiagramKind::Svgbob => {
                DiagramValidator::validate_svgbob_diagram(
                    self.source_path,
                    title.as_deref(),
                    code,
                    &mut self.report.diagnostics,
                );
            }
            DocNode::MathBlock { latex } => MathValidator::validate_math(
                self.source_path,
                latex,
                latex2mathml::DisplayStyle::Block,
                &mut self.report.diagnostics,
            ),
            _ => {}
        }
    }

    fn visit_inline(&mut self, inline: &InlineNode) {
        match inline {
            InlineNode::Link { href, .. } => {
                self.report.links_checked += 1;
                LinkValidator::check_link(
                    self.site_ast,
                    self.index,
                    self.current_slug,
                    href,
                    &mut self.report.diagnostics,
                );
            }
            InlineNode::Image { src, .. } => LinkValidator::check_media_url(
                self.current_slug,
                "source",
                src,
                true,
                self.assets,
                &mut self.report.diagnostics,
            ),
            InlineNode::Math { latex } => MathValidator::validate_math(
                self.source_path,
                latex,
                latex2mathml::DisplayStyle::Inline,
                &mut self.report.diagnostics,
            ),
            _ => {}
        }
    }
}

impl LinkValidator {
    pub fn validate(site_ast: &DocSiteAst) -> LinkAuditReport {
        Self::validate_with_assets(site_ast, &ResolvedAssets::default())
    }

    pub fn validate_with_assets(site_ast: &DocSiteAst, assets: &ResolvedAssets) -> LinkAuditReport {
        let index = evin_doc::SiteIndex::build(site_ast)
            .expect("link validation requires a valid documentation site");
        Self::validate_with_assets_and_index(site_ast, &index, assets)
    }

    pub fn validate_with_assets_and_index(
        site_ast: &DocSiteAst,
        index: &evin_doc::SiteIndex,
        assets: &ResolvedAssets,
    ) -> LinkAuditReport {
        let mut report = LinkAuditReport {
            anchors_indexed: site_ast.pages.iter().map(|page| page.toc.len()).sum(),
            ..LinkAuditReport::default()
        };

        for page in &site_ast.pages {
            let mut checker = PageLinkChecker {
                site_ast,
                index,
                current_slug: &page.slug,
                source_path: &page.source_path,
                assets,
                report: &mut report,
            };
            walk_nodes(&page.ast, &mut checker);
        }

        report
    }

    fn check_media_url(
        current_slug: &str,
        label: &str,
        value: &str,
        allow_local_image: bool,
        assets: &ResolvedAssets,
        diagnostics: &mut Vec<evin_diagnostic::Diagnostic>,
    ) {
        if allow_local_image && assets.is_registered_reference(value) {
            return;
        }
        let valid = Url::parse(value)
            .ok()
            .is_some_and(|url| url.scheme() == "https" && url.host_str().is_some());
        if !valid {
            let requirement = if allow_local_image && ResolvedAssets::is_root_relative_image(value)
            {
                "use a registered local asset or an absolute HTTPS URL"
            } else {
                "use an absolute HTTPS URL"
            };
            diagnostics.push(evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("docs-invalid-media")
                    .arg("label", label)
                    .arg("slug", current_slug)
                    .arg("requirement", requirement),
            ));
        }
    }

    fn check_link(
        site_ast: &DocSiteAst,
        index: &SiteIndex,
        current_slug: &str,
        href: &str,
        diagnostics: &mut Vec<evin_diagnostic::Diagnostic>,
    ) {
        if href.starts_with("http://")
            || href.starts_with("https://")
            || href.starts_with("mailto:")
        {
            return;
        }

        if let Some(target) = index.resolve_alias(href) {
            if let Some(anchor) = target.anchor
                && !index.has_anchor(target.slug, anchor)
            {
                diagnostics.push(evin_locales::diagnostic(
                    evin_diagnostic::IssueCode::new("docs-broken-alias-anchor")
                        .arg("slug", current_slug)
                        .arg("alias", href)
                        .arg("anchor", anchor),
                ));
            }
            return;
        }
        if looks_like_alias(site_ast, href) {
            diagnostics.push(evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("docs-broken-formal-link")
                    .arg("slug", current_slug)
                    .arg("alias", href),
            ));
            return;
        }

        let rooted = href.starts_with('/');
        let clean_href = href.trim_start_matches('/');
        let (target_route, target_anchor) = if let Some(idx) = clean_href.find('#') {
            (&clean_href[..idx], Some(&clean_href[idx + 1..]))
        } else {
            (clean_href, None)
        };
        let resolved_route = (!target_route.is_empty())
            .then(|| resolve_internal_route(site_ast, index, current_slug, target_route, rooted))
            .flatten();

        // Ordinary routes may point outside a filtered documentation build;
        // the renderer deliberately preserves those hrefs. Formal aliases and
        // anchors remain fully validated against the immutable site index.

        if let Some(anchor) = target_anchor {
            let anchor_route = if target_route.is_empty() {
                current_slug
            } else {
                resolved_route.as_deref().unwrap_or(target_route)
            };
            if !index.has_anchor(anchor_route, anchor) {
                diagnostics.push(evin_locales::diagnostic(
                    evin_diagnostic::IssueCode::new("docs-broken-anchor")
                        .arg("slug", current_slug)
                        .arg("anchor", anchor),
                ));
            }
        }
    }
}

/// Mirrors the browser's route resolution for generated `slug/index.html`
/// pages, then falls back to the source-path map for formal-reference files
/// whose numeric filename suffix is intentionally absent from their route.
fn resolve_internal_route(
    site_ast: &DocSiteAst,
    index: &SiteIndex,
    current_slug: &str,
    target_route: &str,
    rooted: bool,
) -> Option<String> {
    let mut segments = if rooted {
        Vec::new()
    } else {
        current_slug
            .split('/')
            .filter(|part| !part.is_empty())
            .collect()
    };
    for segment in target_route.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            segment => segments.push(segment),
        }
    }
    let mut candidate = segments.join("/");
    for suffix in ["/index.html", ".html", ".markdown", ".md", ".rst"] {
        if let Some(stripped) = candidate.strip_suffix(suffix) {
            candidate = stripped.trim_end_matches('/').to_string();
            break;
        }
    }
    if candidate.ends_with("/index") {
        candidate.truncate(candidate.len() - "/index".len());
    }
    if let Some(position) = index.page_position(&candidate) {
        return Some(site_ast.pages[position].slug.clone());
    }
    for extension in ["", ".rst", ".md", ".markdown"] {
        let source_path = format!("{candidate}{extension}");
        if let Some(page) = index.page_by_source(site_ast, &source_path) {
            return Some(page.slug.clone());
        }
    }
    site_ast
        .pages
        .iter()
        .find(|page| SourceResolver::compute_slug(Path::new(&page.source_path)) == candidate)
        .map(|page| page.slug.clone())
}

pub struct DiagramValidator;

impl DiagramValidator {
    pub fn validate(site_ast: &DocSiteAst) -> Vec<evin_diagnostic::Diagnostic> {
        let mut diagnostics = Vec::new();
        for page in &site_ast.pages {
            Self::check_nodes(&page.slug, &page.source_path, &page.ast, &mut diagnostics);
        }
        diagnostics
    }

    fn check_nodes(
        _slug: &str,
        source_path: &str,
        nodes: &[DocNode],
        diagnostics: &mut Vec<evin_diagnostic::Diagnostic>,
    ) {
        for node in nodes {
            match node {
                DocNode::Diagram {
                    kind, code, title, ..
                } => {
                    if *kind == evin_doc::ast::DiagramKind::Svgbob {
                        Self::validate_svgbob_diagram(
                            source_path,
                            title.as_deref(),
                            code,
                            diagnostics,
                        );
                    }
                }
                DocNode::List { items, .. } => {
                    for item in items {
                        Self::check_nodes(_slug, source_path, item, diagnostics);
                    }
                }
                DocNode::TaskList { items } => {
                    for item in items {
                        Self::check_nodes(_slug, source_path, &item.body, diagnostics);
                    }
                }
                DocNode::DefinitionList { items } => {
                    for item in items {
                        Self::check_nodes(_slug, source_path, &item.body, diagnostics);
                    }
                }
                DocNode::OptionList { items } => {
                    for item in items {
                        Self::check_nodes(_slug, source_path, &item.body, diagnostics);
                    }
                }
                DocNode::FieldList { items } => {
                    for item in items {
                        Self::check_nodes(_slug, source_path, &item.body, diagnostics);
                    }
                }
                DocNode::Stepper { steps } => {
                    for step in steps {
                        Self::check_nodes(_slug, source_path, &step.body, diagnostics);
                    }
                }
                DocNode::Admonition { children, .. } => {
                    Self::check_nodes(_slug, source_path, children, diagnostics);
                }
                DocNode::LspSymbol { children, .. } => {
                    Self::check_nodes(_slug, source_path, children, diagnostics);
                }
                _ => {}
            }
        }
    }

    pub fn validate_svgbob_diagram(
        source_path: &str,
        title: Option<&str>,
        code: &str,
        diagnostics: &mut Vec<evin_diagnostic::Diagnostic>,
    ) {
        use unicode_width::UnicodeWidthStr;

        let diagram_name = title.unwrap_or("Untitled Diagram");
        let lines: Vec<&str> = code.lines().collect();

        let mut in_box = false;
        let mut expected_width: Option<usize> = None;
        let mut box_start_line = 0;

        for (idx, raw_line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            let trimmed = raw_line.trim_end();

            // Divider or top/bottom border: +---+ or +===+ (len > 1)
            let is_border = trimmed.starts_with('+')
                && trimmed.ends_with('+')
                && trimmed.len() > 1
                && trimmed[1..trimmed.len() - 1]
                    .chars()
                    .all(|c| c == '-' || c == '=');

            // Body line: | ... |
            let is_body = trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() > 1;

            if is_border || is_body {
                let current_width = UnicodeWidthStr::width(trimmed);
                if !in_box {
                    in_box = true;
                    expected_width = Some(current_width);
                    box_start_line = line_num;
                } else if let Some(exp) = expected_width
                    && current_width != exp
                {
                    diagnostics.push(
                        evin_locales::diagnostic(
                            evin_diagnostic::IssueCode::new("docs-svgbob-box-width-mismatch")
                                .arg("diagram", diagram_name)
                                .arg("source", source_path)
                                .arg("line", line_num.to_string())
                                .arg("expected", exp.to_string())
                                .arg("actual", current_width.to_string())
                                .arg("box_start_line", box_start_line.to_string()),
                        )
                        .with_severity(evin_diagnostic::Severity::Warning)
                        .with_file_location(
                            source_path,
                            Some(line_num),
                            None,
                        ),
                    );
                }
            } else {
                in_box = false;
                expected_width = None;
            }

            // Check quote parity on body lines
            if is_body {
                let inner = &trimmed[1..trimmed.len() - 1];
                let quote_count = inner.matches('"').count();
                if quote_count % 2 != 0 {
                    diagnostics.push(
                        evin_locales::diagnostic(
                            evin_diagnostic::IssueCode::new("docs-svgbob-unpaired-quote")
                                .arg("line", line_num.to_string())
                                .arg("diagram", diagram_name)
                                .arg("source", source_path),
                        )
                        .with_severity(evin_diagnostic::Severity::Warning)
                        .with_file_location(
                            source_path,
                            Some(line_num),
                            None,
                        ),
                    );
                }
            }
        }
    }
}

pub struct MathValidator;

impl MathValidator {
    pub fn validate(site_ast: &DocSiteAst) -> Vec<evin_diagnostic::Diagnostic> {
        let mut diagnostics = Vec::new();
        for page in &site_ast.pages {
            Self::check_nodes(&page.source_path, &page.ast, &mut diagnostics);
        }
        diagnostics
    }

    fn check_nodes(
        source_path: &str,
        nodes: &[DocNode],
        diagnostics: &mut Vec<evin_diagnostic::Diagnostic>,
    ) {
        for node in nodes {
            match node {
                DocNode::MathBlock { latex } => {
                    Self::validate_math(
                        source_path,
                        latex,
                        latex2mathml::DisplayStyle::Block,
                        diagnostics,
                    );
                }
                DocNode::Paragraph { children } | DocNode::Heading { children, .. } => {
                    Self::check_inlines(source_path, children, diagnostics);
                }
                DocNode::List { items, .. } => {
                    for item in items {
                        Self::check_nodes(source_path, item, diagnostics);
                    }
                }
                DocNode::TaskList { items } => {
                    for item in items {
                        Self::check_nodes(source_path, &item.body, diagnostics);
                    }
                }
                DocNode::DefinitionList { items } => {
                    for item in items {
                        Self::check_inlines(source_path, &item.term, diagnostics);
                        for c in &item.classifiers {
                            Self::check_inlines(source_path, c, diagnostics);
                        }
                        Self::check_nodes(source_path, &item.body, diagnostics);
                    }
                }
                DocNode::OptionList { items } => {
                    for item in items {
                        Self::check_nodes(source_path, &item.body, diagnostics);
                    }
                }
                DocNode::FieldList { items } => {
                    for item in items {
                        Self::check_nodes(source_path, &item.body, diagnostics);
                    }
                }
                DocNode::Stepper { steps } => {
                    for step in steps {
                        Self::check_inlines(source_path, &step.title, diagnostics);
                        Self::check_nodes(source_path, &step.body, diagnostics);
                    }
                }
                DocNode::Table { headers, rows } => {
                    for h in headers {
                        Self::check_inlines(source_path, h, diagnostics);
                    }
                    for row in rows {
                        for cell in row {
                            Self::check_inlines(source_path, cell, diagnostics);
                        }
                    }
                }
                DocNode::Admonition { children, .. } => {
                    Self::check_nodes(source_path, children, diagnostics);
                }
                DocNode::LspSymbol { children, .. } => {
                    Self::check_nodes(source_path, children, diagnostics);
                }
                _ => {}
            }
        }
    }

    fn check_inlines(
        source_path: &str,
        inlines: &[InlineNode],
        diagnostics: &mut Vec<evin_diagnostic::Diagnostic>,
    ) {
        for inline in inlines {
            if let InlineNode::Math { latex } = inline {
                Self::validate_math(
                    source_path,
                    latex,
                    latex2mathml::DisplayStyle::Inline,
                    diagnostics,
                );
            }
        }
    }

    pub fn validate_math(
        source_path: &str,
        latex: &str,
        display: latex2mathml::DisplayStyle,
        diagnostics: &mut Vec<evin_diagnostic::Diagnostic>,
    ) {
        let normalized = evin_doc::validation::normalize_latex(latex);
        if let Err(err) = latex2mathml::latex_to_mathml(&normalized, display) {
            diagnostics.push(
                evin_locales::diagnostic(
                    evin_diagnostic::IssueCode::new("docs-invalid-latex-math")
                        .arg("source", source_path)
                        .arg("error", err.to_string())
                        .arg("latex", latex),
                )
                .with_severity(evin_diagnostic::Severity::Warning)
                .with_file_location(source_path, None, None),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_fluent_diagnostic(
        diagnostic: &evin_diagnostic::Diagnostic,
        key: &str,
        expected: String,
    ) {
        assert_eq!(diagnostic.code.key, key);
        assert_eq!(evin_locales::render_issue(&diagnostic.code), expected);
        assert_eq!(diagnostic.title, expected);
    }
    use crate::v1::ast::{DocFormat, DocPage, FormalReference, TocEntry};
    use std::collections::HashMap;

    #[test]
    fn validates_reference_aliases_and_their_anchors() {
        let target = DocPage {
            slug: "reference/workspace-yaml".to_string(),
            source_path: "reference/workspace-yaml-4500.rst".to_string(),
            format: DocFormat::ReStructuredText,
            metadata: HashMap::new(),
            reference: Some(FormalReference {
                id: "REF-4500".to_string(),
                digits: "4500".to_string(),
                number: 4500,
                depends_on: vec![],
                referenced_by: vec![],
            }),
            toc: vec![TocEntry {
                level: 2,
                text: "Task forms".to_string(),
                anchor: "task-forms".to_string(),
            }],
            ast: vec![],
        };
        let source = |href: &str| DocPage {
            slug: "guide".to_string(),
            source_path: "guide.rst".to_string(),
            format: DocFormat::ReStructuredText,
            metadata: HashMap::new(),
            reference: None,
            toc: vec![],
            ast: vec![DocNode::Paragraph {
                children: vec![InlineNode::Link {
                    href: href.to_string(),
                    title: None,
                    content: "Reference".to_string(),
                    code: false,
                }],
            }],
        };
        let site = |href: &str| DocSiteAst {
            version: "1.0".to_string(),
            title: None,
            reference_prefix: "REF".to_string(),
            pages: vec![target.clone(), source(href)],
        };

        let valid = LinkValidator::validate(&site("REF-4500#task-forms"));
        assert!(valid.diagnostics.is_empty());
        assert_eq!(valid.links_checked, 1);
        assert_eq!(valid.anchors_indexed, 1);
        let broken_anchor = LinkValidator::validate(&site("REF-4500#missing"));
        assert_eq!(broken_anchor.diagnostics.len(), 1);
        let expected = evin_locales::t!(
            "docs-broken-alias-anchor",
            slug = "guide",
            alias = "REF-4500#missing",
            anchor = "missing"
        );
        assert!(
            !expected.starts_with("<missing locale key:"),
            "expected Fluent message to exist: {expected}"
        );
        assert_fluent_diagnostic(
            &broken_anchor.diagnostics[0],
            "docs-broken-alias-anchor",
            expected,
        );
        let broken_link = LinkValidator::validate(&site("REF-9999"));
        assert_eq!(broken_link.diagnostics.len(), 1);
        assert_fluent_diagnostic(
            &broken_link.diagnostics[0],
            "docs-broken-formal-link",
            evin_locales::t!(
                "docs-broken-formal-link",
                slug = "guide",
                alias = "REF-9999"
            ),
        );
        let broken_page_anchor = LinkValidator::validate(&site("#missing"));
        assert_eq!(broken_page_anchor.diagnostics.len(), 1);
        assert_fluent_diagnostic(
            &broken_page_anchor.diagnostics[0],
            "docs-broken-anchor",
            evin_locales::t!("docs-broken-anchor", slug = "guide", anchor = "missing"),
        );
    }

    #[test]
    fn resolves_relative_routes_and_formal_reference_source_names() {
        let page = |slug: &str, source_path: &str| DocPage {
            slug: slug.to_string(),
            source_path: source_path.to_string(),
            format: DocFormat::ReStructuredText,
            metadata: HashMap::new(),
            reference: None,
            toc: vec![],
            ast: vec![],
        };
        let mut reference_index = page("reference", "reference/index.rst");
        reference_index.ast = vec![DocNode::Paragraph {
            children: vec![
                InlineNode::Link {
                    href: "environment-configuration#details".to_string(),
                    title: None,
                    content: "Environment".to_string(),
                    code: false,
                },
                InlineNode::Link {
                    href: "/esh/runtime-reference.3010#catalog".to_string(),
                    title: None,
                    content: "Built-ins".to_string(),
                    code: false,
                },
            ],
        }];
        let mut environment = page(
            "reference/environment-configuration",
            "reference/environment-configuration-2000.rst",
        );
        environment.toc.push(TocEntry {
            level: 2,
            text: "Details".to_string(),
            anchor: "details".to_string(),
        });
        let mut catalog = page("esh/runtime-reference", "esh/100.runtime-reference.3010.md");
        catalog.toc.push(TocEntry {
            level: 2,
            text: "Catalog".to_string(),
            anchor: "catalog".to_string(),
        });
        let site = DocSiteAst {
            version: "1.0".to_string(),
            title: None,
            reference_prefix: "REF".to_string(),
            pages: vec![reference_index, environment, catalog],
        };
        assert!(LinkValidator::validate(&site).diagnostics.is_empty());
    }

    #[test]
    fn rejects_unregistered_local_image_paths() {
        let site = DocSiteAst {
            version: "1.0".to_string(),
            title: None,
            reference_prefix: "REF".to_string(),
            pages: vec![DocPage {
                slug: "guide".to_string(),
                source_path: "guide.md".to_string(),
                format: DocFormat::Markdown,
                metadata: HashMap::new(),
                reference: None,
                toc: vec![],
                ast: vec![DocNode::Paragraph {
                    children: vec![InlineNode::Image {
                        src: "/assets/missing.png".to_string(),
                        alt: "Missing image".to_string(),
                        title: None,
                    }],
                }],
            }],
        };
        let audit = LinkValidator::validate(&site);
        assert_eq!(audit.diagnostics.len(), 1);
        assert_fluent_diagnostic(
            &audit.diagnostics[0],
            "docs-invalid-media",
            evin_locales::t!(
                "docs-invalid-media",
                label = "source",
                slug = "guide",
                requirement = "use a registered local asset or an absolute HTTPS URL"
            ),
        );
    }

    #[test]
    fn test_diagram_validator_detects_box_width_mismatch() {
        use unicode_width::UnicodeWidthStr;

        let bad_code = "\
+-------------------------------+
|             Box A             |
|  Too long content line inside  |
+-------------------------------+";

        let mut diagnostics = Vec::new();
        DiagramValidator::validate_svgbob_diagram(
            "guide.rst",
            Some("Mismatch Test"),
            bad_code,
            &mut diagnostics,
        );

        assert_eq!(diagnostics.len(), 1);
        let expected_width = UnicodeWidthStr::width("+-------------------------------+");
        let actual_width = UnicodeWidthStr::width("|  Too long content line inside  |");
        assert_fluent_diagnostic(
            &diagnostics[0],
            "docs-svgbob-box-width-mismatch",
            evin_locales::t!(
                "docs-svgbob-box-width-mismatch",
                diagram = "Mismatch Test",
                source = "guide.rst",
                line = 3,
                expected = expected_width,
                actual = actual_width,
                box_start_line = 1
            ),
        );
    }

    #[test]
    fn test_diagram_validator_detects_unpaired_quotes() {
        let bad_code = "\
+-------------------------------+
|  Line with \"unpaired quote    |
+-------------------------------+";

        let mut diagnostics = Vec::new();
        DiagramValidator::validate_svgbob_diagram(
            "guide.rst",
            Some("Quote Test"),
            bad_code,
            &mut diagnostics,
        );

        assert_eq!(diagnostics.len(), 1);
        assert_fluent_diagnostic(
            &diagnostics[0],
            "docs-svgbob-unpaired-quote",
            evin_locales::t!(
                "docs-svgbob-unpaired-quote",
                line = 2,
                diagram = "Quote Test",
                source = "guide.rst"
            ),
        );
    }

    #[test]
    fn test_diagram_validator_passes_valid_diagrams() {
        let clean_code = "\
+-------------------------------+
|             Box A             |
+-------------------------------+
                |
                v
+-------------------------------+
|  \"Stage 1: Valid & Aligned\"   |
+-------------------------------+";

        let mut diagnostics = Vec::new();
        DiagramValidator::validate_svgbob_diagram(
            "guide.rst",
            Some("Clean Test"),
            clean_code,
            &mut diagnostics,
        );

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_math_validator_piecewise_syntax() {
        let mut diagnostics = Vec::new();
        MathValidator::validate_math(
            "doc.md",
            r"\operatorname{eval}(c \, ? \, t : e) = \begin{cases} \operatorname{eval}(t) & \text{if } c \neq 0 \\ \operatorname{eval}(e) & \text{if } c = 0 \end{cases}",
            latex2mathml::DisplayStyle::Block,
            &mut diagnostics,
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_math_validator_detects_invalid_latex() {
        let mut diagnostics = Vec::new();
        MathValidator::validate_math(
            "doc.md",
            r"\invalidUnclosed{",
            latex2mathml::DisplayStyle::Block,
            &mut diagnostics,
        );

        assert_eq!(diagnostics.len(), 1);
        let latex = r"\invalidUnclosed{";
        let normalized = evin_doc::validation::normalize_latex(latex);
        let error = latex2mathml::latex_to_mathml(&normalized, latex2mathml::DisplayStyle::Block)
            .unwrap_err()
            .to_string();
        assert_fluent_diagnostic(
            &diagnostics[0],
            "docs-invalid-latex-math",
            evin_locales::t!(
                "docs-invalid-latex-math",
                source = "doc.md",
                error = error,
                latex = latex
            ),
        );
    }

    #[test]
    fn test_math_validator_passes_valid_math() {
        let mut diagnostics = Vec::new();
        MathValidator::validate_math(
            "doc.md",
            r"E = mc^2",
            latex2mathml::DisplayStyle::Inline,
            &mut diagnostics,
        );
        MathValidator::validate_math(
            "doc.md",
            r"\int_0^1 x^2 dx = \frac{1}{3}",
            latex2mathml::DisplayStyle::Block,
            &mut diagnostics,
        );

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_math_validator_populates_structured_diagnostics() {
        let site = DocSiteAst {
            version: "1.0".to_string(),
            title: None,
            reference_prefix: "REF".to_string(),
            pages: vec![DocPage {
                slug: "math-page".to_string(),
                source_path: "math_page.md".to_string(),
                format: DocFormat::Markdown,
                metadata: HashMap::new(),
                reference: None,
                toc: vec![],
                ast: vec![
                    DocNode::Paragraph {
                        children: vec![InlineNode::Math {
                            latex: r"\unclosed{".to_string(),
                        }],
                    },
                    DocNode::MathBlock {
                        latex: r"\int_0^1 x dx".to_string(),
                    },
                ],
            }],
        };

        let diagnostics = MathValidator::validate(&site);
        assert_eq!(diagnostics.len(), 1);
        let diag = &diagnostics[0];
        assert_eq!(diag.code.key, "docs-invalid-latex-math");
        assert_eq!(evin_locales::render_issue(&diag.code), diag.title);
        assert_eq!(diag.severity, evin_diagnostic::Severity::Warning);
        match &diag.location {
            Some(evin_diagnostic::FileLocation::Path { path, .. }) => {
                assert_eq!(path, "math_page.md");
            }
            other => panic!("Expected FileLocation::Path, found {:?}", other),
        }
    }
}
