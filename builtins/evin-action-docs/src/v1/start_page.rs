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
use crate::v1::ast::{DocNode, DocPage, DocSiteAst, InlineNode, MediaKind};
use crate::v1::presentation::{self, Presentation, PresentationAction, Spotlight};
use crate::v1::topbar::{self, TopbarSetting};
use evin_doc::{SiteIndex, TapeConfig};
use std::collections::HashSet;
use std::path::Path;
use url::Url;

pub const DEFAULT_SEARCH_PROMPTS: [&str; 1] = ["Search"];

#[derive(Debug, Clone, PartialEq)]
pub struct StartPageModel {
    pub title: String,
    pub lead: String,
    pub search_prompts: Vec<String>,
    /// Root metadata takes precedence over the action configuration, including
    /// an explicit `topbar: false` opt-out.
    pub topbar: Option<TopbarSetting>,
    pub presentation: Option<Presentation>,
    /// A blockquote spotlight is rendered after this many card sections,
    /// preserving its authored position in the start page.
    pub spotlight: Option<StartPageSpotlight>,
    pub features: Vec<StartPageFeature>,
    pub sections: Vec<StartPageSection>,
    pub footer: Option<StartPageFooter>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StartPageSection {
    pub title: String,
    pub paragraphs: Vec<Vec<InlineNode>>,
    pub media: Option<StartPageMedia>,
    pub cards: Vec<StartPageCard>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartPageSpotlight {
    pub after_sections: usize,
    pub content: Spotlight,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StartPageFeature {
    pub after_sections: usize,
    pub title: String,
    pub description: Vec<InlineNode>,
    pub actions: Vec<StartPageFeatureAction>,
    pub aside: StartPageFeatureAside,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartPageFeatureAction {
    pub label: String,
    pub destination: FooterDestination,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StartPageFeatureAside {
    Media(StartPageMedia),
    Code {
        language: Option<String>,
        code: String,
    },
    Tape {
        script: String,
        config: TapeConfig,
        rendered_svg: Option<String>,
    },
    Diagram {
        kind: evin_doc::ast::DiagramKind,
        code: String,
        title: Option<String>,
        caption: Option<String>,
        alt: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartPageMedia {
    pub kind: MediaKind,
    pub src: String,
    pub alt: String,
    pub title: Option<String>,
    pub poster: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartPageCard {
    pub title: String,
    pub description: String,
    pub target_slug: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartPageFooter {
    pub description: String,
    pub columns: Vec<StartPageFooterColumn>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartPageFooterColumn {
    pub title: String,
    pub links: Vec<StartPageFooterLink>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartPageFooterLink {
    pub title: String,
    pub description: Option<String>,
    pub destination: FooterDestination,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FooterDestination {
    Internal(String),
    External(String),
}

const START_PAGE_HINT: &str = "Use docs/index.md or docs/index.rst with: a single H1, one lead paragraph, then H2 sections containing optional paragraphs and one optional HTTPS image or video (or a registered local image under /assets/) before an unordered list of `Link - description` cards. Spaced en-dash and em-dash separators are also accepted. `[!FEATURE]` blockquotes may appear after the lead or between sections; each contains an H2 title, one paragraph, a one- or two-link unordered action list, and exactly one image, video, tape, code-block, or Mermaid/Svgbob diagram aside. A single `> [!SPOTLIGHT] variant | optional badge` blockquote may appear between the lead, card sections, and final Footer. Reserve a final H2 named Footer for an optional description paragraph followed by H3 link columns using internal, HTTPS, or mailto links.";

pub fn transform(
    site_ast: &DocSiteAst,
    source_root: &Path,
) -> Result<StartPageModel, evin_diagnostic::Diagnostic> {
    transform_with_assets(site_ast, source_root, None)
}

pub fn transform_with_assets(
    site_ast: &DocSiteAst,
    source_root: &Path,
    assets: Option<&ResolvedAssets>,
) -> Result<StartPageModel, evin_diagnostic::Diagnostic> {
    // Start-page contract diagnostics intentionally aggregate malformed root
    // routes. In that case a site index cannot be built, so retain the
    // pre-index validation path and use aliases only for an otherwise valid
    // compiled site.
    let index = SiteIndex::build(site_ast).ok();
    transform_with_optional_index(site_ast, index.as_ref(), source_root, assets)
}

pub fn transform_with_assets_and_index(
    site_ast: &DocSiteAst,
    index: &SiteIndex,
    source_root: &Path,
    assets: Option<&ResolvedAssets>,
) -> Result<StartPageModel, evin_diagnostic::Diagnostic> {
    transform_with_optional_index(site_ast, Some(index), source_root, assets)
}

fn transform_with_optional_index(
    site_ast: &DocSiteAst,
    index: Option<&SiteIndex>,
    source_root: &Path,
    assets: Option<&ResolvedAssets>,
) -> Result<StartPageModel, evin_diagnostic::Diagnostic> {
    let root_indexes: Vec<&DocPage> = site_ast
        .pages
        .iter()
        .filter(|page| matches!(page.source_path.as_str(), "index.md" | "index.rst"))
        .collect();
    let routed_indexes: Vec<&DocPage> = site_ast
        .pages
        .iter()
        .filter(|page| page.slug == "index")
        .collect();
    let mut violations = Vec::new();

    if root_indexes.len() != 1 {
        violations.push("require exactly one root source: index.md or index.rst".to_string());
    }
    if routed_indexes.len() != 1 {
        violations
            .push("must not have another root document resolving to the index route".to_string());
    }

    let Some(page) = root_indexes.first().copied() else {
        return Err(invalid_start_page(source_root, None, violations));
    };

    for candidate in &site_ast.pages {
        if candidate.source_path != page.source_path && contains_feature(&candidate.ast) {
            violations.push(format!(
                "FEATURE blockquotes are allowed only in {}, not {}",
                page.source_path, candidate.source_path
            ));
        }
    }

    let search_prompts = parse_search_prompts(page, &mut violations);
    let topbar = parse_topbar(page, &mut violations);
    let presentation = parse_presentation(page, &mut violations);

    let valid_routes: HashSet<&str> = site_ast
        .pages
        .iter()
        .filter(|candidate| candidate.slug != "index")
        .map(|candidate| candidate.slug.as_str())
        .collect();
    let mut cursor = 0;

    let title = match page.ast.get(cursor) {
        Some(DocNode::Heading { level: 1, text, .. }) if !text.trim().is_empty() => {
            cursor += 1;
            text.trim().to_string()
        }
        _ => {
            violations.push("first top-level node must be one non-empty H1".to_string());
            String::new()
        }
    };

    let lead = match page.ast.get(cursor) {
        Some(DocNode::Paragraph { children }) => {
            let lead = plain_text(children);
            if lead.is_empty() {
                violations.push("H1 must be followed by one non-empty lead paragraph".to_string());
            } else {
                cursor += 1;
            }
            lead
        }
        _ => {
            violations.push("H1 must be followed by one non-empty lead paragraph".to_string());
            String::new()
        }
    };

    let mut sections = Vec::new();
    let mut footer = None;
    let mut spotlight = None;
    let mut features = Vec::new();
    while cursor < page.ast.len() {
        if let Some(DocNode::Admonition {
            kind,
            title,
            children,
        }) = page.ast.get(cursor)
            && kind.eq_ignore_ascii_case("spotlight")
        {
            if spotlight.is_some() {
                violations.push("only one Spotlight blockquote is allowed".to_string());
            } else {
                match parse_spotlight_block(title.as_deref(), children) {
                    Ok(content) => {
                        spotlight = Some(StartPageSpotlight {
                            after_sections: sections.len(),
                            content,
                        });
                    }
                    Err(error) => violations.push(format!("invalid Spotlight blockquote: {error}")),
                }
            }
            cursor += 1;
            continue;
        }
        if let Some(DocNode::Admonition { kind, children, .. }) = page.ast.get(cursor)
            && kind.eq_ignore_ascii_case("feature")
        {
            match parse_feature_block(children, &valid_routes, index, assets) {
                Ok((title, description, actions, aside)) => features.push(StartPageFeature {
                    after_sections: sections.len(),
                    title,
                    description,
                    actions,
                    aside,
                }),
                Err(error) => violations.push(format!("invalid FEATURE blockquote: {error}")),
            }
            cursor += 1;
            continue;
        }
        let section_title = match page.ast.get(cursor) {
            Some(DocNode::Heading { level: 2, text, .. }) if !text.trim().is_empty() => {
                cursor += 1;
                text.trim().to_string()
            }
            Some(_) => {
                violations.push(format!(
                    "top-level node {} must be a non-empty H2 card section",
                    cursor + 1
                ));
                cursor += 1;
                continue;
            }
            None => break,
        };

        if section_title.trim().eq_ignore_ascii_case("footer") {
            if footer.is_some() {
                violations.push("only one Footer section is allowed".to_string());
            }
            let (description, columns) = parse_footer_columns(
                &page.ast,
                &mut cursor,
                &valid_routes,
                index,
                &mut violations,
            );
            footer = Some(StartPageFooter {
                description,
                columns,
            });
            break;
        } else {
            let (paragraphs, media, cards) = parse_section_content(
                &page.ast,
                &mut cursor,
                &valid_routes,
                index,
                &section_title,
                assets,
                &mut violations,
            );
            if let Some(cards) = cards {
                sections.push(StartPageSection {
                    title: section_title,
                    paragraphs,
                    media,
                    cards,
                });
            }
        }
    }

    if sections.is_empty() {
        violations
            .push("require at least one H2 card section after the lead paragraph".to_string());
    }
    if violations.is_empty() {
        Ok(StartPageModel {
            title,
            lead,
            search_prompts,
            topbar,
            presentation,
            spotlight,
            features,
            sections,
            footer,
        })
    } else {
        Err(invalid_start_page(source_root, Some(page), violations))
    }
}

fn parse_presentation(page: &DocPage, violations: &mut Vec<String>) -> Option<Presentation> {
    let value = page.metadata.get("presentation")?;
    match presentation::from_metadata(value).and_then(|setting| {
        setting.validate(None)?;
        Ok(setting)
    }) {
        Ok(setting) => Some(setting),
        Err(error) => {
            violations.push(format!("start-page metadata error: {error}"));
            None
        }
    }
}

fn parse_spotlight_block(marker: Option<&str>, children: &[DocNode]) -> Result<Spotlight, String> {
    let marker = marker.unwrap_or_default().trim();
    let (variant, badge) = match marker.split_once('|') {
        Some((variant, badge)) => (variant.trim(), Some(badge.trim())),
        None => (marker, None),
    };
    presentation::validate_non_empty("spotlight variant", variant)?;
    presentation::validate_variant(variant, None, false)?;
    if let Some(badge) = badge {
        presentation::validate_non_empty("spotlight badge", badge)?;
    }
    if children.len() != 3 {
        return Err(
            "expected an H2 title, one description paragraph, and one CTA link paragraph"
                .to_string(),
        );
    }
    let title = match &children[0] {
        DocNode::Heading {
            level: 1 | 2, text, ..
        } if !text.trim().is_empty() => text.trim().to_string(),
        _ => return Err("first child must be one non-empty H2 title".to_string()),
    };
    let description = match &children[1] {
        DocNode::Paragraph { children } => plain_text(children),
        _ => return Err("title must be followed by one description paragraph".to_string()),
    };
    let (label, href) = match &children[2] {
        DocNode::Paragraph { children } if children.len() == 1 => match &children[0] {
            InlineNode::Link { href, content, .. } if !content.trim().is_empty() => {
                (content.trim().to_string(), href.clone())
            }
            _ => return Err("last child must be one Markdown CTA link".to_string()),
        },
        _ => return Err("last child must be one Markdown CTA link".to_string()),
    };
    Ok(Spotlight {
        variant: variant.to_string(),
        badge: badge.map(str::to_string),
        title,
        description,
        action: PresentationAction { label, href },
    })
}

type ParsedFeature = (
    String,
    Vec<InlineNode>,
    Vec<StartPageFeatureAction>,
    StartPageFeatureAside,
);

fn parse_feature_block(
    children: &[DocNode],
    valid_routes: &HashSet<&str>,
    index: Option<&SiteIndex>,
    assets: Option<&ResolvedAssets>,
) -> Result<ParsedFeature, String> {
    if children.len() != 4 {
        return Err(
            "expected an H2 title, one description paragraph, an unordered action list, and one final aside"
                .to_string(),
        );
    }
    let title = match &children[0] {
        DocNode::Heading { level: 2, text, .. } if !text.trim().is_empty() => {
            text.trim().to_string()
        }
        _ => return Err("first child must be one non-empty H2 title".to_string()),
    };
    let description = match &children[1] {
        DocNode::Paragraph { children } if !plain_text(children).is_empty() => children.clone(),
        _ => {
            return Err(
                "title must be followed by one non-empty description paragraph".to_string(),
            );
        }
    };
    let actions = match &children[2] {
        DocNode::List {
            ordered: false,
            items,
            ..
        } if (1..=2).contains(&items.len()) => items
            .iter()
            .enumerate()
            .map(|(index_in_list, item)| {
                parse_feature_action(item, valid_routes, index, index_in_list + 1)
            })
            .collect::<Result<Vec<_>, _>>()?,
        DocNode::List { ordered: true, .. } => {
            return Err("action list must be unordered and contain one or two links".to_string());
        }
        _ => return Err("description must be followed by an unordered action list".to_string()),
    };
    let aside = parse_feature_aside(&children[3], assets)?;
    Ok((title, description, actions, aside))
}

fn parse_feature_action(
    item: &[DocNode],
    valid_routes: &HashSet<&str>,
    index: Option<&SiteIndex>,
    item_number: usize,
) -> Result<StartPageFeatureAction, String> {
    let Some(DocNode::Paragraph { children }) = item.first() else {
        return Err(format!(
            "action {item_number} must contain exactly one link paragraph"
        ));
    };
    let Some(InlineNode::Link { href, content, .. }) = children.first() else {
        return Err(format!(
            "action {item_number} must contain exactly one link"
        ));
    };
    if item.len() != 1 || children.len() != 1 || content.trim().is_empty() {
        return Err(format!(
            "action {item_number} must contain exactly one non-empty link"
        ));
    }
    let destination = if href.starts_with("https://") || href.starts_with("mailto:") {
        FooterDestination::External(href.to_string())
    } else if href.contains("://") {
        return Err(format!(
            "action {item_number} has an unsupported external link '{href}': use HTTPS or mailto"
        ));
    } else {
        normalize_route_with_index(href, valid_routes, index)
            .map(FooterDestination::Internal)
            .map_err(|reason| {
                format!("action {item_number} has an invalid link '{href}': {reason}")
            })?
    };
    Ok(StartPageFeatureAction {
        label: content.trim().to_string(),
        destination,
    })
}

fn parse_feature_aside(
    node: &DocNode,
    assets: Option<&ResolvedAssets>,
) -> Result<StartPageFeatureAside, String> {
    match node {
        DocNode::Paragraph { children } => {
            let Some(InlineNode::Image { src, alt, title }) = standalone_image(children) else {
                return Err(
                    "aside must be one standalone image, video, tape, code block, or diagram"
                        .to_string(),
                );
            };
            let media =
                parse_feature_media(MediaKind::Image, src, alt, title.as_deref(), None, assets)?;
            Ok(StartPageFeatureAside::Media(media))
        }
        DocNode::Media {
            kind,
            src,
            alt,
            title,
            poster,
        } => Ok(StartPageFeatureAside::Media(parse_feature_media(
            kind.clone(),
            src,
            alt.as_deref().unwrap_or_default(),
            title.as_deref(),
            poster.as_deref(),
            assets,
        )?)),
        DocNode::CodeBlock { language, code, .. } => Ok(StartPageFeatureAside::Code {
            language: language.clone(),
            code: code.clone(),
        }),
        DocNode::Tape {
            script,
            config,
            rendered_svg,
        } => Ok(StartPageFeatureAside::Tape {
            script: script.clone(),
            config: config.clone(),
            rendered_svg: rendered_svg.clone(),
        }),
        DocNode::Diagram {
            kind,
            code,
            title,
            caption,
            alt,
        } => Ok(StartPageFeatureAside::Diagram {
            kind: *kind,
            code: code.clone(),
            title: title.clone(),
            caption: caption.clone(),
            alt: alt.clone(),
        }),
        _ => Err("aside must be one image, video, tape, code block, or diagram".to_string()),
    }
}

fn parse_feature_media(
    kind: MediaKind,
    src: &str,
    alt: &str,
    title: Option<&str>,
    poster: Option<&str>,
    assets: Option<&ResolvedAssets>,
) -> Result<StartPageMedia, String> {
    let mut media = None;
    let mut violations = Vec::new();
    add_media(
        &mut media,
        kind,
        src,
        alt,
        title,
        poster,
        "FEATURE",
        assets,
        &mut violations,
    );
    media.ok_or_else(|| violations.join("; "))
}

fn parse_topbar(page: &DocPage, violations: &mut Vec<String>) -> Option<TopbarSetting> {
    let value = page.metadata.get("topbar")?;
    match topbar::from_metadata(value).and_then(|setting| {
        setting.validate(None)?;
        Ok(setting)
    }) {
        Ok(setting) => Some(setting),
        Err(error) => {
            violations.push(format!("start-page metadata error: {error}"));
            None
        }
    }
}

fn parse_search_prompts(page: &DocPage, violations: &mut Vec<String>) -> Vec<String> {
    let fallback = || {
        DEFAULT_SEARCH_PROMPTS
            .iter()
            .map(|prompt| (*prompt).to_string())
            .collect()
    };
    if let Some(error) = page
        .metadata
        .get("__evin_metadata_error")
        .and_then(serde_json::Value::as_str)
    {
        violations.push(format!("start-page metadata error: {error}"));
    }
    let Some(value) = page.metadata.get("search_prompts") else {
        return fallback();
    };
    let Some(values) = value.as_array() else {
        violations.push(
            "start-page metadata `search_prompts` must be a non-empty list of non-empty strings"
                .to_string(),
        );
        return fallback();
    };
    if values.is_empty() {
        violations.push(
            "start-page metadata `search_prompts` must be a non-empty list of non-empty strings"
                .to_string(),
        );
        return fallback();
    }
    let prompts: Option<Vec<String>> = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|prompt| !prompt.is_empty())
                .map(str::to_string)
        })
        .collect();
    let Some(prompts) = prompts else {
        violations.push(
            "start-page metadata `search_prompts` must be a non-empty list of non-empty strings"
                .to_string(),
        );
        return fallback();
    };
    prompts
}

fn parse_section_content(
    nodes: &[DocNode],
    cursor: &mut usize,
    valid_routes: &HashSet<&str>,
    index: Option<&SiteIndex>,
    section_title: &str,
    assets: Option<&ResolvedAssets>,
    violations: &mut Vec<String>,
) -> (
    Vec<Vec<InlineNode>>,
    Option<StartPageMedia>,
    Option<Vec<StartPageCard>>,
) {
    let mut paragraphs = Vec::new();
    let mut media = None;

    loop {
        match nodes.get(*cursor) {
            Some(DocNode::Paragraph { children }) => {
                if let Some(InlineNode::Image { src, alt, title }) = standalone_image(children) {
                    add_media(
                        &mut media,
                        MediaKind::Image,
                        src,
                        alt,
                        title.as_deref(),
                        None,
                        section_title,
                        assets,
                        violations,
                    );
                } else if children
                    .iter()
                    .any(|inline| matches!(inline, InlineNode::Image { .. }))
                {
                    violations.push(format!(
                        "section '{}' media must be a standalone image paragraph",
                        section_title
                    ));
                } else if media.is_some() {
                    violations.push(format!(
                        "section '{}' paragraphs must appear before supporting media",
                        section_title
                    ));
                } else {
                    paragraphs.push(children.clone());
                }
                *cursor += 1;
            }
            Some(DocNode::Media {
                kind,
                src,
                alt,
                title,
                poster,
            }) => {
                add_media(
                    &mut media,
                    kind.clone(),
                    src,
                    alt.as_deref().unwrap_or_default(),
                    title.as_deref(),
                    poster.as_deref(),
                    section_title,
                    assets,
                    violations,
                );
                *cursor += 1;
            }
            Some(DocNode::RawHtml { .. }) => {
                violations.push(format!(
                    "section '{}' supports only a constrained <video> block for HTML media",
                    section_title
                ));
                *cursor += 1;
            }
            Some(DocNode::List {
                ordered: false,
                items,
                ..
            }) => {
                *cursor += 1;
                if items.is_empty() {
                    violations.push(format!(
                        "section '{}' must contain at least one card",
                        section_title
                    ));
                    return (paragraphs, media, None);
                }
                let cards = items
                    .iter()
                    .enumerate()
                    .filter_map(|(item_index, item)| {
                        parse_card(
                            item,
                            valid_routes,
                            index,
                            section_title,
                            item_index + 1,
                            violations,
                        )
                    })
                    .collect();
                while let Some(node) = nodes.get(*cursor) {
                    if is_media_node(node) {
                        violations.push(format!(
                            "section '{}' supporting media must appear before its card list",
                            section_title
                        ));
                        *cursor += 1;
                    } else {
                        break;
                    }
                }
                return (paragraphs, media, Some(cards));
            }
            Some(DocNode::Heading { level: 2, .. }) | None => {
                violations.push(format!(
                    "section '{}' must contain an unordered card list",
                    section_title
                ));
                return (paragraphs, media, None);
            }
            Some(_) => {
                violations.push(format!(
                    "section '{}' supports only paragraphs, one image or video, and an unordered card list",
                    section_title
                ));
                *cursor += 1;
            }
        }
    }
}

fn standalone_image(children: &[InlineNode]) -> Option<&InlineNode> {
    match children {
        [image @ InlineNode::Image { .. }] => Some(image),
        _ => None,
    }
}

fn contains_feature(nodes: &[DocNode]) -> bool {
    nodes.iter().any(|node| match node {
        DocNode::Admonition { kind, children, .. }
        | DocNode::Directive {
            name: kind,
            body: children,
            ..
        } => kind.eq_ignore_ascii_case("feature") || contains_feature(children),
        DocNode::List { items, .. } => items.iter().any(|item| contains_feature(item)),
        DocNode::TaskList { items } => items.iter().any(|item| contains_feature(&item.body)),
        DocNode::DefinitionList { items } => items.iter().any(|item| contains_feature(&item.body)),
        DocNode::OptionList { items } => items.iter().any(|item| contains_feature(&item.body)),
        DocNode::FieldList { items } => items.iter().any(|item| contains_feature(&item.body)),
        DocNode::Stepper { steps } => steps.iter().any(|step| contains_feature(&step.body)),
        _ => false,
    })
}

fn is_media_node(node: &DocNode) -> bool {
    matches!(node, DocNode::Media { .. } | DocNode::RawHtml { .. })
        || matches!(node, DocNode::Paragraph { children } if standalone_image(children).is_some())
}

#[allow(clippy::too_many_arguments)]
fn add_media(
    media: &mut Option<StartPageMedia>,
    kind: MediaKind,
    src: &str,
    alt: &str,
    title: Option<&str>,
    poster: Option<&str>,
    section_title: &str,
    assets: Option<&ResolvedAssets>,
    violations: &mut Vec<String>,
) {
    if media.is_some() {
        violations.push(format!(
            "section '{}' supports at most one supporting image or video",
            section_title
        ));
        return;
    }
    let src = if kind == MediaKind::Image
        && assets.is_some_and(|assets| assets.is_registered_reference(src))
    {
        src.trim().to_string()
    } else {
        match https_url(src) {
            Some(src) => src,
            None => {
                violations.push(format!(
                "section '{}' media source must be an absolute HTTPS URL or a registered local image",
                section_title
            ));
                return;
            }
        }
    };
    let alt = alt.trim();
    if alt.is_empty() {
        let requirement = match kind {
            MediaKind::Image => "a non-empty alternative text",
            MediaKind::Video => "a non-empty accessible label",
        };
        violations.push(format!(
            "section '{}' media requires {}",
            section_title, requirement
        ));
        return;
    }
    let poster = match poster {
        Some(_) if kind == MediaKind::Image => {
            violations.push(format!(
                "section '{}' image media cannot declare a video poster",
                section_title
            ));
            return;
        }
        Some(poster) => match https_url(poster) {
            Some(poster) => Some(poster),
            None => {
                violations.push(format!(
                    "section '{}' video poster must be an absolute HTTPS URL",
                    section_title
                ));
                return;
            }
        },
        None => None,
    };
    *media = Some(StartPageMedia {
        kind,
        src,
        alt: alt.to_string(),
        title: title
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string),
        poster,
    });
}

fn https_url(value: &str) -> Option<String> {
    let value = value.trim();
    let url = Url::parse(value).ok()?;
    (url.scheme() == "https" && url.host_str().is_some()).then(|| value.to_string())
}

fn parse_footer_columns(
    nodes: &[DocNode],
    cursor: &mut usize,
    valid_routes: &HashSet<&str>,
    index: Option<&SiteIndex>,
    violations: &mut Vec<String>,
) -> (String, Vec<StartPageFooterColumn>) {
    let description = match nodes.get(*cursor) {
        Some(DocNode::Paragraph { children }) => {
            *cursor += 1;
            let description = plain_text(children);
            if description.is_empty() {
                violations.push("Footer description must not be empty".to_string());
            }
            description
        }
        _ => String::new(),
    };
    let mut columns = Vec::new();
    while *cursor < nodes.len() {
        let title = match nodes.get(*cursor) {
            Some(DocNode::Heading { level: 3, text, .. }) if !text.trim().is_empty() => {
                *cursor += 1;
                text.trim().to_string()
            }
            Some(DocNode::Heading { level: 2, text, .. }) => {
                if text.trim().eq_ignore_ascii_case("footer") {
                    violations.push("only one Footer section is allowed".to_string());
                }
                violations.push("Footer must be the final H2 section".to_string());
                *cursor += 1;
                continue;
            }
            Some(_) => {
                violations.push(format!(
                    "Footer node {} must be a non-empty H3 link column",
                    *cursor + 1
                ));
                *cursor += 1;
                continue;
            }
            None => break,
        };
        let Some(DocNode::List {
            ordered: false,
            items,
            ..
        }) = nodes.get(*cursor)
        else {
            violations.push(format!(
                "Footer column '{}' must be immediately followed by an unordered link list",
                title
            ));
            continue;
        };
        *cursor += 1;
        if items.is_empty() {
            violations.push(format!(
                "Footer column '{}' must contain at least one link",
                title
            ));
            continue;
        }
        let links = items
            .iter()
            .enumerate()
            .filter_map(|(item_index, item)| {
                parse_footer_link(
                    item,
                    valid_routes,
                    index,
                    &title,
                    item_index + 1,
                    violations,
                )
            })
            .collect();
        columns.push(StartPageFooterColumn { title, links });
    }
    if columns.is_empty() {
        violations.push("Footer must contain at least one H3 link column".to_string());
    }
    (description, columns)
}

fn parse_footer_link(
    item: &[DocNode],
    valid_routes: &HashSet<&str>,
    index: Option<&SiteIndex>,
    column_title: &str,
    item_number: usize,
    violations: &mut Vec<String>,
) -> Option<StartPageFooterLink> {
    let label = format!("Footer column '{}', link {}", column_title, item_number);
    let Some(DocNode::Paragraph { children }) = item.first() else {
        violations.push(format!("{} must contain exactly one paragraph", label));
        return None;
    };
    if item.len() != 1 || !(children.len() == 1 || children.len() == 2) {
        violations.push(format!(
            "{} must use one link with an optional spaced-dash description",
            label
        ));
        return None;
    }
    let Some(InlineNode::Link { href, content, .. }) = children.first() else {
        violations.push(format!("{} must begin with a link", label));
        return None;
    };
    let title = content.trim();
    let description = match children.get(1) {
        None => None,
        Some(InlineNode::Text { content }) => strip_card_description_prefix(content),
        Some(_) => None,
    };
    if title.is_empty()
        || matches!(description, Some(""))
        || (children.len() == 2 && description.is_none())
    {
        violations.push(format!("{} has an invalid title or description", label));
        return None;
    }
    let destination = if href.starts_with("https://") || href.starts_with("mailto:") {
        FooterDestination::External(href.to_string())
    } else if href.contains("://") {
        violations.push(format!(
            "{} has an unsupported external link '{}': use HTTPS or mailto",
            label, href
        ));
        return None;
    } else {
        match normalize_route_with_index(href, valid_routes, index) {
            Ok(route) => FooterDestination::Internal(route),
            Err(reason) => {
                violations.push(format!(
                    "{} has an invalid footer link '{}': {}",
                    label, href, reason
                ));
                return None;
            }
        }
    };
    Some(StartPageFooterLink {
        title: title.to_string(),
        description: description.map(str::to_string),
        destination,
    })
}

fn parse_card(
    item: &[DocNode],
    valid_routes: &HashSet<&str>,
    index: Option<&SiteIndex>,
    section_title: &str,
    item_number: usize,
    violations: &mut Vec<String>,
) -> Option<StartPageCard> {
    let label = format!("section '{}', card {}", section_title, item_number);
    let (title, description, href) = parse_card_content(item, &label, violations)?;
    match normalize_route_with_index(href, valid_routes, index) {
        Ok(target_slug) => Some(StartPageCard {
            title: title.to_string(),
            description: description.to_string(),
            target_slug,
        }),
        Err(reason) => {
            violations.push(format!(
                "{} has an invalid card link '{}': {}",
                label, href, reason
            ));
            None
        }
    }
}

fn parse_card_content<'a>(
    item: &'a [DocNode],
    label: &str,
    violations: &mut Vec<String>,
) -> Option<(&'a str, &'a str, &'a str)> {
    let Some(DocNode::Paragraph { children }) = item.first() else {
        violations.push(format!("{} must contain exactly one paragraph", label));
        return None;
    };
    if item.len() != 1 || children.len() != 2 {
        violations.push(format!(
            "{} must use exactly one link followed by a spaced-dash description",
            label
        ));
        return None;
    }
    let (
        InlineNode::Link { href, content, .. },
        InlineNode::Text {
            content: description,
        },
    ) = (&children[0], &children[1])
    else {
        violations.push(format!(
            "{} must use exactly one link followed by a spaced-dash description",
            label
        ));
        return None;
    };
    let title = content.trim();
    let description = strip_card_description_prefix(description).unwrap_or_default();
    if title.is_empty() || description.is_empty() {
        violations.push(format!(
            "{} needs a non-empty link title and plain-text description after a spaced dash",
            label
        ));
        return None;
    }
    Some((title, description, href))
}

/// Accept keyboard-friendly and typographic separators for start-page card
/// descriptions. Restricting this normalization to the card grammar keeps
/// ordinary prose and link text untouched.
fn strip_card_description_prefix(content: &str) -> Option<&str> {
    [" - ", " – ", " — "]
        .into_iter()
        .find_map(|separator| content.strip_prefix(separator))
        .map(str::trim)
}

pub fn normalize_route(href: &str, valid_routes: &HashSet<&str>) -> Result<String, &'static str> {
    let href = href.trim();
    if href.is_empty() || href.starts_with('#') || href.contains('#') {
        return Err("fragments are not allowed");
    }
    if href.contains("://") || href.starts_with("mailto:") {
        return Err("only internal documentation routes are allowed");
    }
    let mut route = href
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_string();
    for suffix in ["/index.html", ".html", ".markdown", ".md", ".rst"] {
        if let Some(stripped) = route.strip_suffix(suffix) {
            route = stripped.trim_end_matches('/').to_string();
            break;
        }
    }
    if route.ends_with("/index") {
        route.truncate(route.len() - "/index".len());
    }
    if route.is_empty() || route == "index" {
        return Err("the start page cannot link to itself");
    }
    if valid_routes.contains(route.as_str()) {
        Ok(route)
    } else {
        Err("route does not resolve to a generated documentation page")
    }
}

fn normalize_route_with_index(
    href: &str,
    valid_routes: &HashSet<&str>,
    index: Option<&SiteIndex>,
) -> Result<String, &'static str> {
    if let Some(target) = index.and_then(|index| index.resolve_alias(href)) {
        if target.anchor.is_some() {
            return Err("fragments are not allowed");
        }
        return normalize_route(target.slug, valid_routes);
    }
    normalize_route(href, valid_routes)
}

fn plain_text(children: &[InlineNode]) -> String {
    children
        .iter()
        .map(InlineNode::plain_text)
        .collect::<String>()
        .trim()
        .to_string()
}

fn invalid_start_page(
    source_root: &Path,
    page: Option<&DocPage>,
    violations: Vec<String>,
) -> evin_diagnostic::Diagnostic {
    let source = page
        .map(|page| source_root.join(&page.source_path))
        .unwrap_or_else(|| source_root.join("index.md"));
    let details = violations
        .iter()
        .map(|violation| format!("- {violation}"))
        .collect::<Vec<_>>()
        .join("\n");
    evin_diagnostic::Diagnostic::new(
        format!("Invalid documentation start page:\n{details}"),
        evin_diagnostic::IssueCode::new("start-page-invalid").arg("error", &details),
    )
    .with_file_location(source.display().to_string(), None, None)
    .with_hint(START_PAGE_HINT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::ast::{DocFormat, TocEntry};
    use std::collections::HashMap;

    fn page(source: &str, slug: &str, ast: Vec<DocNode>) -> DocPage {
        DocPage {
            slug: slug.to_string(),
            source_path: source.to_string(),
            format: DocFormat::Markdown,
            metadata: HashMap::new(),
            reference: None,
            toc: Vec::<TocEntry>::new(),
            ast,
        }
    }

    fn valid_site(href: &str) -> DocSiteAst {
        DocSiteAst {
            version: "1.0".to_string(),
            title: None,
            reference_prefix: "REF".to_string(),
            pages: vec![
                page(
                    "index.md",
                    "index",
                    vec![
                        DocNode::Heading {
                            level: 1,
                            text: "Docs".to_string(),
                            children: vec![],
                            anchor: "docs".to_string(),
                        },
                        DocNode::Paragraph {
                            children: vec![InlineNode::Text {
                                content: "A lead.".to_string(),
                            }],
                        },
                        DocNode::Heading {
                            level: 2,
                            text: "Get started".to_string(),
                            children: vec![],
                            anchor: "get-started".to_string(),
                        },
                        DocNode::List {
                            ordered: false,
                            numbering: evin_doc::NumberingKind::Arabic,
                            start: 1,
                            items: vec![vec![DocNode::Paragraph {
                                children: vec![
                                    InlineNode::Link {
                                        href: href.to_string(),
                                        title: None,
                                        content: "Configure".to_string(),
                                    },
                                    InlineNode::Text {
                                        content: " — Create a workspace.".to_string(),
                                    },
                                ],
                            }]],
                        },
                    ],
                ),
                page("workspace_yaml.rst", "workspace_yaml", vec![]),
            ],
        }
    }

    #[test]
    fn transforms_a_valid_start_page() {
        let model = transform(&valid_site("workspace_yaml.html"), Path::new("docs")).unwrap();
        assert_eq!(model.title, "Docs");
        assert_eq!(model.sections[0].cards[0].target_slug, "workspace_yaml");
    }

    #[test]
    fn accepts_ascii_and_typographic_card_description_separators() {
        for separator in [" - ", " – ", " — "] {
            let item = vec![DocNode::Paragraph {
                children: vec![
                    InlineNode::Link {
                        href: "/workspace_yaml".to_string(),
                        title: None,
                        content: "Configure".to_string(),
                    },
                    InlineNode::Text {
                        content: format!("{separator}Create a workspace."),
                    },
                ],
            }];
            let mut violations = Vec::new();

            let (title, description, href) =
                parse_card_content(&item, "section 'Get started', card 1", &mut violations)
                    .expect("separator should be accepted");

            assert_eq!(title, "Configure");
            assert_eq!(description, "Create a workspace.");
            assert_eq!(href, "/workspace_yaml");
            assert!(violations.is_empty());
        }
    }

    #[test]
    fn preserves_a_spotlight_blockquote_at_its_authored_position() {
        let mut site = valid_site("workspace_yaml");
        site.pages[0].ast.insert(
            2,
            DocNode::Admonition {
                kind: "spotlight".to_string(),
                title: Some("holo | Canary".to_string()),
                children: vec![
                    DocNode::Heading {
                        level: 2,
                        text: "Make work flow.".to_string(),
                        children: vec![],
                        anchor: "make-work-flow".to_string(),
                    },
                    DocNode::Paragraph {
                        children: vec![InlineNode::Text {
                            content: "A focused invitation.".to_string(),
                        }],
                    },
                    DocNode::Paragraph {
                        children: vec![InlineNode::Link {
                            href: "/workspace_yaml".to_string(),
                            title: None,
                            content: "Start now".to_string(),
                        }],
                    },
                ],
            },
        );

        let model = transform(&site, Path::new("docs")).unwrap();
        let spotlight = model.spotlight.expect("spotlight blockquote");
        assert_eq!(spotlight.after_sections, 0);
        assert_eq!(spotlight.content.variant, "holo");
        assert_eq!(spotlight.content.badge.as_deref(), Some("Canary"));
        assert_eq!(spotlight.content.action.href, "/workspace_yaml");
    }

    #[test]
    fn parses_multiple_features_with_actions_and_supported_asides() {
        let mut site = valid_site("workspace_yaml");
        let feature = |title: &str, aside| DocNode::Admonition {
            kind: "feature".to_string(),
            title: None,
            children: vec![
                DocNode::Heading {
                    level: 2,
                    text: title.to_string(),
                    children: vec![],
                    anchor: title.to_string(),
                },
                DocNode::Paragraph {
                    children: vec![InlineNode::Text {
                        content: "A feature description.".to_string(),
                    }],
                },
                DocNode::List {
                    ordered: false,
                    numbering: evin_doc::NumberingKind::Arabic,
                    start: 1,
                    items: vec![vec![DocNode::Paragraph {
                        children: vec![InlineNode::Link {
                            href: "workspace_yaml".to_string(),
                            title: None,
                            content: "Read more".to_string(),
                        }],
                    }]],
                },
                aside,
            ],
        };
        site.pages[0].ast.insert(
            2,
            feature(
                "Terminal recording",
                DocNode::Tape {
                    script: "type \"pwd\"\nenter\nwait".to_string(),
                    config: TapeConfig::default(),
                    rendered_svg: Some("<svg></svg>".to_string()),
                },
            ),
        );
        site.pages[0].ast.insert(
            3,
            feature(
                "Code sample",
                DocNode::CodeBlock {
                    language: Some("esh".to_string()),
                    code: "echo feature".to_string(),
                    title: None,
                },
            ),
        );
        site.pages[0].ast.insert(
            4,
            feature(
                "Task graph",
                DocNode::Diagram {
                    kind: evin_doc::ast::DiagramKind::Mermaid,
                    code: "flowchart LR\nCheck --> Build".to_string(),
                    title: Some("Task graph".to_string()),
                    caption: None,
                    alt: Some("Check flows into build".to_string()),
                },
            ),
        );

        let model = transform(&site, Path::new("docs")).unwrap();
        assert_eq!(model.features.len(), 3);
        assert!(
            model
                .features
                .iter()
                .all(|feature| feature.after_sections == 0)
        );
        assert_eq!(model.features[0].actions[0].label, "Read more");
        assert!(matches!(
            model.features[0].aside,
            StartPageFeatureAside::Tape { .. }
        ));
        assert!(matches!(
            model.features[1].aside,
            StartPageFeatureAside::Code { .. }
        ));
        assert!(matches!(
            model.features[2].aside,
            StartPageFeatureAside::Diagram {
                kind: evin_doc::ast::DiagramKind::Mermaid,
                ..
            }
        ));
    }

    #[test]
    fn transforms_rich_section_content_with_external_media() {
        let mut site = valid_site("workspace_yaml");
        site.pages[0].ast.insert(
            3,
            DocNode::Paragraph {
                children: vec![
                    InlineNode::Text {
                        content: "Read the ".to_string(),
                    },
                    InlineNode::Strong {
                        children: vec![InlineNode::Text {
                            content: "guide".to_string(),
                        }],
                    },
                    InlineNode::Text {
                        content: " before configuring a workspace.".to_string(),
                    },
                ],
            },
        );
        site.pages[0].ast.insert(
            4,
            DocNode::Media {
                kind: MediaKind::Video,
                src: "https://cdn.example.com/demo.mp4".to_string(),
                alt: Some("Workspace demonstration".to_string()),
                title: None,
                poster: Some("https://cdn.example.com/poster.png".to_string()),
            },
        );

        let model = transform(&site, Path::new("docs")).unwrap();
        let section = &model.sections[0];
        assert_eq!(section.paragraphs.len(), 1);
        assert_eq!(section.media.as_ref().unwrap().kind, MediaKind::Video);
        assert_eq!(
            section.media.as_ref().unwrap().poster.as_deref(),
            Some("https://cdn.example.com/poster.png")
        );
    }

    #[test]
    fn rejects_invalid_or_misplaced_start_page_media() {
        let invalid_media = [
            (
                DocNode::Media {
                    kind: MediaKind::Image,
                    src: "http://cdn.example.com/overview.png".to_string(),
                    alt: Some("Overview".to_string()),
                    title: None,
                    poster: None,
                },
                "media source must be an absolute HTTPS URL",
            ),
            (
                DocNode::Media {
                    kind: MediaKind::Image,
                    src: "https://cdn.example.com/overview.png".to_string(),
                    alt: None,
                    title: None,
                    poster: None,
                },
                "media requires a non-empty alternative text",
            ),
            (
                DocNode::Media {
                    kind: MediaKind::Video,
                    src: "https://cdn.example.com/demo.mp4".to_string(),
                    alt: Some("Demonstration".to_string()),
                    title: None,
                    poster: Some("http://cdn.example.com/poster.png".to_string()),
                },
                "video poster must be an absolute HTTPS URL",
            ),
        ];
        for (media, expected) in invalid_media {
            let mut site = valid_site("workspace_yaml");
            site.pages[0].ast.insert(3, media);
            let error = transform(&site, Path::new("docs")).unwrap_err();
            assert!(error.title.contains(expected), "{}", error.title);
        }

        let mut site = valid_site("workspace_yaml");
        site.pages[0].ast.extend([
            DocNode::Media {
                kind: MediaKind::Image,
                src: "http://cdn.example.com/overview.png".to_string(),
                alt: Some("Overview".to_string()),
                title: None,
                poster: None,
            },
            DocNode::Media {
                kind: MediaKind::Image,
                src: "https://cdn.example.com/second.png".to_string(),
                alt: Some("Second".to_string()),
                title: None,
                poster: None,
            },
        ]);

        let error = transform(&site, Path::new("docs")).unwrap_err();
        assert!(
            error
                .title
                .contains("supporting media must appear before its card list")
        );
    }

    #[test]
    fn rejects_external_card_links() {
        let error = transform(&valid_site("https://example.com"), Path::new("docs")).unwrap_err();
        assert!(error.title.contains("only internal documentation routes"));
    }

    #[test]
    fn validates_start_page_search_prompts() {
        let mut site = valid_site("workspace_yaml");
        site.pages[0].metadata.insert(
            "search_prompts".to_string(),
            serde_json::json!(["Search the workspace", "Find a task"]),
        );
        let model = transform(&site, Path::new("docs")).unwrap();
        assert_eq!(
            model.search_prompts,
            ["Search the workspace", "Find a task"]
        );

        site.pages[0]
            .metadata
            .insert("search_prompts".to_string(), serde_json::json!([]));
        let error = transform(&site, Path::new("docs")).unwrap_err();
        assert!(
            error
                .title
                .contains("`search_prompts` must be a non-empty list")
        );
    }

    #[test]
    fn reads_root_topbar_metadata_and_preserves_explicit_disablement() {
        let mut site = valid_site("workspace_yaml");
        site.pages[0].metadata.insert(
            "topbar".to_string(),
            serde_json::json!({
                "id": "canary-1",
                "variant": "genome",
                "message": "A release is coming.",
                "dismissible": true,
            }),
        );
        let model = transform(&site, Path::new("docs")).unwrap();
        assert!(matches!(model.topbar, Some(TopbarSetting::Enabled(_))));

        site.pages[0]
            .metadata
            .insert("topbar".to_string(), serde_json::json!(false));
        let model = transform(&site, Path::new("docs")).unwrap();
        assert!(matches!(model.topbar, Some(TopbarSetting::Disabled(false))));
    }

    #[test]
    fn aggregates_card_and_root_contract_violations() {
        let mut site = valid_site("#fragment");
        site.pages.push(page("index.rst", "index", vec![]));
        let error = transform(&site, Path::new("docs")).unwrap_err();
        assert!(error.title.contains("require exactly one root source"));
        assert!(error.title.contains("another root document"));
        assert!(error.title.contains("fragments are not allowed"));
        assert!(error.hint.is_some());
    }

    #[test]
    fn normalizes_common_internal_routes() {
        let routes = HashSet::from(["guide"]);
        for route in [
            "guide",
            "guide/",
            "guide.html",
            "guide/index.html",
            "guide.md",
            "guide.rst",
        ] {
            assert_eq!(normalize_route(route, &routes).unwrap(), "guide");
        }
    }

    #[test]
    fn extracts_a_final_footer_with_internal_and_external_destinations() {
        let mut site = valid_site("workspace_yaml");
        site.pages[0].ast.extend([
            DocNode::Heading {
                level: 2,
                text: "Footer".to_string(),
                children: vec![],
                anchor: "footer".to_string(),
            },
            DocNode::Paragraph {
                children: vec![InlineNode::Text {
                    content: "A fuller project description.".to_string(),
                }],
            },
            DocNode::Heading {
                level: 3,
                text: "Project".to_string(),
                children: vec![],
                anchor: "project".to_string(),
            },
            DocNode::List {
                ordered: false,
                numbering: evin_doc::NumberingKind::Arabic,
                start: 1,
                items: vec![
                    vec![DocNode::Paragraph {
                        children: vec![InlineNode::Link {
                            href: "https://example.com/source".to_string(),
                            title: None,
                            content: "Source".to_string(),
                        }],
                    }],
                    vec![DocNode::Paragraph {
                        children: vec![
                            InlineNode::Link {
                                href: "workspace_yaml".to_string(),
                                title: None,
                                content: "Reference".to_string(),
                            },
                            InlineNode::Text {
                                content: " — Read the reference.".to_string(),
                            },
                        ],
                    }],
                ],
            },
        ]);

        let model = transform(&site, Path::new("docs")).unwrap();
        assert_eq!(model.sections.len(), 1);
        let footer = model.footer.unwrap();
        assert_eq!(footer.description, "A fuller project description.");
        assert_eq!(footer.columns.len(), 1);
        assert_eq!(footer.columns[0].links.len(), 2);
        assert_eq!(
            footer.columns[0].links[0].destination,
            FooterDestination::External("https://example.com/source".to_string())
        );
        assert_eq!(
            footer.columns[0].links[1].destination,
            FooterDestination::Internal("workspace_yaml".to_string())
        );
    }

    #[test]
    fn rejects_non_final_footer_and_unsupported_external_links() {
        let mut site = valid_site("workspace_yaml");
        site.pages[0].ast.extend([
            DocNode::Heading {
                level: 2,
                text: "Footer".to_string(),
                children: vec![],
                anchor: "footer".to_string(),
            },
            DocNode::List {
                ordered: false,
                numbering: evin_doc::NumberingKind::Arabic,
                start: 1,
                items: vec![vec![DocNode::Paragraph {
                    children: vec![
                        InlineNode::Link {
                            href: "http://example.com".to_string(),
                            title: None,
                            content: "Insecure".to_string(),
                        },
                        InlineNode::Text {
                            content: " — Unsupported.".to_string(),
                        },
                    ],
                }]],
            },
            DocNode::Heading {
                level: 3,
                text: "Project".to_string(),
                children: vec![],
                anchor: "project".to_string(),
            },
            DocNode::List {
                ordered: false,
                numbering: evin_doc::NumberingKind::Arabic,
                start: 1,
                items: vec![vec![DocNode::Paragraph {
                    children: vec![InlineNode::Link {
                        href: "http://example.com".to_string(),
                        title: None,
                        content: "Insecure".to_string(),
                    }],
                }]],
            },
            DocNode::Heading {
                level: 2,
                text: "Later".to_string(),
                children: vec![],
                anchor: "later".to_string(),
            },
            DocNode::List {
                ordered: false,
                numbering: evin_doc::NumberingKind::Arabic,
                start: 1,
                items: vec![vec![DocNode::Paragraph {
                    children: vec![
                        InlineNode::Link {
                            href: "workspace_yaml".to_string(),
                            title: None,
                            content: "Reference".to_string(),
                        },
                        InlineNode::Text {
                            content: " — Read the reference.".to_string(),
                        },
                    ],
                }]],
            },
        ]);

        let error = transform(&site, Path::new("docs")).unwrap_err();
        assert!(error.title.contains("Footer must be the final"));
        assert!(error.title.contains("unsupported external link"));
    }
}
