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
use crate::v1::seo::SeoCatalog;
use crate::v1::start_page::{
    FooterDestination, StartPageFeatureAside, StartPageMedia, StartPageModel, StartPageSpotlight,
};
use crate::v1::templates::ComponentRegistry;
use crate::v1::topbar::Topbar;
use evin_doc::analysis::{Visitor, walk_nodes};
use evin_doc::{SiteIndex, SourceResolver, TapeRuntime, TapeScript, push_escaped_html};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellSlot {
    Title,
    SiteTitle,
    SiteBrand,
    BrandLogo,
    HomeHref,
    Meta,
    Favicons,
    FontPreloads,
    IconsSprite,
    Topbar,
    AssetsPrefix,
    DepthPrefix,
    Css,
    Nav,
    Breadcrumbs,
    Content,
    Toc,
    InlineToc,
    TocRail,
    DocumentToc,
    Pagination,
    SearchData,
    SearchJs,
    ComponentJs,
    LiveReload,
    PageKind,
    PageLanguage,
    JsonLd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotWrapper {
    None,
    Title,
    Css,
    IconsSprite,
    DepthPrefix,
}

enum ShellSegment {
    Text(String),
    Slot(ShellSlot, SlotWrapper),
}

struct ShellTemplate {
    segments: Vec<ShellSegment>,
}

impl ShellTemplate {
    fn parse(shell_html: &str) -> Self {
        let mut segments = Vec::new();
        let mut rest = shell_html;

        const PATTERNS: &[(&str, ShellSlot, SlotWrapper)] = &[
            (
                "<page-title></page-title>",
                ShellSlot::Title,
                SlotWrapper::Title,
            ),
            ("<page-title/>", ShellSlot::Title, SlotWrapper::Title),
            (
                "<site-title></site-title>",
                ShellSlot::SiteTitle,
                SlotWrapper::None,
            ),
            ("<site-title/>", ShellSlot::SiteTitle, SlotWrapper::None),
            (
                "<site-brand></site-brand>",
                ShellSlot::SiteBrand,
                SlotWrapper::None,
            ),
            ("<site-brand/>", ShellSlot::SiteBrand, SlotWrapper::None),
            (
                "<brand-logo></brand-logo>",
                ShellSlot::BrandLogo,
                SlotWrapper::None,
            ),
            ("<brand-logo/>", ShellSlot::BrandLogo, SlotWrapper::None),
            (
                "<site-logo></site-logo>",
                ShellSlot::BrandLogo,
                SlotWrapper::None,
            ),
            ("<site-logo/>", ShellSlot::BrandLogo, SlotWrapper::None),
            (
                "<home-href></home-href>",
                ShellSlot::HomeHref,
                SlotWrapper::None,
            ),
            ("<home-href/>", ShellSlot::HomeHref, SlotWrapper::None),
            (
                "<page-meta></page-meta>",
                ShellSlot::Meta,
                SlotWrapper::None,
            ),
            ("<page-meta/>", ShellSlot::Meta, SlotWrapper::None),
            (
                "<favicons></favicons>",
                ShellSlot::Favicons,
                SlotWrapper::None,
            ),
            ("<favicons/>", ShellSlot::Favicons, SlotWrapper::None),
            (
                "<font-preloads></font-preloads>",
                ShellSlot::FontPreloads,
                SlotWrapper::None,
            ),
            (
                "<font-preloads/>",
                ShellSlot::FontPreloads,
                SlotWrapper::None,
            ),
            (
                "<icons-sprite></icons-sprite>",
                ShellSlot::IconsSprite,
                SlotWrapper::IconsSprite,
            ),
            (
                "<icons-sprite/>",
                ShellSlot::IconsSprite,
                SlotWrapper::IconsSprite,
            ),
            ("<topbar></topbar>", ShellSlot::Topbar, SlotWrapper::None),
            ("<topbar/>", ShellSlot::Topbar, SlotWrapper::None),
            (
                "<assets-prefix></assets-prefix>",
                ShellSlot::AssetsPrefix,
                SlotWrapper::None,
            ),
            (
                "<assets-prefix/>",
                ShellSlot::AssetsPrefix,
                SlotWrapper::None,
            ),
            (
                "<depth-prefix></depth-prefix>",
                ShellSlot::DepthPrefix,
                SlotWrapper::DepthPrefix,
            ),
            (
                "<depth-prefix/>",
                ShellSlot::DepthPrefix,
                SlotWrapper::DepthPrefix,
            ),
            ("<css></css>", ShellSlot::Css, SlotWrapper::Css),
            ("<css/>", ShellSlot::Css, SlotWrapper::Css),
            (
                "<sidebar-nav></sidebar-nav>",
                ShellSlot::Nav,
                SlotWrapper::None,
            ),
            ("<sidebar-nav/>", ShellSlot::Nav, SlotWrapper::None),
            ("<nav-tree></nav-tree>", ShellSlot::Nav, SlotWrapper::None),
            (
                "<breadcrumbs></breadcrumbs>",
                ShellSlot::Breadcrumbs,
                SlotWrapper::None,
            ),
            ("<breadcrumbs/>", ShellSlot::Breadcrumbs, SlotWrapper::None),
            ("<content></content>", ShellSlot::Content, SlotWrapper::None),
            ("<content/>", ShellSlot::Content, SlotWrapper::None),
            ("<toc></toc>", ShellSlot::Toc, SlotWrapper::None),
            ("<toc/>", ShellSlot::Toc, SlotWrapper::None),
            (
                "<inline-toc></inline-toc>",
                ShellSlot::InlineToc,
                SlotWrapper::None,
            ),
            ("<inline-toc/>", ShellSlot::InlineToc, SlotWrapper::None),
            (
                "<toc-rail></toc-rail>",
                ShellSlot::TocRail,
                SlotWrapper::None,
            ),
            ("<toc-rail/>", ShellSlot::TocRail, SlotWrapper::None),
            (
                "<document-toc></document-toc>",
                ShellSlot::DocumentToc,
                SlotWrapper::None,
            ),
            ("<document-toc/>", ShellSlot::DocumentToc, SlotWrapper::None),
            (
                "<pagination></pagination>",
                ShellSlot::Pagination,
                SlotWrapper::None,
            ),
            ("<pagination/>", ShellSlot::Pagination, SlotWrapper::None),
            (
                "<search-data></search-data>",
                ShellSlot::SearchData,
                SlotWrapper::None,
            ),
            ("<search-data/>", ShellSlot::SearchData, SlotWrapper::None),
            (
                "<search-js></search-js>",
                ShellSlot::SearchJs,
                SlotWrapper::None,
            ),
            ("<search-js/>", ShellSlot::SearchJs, SlotWrapper::None),
            (
                "<component-js></component-js>",
                ShellSlot::ComponentJs,
                SlotWrapper::None,
            ),
            ("<component-js/>", ShellSlot::ComponentJs, SlotWrapper::None),
            (
                "<live-reload></live-reload>",
                ShellSlot::LiveReload,
                SlotWrapper::None,
            ),
            ("<live-reload/>", ShellSlot::LiveReload, SlotWrapper::None),
            (
                "<page-kind></page-kind>",
                ShellSlot::PageKind,
                SlotWrapper::None,
            ),
            ("<page-kind/>", ShellSlot::PageKind, SlotWrapper::None),
            (
                "<page-language></page-language>",
                ShellSlot::PageLanguage,
                SlotWrapper::None,
            ),
            (
                "<page-language/>",
                ShellSlot::PageLanguage,
                SlotWrapper::None,
            ),
            ("<json-ld></json-ld>", ShellSlot::JsonLd, SlotWrapper::None),
            ("<json-ld/>", ShellSlot::JsonLd, SlotWrapper::None),
            (
                "<!-- evin:slot:json_ld -->",
                ShellSlot::JsonLd,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:json-ld -->",
                ShellSlot::JsonLd,
                SlotWrapper::None,
            ),
            // Legacy comment fallbacks
            (
                "<!-- evin:slot:title -->",
                ShellSlot::Title,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:site_title -->",
                ShellSlot::SiteTitle,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:site_brand -->",
                ShellSlot::SiteBrand,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:brand_logo -->",
                ShellSlot::BrandLogo,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:logo -->",
                ShellSlot::BrandLogo,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:home_href -->",
                ShellSlot::HomeHref,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:meta -->",
                ShellSlot::Meta,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:favicons -->",
                ShellSlot::Favicons,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:font_preloads -->",
                ShellSlot::FontPreloads,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:icons_sprite -->",
                ShellSlot::IconsSprite,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:topbar -->",
                ShellSlot::Topbar,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:assets_prefix -->",
                ShellSlot::AssetsPrefix,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:depth_prefix -->",
                ShellSlot::DepthPrefix,
                SlotWrapper::None,
            ),
            ("<!-- evin:slot:css -->", ShellSlot::Css, SlotWrapper::None),
            ("<!-- evin:slot:nav -->", ShellSlot::Nav, SlotWrapper::None),
            (
                "<!-- evin:slot:breadcrumbs -->",
                ShellSlot::Breadcrumbs,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:content -->",
                ShellSlot::Content,
                SlotWrapper::None,
            ),
            ("<!-- evin:slot:toc -->", ShellSlot::Toc, SlotWrapper::None),
            (
                "<!-- evin:slot:inline_toc -->",
                ShellSlot::InlineToc,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:toc_rail -->",
                ShellSlot::TocRail,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:document_toc -->",
                ShellSlot::DocumentToc,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:pagination -->",
                ShellSlot::Pagination,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:search_data -->",
                ShellSlot::SearchData,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:search_js -->",
                ShellSlot::SearchJs,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:component_js -->",
                ShellSlot::ComponentJs,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:live_reload -->",
                ShellSlot::LiveReload,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:page_kind -->",
                ShellSlot::PageKind,
                SlotWrapper::None,
            ),
            (
                "<!-- evin:slot:page_language -->",
                ShellSlot::PageLanguage,
                SlotWrapper::None,
            ),
        ];

        while !rest.is_empty() {
            let mut earliest: Option<(usize, usize, ShellSlot, SlotWrapper)> = None;

            for &(pat, slot, wrapper) in PATTERNS {
                if let Some(idx) = rest.find(pat) {
                    match earliest {
                        None => earliest = Some((idx, pat.len(), slot, wrapper)),
                        Some((best_idx, _, _, _)) if idx < best_idx => {
                            earliest = Some((idx, pat.len(), slot, wrapper));
                        }
                        _ => {}
                    }
                }
            }

            if let Some((idx, len, slot, wrapper)) = earliest {
                if idx > 0 {
                    segments.push(ShellSegment::Text(rest[..idx].to_string()));
                }
                segments.push(ShellSegment::Slot(slot, wrapper));
                rest = &rest[idx + len..];
            } else {
                segments.push(ShellSegment::Text(rest.to_string()));
                break;
            }
        }

        Self { segments }
    }

    fn uses(&self, slot: ShellSlot) -> bool {
        self.segments.iter().any(
            |segment| matches!(segment, ShellSegment::Slot(candidate, _) if *candidate == slot),
        )
    }

    fn slot_count(&self, slot: ShellSlot) -> usize {
        self.segments
            .iter()
            .filter(
                |segment| matches!(segment, ShellSegment::Slot(candidate, _) if *candidate == slot),
            )
            .count()
    }

    fn render(&self, slots: &PageSlots<'_>) -> String {
        let mut out = String::with_capacity(8192);
        for segment in &self.segments {
            match segment {
                ShellSegment::Text(t) => out.push_str(t),
                ShellSegment::Slot(s, wrapper) => {
                    let val = match s {
                        ShellSlot::Title => slots.title,
                        ShellSlot::SiteTitle => slots.site_title,
                        ShellSlot::SiteBrand => slots.site_brand,
                        ShellSlot::BrandLogo => slots.brand_logo,
                        ShellSlot::HomeHref => slots.home_href,
                        ShellSlot::Meta => slots.meta,
                        ShellSlot::Favicons => slots.favicons,
                        ShellSlot::FontPreloads => slots.font_preloads,
                        ShellSlot::IconsSprite => slots.icons_sprite,
                        ShellSlot::Topbar => slots.topbar,
                        ShellSlot::AssetsPrefix => slots.assets_prefix,
                        ShellSlot::DepthPrefix => slots.depth_prefix,
                        ShellSlot::Css => slots.css,
                        ShellSlot::Nav => slots.nav,
                        ShellSlot::Breadcrumbs => slots.breadcrumbs,
                        ShellSlot::Content => slots.content,
                        ShellSlot::Toc => slots.toc,
                        ShellSlot::InlineToc => slots.inline_toc,
                        ShellSlot::TocRail => slots.toc_rail,
                        ShellSlot::DocumentToc => slots.document_toc,
                        ShellSlot::Pagination => slots.pagination,
                        ShellSlot::SearchData => slots.search_data,
                        ShellSlot::SearchJs => slots.search_js,
                        ShellSlot::ComponentJs => slots.component_js,
                        ShellSlot::LiveReload => slots.live_reload,
                        ShellSlot::PageKind => slots.page_kind,
                        ShellSlot::PageLanguage => slots.page_language,
                        ShellSlot::JsonLd => slots.json_ld,
                    };
                    match wrapper {
                        SlotWrapper::None => out.push_str(val),
                        SlotWrapper::Title => {
                            if !val.is_empty() {
                                out.push_str("<title>");
                                out.push_str(val);
                                out.push_str("</title>");
                            }
                        }
                        SlotWrapper::Css => {
                            if !val.is_empty() {
                                let trimmed = val.trim_start();
                                if trimmed.starts_with("<link") || trimmed.starts_with("<style") {
                                    out.push_str(val);
                                } else {
                                    out.push_str("<style>");
                                    out.push_str(val);
                                    out.push_str("</style>");
                                }
                            }
                        }
                        SlotWrapper::IconsSprite => {
                            if !val.is_empty() {
                                out.push_str("<div style=\"display:none;\" aria-hidden=\"true\">");
                                out.push_str(val);
                                out.push_str("</div>");
                            }
                        }
                        SlotWrapper::DepthPrefix => {
                            out.push_str("<meta name=\"docs-depth-prefix\" content=\"");
                            out.push_str(val);
                            out.push_str("\">");
                        }
                    }
                }
            }
        }
        if !self.uses(ShellSlot::JsonLd)
            && !slots.json_ld.is_empty()
            && let Some(pos) = out.find("</head>")
        {
            out.insert_str(pos, &format!("    {}\n", slots.json_ld));
        }
        if !self.uses(ShellSlot::PageLanguage) {
            return apply_html_language(out, slots.page_language);
        }
        out
    }
}

pub fn validate_topbar_shell(shell_html: &str, enabled: bool) -> Result<(), String> {
    if !enabled {
        return Ok(());
    }
    match ShellTemplate::parse(shell_html).slot_count(ShellSlot::Topbar) {
        1 => Ok(()),
        0 => Err("the selected layout must include exactly one <topbar></topbar> shell slot when topbar is enabled".to_string()),
        _ => Err("the selected layout must include the <topbar></topbar> shell slot only once".to_string()),
    }
}

fn apply_html_language(mut html: String, language: &str) -> String {
    let Some(html_start) = html.find("<html") else {
        return html;
    };
    let Some(relative_end) = html[html_start..].find('>') else {
        return html;
    };
    let html_end = html_start + relative_end;
    let tag = &html[html_start..=html_end];
    let replacement = if let Some(lang_start) = tag.find(" lang=\"") {
        let value_start = lang_start + " lang=\"".len();
        let Some(value_end) = tag[value_start..].find('"') else {
            return html;
        };
        let mut updated = tag.to_string();
        updated.replace_range(value_start..value_start + value_end, language);
        updated
    } else {
        format!("{} lang=\"{}\">", tag.trim_end_matches('>'), language)
    };
    html.replace_range(html_start..=html_end, &replacement);
    html
}

struct PageSlots<'a> {
    title: &'a str,
    site_title: &'a str,
    site_brand: &'a str,
    brand_logo: &'a str,
    home_href: &'a str,
    meta: &'a str,
    favicons: &'a str,
    font_preloads: &'a str,
    icons_sprite: &'a str,
    topbar: &'a str,
    assets_prefix: &'a str,
    depth_prefix: &'a str,
    css: &'a str,
    nav: &'a str,
    breadcrumbs: &'a str,
    content: &'a str,
    toc: &'a str,
    inline_toc: &'a str,
    toc_rail: &'a str,
    document_toc: &'a str,
    pagination: &'a str,
    search_data: &'a str,
    search_js: &'a str,
    component_js: &'a str,
    live_reload: &'a str,
    page_kind: &'a str,
    page_language: &'a str,
    json_ld: &'a str,
}

#[derive(Debug, Clone)]
enum NavigationNode {
    Page {
        page_position: usize,
        title: String,
    },
    Subgroup {
        key: String,
        label: String,
        index_page: Option<usize>,
        children: Vec<NavigationNode>,
    },
}

impl NavigationNode {
    fn contains_slug(&self, site: &DocSiteAst, current_slug: &str) -> bool {
        match self {
            NavigationNode::Page { page_position, .. } => {
                site.pages[*page_position].slug == current_slug
            }
            NavigationNode::Subgroup {
                key,
                index_page,
                children,
                ..
            } => {
                if let Some(pos) = index_page
                    && site.pages[*pos].slug == current_slug
                {
                    return true;
                }
                if key == current_slug {
                    return true;
                }
                children
                    .iter()
                    .any(|child| child.contains_slug(site, current_slug))
            }
        }
    }

    fn min_nav_order(&self, site: &DocSiteAst) -> (bool, u64) {
        match self {
            NavigationNode::Page { page_position, .. } => {
                page_nav_order(&site.pages[*page_position])
            }
            NavigationNode::Subgroup {
                index_page,
                children,
                ..
            } => {
                if let Some(pos) = index_page {
                    return page_nav_order(&site.pages[*pos]);
                }
                children
                    .iter()
                    .map(|child| child.min_nav_order(site))
                    .min()
                    .unwrap_or((true, u64::MAX))
            }
        }
    }

    fn title(&self) -> &str {
        match self {
            NavigationNode::Page { title, .. } => title,
            NavigationNode::Subgroup { label, .. } => label,
        }
    }

    fn render(&self, site: &DocSiteAst, current_slug: &str, level: usize, out: &mut String) {
        match self {
            NavigationNode::Page {
                page_position,
                title,
            } => {
                let page = &site.pages[*page_position];
                out.push_str("<li>");
                HtmlRenderer::render_nav_link(out, page, title, current_slug, "nav-item", level);
                out.push_str("</li>\n");
            }
            NavigationNode::Subgroup {
                key,
                label,
                index_page,
                children,
            } => {
                let active = self.contains_slug(site, current_slug);
                let subgroup_id = format!("nav-subgroup-{}", key.replace('/', "-"));
                let escaped_key = HtmlRenderer::escape_html(key);
                let escaped_label = HtmlRenderer::escape_html(label);
                let level_attr = format!(" style=\"--nav-level: {};\"", level);

                out.push_str(&format!(
                    "<li class=\"nav-subgroup\" data-nav-subgroup=\"{}\">\n<button class=\"nav-subgroup-toggle\" type=\"button\" aria-expanded=\"{}\" aria-controls=\"{}\"{}>\n<span>{}</span>\n<svg class=\"nav-group-icon\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.5\" aria-hidden=\"true\"><path d=\"M6 9l6 6 6-6\"/></svg>\n</button>\n<ul class=\"nav-subgroup-list\" id=\"{}\" style=\"--nav-level: {};\"{}{}>\n",
                    escaped_key,
                    active,
                    subgroup_id,
                    level_attr,
                    escaped_label,
                    subgroup_id,
                    level,
                    if active { " data-active-group=\"true\"" } else { "" },
                    if active { "" } else { " hidden" },
                ));

                if let Some(pos) = index_page {
                    let page = &site.pages[*pos];
                    out.push_str("<li>");
                    HtmlRenderer::render_nav_link(
                        out,
                        page,
                        "Overview",
                        current_slug,
                        "nav-item",
                        level + 1,
                    );
                    out.push_str("</li>\n");
                }

                for child in children {
                    child.render(site, current_slug, level + 1, out);
                }

                out.push_str("</ul>\n</li>\n");
            }
        }
    }
}

#[derive(Debug, Clone)]
struct NavigationGroup {
    key: String,
    label: String,
    index_page: Option<usize>,
    children: Vec<NavigationNode>,
}

impl NavigationGroup {
    fn contains_slug(&self, site: &DocSiteAst, current_slug: &str) -> bool {
        if let Some(pos) = self.index_page
            && site.pages[pos].slug == current_slug
        {
            return true;
        }
        if self.key == current_slug {
            return true;
        }
        self.children
            .iter()
            .any(|child| child.contains_slug(site, current_slug))
    }

    fn render(&self, site: &DocSiteAst, current_slug: &str, out: &mut String) {
        let active = self.contains_slug(site, current_slug);
        let escaped_label = HtmlRenderer::escape_html(&self.label);
        let group_id = format!("nav-group-{}", self.key.replace('/', "-"));
        out.push_str(&format!(
            "<section class=\"nav-group\" data-nav-group=\"{}\">\n<button class=\"nav-group-toggle\" type=\"button\" aria-expanded=\"{}\" aria-controls=\"{}\" style=\"--nav-level: 1;\">\n<span>{}</span>\n<svg class=\"nav-group-icon\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.5\" aria-hidden=\"true\"><path d=\"M6 9l6 6 6-6\"/></svg>\n</button>\n<ul class=\"nav-group-list\" id=\"{}\" style=\"--nav-level: 1;\"{}{}>\n",
            HtmlRenderer::escape_html(&self.key),
            active,
            group_id,
            escaped_label,
            group_id,
            if active { " data-active-group=\"true\"" } else { "" },
            if active { "" } else { " hidden" },
        ));

        if let Some(pos) = self.index_page {
            let page = &site.pages[pos];
            out.push_str("<li>");
            HtmlRenderer::render_nav_link(out, page, "Overview", current_slug, "nav-item", 1);
            out.push_str("</li>\n");
        }

        for child in &self.children {
            child.render(site, current_slug, 1, out);
        }

        out.push_str("</ul>\n</section>\n");
    }
}

#[allow(clippy::type_complexity)]
fn build_navigation_subtree(
    site: &DocSiteAst,
    parent_prefix: &str,
    items: Vec<(Vec<&str>, usize)>,
) -> Vec<NavigationNode> {
    let mut grouped_children: Vec<(String, Option<usize>, Vec<(Vec<&str>, usize)>)> = Vec::new();
    let mut map: HashMap<String, usize> = HashMap::new();

    for (segments, page_pos) in items {
        if segments.is_empty() {
            continue;
        }
        let first_seg = segments[0];
        let sub_key = if parent_prefix.is_empty() {
            first_seg.to_string()
        } else {
            format!("{}/{}", parent_prefix, first_seg)
        };

        let idx = *map.entry(first_seg.to_string()).or_insert_with(|| {
            let i = grouped_children.len();
            grouped_children.push((sub_key, None, Vec::new()));
            i
        });

        if segments.len() == 1 {
            grouped_children[idx].1 = Some(page_pos);
        } else {
            grouped_children[idx]
                .2
                .push((segments[1..].to_vec(), page_pos));
        }
    }

    let mut nodes = Vec::new();
    for (sub_key, self_page_pos, sub_items) in grouped_children {
        if sub_items.is_empty() {
            if let Some(page_pos) = self_page_pos {
                nodes.push(NavigationNode::Page {
                    page_position: page_pos,
                    title: HtmlRenderer::page_title(&site.pages[page_pos]).to_string(),
                });
            }
        } else {
            let label = HtmlRenderer::nav_group_label_for_site(site, &sub_key);
            let children = build_navigation_subtree(site, &sub_key, sub_items);
            nodes.push(NavigationNode::Subgroup {
                key: sub_key,
                label,
                index_page: self_page_pos,
                children,
            });
        }
    }

    nodes.sort_by(|left, right| match (left, right) {
        (NavigationNode::Page { .. }, NavigationNode::Subgroup { .. }) => std::cmp::Ordering::Less,
        (NavigationNode::Subgroup { .. }, NavigationNode::Page { .. }) => {
            std::cmp::Ordering::Greater
        }
        (
            NavigationNode::Page {
                page_position: lp, ..
            },
            NavigationNode::Page {
                page_position: rp, ..
            },
        ) => {
            let left_page = &site.pages[*lp];
            let right_page = &site.pages[*rp];
            page_nav_order(left_page)
                .cmp(&page_nav_order(right_page))
                .then_with(|| match (&left_page.reference, &right_page.reference) {
                    (Some(lr), Some(rr)) => lr
                        .number
                        .cmp(&rr.number)
                        .then_with(|| left.title().cmp(right.title())),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => left
                        .title()
                        .cmp(right.title())
                        .then_with(|| left_page.slug.cmp(&right_page.slug)),
                })
        }
        (NavigationNode::Subgroup { .. }, NavigationNode::Subgroup { .. }) => left
            .min_nav_order(site)
            .cmp(&right.min_nav_order(site))
            .then_with(|| left.title().cmp(right.title())),
    });

    nodes
}

#[derive(Debug, Default)]
struct NavigationModel {
    groups: Vec<NavigationGroup>,
}

fn page_nav_order(page: &DocPage) -> (bool, u64) {
    SourceResolver::navigation_order(Path::new(&page.source_path))
        .ok()
        .flatten()
        .map_or((true, u64::MAX), |order| (false, order))
}

impl NavigationModel {
    #[allow(clippy::type_complexity)]
    fn build(site: &DocSiteAst) -> Self {
        let mut top_groups: HashMap<String, (Option<usize>, Vec<(Vec<&str>, usize)>)> =
            HashMap::new();

        for (page_position, page) in site.pages.iter().enumerate() {
            if page.slug == "index"
                || page
                    .metadata
                    .get("dev_only")
                    .and_then(|value| value.as_bool())
                    == Some(true)
            {
                continue;
            }
            let segments: Vec<&str> = page.slug.split('/').collect();
            let group_key = segments[0].to_string();

            let entry = top_groups
                .entry(group_key.clone())
                .or_insert_with(|| (None, Vec::new()));
            if segments.len() == 1 {
                entry.0 = Some(page_position);
            } else {
                entry.1.push((segments[1..].to_vec(), page_position));
            }
        }

        const READER_JOURNEY: [&str; 7] = [
            "book",
            "getting-started",
            "guides",
            "esh",
            "builtins",
            "reference",
            "contributing",
        ];
        let mut keys: Vec<String> = top_groups.keys().cloned().collect();
        keys.sort_by(|left, right| {
            let rank = |key: &str| {
                READER_JOURNEY
                    .iter()
                    .position(|known| *known == key)
                    .unwrap_or(READER_JOURNEY.len())
            };
            rank(left).cmp(&rank(right)).then_with(|| left.cmp(right))
        });

        let mut groups = Vec::new();
        for key in keys {
            let (index_page, sub_items) = top_groups.remove(&key).unwrap();
            let label = HtmlRenderer::nav_group_label_for_site(site, &key);
            let children = build_navigation_subtree(site, &key, sub_items);
            groups.push(NavigationGroup {
                key,
                label,
                index_page,
                children,
            });
        }

        Self { groups }
    }

    fn render(&self, site: &DocSiteAst, current_slug: &str) -> String {
        let mut nav = String::new();
        for group in &self.groups {
            group.render(site, current_slug, &mut nav);
        }
        nav
    }
}

/// Immutable web-publishing inputs prepared once per documentation build.
///
/// The document AST is never rewritten for rendering: formal aliases and local
/// media are projected to output URLs at the last responsible moment.
pub struct PreparedRenderContext<'a> {
    pub site: &'a DocSiteAst,
    pub index: SiteIndex,
    pub assets: &'a ResolvedAssets,
    shell_template: ShellTemplate,
    highlighter: crate::v1::syntax::SyntaxHighlighter,
    navigation: NavigationModel,
    site_title: String,
    site_brand: String,
    pub brand_logo_html: String,
    pub footer_brand_logo_html: String,
    search_data_code: String,
    search_js_code: String,
    live_reload_code: &'static str,
    pub processed_css: &'a str,
    pub sprite_svg: &'a str,
    pub registry: &'a ComponentRegistry,
    pub single_file: bool,
    pub live_reload: bool,
    pub resolved_favicon: Option<crate::v1::resolver::ResolvedFavicon>,
    pub theme_color: Option<&'a str>,
    pub background_color: Option<&'a str>,
    pub start_page: Option<&'a StartPageModel>,
    pub topbar: Option<&'a Topbar>,
    pub seo: &'a SeoCatalog,
    pub social_image: Option<&'a str>,
}

impl<'a> PreparedRenderContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        site: &'a DocSiteAst,
        index: SiteIndex,
        assets: &'a ResolvedAssets,
        shell_html: &'a str,
        processed_css: &'a str,
        search_js: &'a str,
        sprite_svg: &'a str,
        registry: &'a ComponentRegistry,
        single_file: bool,
        live_reload: bool,
        brand_logo: Option<&crate::v1::resolver::ResolvedLogo>,
        resolved_favicon: Option<crate::v1::resolver::ResolvedFavicon>,
        theme_color: Option<&'a str>,
        background_color: Option<&'a str>,
        start_page: Option<&'a StartPageModel>,
        topbar: Option<&'a Topbar>,
        seo: &'a SeoCatalog,
        social_image: Option<&'a str>,
    ) -> Self {
        let shell_template = ShellTemplate::parse(shell_html);
        let site_title = site.title.as_deref().unwrap_or("Documentation").to_string();
        let site_brand = index
            .page(site, "index")
            .and_then(|page| page.metadata.get("title"))
            .and_then(|value| value.as_str())
            .unwrap_or(&site_title)
            .to_string();
        let (brand_logo_html, footer_brand_logo_html) = match brand_logo {
            Some(crate::v1::resolver::ResolvedLogo::CustomSvg(svg)) => (
                format!("<span class=\"brand-logo-slot brand-logo-custom\" aria-hidden=\"true\">{svg}</span>"),
                format!("<span class=\"start-footer-logo brand-logo-custom\" aria-hidden=\"true\">{svg}</span>"),
            ),
            _ => (
                "<span class=\"brand-logo-slot brand-fallback-glyph\" aria-hidden=\"true\">🕮</span>".to_string(),
                "<span class=\"start-footer-logo brand-fallback-glyph\" aria-hidden=\"true\">🕮</span>".to_string(),
            ),
        };
        let search_data_code = if shell_template.uses(ShellSlot::SearchData) {
            let prompts = start_page
                .map(|page| page.search_prompts.clone())
                .unwrap_or_else(|| {
                    crate::v1::start_page::DEFAULT_SEARCH_PROMPTS
                        .iter()
                        .map(|prompt| (*prompt).to_string())
                        .collect()
                });
            if single_file {
                let search_payload = crate::v1::search::SearchIndexBuilder::build(site, prompts);
                let binary = search_payload.to_binary();
                let b64 = crate::v1::search_binary::to_base64(&binary);
                format!(
                    "<script id=\"docs-search-bin\" type=\"application/octet-stream\">{b64}</script>"
                )
            } else {
                let prompts_obj = serde_json::json!({
                    "prompts": prompts,
                });
                let prompts_json = serde_json::to_string(&prompts_obj)
                    .unwrap_or_default()
                    .replace('<', "\\u003c")
                    .replace('>', "\\u003e")
                    .replace('&', "\\u0026");
                format!(
                    "<script id=\"docs-search-data\" type=\"application/json\">{prompts_json}</script>"
                )
            }
        } else {
            String::new()
        };
        let search_js_code = if shell_template.uses(ShellSlot::SearchJs) && !search_js.is_empty() {
            format!("<script>\n{search_js}\n</script>")
        } else {
            String::new()
        };
        let live_reload_code = if live_reload && shell_template.uses(ShellSlot::LiveReload) {
            "<script>\n(function() {\n  if (window.location.protocol === 'file:') return;\n  try {\n    const evtSource = new EventSource('/__evin_reload');\n    evtSource.addEventListener('reload', function() {\n      window.location.reload();\n    });\n  } catch (err) {}\n})();\n</script>"
        } else {
            ""
        };
        Self {
            site,
            index,
            assets,
            shell_template,
            highlighter: crate::v1::syntax::SyntaxHighlighter::new(),
            navigation: NavigationModel::build(site),
            site_title,
            site_brand,
            brand_logo_html,
            footer_brand_logo_html,
            search_data_code,
            search_js_code,
            live_reload_code,
            processed_css,
            sprite_svg,
            registry,
            single_file,
            live_reload,
            resolved_favicon,
            theme_color,
            background_color,
            start_page,
            topbar,
            seo,
            social_image,
        }
    }

    fn asset_url(&self, source: &str, page_slug: &str) -> Option<String> {
        self.assets.render_url(source, page_slug)
    }
}

pub struct HtmlRenderer;

struct ComponentCollector<'a> {
    used: &'a mut HashSet<String>,
}

fn component_template_name(name: &str) -> String {
    if name.trim().replace('_', "-").starts_with("metrics-") {
        name.trim().replace('-', "_")
    } else {
        name.to_string()
    }
}

impl Visitor for ComponentCollector<'_> {
    fn visit_node(&mut self, node: &DocNode) {
        match node {
            DocNode::Heading { .. } => {
                self.used.insert("heading".to_string());
            }
            DocNode::Paragraph { .. } => {
                self.used.insert("paragraph".to_string());
            }
            DocNode::CodeBlock { .. } => {
                self.used.insert("code_block".to_string());
            }
            DocNode::Tape { .. } => {
                self.used.insert("tape".to_string());
            }
            DocNode::Diagram { kind, .. } => match kind {
                evin_doc::ast::DiagramKind::Mermaid => {
                    self.used.insert("diagram_mermaid".to_string());
                }
                evin_doc::ast::DiagramKind::Svgbob => {
                    self.used.insert("diagram_svgbob".to_string());
                }
                _ => {}
            },
            DocNode::Admonition { kind, .. } => {
                if kind.eq_ignore_ascii_case("feature") {
                    self.used.insert("start_feature".to_string());
                }
                if !kind.eq_ignore_ascii_case("spotlight") && !kind.eq_ignore_ascii_case("feature")
                {
                    self.used.insert("callout".to_string());
                }
            }
            DocNode::List { .. }
            | DocNode::TaskList { .. }
            | DocNode::DefinitionList { .. }
            | DocNode::OptionList { .. }
            | DocNode::FieldList { .. }
            | DocNode::Stepper { .. } => {
                self.used.insert("list".to_string());
            }
            DocNode::HorizontalRule => {
                self.used.insert("horizontal_rule".to_string());
            }
            DocNode::Table { .. } => {
                self.used.insert("table".to_string());
            }
            DocNode::MathBlock { .. } => {
                self.used.insert("math".to_string());
            }
            DocNode::Directive { name, .. } => {
                if name == "contents" || name == "local-toc" || name == "toctree" {
                    self.used.insert("toc_block".to_string());
                } else {
                    let component_name = component_template_name(name);
                    if component_name.starts_with("metrics_") {
                        self.used.insert("metrics_runtime".to_string());
                        // Metric data panes use Matter's native code-surface controls.
                        self.used.insert("code_block".to_string());
                    }
                    self.used.insert(component_name);
                    self.used.insert("callout".to_string());
                }
            }
            DocNode::Media { .. } | DocNode::RawHtml { .. } | DocNode::LspSymbol { .. } => {}
        }
    }

    fn visit_inline(&mut self, inline: &InlineNode) {
        if let InlineNode::Math { .. } = inline {
            self.used.insert("math".to_string());
        }
    }
}

impl HtmlRenderer {
    pub fn render_prepared(context: &PreparedRenderContext<'_>) -> HashMap<String, String> {
        let site_ast = context.site;
        let processed_css = context.processed_css;
        let sprite_svg = context.sprite_svg;
        let registry = context.registry;
        let single_file = context.single_file;
        let live_reload = context.live_reload;
        let theme_color = context.theme_color;
        let background_color = context.background_color;
        let start_page = context.start_page;
        let seo = context.seo;
        let mut pages_output = HashMap::new();
        for page in &site_ast.pages {
            let page_html = Self::render_page(
                page,
                &context.site_title,
                &context.site_brand,
                &context.shell_template,
                processed_css,
                &context.search_data_code,
                &context.search_js_code,
                context.live_reload_code,
                sprite_svg,
                site_ast,
                context,
                registry,
                &context.highlighter,
                single_file,
                theme_color,
                background_color,
                start_page,
                seo,
                live_reload,
            );
            let target_rel_path = if page.slug == "index" {
                "index.html".to_string()
            } else {
                format!("{}/index.html", page.slug)
            };
            pages_output.insert(target_rel_path, page_html);
        }

        pages_output
    }

    /// Compatibility entrypoint for renderer unit tests and downstream callers.
    #[allow(clippy::too_many_arguments)]
    pub fn render_site(
        site: &DocSiteAst,
        shell_html: &str,
        processed_css: &str,
        search_js: &str,
        sprite_svg: &str,
        registry: &ComponentRegistry,
        single_file: bool,
        live_reload: bool,
        favicon_path: Option<&str>,
        theme_color: Option<&str>,
        background_color: Option<&str>,
        start_page: Option<&StartPageModel>,
        seo: &SeoCatalog,
    ) -> HashMap<String, String> {
        let assets = ResolvedAssets::default();
        let index = SiteIndex::build(site).expect("rendering requires a valid site index");
        let resolved_favicon = favicon_path.map(|path| {
            let p = std::path::Path::new(path);
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("svg")
                .to_lowercase();
            let mime = crate::v1::resolver::mime_for_extension(&ext).to_string();
            let bytes = std::fs::read(p).unwrap_or_default();
            crate::v1::resolver::ResolvedFavicon::Custom {
                bytes,
                extension: ext,
                mime_type: mime,
            }
        });
        let context = PreparedRenderContext::new(
            site,
            index,
            &assets,
            shell_html,
            processed_css,
            search_js,
            sprite_svg,
            registry,
            single_file,
            live_reload,
            None,
            resolved_favicon,
            theme_color,
            background_color,
            start_page,
            None,
            seo,
            None,
        );
        Self::render_prepared(&context)
    }

    #[allow(clippy::too_many_arguments)]
    fn render_page(
        page: &DocPage,
        site_title: &str,
        site_brand: &str,
        shell_template: &ShellTemplate,
        processed_css: &str,
        search_data_code: &str,
        search_js_code: &str,
        live_reload_code: &str,
        sprite_svg: &str,
        site_ast: &DocSiteAst,
        context: &PreparedRenderContext<'_>,
        registry: &ComponentRegistry,
        highlighter: &crate::v1::syntax::SyntaxHighlighter,
        single_file: bool,
        theme_color: Option<&str>,
        background_color: Option<&str>,
        start_page: Option<&StartPageModel>,
        seo: &SeoCatalog,
        serve_mode: bool,
    ) -> String {
        let page_title = page
            .metadata
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or(&page.slug);

        let is_metrics_page = crate::v1::is_metrics_page(page);
        let is_start_page = (page.slug == "index" && start_page.is_some()) || is_metrics_page;
        let full_title = if is_metrics_page {
            format!("Metrics — {site_title}")
        } else {
            Self::document_title(page_title, site_title, is_start_page)
        };
        let mut used_components = HashSet::new();

        if is_start_page {
            used_components.insert("start_page".to_string());
            if is_metrics_page {
                used_components.insert("metrics_page".to_string());
            }
        } else if site_ast.pages.len() > 1 {
            used_components.insert("pagination".to_string());
        }

        let content_html = if page.slug == "index"
            && let Some(sp) = start_page
        {
            Self::render_start_page(sp, context, registry, &mut used_components)
        } else {
            let mut content_html = String::with_capacity(4096);
            Self::render_nodes(
                page.ast.as_slice(),
                page,
                context.site,
                context,
                registry,
                highlighter,
                &mut used_components,
                &mut content_html,
            );
            if page.reference.is_some() {
                content_html.push_str(&Self::render_reference_relations(context, page));
            }
            if is_metrics_page {
                let (hero_html, sections_html) =
                    if let Some(end_idx) = content_html.find("</header>") {
                        let split_idx = end_idx + "</header>".len();
                        (&content_html[..split_idx], &content_html[split_idx..])
                    } else {
                        ("", content_html.as_str())
                    };
                registry.render(
                    "metrics_page",
                    &[
                        ("hero", hero_html),
                        ("sections", sections_html),
                        ("content", &content_html),
                    ],
                    &mut used_components,
                )
            } else {
                content_html
            }
        };

        let toc_html = if is_start_page {
            String::new()
        } else {
            Self::render_toc(page)
        };
        let inline_toc_html = if toc_html.is_empty() {
            String::new()
        } else {
            format!(
                "<details class=\"inline-toc\"><summary>On this page</summary><nav aria-label=\"On this page\">{toc_html}</nav></details>"
            )
        };
        let toc_rail_html = if toc_html.is_empty() {
            String::new()
        } else {
            format!(
                "<aside class=\"toc-rail\" aria-label=\"On this page\"><div class=\"toc-rail-title\">On this page</div><nav>{toc_html}</nav></aside>"
            )
        };
        let document_toc_html = if toc_html.is_empty() {
            String::new()
        } else {
            format!(
                "<details class=\"compact-toc\"><summary>On this page</summary><nav aria-label=\"On this page\">{toc_html}</nav></details><aside class=\"toc-rail\" aria-label=\"On this page\"><div class=\"toc-rail-title\">On this page</div><nav>{toc_html}</nav></aside>"
            )
        };
        let home_href = Self::compute_relative_href(&page.slug, "index");
        let breadcrumbs_html = if is_start_page {
            String::new()
        } else {
            Self::render_breadcrumbs(site_ast, page)
        };

        let page_description = page
            .metadata
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let seo_page = seo.page(&page.slug);
        let robots = if !serve_mode {
            if seo_page.indexable {
                "index, follow"
            } else {
                "noindex, follow"
            }
        } else {
            "noindex, nofollow"
        };

        let og_image = page
            .metadata
            .get("image")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| Self::resolve_absolute_url(s, &context.seo.site_url))
            .or_else(|| {
                context
                    .social_image
                    .map(|s| Self::resolve_absolute_url(s, &context.seo.site_url))
            })
            .or_else(|| match &context.resolved_favicon {
                Some(crate::v1::resolver::ResolvedFavicon::Custom { extension, .. })
                    if extension != "svg" =>
                {
                    context
                        .seo
                        .site_url
                        .join(&format!("assets/favicons/favicon.{}", extension))
                        .ok()
                        .map(|u| u.to_string())
                }
                _ => None,
            });

        let mut meta_tags = Vec::new();
        meta_tags.push(format!(
            "<link rel=\"canonical\" href=\"{}\">",
            Self::escape_html(&seo_page.canonical_url)
        ));
        meta_tags.push(format!("<meta name=\"robots\" content=\"{}\">", robots));
        meta_tags.push("<meta name=\"generator\" content=\"evin\">".to_string());

        if !page_description.is_empty() {
            meta_tags.push(format!(
                "<meta name=\"description\" content=\"{}\">",
                Self::escape_html(page_description)
            ));
        }

        meta_tags.push(format!(
            "<meta property=\"og:url\" content=\"{}\">",
            Self::escape_html(&seo_page.canonical_url)
        ));
        meta_tags.push("<meta property=\"og:type\" content=\"website\">".to_string());
        meta_tags.push(format!(
            "<meta property=\"og:site_name\" content=\"{}\">",
            Self::escape_html(site_title)
        ));
        meta_tags.push(format!(
            "<meta property=\"og:title\" content=\"{}\">",
            Self::escape_html(page_title)
        ));
        if !page_description.is_empty() {
            meta_tags.push(format!(
                "<meta property=\"og:description\" content=\"{}\">",
                Self::escape_html(page_description)
            ));
        }
        if let Some(image_url) = &og_image {
            meta_tags.push(format!(
                "<meta property=\"og:image\" content=\"{}\">",
                Self::escape_html(image_url)
            ));
        }

        meta_tags.push("<meta name=\"twitter:card\" content=\"summary\">".to_string());
        meta_tags.push(format!(
            "<meta name=\"twitter:title\" content=\"{}\">",
            Self::escape_html(page_title)
        ));
        if !page_description.is_empty() {
            meta_tags.push(format!(
                "<meta name=\"twitter:description\" content=\"{}\">",
                Self::escape_html(page_description)
            ));
        }
        if let Some(image_url) = &og_image {
            meta_tags.push(format!(
                "<meta name=\"twitter:image\" content=\"{}\">",
                Self::escape_html(image_url)
            ));
        }

        for (alt_lang, alt_url) in &seo_page.hreflang_alternates {
            meta_tags.push(format!(
                "<link rel=\"alternate\" hreflang=\"{}\" href=\"{}\">",
                Self::escape_html(alt_lang),
                Self::escape_html(alt_url)
            ));
        }

        if !is_start_page && let Some(idx) = context.index.page_position(&page.slug) {
            let pages = &context.site.pages;
            if idx > 0
                && let Some(prev) = pages.get(idx - 1)
            {
                let prev_canonical = &context.seo.page(&prev.slug).canonical_url;
                meta_tags.push(format!(
                    "<link rel=\"prev\" href=\"{}\">",
                    Self::escape_html(prev_canonical)
                ));
            }
            if let Some(next) = pages.get(idx + 1) {
                let next_canonical = &context.seo.page(&next.slug).canonical_url;
                meta_tags.push(format!(
                    "<link rel=\"next\" href=\"{}\">",
                    Self::escape_html(next_canonical)
                ));
            }
        }

        let meta_html = meta_tags.join("\n    ");

        let json_ld = if is_start_page {
            let mut schema = serde_json::json!({
                "@context": "https://schema.org",
                "@type": "WebSite",
                "name": site_title,
                "url": seo_page.canonical_url,
            });
            if !page_description.is_empty() {
                schema["description"] = serde_json::Value::String(page_description.to_string());
            }
            format!(
                "<script type=\"application/ld+json\">\n{}\n</script>",
                serde_json::to_string_pretty(&schema).unwrap_or_default()
            )
        } else {
            let mut schema = serde_json::json!({
                "@context": "https://schema.org",
                "@type": "TechArticle",
                "headline": page_title,
                "url": seo_page.canonical_url,
                "isPartOf": {
                    "@type": "WebSite",
                    "name": site_title,
                    "url": context.seo.site_url.as_str()
                }
            });
            if !page_description.is_empty() {
                schema["description"] = serde_json::Value::String(page_description.to_string());
            }
            if let Some(updated) = &seo_page.updated {
                schema["dateModified"] = serde_json::Value::String(updated.clone());
            }
            if let Some(author) = page
                .metadata
                .get("author")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                schema["author"] = serde_json::json!({
                    "@type": "Person",
                    "name": author
                });
            }
            format!(
                "<script type=\"application/ld+json\">\n{}\n</script>",
                serde_json::to_string_pretty(&schema).unwrap_or_default()
            )
        };

        let nav_html = Self::render_nav_for_context(context, &page.slug);
        let pagination_html = if is_start_page {
            String::new()
        } else {
            Self::render_pagination_for_context(context, &page.slug)
        };
        let depth_prefix = Self::compute_relative_prefix(&page.slug);
        let assets_prefix = format!("{}assets/", depth_prefix);
        let font_preloads = if !single_file
            && used_components.contains("tape")
            && shell_template.uses(ShellSlot::FontPreloads)
        {
            crate::v1::tape_fonts::used_by_markup(&content_html)
                .into_iter()
                .map(|font| format!(
                    "<link rel=\"preload\" href=\"{}fonts/{}\" as=\"font\" type=\"font/woff2\" crossorigin>",
                    assets_prefix, font.filename
                ))
                .collect()
        } else {
            String::new()
        };
        let page_search_js = if single_file {
            search_js_code.to_string()
        } else if shell_template.uses(ShellSlot::SearchJs) {
            format!("<script src=\"{}app.js\" defer></script>", assets_prefix)
        } else {
            String::new()
        };
        let (_, favicon_head_html) = crate::v1::favicon::FaviconPipeline::process(
            context.resolved_favicon.as_ref(),
            site_title,
            theme_color,
            background_color,
            &depth_prefix,
        );
        let topbar_html = context
            .topbar
            .map(|topbar| {
                Self::render_topbar(topbar, context, &page.slug, registry, &mut used_components)
            })
            .unwrap_or_default();

        let component_js = registry.collect_js(&used_components);

        let final_css = if single_file {
            format!(
                "<style>\n{}\nhtml body {{ opacity: 1 !important; transition: none !important; }}\n</style>",
                processed_css
            )
        } else {
            format!(
                "<link rel=\"preload\" href=\"{}theme.css\" as=\"style\">\n<link rel=\"stylesheet\" href=\"{}theme.css\" onload=\"document.documentElement.classList.add('theme-ready')\">",
                assets_prefix, assets_prefix
            )
        };

        let component_js_code =
            if !shell_template.uses(ShellSlot::ComponentJs) || component_js.is_empty() {
                String::new()
            } else {
                format!("<script>\n{}\n</script>", component_js)
            };

        let slots = PageSlots {
            title: &full_title,
            site_title,
            site_brand,
            brand_logo: &context.brand_logo_html,
            home_href: &home_href,
            meta: &meta_html,
            favicons: &favicon_head_html,
            font_preloads: &font_preloads,
            icons_sprite: sprite_svg,
            topbar: &topbar_html,
            assets_prefix: &assets_prefix,
            depth_prefix: &depth_prefix,
            css: &final_css,
            nav: &nav_html,
            breadcrumbs: &breadcrumbs_html,
            content: &content_html,
            toc: &toc_html,
            inline_toc: &inline_toc_html,
            toc_rail: &toc_rail_html,
            document_toc: &document_toc_html,
            pagination: &pagination_html,
            search_data: search_data_code,
            search_js: &page_search_js,
            component_js: &component_js_code,
            live_reload: live_reload_code,
            page_kind: if is_start_page { "start" } else { "document" },
            page_language: &seo_page.language,
            json_ld: &json_ld,
        };

        shell_template.render(&slots)
    }

    /// Render a live developer route through the selected documentation shell.
    /// The route deliberately uses the normal start-page page kind so it inherits
    /// layout behavior instead of carrying a second shell or theme.
    #[allow(clippy::too_many_arguments)]
    pub fn render_workbench_shell(
        shell_html: &str,
        site_title: &str,
        site_brand: &str,
        brand_logo: &str,
        icons_sprite: &str,
        content: &str,
        topbar: Option<&Topbar>,
        topbar_scope: &str,
        registry: &ComponentRegistry,
        language: &str,
        live_reload: bool,
    ) -> String {
        let shell_template = ShellTemplate::parse(shell_html);
        let mut used_components = HashSet::new();
        let topbar_html = topbar
            .map(|setting| {
                Self::render_workbench_topbar(setting, topbar_scope, registry, &mut used_components)
            })
            .unwrap_or_default();
        let search_js = if shell_template.uses(ShellSlot::SearchJs) {
            "<script src=\"/assets/app.js\" defer></script>"
        } else {
            ""
        };
        let css = "<link rel=\"preload\" href=\"/assets/theme.css\" as=\"style\">\n<link rel=\"stylesheet\" href=\"/assets/theme.css\" onload=\"document.documentElement.classList.add('theme-ready')\">";
        let live_reload = if live_reload && shell_template.uses(ShellSlot::LiveReload) {
            "<script>(function(){try{const source=new EventSource('/__evin_reload');source.addEventListener('reload',function(){window.location.reload();});}catch(_){}})();</script>"
        } else {
            ""
        };
        let title = format!("Metrics — {site_title}");
        let slots = PageSlots {
            title: &title,
            site_title,
            site_brand,
            brand_logo,
            home_href: "/",
            meta: "<meta name=\"robots\" content=\"noindex, nofollow\">",
            favicons: "",
            font_preloads: "",
            icons_sprite,
            topbar: &topbar_html,
            assets_prefix: "/assets/",
            depth_prefix: "",
            css,
            nav: "",
            breadcrumbs: "",
            content,
            toc: "",
            inline_toc: "",
            toc_rail: "",
            document_toc: "",
            pagination: "",
            search_data: "",
            search_js,
            component_js: "",
            live_reload,
            page_kind: "start",
            page_language: language,
            json_ld: "",
        };
        shell_template.render(&slots)
    }

    fn render_workbench_topbar(
        topbar: &Topbar,
        scope: &str,
        registry: &ComponentRegistry,
        used_components: &mut HashSet<String>,
    ) -> String {
        let mut surface_open = String::from(
            "<section class=\"docs-topbar topbar presentation-surface presentation--topbar presentation-scale-compact topbar--",
        );
        Self::escape_into(&mut surface_open, &topbar.variant);
        surface_open.push_str(" presentation-surface--");
        Self::escape_into(&mut surface_open, &topbar.variant);
        surface_open.push_str("\" data-presentation-variant=\"");
        Self::escape_into(&mut surface_open, &topbar.variant);
        surface_open.push_str("\" data-presentation-role=\"topbar\" data-presentation-scale=\"compact\" data-topbar-variant=\"");
        Self::escape_into(&mut surface_open, &topbar.variant);
        surface_open.push_str("\" data-topbar-scope=\"");
        Self::escape_into(&mut surface_open, scope);
        surface_open.push('"');
        if let Some(id) = &topbar.id {
            surface_open.push_str(" data-topbar-id=\"");
            Self::escape_into(&mut surface_open, id);
            surface_open.push('"');
        }
        surface_open.push_str(" role=\"status\" aria-label=\"Site announcement\">");
        let mut content = String::from("<div class=\"topbar-content\">");
        if let Some(badge) = &topbar.badge {
            content.push_str("<span class=\"topbar-badge presentation-badge\">");
            Self::escape_into(&mut content, badge);
            content.push_str("</span>");
        }
        content.push_str("<p class=\"topbar-message\"><span class=\"topbar-message-track\">");
        Self::escape_into(&mut content, &topbar.message);
        content.push_str("</span></p>");
        if let Some(action) = &topbar.action {
            content.push_str("<a class=\"topbar-action presentation-action\" href=\"");
            Self::escape_into(&mut content, &action.href);
            content.push('"');
            if action.href.starts_with("https://") {
                content.push_str(" target=\"_blank\" rel=\"noreferrer\"");
            }
            content.push_str("><span>");
            Self::escape_into(&mut content, &action.label);
            content.push_str("</span><svg class=\"docs-icon\" aria-hidden=\"true\"><use href=\"#icon-arrow-right\"></use></svg></a>");
        }
        content.push_str("</div>");
        if topbar.dismissible {
            content.push_str("<button class=\"topbar-close\" type=\"button\" aria-label=\"Dismiss announcement\" data-action=\"topbar:dismiss\"><svg class=\"docs-icon\" aria-hidden=\"true\"><use href=\"#icon-x\"></use></svg></button>");
        }
        registry.render(
            "presentation",
            &[
                ("surface_open", &surface_open),
                ("surface_close", "</section>"),
                ("layer_one_classes", "topbar-layer topbar-layer-one"),
                ("layer_two_classes", "topbar-layer topbar-layer-two"),
                ("content", &content),
            ],
            used_components,
        )
    }

    fn document_title(page_title: &str, site_title: &str, is_start_page: bool) -> String {
        if is_start_page {
            site_title.to_string()
        } else {
            format!("{page_title} — {site_title}")
        }
    }

    fn render_topbar(
        topbar: &Topbar,
        context: &PreparedRenderContext<'_>,
        page_slug: &str,
        registry: &ComponentRegistry,
        used_components: &mut HashSet<String>,
    ) -> String {
        let mut surface_open = String::from(
            "<section class=\"docs-topbar topbar presentation-surface presentation--topbar presentation-scale-compact topbar--",
        );
        Self::escape_into(&mut surface_open, &topbar.variant);
        surface_open.push_str(" presentation-surface--");
        Self::escape_into(&mut surface_open, &topbar.variant);
        surface_open.push_str("\" data-presentation-variant=\"");
        Self::escape_into(&mut surface_open, &topbar.variant);
        surface_open.push_str("\" data-presentation-role=\"topbar\" data-presentation-scale=\"compact\" data-topbar-variant=\"");
        Self::escape_into(&mut surface_open, &topbar.variant);
        surface_open.push_str("\" data-topbar-scope=\"");
        Self::escape_into(&mut surface_open, context.seo.site_url.as_str());
        surface_open.push('"');
        if let Some(id) = &topbar.id {
            surface_open.push_str(" data-topbar-id=\"");
            Self::escape_into(&mut surface_open, id);
            surface_open.push('"');
        }
        surface_open.push_str(" role=\"status\" aria-label=\"Site announcement\">");

        let mut html = String::new();
        html.push_str("<div class=\"topbar-content\">");
        if let Some(badge) = &topbar.badge {
            html.push_str("<span class=\"topbar-badge presentation-badge\">");
            Self::escape_into(&mut html, badge);
            html.push_str("</span>");
        }
        html.push_str("<p class=\"topbar-message\"><span class=\"topbar-message-track\">");
        Self::escape_into(&mut html, &topbar.message);
        html.push_str("</span></p>");
        if let Some(action) = &topbar.action {
            let href = Self::resolve_rendered_href(context, page_slug, &action.href);
            html.push_str("<a class=\"topbar-action presentation-action\" href=\"");
            Self::escape_into(&mut html, &href);
            html.push('"');
            if action.href.starts_with("https://") {
                html.push_str(" target=\"_blank\" rel=\"noreferrer\"");
            }
            html.push_str("><span>");
            Self::escape_into(&mut html, &action.label);
            html.push_str("</span><svg class=\"docs-icon\" aria-hidden=\"true\"><use href=\"#icon-arrow-right\"></use></svg></a>");
        }
        html.push_str("</div>");
        if topbar.dismissible {
            html.push_str("<button class=\"topbar-close\" type=\"button\" aria-label=\"Dismiss announcement\" data-action=\"topbar:dismiss\"><svg class=\"docs-icon\" aria-hidden=\"true\"><use href=\"#icon-x\"></use></svg></button>");
        }
        registry.render(
            "presentation",
            &[
                ("surface_open", &surface_open),
                ("surface_close", "</section>"),
                ("layer_one_classes", "topbar-layer topbar-layer-one"),
                ("layer_two_classes", "topbar-layer topbar-layer-two"),
                ("content", &html),
            ],
            used_components,
        )
    }

    fn render_start_page(
        start_page: &StartPageModel,
        context: &PreparedRenderContext<'_>,
        registry: &ComponentRegistry,
        used_components: &mut HashSet<String>,
    ) -> String {
        let mut html = String::from("<div class=\"start-page\">\n<header class=\"start-hero\">\n");
        html.push_str("<div class=\"start-hero-content\">\n<p class=\"start-eyebrow\">Documentation</p>\n<h1>");
        Self::escape_into(&mut html, &start_page.title);
        html.push_str("</h1>\n<p class=\"start-lead\">");
        Self::escape_into(&mut html, &start_page.lead);
        html.push_str("</p>\n</div>\n</header>\n<div class=\"start-sections\" aria-label=\"Documentation topics\">\n");
        if let Some(spotlight) = &start_page.spotlight
            && spotlight.after_sections == 0
        {
            Self::render_start_spotlight(&mut html, spotlight, context, registry, used_components);
        }
        Self::render_start_features(&mut html, start_page, 0, context, registry, used_components);
        for (index, section) in start_page.sections.iter().enumerate() {
            let heading_id = format!("start-section-{}", index + 1);
            html.push_str("<section class=\"start-section\" aria-labelledby=\"");
            html.push_str(&heading_id);
            html.push_str("\">\n<h2 id=\"");
            html.push_str(&heading_id);
            html.push_str("\">");
            Self::escape_into(&mut html, &section.title);
            html.push_str("</h2>\n<div class=\"start-section-layout");
            if section.media.is_some() {
                html.push_str(" has-media");
            }
            html.push_str("\"><div class=\"start-section-content\">\n");
            if !section.paragraphs.is_empty() {
                html.push_str("<div class=\"start-section-description\">");
                for paragraph in &section.paragraphs {
                    html.push_str("<p>");
                    for inline in paragraph {
                        Self::render_start_page_inline(inline, context, &mut html);
                    }
                    html.push_str("</p>");
                }
                html.push_str("</div>\n");
            }
            html.push_str("<div class=\"start-card-collection\"><div class=\"start-card-grid\" role=\"list\">\n");
            for (card_index, card) in section.cards.iter().enumerate() {
                let title_id = format!("start-card-{}-{}-title", index + 1, card_index + 1);
                let description_id =
                    format!("start-card-{}-{}-description", index + 1, card_index + 1);
                html.push_str("<a class=\"start-card\" role=\"listitem\" aria-labelledby=\"");
                html.push_str(&title_id);
                html.push(' ');
                html.push_str(&description_id);
                html.push_str("\" href=\"");
                html.push_str(&Self::compute_relative_href("index", &card.target_slug));
                html.push_str(
                    "\">\n<span class=\"start-card-copy\"><span class=\"start-card-title\" id=\"",
                );
                html.push_str(&title_id);
                html.push_str("\">");
                Self::escape_into(&mut html, &card.title);
                html.push_str("</span><span class=\"start-card-description\" id=\"");
                html.push_str(&description_id);
                html.push_str("\">");
                Self::escape_into(&mut html, &card.description);
                html.push_str("</span></span><svg class=\"docs-icon start-card-icon\" aria-hidden=\"true\"><use href=\"#icon-arrow-right\"></use></svg>\n</a>\n");
            }
            html.push_str("</div></div>\n</div>");
            if let Some(media) = &section.media {
                Self::render_start_section_media(&mut html, media, context);
            }
            html.push_str("</div>\n</section>\n");
            if let Some(spotlight) = &start_page.spotlight
                && spotlight.after_sections == index + 1
            {
                Self::render_start_spotlight(
                    &mut html,
                    spotlight,
                    context,
                    registry,
                    used_components,
                );
            }
            Self::render_start_features(
                &mut html,
                start_page,
                index + 1,
                context,
                registry,
                used_components,
            );
        }
        html.push_str("</div>\n");
        if let Some(footer) = &start_page.footer {
            let mut columns = String::new();
            for (column_index, column) in footer.columns.iter().enumerate() {
                let heading_id = format!("start-footer-column-{}", column_index + 1);
                columns.push_str("<section class=\"start-footer-column\" aria-labelledby=\"");
                columns.push_str(&heading_id);
                columns.push_str("\"><h3 id=\"");
                columns.push_str(&heading_id);
                columns.push_str("\">");
                Self::escape_into(&mut columns, &column.title);
                columns.push_str("</h3><ul>");
                for link in &column.links {
                    let (href, external_attributes) = match &link.destination {
                        crate::v1::start_page::FooterDestination::Internal(slug) => {
                            (Self::compute_relative_href("index", slug), "")
                        }
                        crate::v1::start_page::FooterDestination::External(href) => {
                            (href.clone(), " target=\"_blank\" rel=\"noreferrer\"")
                        }
                    };
                    columns.push_str("<li><a href=\"");
                    columns.push_str(&Self::escape_html(&href));
                    columns.push('"');
                    columns.push_str(external_attributes);
                    columns.push('>');
                    Self::escape_into(&mut columns, &link.title);
                    columns.push_str("</a>");
                    if let Some(description) = &link.description {
                        columns.push_str("<span class=\"start-footer-link-description\">");
                        Self::escape_into(&mut columns, description);
                        columns.push_str("</span>");
                    }
                    columns.push_str("</li>");
                }
                columns.push_str("</ul></section>");
            }
            let footer_html = registry.render(
                "footer",
                &[
                    ("title", &start_page.title),
                    ("logo", &context.footer_brand_logo_html),
                    ("description", &footer.description),
                    ("columns", &columns),
                ],
                used_components,
            );
            html.push_str(&footer_html);
        }
        html.push_str("</div>\n");
        html
    }

    fn render_start_spotlight(
        html: &mut String,
        spotlight: &StartPageSpotlight,
        context: &PreparedRenderContext<'_>,
        registry: &ComponentRegistry,
        used_components: &mut HashSet<String>,
    ) {
        let mut surface_open = String::from(
            "<section class=\"start-spotlight presentation-surface presentation--spotlight presentation-scale-feature topbar--",
        );
        Self::escape_into(&mut surface_open, &spotlight.content.variant);
        surface_open.push_str(" presentation-surface--");
        Self::escape_into(&mut surface_open, &spotlight.content.variant);
        surface_open.push_str("\" data-presentation-variant=\"");
        Self::escape_into(&mut surface_open, &spotlight.content.variant);
        surface_open.push_str("\" data-presentation-role=\"spotlight\" data-presentation-scale=\"feature\" aria-labelledby=\"start-spotlight-title\">");

        let mut content = String::from("<div class=\"start-spotlight-content\">");
        if let Some(badge) = &spotlight.content.badge {
            content.push_str("<span class=\"start-spotlight-badge presentation-badge\">");
            Self::escape_into(&mut content, badge);
            content.push_str("</span>");
        }
        content.push_str("<h2 id=\"start-spotlight-title\">");
        Self::escape_into(&mut content, &spotlight.content.title);
        content.push_str("</h2><p>");
        Self::escape_into(&mut content, &spotlight.content.description);
        content.push_str("</p>");
        let href = Self::resolve_rendered_href(context, "index", &spotlight.content.action.href);
        content.push_str("<a class=\"start-spotlight-action presentation-action\" href=\"");
        Self::escape_into(&mut content, &href);
        content.push('"');
        if spotlight.content.action.href.starts_with("https://") {
            content.push_str(" target=\"_blank\" rel=\"noreferrer\"");
        }
        content.push_str("><span>");
        Self::escape_into(&mut content, &spotlight.content.action.label);
        content.push_str("</span><svg class=\"docs-icon\" aria-hidden=\"true\"><use href=\"#icon-arrow-right\"></use></svg></a></div>");
        html.push_str(&registry.render(
            "presentation",
            &[
                ("surface_open", &surface_open),
                ("surface_close", "</section>"),
                ("layer_one_classes", ""),
                ("layer_two_classes", ""),
                ("content", &content),
            ],
            used_components,
        ));
        html.push('\n');
    }

    fn render_start_features(
        html: &mut String,
        start_page: &StartPageModel,
        after_sections: usize,
        context: &PreparedRenderContext<'_>,
        registry: &ComponentRegistry,
        used_components: &mut HashSet<String>,
    ) {
        for (feature_index, feature) in start_page.features.iter().enumerate() {
            if feature.after_sections != after_sections {
                continue;
            }
            let heading_id = format!("start-feature-{}-title", feature_index + 1);
            let mut content = format!("<h2 id=\"{heading_id}\">");
            Self::escape_into(&mut content, &feature.title);
            content.push_str("</h2><p>");
            for inline in &feature.description {
                Self::render_start_page_inline(inline, context, &mut content);
            }
            content.push_str("</p><div class=\"start-feature-actions\">");
            for (action_index, action) in feature.actions.iter().enumerate() {
                let href = match &action.destination {
                    FooterDestination::Internal(slug) => Self::compute_relative_href("index", slug),
                    FooterDestination::External(href) => href.clone(),
                };
                content.push_str("<a class=\"start-feature-action");
                if action_index == 1 {
                    content.push_str(" start-feature-action--secondary");
                }
                content.push_str("\" href=\"");
                Self::escape_into(&mut content, &href);
                content.push('"');
                if href.starts_with("https://") {
                    content.push_str(" target=\"_blank\" rel=\"noreferrer\"");
                }
                content.push('>');
                Self::escape_into(&mut content, &action.label);
                content.push_str("</a>");
            }
            content.push_str("</div>");

            let aside = match &feature.aside {
                StartPageFeatureAside::Media(media) => {
                    let mut media = media.clone();
                    if media.kind == MediaKind::Image
                        && let Some(url) = context.asset_url(&media.src, "index")
                    {
                        media.src = url;
                    }
                    let mut output = String::new();
                    Self::render_media(&mut output, "start-feature-media", &media);
                    output
                }
                StartPageFeatureAside::Code {
                    language,
                    title,
                    code,
                } => {
                    let data_language = language
                        .as_ref()
                        .map(|language| {
                            format!(" data-language=\"{}\"", Self::escape_html(language))
                        })
                        .unwrap_or_default();
                    let (display_language, extension) =
                        Self::format_language_details(language.as_deref());
                    let display_title = title.as_deref().map(Self::escape_html).unwrap_or_default();
                    let highlighted = context.highlighter.highlight(language.as_deref(), code);
                    registry.render(
                        "code_block",
                        &[
                            ("data_language", &data_language),
                            ("display_title", &display_title),
                            ("display_language", &display_language),
                            ("extension", &extension),
                            ("code", &highlighted),
                        ],
                        used_components,
                    )
                }
                StartPageFeatureAside::Tape {
                    script,
                    config,
                    rendered_svg,
                } => {
                    let svg = rendered_svg.as_deref().unwrap_or(
                        "<p class=\"terminal-tape-error\">Terminal tape was not compiled.</p>",
                    );
                    let runtime_label = match config.runtime {
                        TapeRuntime::Esh => "ESH",
                        TapeRuntime::System => "System shell",
                    };
                    let copy_script = TapeScript::parse(script)
                        .map(|script| script.to_shell_script())
                        .unwrap_or_default();
                    let copy_disabled = if copy_script.is_empty() {
                        "disabled"
                    } else {
                        ""
                    };
                    let escaped_copy_script = Self::escape_html(&copy_script);
                    registry.render(
                        "tape",
                        &[
                            ("svg", svg),
                            (
                                "title",
                                config.title.as_deref().unwrap_or("Terminal recording"),
                            ),
                            ("runtime_label", runtime_label),
                            ("copy_script", &escaped_copy_script),
                            ("copy_disabled", copy_disabled),
                        ],
                        used_components,
                    )
                }
                StartPageFeatureAside::Diagram {
                    kind,
                    code,
                    title,
                    caption,
                    alt,
                    rendered_mermaid,
                } => Self::render_diagram(
                    kind,
                    code,
                    title.as_deref(),
                    caption.as_deref(),
                    alt.as_deref(),
                    rendered_mermaid.as_ref(),
                    registry,
                    used_components,
                ),
            };
            let placement = if feature_index % 2 == 0 {
                "right"
            } else {
                "left"
            };
            html.push_str(&registry.render(
                "start_feature",
                &[
                    ("heading_id", &heading_id),
                    ("placement", placement),
                    ("content", &content),
                    ("aside", &aside),
                ],
                used_components,
            ));
            html.push('\n');
        }
    }

    fn render_start_page_inline(
        inline: &InlineNode,
        context: &PreparedRenderContext<'_>,
        out: &mut String,
    ) {
        let InlineNode::Link {
            href,
            title,
            content,
            code,
        } = inline
        else {
            let root_page = context
                .index
                .page(context.site, "index")
                .expect("start page exists");
            let mut tracker = HashSet::new();
            Self::render_inline(inline, root_page, context.site, context, &mut tracker, out);
            return;
        };

        let href = context.index.resolve_alias(href).map_or_else(
            || {
                let route = href.trim_matches('/');
                if context.index.contains_route(route) {
                    Self::compute_relative_href("index", route)
                } else {
                    href.clone()
                }
            },
            |target| {
                let base = Self::compute_relative_href("index", target.slug);
                target
                    .anchor
                    .map_or_else(|| base.clone(), |anchor| format!("{base}#{anchor}"))
            },
        );
        out.push_str("<a href=\"");
        Self::escape_into(out, &href);
        out.push('"');
        if let Some(title) = title {
            out.push_str(" title=\"");
            Self::escape_into(out, title);
            out.push('"');
        }
        out.push('>');
        if *code {
            out.push_str("<code>");
        }
        Self::escape_into(out, content);
        if *code {
            out.push_str("</code>");
        }
        out.push_str("</a>");
    }

    fn render_start_section_media(
        out: &mut String,
        media: &StartPageMedia,
        context: &PreparedRenderContext<'_>,
    ) {
        let mut projected = media.clone();
        if projected.kind == MediaKind::Image
            && let Some(url) = context.asset_url(&projected.src, "index")
        {
            projected.src = url;
        }
        Self::render_media(out, "start-section-media", &projected);
    }

    fn render_media(out: &mut String, class_name: &str, media: &StartPageMedia) {
        out.push_str("<figure class=\"");
        out.push_str(class_name);
        out.push_str("\">");
        match media.kind {
            MediaKind::Image => {
                out.push_str("<img loading=\"lazy\" src=\"");
                Self::escape_into(out, &media.src);
                out.push_str("\" alt=\"");
                Self::escape_into(out, &media.alt);
                out.push('"');
                if let Some(title) = &media.title {
                    out.push_str(" title=\"");
                    Self::escape_into(out, title);
                    out.push('"');
                }
                out.push('>');
            }
            MediaKind::Video => {
                out.push_str("<video controls preload=\"metadata\" src=\"");
                Self::escape_into(out, &media.src);
                out.push_str("\" aria-label=\"");
                Self::escape_into(out, &media.alt);
                out.push('"');
                if let Some(poster) = &media.poster {
                    out.push_str(" poster=\"");
                    Self::escape_into(out, poster);
                    out.push('"');
                }
                out.push_str("></video>");
            }
        }
        out.push_str("</figure>");
    }

    fn escape_into(out: &mut String, value: &str) {
        push_escaped_html(out, value);
    }

    fn resolve_absolute_url(url_or_path: &str, site_url: &url::Url) -> String {
        if url_or_path.starts_with("http://") || url_or_path.starts_with("https://") {
            url_or_path.to_string()
        } else {
            site_url
                .join(url_or_path.trim_start_matches('/'))
                .map(|u| u.to_string())
                .unwrap_or_else(|_| url_or_path.to_string())
        }
    }

    fn compute_relative_prefix(from_slug: &str) -> String {
        let depth = if from_slug == "index" || from_slug.is_empty() {
            0
        } else {
            from_slug.split('/').count()
        };

        if depth == 0 {
            "./".to_string()
        } else {
            "../".repeat(depth)
        }
    }

    fn compute_relative_href(from_slug: &str, to_slug: &str) -> String {
        if from_slug == to_slug {
            return "index.html".to_string();
        }

        let depth = if from_slug == "index" || from_slug.is_empty() {
            0
        } else {
            from_slug.split('/').count()
        };

        let prefix = if depth == 0 {
            "./".to_string()
        } else {
            "../".repeat(depth)
        };

        if to_slug == "index" || to_slug.is_empty() {
            format!("{}index.html", prefix)
        } else {
            format!("{}{}/index.html", prefix, to_slug)
        }
    }

    fn render_nav_for_context(context: &PreparedRenderContext<'_>, current_slug: &str) -> String {
        context.navigation.render(context.site, current_slug)
    }

    fn render_breadcrumbs(site: &DocSiteAst, page: &DocPage) -> String {
        let mut crumbs = format!(
            "<a href=\"{}\">Home</a>",
            Self::compute_relative_href(&page.slug, "index")
        );
        let segments: Vec<&str> = page.slug.split('/').collect();
        if segments.len() <= 1 {
            crumbs.push_str(" &nbsp;/&nbsp; <span>");
            crumbs.push_str(&Self::escape_html(Self::page_title(page)));
            crumbs.push_str("</span>");
            return crumbs;
        }

        let mut current_path = String::new();
        for (i, segment) in segments.iter().enumerate() {
            if i == segments.len() - 1 {
                crumbs.push_str(" &nbsp;/&nbsp; <span>");
                crumbs.push_str(&Self::escape_html(Self::page_title(page)));
                crumbs.push_str("</span>");
            } else {
                if !current_path.is_empty() {
                    current_path.push('/');
                }
                current_path.push_str(segment);
                let label = Self::nav_group_label_for_site(site, &current_path);
                crumbs.push_str(" &nbsp;/&nbsp; ");
                if site
                    .pages
                    .iter()
                    .any(|candidate| candidate.slug == current_path)
                {
                    crumbs.push_str(&format!(
                        "<a href=\"{}\">{}</a>",
                        Self::compute_relative_href(&page.slug, &current_path),
                        Self::escape_html(&label)
                    ));
                } else {
                    crumbs.push_str("<span>");
                    crumbs.push_str(&Self::escape_html(&label));
                    crumbs.push_str("</span>");
                }
            }
        }
        crumbs
    }

    #[cfg(test)]
    fn render_nav_for_page(site_ast: &DocSiteAst, current_slug: &str) -> String {
        let model = NavigationModel::build(site_ast);
        model.render(site_ast, current_slug)
    }

    fn render_nav_link(
        out: &mut String,
        page: &DocPage,
        title: &str,
        current_slug: &str,
        base_class: &str,
        level: usize,
    ) {
        let href = Self::compute_relative_href(current_slug, &page.slug);
        let active = page.slug == current_slug;
        let escaped_title = Self::escape_html(title);
        let level_attr = format!(" style=\"--nav-level: {};\"", level);
        out.push_str(&format!(
            "<a href=\"{}\" class=\"{}{}\"{}{} title=\"{}\"><span>{}</span></a>\n",
            href,
            base_class,
            if active { " active" } else { "" },
            if active { " aria-current=\"page\"" } else { "" },
            level_attr,
            escaped_title,
            escaped_title,
        ));
    }

    fn nav_group_label_for_site(site: &DocSiteAst, group: &str) -> String {
        if let Some(index_page) = site.pages.iter().find(|p| p.slug == group) {
            let title = Self::page_title(index_page);
            if !title.is_empty() {
                return title.to_string();
            }
        }
        Self::nav_group_label(group)
    }

    fn nav_group_label(group: &str) -> String {
        if group == "overview" {
            return "Overview".to_string();
        }
        group
            .split(['-', '_'])
            .filter(|part| !part.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn render_pagination_for_context(
        context: &PreparedRenderContext<'_>,
        current_slug: &str,
    ) -> String {
        let pages = &context.site.pages;
        let idx = match context.index.page_position(current_slug) {
            Some(i) => i,
            None => return String::new(),
        };

        Self::render_pagination_at(pages, idx, current_slug)
    }

    fn render_pagination_at(pages: &[DocPage], idx: usize, current_slug: &str) -> String {
        let prev = if idx > 0 { pages.get(idx - 1) } else { None };
        let next = pages.get(idx + 1);

        if prev.is_none() && next.is_none() {
            return String::new();
        }

        let mut html =
            String::from("<nav class=\"doc-pagination\" aria-label=\"Page Navigation\">\n");

        if let Some(p) = prev {
            let title = Self::escape_html(Self::page_title(p));
            let href = Self::compute_relative_href(current_slug, &p.slug);
            html.push_str(&format!(
                "  <a href=\"{}\" class=\"pagination-card prev-card\">\n    <svg class=\"docs-icon pagination-icon\" aria-hidden=\"true\"><use href=\"#icon-arrow-left\"></use></svg>\n    <div class=\"pagination-text\">\n      <span class=\"pagination-label\">Previous</span>\n      <span class=\"pagination-title\">{}</span>\n    </div>\n  </a>\n",
                href, title
            ));
        } else {
            html.push_str("  <div></div>\n");
        }

        if let Some(n) = next {
            let title = Self::escape_html(Self::page_title(n));
            let href = Self::compute_relative_href(current_slug, &n.slug);
            html.push_str(&format!(
                "  <a href=\"{}\" class=\"pagination-card next-card\">\n    <div class=\"pagination-text\">\n      <span class=\"pagination-label\">Next</span>\n      <span class=\"pagination-title\">{}</span>\n    </div>\n    <svg class=\"docs-icon pagination-icon\" aria-hidden=\"true\"><use href=\"#icon-arrow-right\"></use></svg>\n  </a>\n",
                href, title
            ));
        } else {
            html.push_str("  <div></div>\n");
        }

        html.push_str("</nav>\n");
        html
    }

    #[cfg(test)]
    fn render_pagination(site_ast: &DocSiteAst, current_slug: &str) -> String {
        let pages = &site_ast.pages;
        let Some(index) = pages.iter().position(|page| page.slug == current_slug) else {
            return String::new();
        };
        Self::render_pagination_at(pages, index, current_slug)
    }

    fn render_toc(page: &DocPage) -> String {
        if page.toc.is_empty() {
            return String::new();
        }
        let mut toc = String::new();
        for entry in &page.toc {
            toc.push_str(&format!(
                "<a href=\"#{}\" class=\"toc-link toc-level-{}\"><span>{}</span></a>\n",
                entry.anchor, entry.level, entry.text
            ));
        }
        toc
    }

    #[allow(clippy::too_many_arguments)]
    fn render_diagram(
        kind: &evin_doc::ast::DiagramKind,
        code: &str,
        title: Option<&str>,
        caption: Option<&str>,
        alt: Option<&str>,
        rendered_mermaid: Option<&evin_doc::ast::RenderedMermaid>,
        registry: &ComponentRegistry,
        used_tracker: &mut HashSet<String>,
    ) -> String {
        let display_title = if let Some(title) = title {
            Self::escape_html(title)
        } else {
            match kind {
                evin_doc::ast::DiagramKind::Svgbob => "Svgbob".to_string(),
                evin_doc::ast::DiagramKind::Mermaid => "Mermaid".to_string(),
                _ => "Diagram".to_string(),
            }
        };

        let filename = title
            .map(|title| {
                let slug: String = title
                    .chars()
                    .map(|character| {
                        if character.is_alphanumeric() || character == '-' {
                            character
                        } else {
                            '_'
                        }
                    })
                    .collect();
                let trimmed = slug.trim_matches('_');
                if trimmed.is_empty() {
                    "diagram.svg".to_string()
                } else {
                    format!("{trimmed}.svg")
                }
            })
            .unwrap_or_else(|| "diagram.svg".to_string());

        let caption_html = caption
            .map(|caption| {
                format!(
                    "<p class=\"docs-diagram-caption diagram-caption\">{}</p>",
                    Self::escape_html(caption)
                )
            })
            .unwrap_or_default();
        let alt_attr = alt
            .map(|alt| format!(" aria-label=\"{}\" role=\"img\"", Self::escape_html(alt)))
            .unwrap_or_default();

        match kind {
            evin_doc::ast::DiagramKind::Svgbob => {
                used_tracker.insert("diagram_svgbob".to_string());
                let sanitized_code = evin_doc::render::sanitize_svgbob_code(code);
                let settings = svgbob::Settings {
                    include_styles: false,
                    include_backdrop: false,
                    ..Default::default()
                };
                let svg = svgbob::to_svg_with_settings(&sanitized_code, &settings);
                registry.render(
                    "diagram_svgbob",
                    &[
                        ("svg", &svg),
                        ("display_title", &display_title),
                        ("filename", &filename),
                        ("caption_html", &caption_html),
                        ("alt_attr", &alt_attr),
                        ("id_attr", ""),
                    ],
                    used_tracker,
                )
            }
            evin_doc::ast::DiagramKind::Mermaid => {
                used_tracker.insert("diagram_mermaid".to_string());
                let escaped_code = Self::escape_html(code);
                let rendered = rendered_mermaid
                    .expect("web Mermaid nodes must be compiled before HTML rendering");
                registry.render(
                    "diagram_mermaid",
                    &[
                        ("code", &escaped_code),
                        ("svg", &rendered.svg),
                        ("display_title", &display_title),
                        ("filename", &filename),
                        ("caption_html", &caption_html),
                        ("alt_attr", &alt_attr),
                        ("id_attr", ""),
                    ],
                    used_tracker,
                )
            }
            evin_doc::ast::DiagramKind::Pikchr => {
                let mut output = String::from("<pre class=\"language-pikchr\"><code>");
                push_escaped_html(&mut output, code);
                output.push_str("</code></pre>");
                output
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_nodes(
        nodes: &[DocNode],
        page: &DocPage,
        site_ast: &DocSiteAst,
        context: &PreparedRenderContext<'_>,
        registry: &ComponentRegistry,
        highlighter: &crate::v1::syntax::SyntaxHighlighter,
        used_tracker: &mut HashSet<String>,
        out: &mut String,
    ) {
        for node in nodes {
            match node {
                DocNode::Heading {
                    level,
                    text,
                    children,
                    anchor,
                } => {
                    let mut heading_html = String::new();
                    let is_page_title = page
                        .metadata
                        .get("title")
                        .and_then(|value| value.as_str())
                        .is_some_and(|title| title == text);
                    if is_page_title {
                        let badge = page.metadata.get("badge").and_then(|value| value.as_str());
                        let reference = page.reference.as_ref();

                        if badge.is_some() || reference.is_some() {
                            let mut eyebrow_content = String::new();
                            if let Some(badge) = badge {
                                eyebrow_content.push_str("<span class=\"document-badge\">");
                                eyebrow_content.push_str(&Self::escape_html(badge));
                                eyebrow_content.push_str("</span>");
                            }
                            if let Some(reference) = reference {
                                eyebrow_content.push_str(
                                    "<span class=\"document-badge formal-reference-prefix\">",
                                );
                                eyebrow_content.push_str(&Self::escape_html(&reference.id));
                                eyebrow_content.push_str("</span>");
                            }
                            let rendered_eyebrow = registry.render(
                                "document_eyebrow",
                                &[("content", &eyebrow_content)],
                                used_tracker,
                            );
                            if !rendered_eyebrow.is_empty() {
                                out.push_str(&rendered_eyebrow);
                            } else {
                                out.push_str("<div class=\"document-eyebrow\">");
                                out.push_str(&eyebrow_content);
                                out.push_str("</div>");
                            }
                            out.push('\n');
                        }
                    }
                    for c in children {
                        Self::render_inline(
                            c,
                            page,
                            site_ast,
                            context,
                            used_tracker,
                            &mut heading_html,
                        );
                    }
                    let level_str = level.to_string();
                    registry.render_to(
                        "heading",
                        &[
                            ("level", &level_str),
                            ("anchor", anchor),
                            ("content", &heading_html),
                        ],
                        used_tracker,
                        out,
                    );
                    out.push('\n');
                }
                DocNode::Paragraph { children } => {
                    let mut paragraph_html = String::new();
                    for c in children {
                        Self::render_inline(
                            c,
                            page,
                            site_ast,
                            context,
                            used_tracker,
                            &mut paragraph_html,
                        );
                    }
                    registry.render_to(
                        "paragraph",
                        &[("content", &paragraph_html)],
                        used_tracker,
                        out,
                    );
                    out.push('\n');
                }
                DocNode::Media {
                    kind,
                    src,
                    alt,
                    title,
                    poster,
                } => {
                    let mut media = StartPageMedia {
                        kind: kind.clone(),
                        src: src.clone(),
                        alt: alt.clone().unwrap_or_default(),
                        title: title.clone(),
                        poster: poster.clone(),
                    };
                    if media.kind == MediaKind::Image
                        && let Some(url) = context.asset_url(&media.src, &page.slug)
                    {
                        media.src = url;
                    }
                    Self::render_media(out, "document-media", &media);
                    out.push('\n');
                }
                DocNode::RawHtml { .. } => {}
                DocNode::CodeBlock {
                    language,
                    title,
                    code,
                } => {
                    let data_lang = language
                        .as_ref()
                        .map(|l| format!(" data-language=\"{}\"", Self::escape_html(l)))
                        .unwrap_or_default();
                    let (display_lang, extension) =
                        Self::format_language_details(language.as_deref());
                    let display_title = title.as_deref().map(Self::escape_html).unwrap_or_default();
                    let highlighted_code = highlighter.highlight(language.as_deref(), code);
                    registry.render_to(
                        "code_block",
                        &[
                            ("data_language", &data_lang),
                            ("display_title", &display_title),
                            ("display_language", &display_lang),
                            ("extension", &extension),
                            ("code", &highlighted_code),
                        ],
                        used_tracker,
                        out,
                    );
                    out.push('\n');
                }
                DocNode::Tape {
                    script,
                    config,
                    rendered_svg,
                } => {
                    let svg = rendered_svg.as_deref().unwrap_or(
                        "<p class=\"terminal-tape-error\">Terminal tape was not compiled.</p>",
                    );
                    let title = config.title.as_deref().unwrap_or("Terminal recording");
                    let runtime_label = match config.runtime {
                        TapeRuntime::Esh => "ESH",
                        TapeRuntime::System => "System shell",
                    };
                    let copy_script = TapeScript::parse(script)
                        .map(|script| script.to_shell_script())
                        .unwrap_or_default();
                    let copy_disabled = if copy_script.is_empty() {
                        "disabled"
                    } else {
                        ""
                    };
                    let copy_script = Self::escape_html(&copy_script);
                    registry.render_to(
                        "tape",
                        &[
                            ("svg", svg),
                            ("title", title),
                            ("runtime_label", runtime_label),
                            ("copy_script", &copy_script),
                            ("copy_disabled", copy_disabled),
                        ],
                        used_tracker,
                        out,
                    );
                    out.push('\n');
                }
                DocNode::Admonition {
                    kind,
                    title,
                    children,
                } => {
                    let k_lower = kind.to_ascii_lowercase();
                    if k_lower == "quote" || k_lower == "blockquote" {
                        out.push_str("<blockquote class=\"doc-blockquote\">\n");
                        Self::render_nodes(
                            children,
                            page,
                            site_ast,
                            context,
                            registry,
                            highlighter,
                            used_tracker,
                            out,
                        );
                        out.push_str("</blockquote>\n");
                    } else {
                        let normalized_kind = match k_lower.as_str() {
                            "tip" | "hint" => "tip",
                            "important" => "important",
                            "warning" | "attention" => "warning",
                            "caution" | "danger" | "error" => "caution",
                            _ => "note",
                        };

                        let icon_name = match normalized_kind {
                            "tip" => "lightbulb",
                            "important" => "alert-circle",
                            "warning" => "warning",
                            "caution" => "alert-octagon",
                            _ => "info",
                        };

                        let default_title = match k_lower.as_str() {
                            "tip" => "Tip",
                            "hint" => "Hint",
                            "important" => "Important",
                            "warning" => "Warning",
                            "attention" => "Attention",
                            "caution" => "Caution",
                            "danger" => "Danger",
                            "error" => "Error",
                            "seealso" => "See Also",
                            "note" => "Note",
                            other => {
                                let mut chars = other.chars();
                                match chars.next() {
                                    None => "Note",
                                    Some(f) => {
                                        &(f.to_uppercase().collect::<String>() + chars.as_str())
                                    }
                                }
                            }
                        };

                        let title_text = title.as_deref().unwrap_or(default_title);
                        let mut body_html = String::new();
                        Self::render_nodes(
                            children,
                            page,
                            site_ast,
                            context,
                            registry,
                            highlighter,
                            used_tracker,
                            &mut body_html,
                        );
                        registry.render_to(
                            "callout",
                            &[
                                ("kind", normalized_kind),
                                ("icon", icon_name),
                                ("title", title_text),
                                ("content", &body_html),
                            ],
                            used_tracker,
                            out,
                        );
                        out.push('\n');
                    }
                }
                DocNode::List {
                    ordered,
                    numbering,
                    start,
                    items,
                } => {
                    used_tracker.insert("list".to_string());
                    if *ordered {
                        let type_attr = match numbering {
                            evin_doc::NumberingKind::Arabic => "arabic",
                            evin_doc::NumberingKind::LowerAlpha => "lower-alpha",
                            evin_doc::NumberingKind::UpperAlpha => "upper-alpha",
                            evin_doc::NumberingKind::LowerRoman => "lower-roman",
                            evin_doc::NumberingKind::UpperRoman => "upper-roman",
                        };
                        out.push_str(&format!(
                            "<ol class=\"doc-list-ol\" data-type=\"{}\" style=\"--ol-start: {};\" data-c-list>\n",
                            type_attr, start
                        ));
                        for item in items {
                            out.push_str("  <li class=\"ol-item\"><span class=\"ol-content\">");
                            if item.len() == 1 {
                                if let DocNode::Paragraph { children } = &item[0] {
                                    for c in children {
                                        Self::render_inline(
                                            c,
                                            page,
                                            site_ast,
                                            context,
                                            used_tracker,
                                            out,
                                        );
                                    }
                                } else {
                                    Self::render_nodes(
                                        item,
                                        page,
                                        site_ast,
                                        context,
                                        registry,
                                        highlighter,
                                        used_tracker,
                                        out,
                                    );
                                }
                            } else {
                                Self::render_nodes(
                                    item,
                                    page,
                                    site_ast,
                                    context,
                                    registry,
                                    highlighter,
                                    used_tracker,
                                    out,
                                );
                            }
                            out.push_str("</span></li>\n");
                        }
                        out.push_str("</ol>\n");
                    } else {
                        out.push_str("<ul class=\"doc-list\" data-c-list>\n");
                        for item in items {
                            out.push_str("  <li>");
                            if item.len() == 1 {
                                if let DocNode::Paragraph { children } = &item[0] {
                                    for c in children {
                                        Self::render_inline(
                                            c,
                                            page,
                                            site_ast,
                                            context,
                                            used_tracker,
                                            out,
                                        );
                                    }
                                } else {
                                    Self::render_nodes(
                                        item,
                                        page,
                                        site_ast,
                                        context,
                                        registry,
                                        highlighter,
                                        used_tracker,
                                        out,
                                    );
                                }
                            } else {
                                Self::render_nodes(
                                    item,
                                    page,
                                    site_ast,
                                    context,
                                    registry,
                                    highlighter,
                                    used_tracker,
                                    out,
                                );
                            }
                            out.push_str("</li>\n");
                        }
                        out.push_str("</ul>\n");
                    }
                }
                DocNode::TaskList { items } => {
                    used_tracker.insert("list".to_string());
                    out.push_str("<ul class=\"doc-list-task\" data-c-list>\n");
                    for item in items {
                        let checked_class = if item.checked { " is-checked" } else { "" };
                        let checked_attr = if item.checked { " checked" } else { "" };
                        out.push_str(&format!("  <li class=\"task-item{}\">\n", checked_class));
                        out.push_str(&format!(
                            "    <input type=\"checkbox\"{} disabled class=\"task-checkbox\" aria-hidden=\"true\" />\n",
                            checked_attr
                        ));
                        out.push_str("    <span class=\"task-label\">");
                        if item.body.len() == 1 {
                            if let DocNode::Paragraph { children } = &item.body[0] {
                                for c in children {
                                    Self::render_inline(
                                        c,
                                        page,
                                        site_ast,
                                        context,
                                        used_tracker,
                                        out,
                                    );
                                }
                            } else {
                                Self::render_nodes(
                                    &item.body,
                                    page,
                                    site_ast,
                                    context,
                                    registry,
                                    highlighter,
                                    used_tracker,
                                    out,
                                );
                            }
                        } else {
                            Self::render_nodes(
                                &item.body,
                                page,
                                site_ast,
                                context,
                                registry,
                                highlighter,
                                used_tracker,
                                out,
                            );
                        }
                        out.push_str("</span>\n");
                        out.push_str("  </li>\n");
                    }
                    out.push_str("</ul>\n");
                }
                DocNode::DefinitionList { items } => {
                    used_tracker.insert("list".to_string());
                    out.push_str("<dl class=\"doc-list-def\" data-c-list>\n");
                    for item in items {
                        out.push_str("  <div class=\"def-entry\">\n");
                        out.push_str("    <dt class=\"def-term\">\n");
                        out.push_str("      <code class=\"term-name\">");
                        for t in &item.term {
                            Self::render_inline(t, page, site_ast, context, used_tracker, out);
                        }
                        out.push_str("</code>\n");
                        for c in &item.classifiers {
                            let plain_c = c.iter().map(|n| n.plain_text()).collect::<String>();
                            let opt_class = if plain_c.to_lowercase().contains("optional") {
                                " def-classifier-opt"
                            } else {
                                ""
                            };
                            out.push_str(&format!(
                                "      <span class=\"def-classifier{}\">",
                                opt_class
                            ));
                            for ci in c {
                                Self::render_inline(ci, page, site_ast, context, used_tracker, out);
                            }
                            out.push_str("</span>\n");
                        }
                        out.push_str("    </dt>\n");
                        out.push_str("    <dd class=\"def-body\">\n");
                        Self::render_nodes(
                            &item.body,
                            page,
                            site_ast,
                            context,
                            registry,
                            highlighter,
                            used_tracker,
                            out,
                        );
                        out.push_str("    </dd>\n");
                        out.push_str("  </div>\n");
                    }
                    out.push_str("</dl>\n");
                }
                DocNode::OptionList { items } => {
                    used_tracker.insert("list".to_string());
                    out.push_str("<dl class=\"doc-list-options\" data-c-list>\n");
                    for item in items {
                        out.push_str("  <div class=\"option-group\">\n");
                        out.push_str("    <dt class=\"option-synopsis\">\n      ");
                        for (f_idx, flag) in item.flags.iter().enumerate() {
                            if f_idx > 0 {
                                out.push_str(", ");
                            }
                            out.push_str("<kbd class=\"option-flag\">");
                            push_escaped_html(out, flag);
                            out.push_str("</kbd>");
                        }
                        if let Some(arg) = &item.argument {
                            out.push_str(" <var class=\"option-arg\">");
                            push_escaped_html(out, arg);
                            out.push_str("</var>");
                        }
                        out.push_str("\n    </dt>\n");
                        out.push_str("    <dd class=\"option-description\">\n");
                        Self::render_nodes(
                            &item.body,
                            page,
                            site_ast,
                            context,
                            registry,
                            highlighter,
                            used_tracker,
                            out,
                        );
                        out.push_str("    </dd>\n");
                        out.push_str("  </div>\n");
                    }
                    out.push_str("</dl>\n");
                }
                DocNode::FieldList { items } => {
                    used_tracker.insert("list".to_string());
                    out.push_str("<dl class=\"doc-list-fields\" data-c-list>\n");
                    for item in items {
                        out.push_str("  <div class=\"field-row\">\n");
                        out.push_str("    <dt class=\"field-name\">");
                        push_escaped_html(out, &item.name);
                        out.push_str("</dt>\n");
                        out.push_str("    <dd class=\"field-body\">");
                        Self::render_nodes(
                            &item.body,
                            page,
                            site_ast,
                            context,
                            registry,
                            highlighter,
                            used_tracker,
                            out,
                        );
                        out.push_str("</dd>\n");
                        out.push_str("  </div>\n");
                    }
                    out.push_str("</dl>\n");
                }
                DocNode::Stepper { steps } => {
                    used_tracker.insert("list".to_string());
                    out.push_str("<ol class=\"doc-list-stepper\" data-c-list>\n");
                    for (idx, step) in steps.iter().enumerate() {
                        let is_done = step
                            .status
                            .as_deref()
                            .map(|s| s == "done" || s == "completed")
                            .unwrap_or(false);
                        let is_active = step
                            .status
                            .as_deref()
                            .map(|s| s == "active" || s == "in-progress" || s == "current")
                            .unwrap_or(false);

                        let node_class = if is_done {
                            "stepper-node is-done"
                        } else if is_active {
                            "stepper-node is-active"
                        } else {
                            "stepper-node"
                        };

                        out.push_str("  <li class=\"");
                        out.push_str(node_class);
                        out.push_str("\">\n");

                        out.push_str("    <div class=\"stepper-badge\"");
                        if is_done {
                            out.push_str(" title=\"Completed\">\n");
                            out.push_str("      <svg class=\"docs-icon\" aria-hidden=\"true\"><use href=\"#icon-check\"></use></svg>\n");
                        } else {
                            out.push_str(">\n      ");
                            if let Some(badge_str) = &step.badge {
                                push_escaped_html(out, badge_str);
                            } else {
                                out.push_str(&(idx + 1).to_string());
                            }
                            out.push('\n');
                        }
                        out.push_str("    </div>\n");

                        out.push_str("    <div class=\"stepper-body-card\">\n");
                        out.push_str("      <div class=\"stepper-header\">\n");
                        out.push_str("        <span class=\"stepper-title\">");
                        for t in &step.title {
                            Self::render_inline(t, page, site_ast, context, used_tracker, out);
                        }
                        out.push_str("</span>\n");

                        if let Some(status) = &step.status {
                            let pill_class = if is_done {
                                "stepper-pill done"
                            } else if is_active {
                                "stepper-pill active"
                            } else {
                                "stepper-pill"
                            };
                            let label = match status.as_str() {
                                "done" | "completed" => "Done",
                                "active" | "in-progress" | "current" => "In Progress",
                                "upcoming" => "Upcoming",
                                other => other,
                            };
                            out.push_str("        <span class=\"");
                            out.push_str(pill_class);
                            out.push_str("\">");
                            push_escaped_html(out, label);
                            out.push_str("</span>\n");
                        }
                        out.push_str("      </div>\n");

                        if !step.body.is_empty() {
                            out.push_str("      <div class=\"stepper-desc\">\n");
                            Self::render_nodes(
                                &step.body,
                                page,
                                site_ast,
                                context,
                                registry,
                                highlighter,
                                used_tracker,
                                out,
                            );
                            out.push_str("      </div>\n");
                        }

                        out.push_str("    </div>\n");
                        out.push_str("  </li>\n");
                    }
                    out.push_str("</ol>\n");
                }
                DocNode::HorizontalRule => {
                    registry.render_to("horizontal_rule", &[], used_tracker, out);
                    out.push('\n');
                }
                DocNode::Table { headers, rows } => {
                    let mut header_html = String::new();
                    if !headers.is_empty() {
                        header_html.push_str("  <thead>\n    <tr>\n");
                        for header in headers {
                            header_html.push_str("      <th>");
                            for cell in header {
                                Self::render_inline(
                                    cell,
                                    page,
                                    site_ast,
                                    context,
                                    used_tracker,
                                    &mut header_html,
                                );
                            }
                            header_html.push_str("</th>\n");
                        }
                        header_html.push_str("    </tr>\n  </thead>\n");
                    }
                    let mut rows_html = String::new();
                    for row in rows {
                        rows_html.push_str("    <tr>\n");
                        for cell in row {
                            rows_html.push_str("      <td>");
                            for inl in cell {
                                Self::render_inline(
                                    inl,
                                    page,
                                    site_ast,
                                    context,
                                    used_tracker,
                                    &mut rows_html,
                                );
                            }
                            rows_html.push_str("</td>\n");
                        }
                        rows_html.push_str("    </tr>\n");
                    }
                    registry.render_to(
                        "table",
                        &[("header_html", &header_html), ("rows_html", &rows_html)],
                        used_tracker,
                        out,
                    );
                    out.push('\n');
                }
                DocNode::Directive { name, args, body } => {
                    if name == "contents" {
                        if !page.toc.is_empty() {
                            let title = if args.is_empty() {
                                "Table of Contents"
                            } else {
                                &args[0]
                            };
                            let mut items = String::new();
                            for entry in &page.toc {
                                let level_class = match entry.level {
                                    1 | 2 => "level-2",
                                    3 => "level-3",
                                    _ => "level-4",
                                };
                                items.push_str(&format!(
                                    "    <li class=\"toc-item {}\"><a href=\"#{}\">{}</a></li>\n",
                                    level_class,
                                    entry.anchor,
                                    Self::escape_html(&entry.text)
                                ));
                            }
                            let escaped_title = Self::escape_html(title);
                            registry.render_to(
                                "toc_block",
                                &[("title", &escaped_title), ("items", &items)],
                                used_tracker,
                                out,
                            );
                            out.push('\n');
                        }
                    } else if name == "toctree" {
                        let title = if args.is_empty() {
                            "Document Navigation"
                        } else {
                            &args[0]
                        };
                        let mut items = String::new();
                        for child in body {
                            if let DocNode::Paragraph { children } = child {
                                for inline in children {
                                    if let InlineNode::Link { href, content, .. } = inline {
                                        let href =
                                            Self::resolve_rendered_href(context, &page.slug, href);
                                        items.push_str(&format!(
                                            "    <li class=\"toc-item level-2\"><a href=\"{}\">{}</a></li>\n",
                                            Self::escape_html(&href), Self::escape_html(content)
                                        ));
                                    }
                                }
                            }
                        }
                        let escaped_title = Self::escape_html(title);
                        registry.render_to(
                            "toc_block",
                            &[("title", &escaped_title), ("items", &items)],
                            used_tracker,
                            out,
                        );
                        out.push('\n');
                    } else {
                        let template_name = component_template_name(name);
                        if registry.has_component(&template_name) {
                            let arg_text = args.join(" ");
                            let mut body_html = String::new();
                            Self::render_nodes(
                                body,
                                page,
                                site_ast,
                                context,
                                registry,
                                highlighter,
                                used_tracker,
                                &mut body_html,
                            );
                            let first_arg = args.first().map(|s| s.as_str()).unwrap_or("");
                            let title_val = if arg_text.is_empty() {
                                name.as_str()
                            } else {
                                &arg_text
                            };
                            registry.render_to(
                                &template_name,
                                &[
                                    ("title", title_val),
                                    ("args", &arg_text),
                                    ("arg0", first_arg),
                                    ("content", &body_html),
                                ],
                                used_tracker,
                                out,
                            );
                            out.push('\n');
                        } else {
                            let arg_text = args.join(" ");
                            let mut body_html = String::new();
                            Self::render_nodes(
                                body,
                                page,
                                site_ast,
                                context,
                                registry,
                                highlighter,
                                used_tracker,
                                &mut body_html,
                            );
                            let title_val = if arg_text.is_empty() {
                                name.clone()
                            } else {
                                format!("{}: {}", name, arg_text)
                            };
                            registry.render_to(
                                "callout",
                                &[("title", &title_val), ("content", &body_html)],
                                used_tracker,
                                out,
                            );
                            out.push('\n');
                        }
                    }
                }
                DocNode::Diagram {
                    kind,
                    code,
                    title,
                    caption,
                    alt,
                    rendered_mermaid,
                } => {
                    out.push_str(&Self::render_diagram(
                        kind,
                        code,
                        title.as_deref(),
                        caption.as_deref(),
                        alt.as_deref(),
                        rendered_mermaid.as_ref(),
                        registry,
                        used_tracker,
                    ));
                    out.push('\n');
                }
                DocNode::MathBlock { latex } => {
                    used_tracker.insert("math".to_string());
                    let html = Self::render_math_to_html(latex, latex2mathml::DisplayStyle::Block);
                    out.push_str(&html);
                    out.push('\n');
                }
                DocNode::LspSymbol {
                    name,
                    synopsis,
                    summary,
                    category,
                    children,
                    ..
                } => {
                    let symbol_id =
                        format!("symbol-{}", name.trim().to_lowercase().replace(' ', "-"));
                    out.push_str(&format!(
                        "<div class=\"docs-lsp-symbol\" id=\"{}\">\n",
                        symbol_id
                    ));
                    out.push_str("  <div class=\"docs-lsp-header\">\n");
                    out.push_str(&format!(
                        "    <h3 class=\"docs-lsp-name\"><code>{}</code></h3>\n",
                        name
                    ));
                    if !category.is_empty() {
                        out.push_str(&format!(
                            "    <span class=\"docs-lsp-category badge\">{}</span>\n",
                            category
                        ));
                    }
                    out.push_str("  </div>\n");

                    if !synopsis.is_empty() {
                        out.push_str("  <div class=\"docs-lsp-synopsis\">\n");
                        let highlighted = highlighter.highlight(Some("esh"), synopsis);
                        out.push_str(&format!(
                            "    <pre class=\"syn-container\"><code>{}</code></pre>\n",
                            highlighted
                        ));
                        out.push_str("  </div>\n");
                    }

                    if !summary.is_empty() {
                        out.push_str(&format!(
                            "  <p class=\"docs-lsp-summary\"><strong>{}</strong>: {}</p>\n",
                            name, summary
                        ));
                    }

                    if !children.is_empty() {
                        out.push_str("  <div class=\"docs-lsp-body\">\n");
                        Self::render_nodes(
                            children,
                            page,
                            site_ast,
                            context,
                            registry,
                            highlighter,
                            used_tracker,
                            out,
                        );
                        out.push_str("  </div>\n");
                    }

                    out.push_str("</div>\n\n");
                }
            }
        }
    }

    pub fn collect_site_used_components(
        site_ast: &DocSiteAst,
        start_page: Option<&StartPageModel>,
    ) -> HashSet<String> {
        let mut used = HashSet::new();
        if start_page.is_some() {
            used.insert("start_page".to_string());
        }
        if site_ast.pages.len() > 1 {
            used.insert("pagination".to_string());
        }
        for page in &site_ast.pages {
            if crate::v1::is_metrics_page(page) {
                used.insert("start_page".to_string());
                used.insert("metrics_page".to_string());
            }
            walk_nodes(&page.ast, &mut ComponentCollector { used: &mut used });
        }
        used
    }

    #[allow(clippy::only_used_in_recursion)]
    fn render_inline(
        inline: &InlineNode,
        page: &DocPage,
        site_ast: &DocSiteAst,
        context: &PreparedRenderContext<'_>,
        used_tracker: &mut HashSet<String>,
        out: &mut String,
    ) {
        match inline {
            InlineNode::Text { content } => push_escaped_html(out, content),
            InlineNode::Strong { children } => {
                out.push_str("<strong>");
                for child in children {
                    Self::render_inline(child, page, site_ast, context, used_tracker, out);
                }
                out.push_str("</strong>");
            }
            InlineNode::Emphasis { children } => {
                out.push_str("<em>");
                for child in children {
                    Self::render_inline(child, page, site_ast, context, used_tracker, out);
                }
                out.push_str("</em>");
            }
            InlineNode::Code { content } => {
                out.push_str("<code>");
                push_escaped_html(out, content);
                out.push_str("</code>");
            }
            InlineNode::Link {
                href,
                title,
                content,
                code,
            } => {
                let href = Self::resolve_rendered_href(context, &page.slug, href);
                out.push_str("<a href=\"");
                out.push_str(&Self::escape_html(&href));
                out.push('"');
                if let Some(t) = title {
                    out.push_str(" title=\"");
                    out.push_str(t);
                    out.push('"');
                }
                out.push('>');
                if *code {
                    out.push_str("<code>");
                }
                push_escaped_html(out, content);
                if *code {
                    out.push_str("</code>");
                }
                out.push_str("</a>");
            }
            InlineNode::Image { src, alt, title } => {
                let src = context
                    .asset_url(src, &page.slug)
                    .unwrap_or_else(|| src.clone());
                out.push_str("<img src=\"");
                Self::escape_into(out, &src);
                out.push_str("\" alt=\"");
                Self::escape_into(out, alt);
                out.push('"');
                if let Some(t) = title {
                    out.push_str(" title=\"");
                    Self::escape_into(out, t);
                    out.push('"');
                }
                out.push_str(" />");
            }
            InlineNode::Math { latex } => {
                used_tracker.insert("math".to_string());
                let html = Self::render_math_to_html(latex, latex2mathml::DisplayStyle::Inline);
                out.push_str(&html);
            }
        }
    }

    pub fn normalize_latex(latex: &str) -> String {
        latex
            .replace(r"\begin{cases}", r"\left\{ \begin{matrix}")
            .replace(r"\end{cases}", r"\end{matrix} \right.")
    }

    pub fn render_math_to_html(latex: &str, display: latex2mathml::DisplayStyle) -> String {
        let normalized = Self::normalize_latex(latex);
        match latex2mathml::latex_to_mathml(&normalized, display) {
            Ok(mathml) => match display {
                latex2mathml::DisplayStyle::Inline => {
                    format!(
                        "<span class=\"math-inline docs-math-inline\">{}</span>",
                        mathml
                    )
                }
                latex2mathml::DisplayStyle::Block => {
                    format!("<div class=\"math-block docs-math-block\">{}</div>", mathml)
                }
            },
            Err(_) => {
                let mut escaped = String::new();
                push_escaped_html(&mut escaped, latex);
                match display {
                    latex2mathml::DisplayStyle::Inline => {
                        format!(
                            "<code class=\"math-fallback docs-math-fallback\">{}</code>",
                            escaped
                        )
                    }
                    latex2mathml::DisplayStyle::Block => {
                        format!(
                            "<div class=\"math-block docs-math-block\"><code class=\"math-fallback docs-math-fallback math-fallback-block docs-math-fallback-block\">{}</code></div>",
                            escaped
                        )
                    }
                }
            }
        }
    }

    pub fn escape_html(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        push_escaped_html(&mut out, s);
        out
    }

    pub fn format_language_details(lang: Option<&str>) -> (String, String) {
        let raw = lang.unwrap_or("").trim();
        if raw.is_empty() {
            return ("Text".to_string(), "txt".to_string());
        }
        match raw.to_ascii_lowercase().as_str() {
            "rs" | "rust" => ("Rust".to_string(), "rs".to_string()),
            "esh" | "evin-shell" | "esh-script" => ("ESH".to_string(), "esh".to_string()),
            "sh" | "bash" | "zsh" | "shell" => ("Bash".to_string(), "sh".to_string()),
            "yml" | "yaml" => ("YAML".to_string(), "yaml".to_string()),
            "json" => ("JSON".to_string(), "json".to_string()),
            "toml" => ("TOML".to_string(), "toml".to_string()),
            "js" | "javascript" => ("JavaScript".to_string(), "js".to_string()),
            "ts" | "typescript" => ("TypeScript".to_string(), "ts".to_string()),
            "py" | "python" => ("Python".to_string(), "py".to_string()),
            "html" => ("HTML".to_string(), "html".to_string()),
            "css" => ("CSS".to_string(), "css".to_string()),
            "scss" => ("SCSS".to_string(), "scss".to_string()),
            "sql" => ("SQL".to_string(), "sql".to_string()),
            "c" => ("C".to_string(), "c".to_string()),
            "cpp" | "c++" => ("C++".to_string(), "cpp".to_string()),
            "go" => ("Go".to_string(), "go".to_string()),
            "md" | "markdown" => ("Markdown".to_string(), "md".to_string()),
            "rst" => ("reST".to_string(), "rst".to_string()),
            "xml" => ("XML".to_string(), "xml".to_string()),
            "ebnf" => ("EBNF".to_string(), "ebnf".to_string()),
            other => {
                let mut c = other.chars();
                let cap = match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                };
                (cap, other.to_string())
            }
        }
    }

    fn resolve_rendered_href(
        context: &PreparedRenderContext<'_>,
        current_slug: &str,
        href: &str,
    ) -> String {
        let Some(target) = context.index.resolve_alias(href) else {
            return href.to_string();
        };
        let mut resolved = Self::compute_relative_href(current_slug, target.slug);
        if let Some(anchor) = target.anchor {
            resolved.push('#');
            resolved.push_str(anchor);
        }
        resolved
    }

    fn render_reference_relations(context: &PreparedRenderContext<'_>, page: &DocPage) -> String {
        let Some(reference) = &page.reference else {
            return String::new();
        };
        if reference.depends_on.is_empty() && reference.referenced_by.is_empty() {
            return String::new();
        }
        let mut html = String::from(
            "<aside class=\"formal-reference-relations\" aria-label=\"Reference links\">\n",
        );
        for (label, identifiers) in [
            ("References", &reference.depends_on),
            ("Referenced by", &reference.referenced_by),
        ] {
            if identifiers.is_empty() {
                continue;
            }
            html.push_str(
                "<section class=\"formal-reference-group\"><h2 class=\"formal-reference-label\">",
            );
            html.push_str(label);
            html.push_str("</h2><ul class=\"formal-reference-list\">");
            for identifier in identifiers {
                if let Some(target) = context.index.resolve_alias(identifier) {
                    let target_page = context.index.page(context.site, target.slug);
                    let title = target_page
                        .map(Self::display_title)
                        .unwrap_or_else(|| identifier.clone());
                    html.push_str("<li><a href=\"");
                    html.push_str(&Self::compute_relative_href(&page.slug, target.slug));
                    html.push_str("\">");
                    html.push_str(&Self::escape_html(&title));
                    html.push_str("</a></li>");
                }
            }
            html.push_str("</ul></section>\n");
        }
        html.push_str("</aside>\n");
        html
    }

    fn display_title(page: &DocPage) -> String {
        let title = Self::page_title(page);
        page.reference.as_ref().map_or_else(
            || title.to_string(),
            |reference| format!("{} — {}", reference.id, title),
        )
    }

    fn page_title(page: &DocPage) -> &str {
        page.metadata
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or(&page.slug)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::LAYOUTS_DIR;
    use crate::v1::ast::{DocFormat, FormalReference, TocEntry};

    fn formal_reference(id: &str, number: u64) -> FormalReference {
        FormalReference {
            id: id.to_string(),
            digits: id.split_once('-').unwrap().1.to_string(),
            number,
            depends_on: vec![],
            referenced_by: vec![],
        }
    }

    fn seo(site: &DocSiteAst) -> SeoCatalog {
        SeoCatalog::build(site, "https://example.com/docs/", "en").unwrap()
    }

    #[test]
    fn formal_references_sort_numerically_and_alias_links_use_descriptive_routes() {
        let page = |slug: &str, title: &str, reference: Option<FormalReference>| DocPage {
            slug: slug.to_string(),
            source_path: format!("{slug}.rst"),
            format: DocFormat::ReStructuredText,
            metadata: HashMap::from([(
                "title".to_string(),
                serde_json::Value::String(title.to_string()),
            )]),
            reference,
            toc: vec![],
            ast: vec![],
        };
        let mut source = page("guides/overview", "Overview", None);
        source.ast = vec![DocNode::Paragraph {
            children: vec![InlineNode::Link {
                href: "REF-4500#task-forms".to_string(),
                title: None,
                content: "Task forms".to_string(),
                code: false,
            }],
        }];
        let mut workspace = page(
            "reference/workspace-yaml",
            "Workspace configuration",
            Some(formal_reference("REF-4500", 4500)),
        );
        workspace.ast = vec![DocNode::Heading {
            level: 2,
            text: "Workspace configuration".to_string(),
            children: vec![InlineNode::Text {
                content: "Workspace configuration".to_string(),
            }],
            anchor: "workspace-configuration".to_string(),
        }];
        workspace.reference.as_mut().unwrap().depends_on = vec!["REF-1000".to_string()];
        let mut cli = page(
            "reference/cli",
            "CLI reference",
            Some(formal_reference("REF-1000", 1000)),
        );
        cli.reference.as_mut().unwrap().referenced_by = vec!["REF-4500".to_string()];
        let site = DocSiteAst {
            version: "1.0".to_string(),
            title: Some("Docs".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![workspace, cli, source],
        };

        let nav = HtmlRenderer::render_nav_for_page(&site, "reference/cli");
        assert!(nav.find("CLI reference").unwrap() < nav.find("Workspace configuration").unwrap());
        assert!(!nav.contains("REF-1000"));
        assert!(!nav.contains("REF-4500"));

        let registry = ComponentRegistry::load_from_layout(&LAYOUTS_DIR, "matter", None);
        let original_site = site.clone();
        let pages = HtmlRenderer::render_site(
            &site,
            "<content></content>",
            "",
            "",
            "",
            &registry,
            false,
            false,
            None,
            None,
            None,
            None,
            &seo(&site),
        );
        assert!(
            pages["guides/overview/index.html"]
                .contains("href=\"../../reference/workspace-yaml/index.html#task-forms\"")
        );
        assert_eq!(
            site, original_site,
            "rendering must not rewrite formal aliases"
        );
        let workspace_html = &pages["reference/workspace-yaml/index.html"];
        assert!(
            workspace_html
                .contains("<span class=\"document-badge formal-reference-prefix\">REF-4500</span>")
        );
        assert!(
            workspace_html
                .contains("<span class=\"heading-content\">Workspace configuration</span>")
        );
        assert!(workspace_html.contains("<h2 class=\"formal-reference-label\">References</h2>"));
        assert!(!workspace_html.contains("Reference relationships"));
        assert!(!workspace_html.contains(">Depends on<"));
        let pagination = HtmlRenderer::render_pagination(&site, "reference/workspace-yaml");
        assert!(pagination.contains("<span class=\"pagination-title\">CLI reference</span>"));
        assert!(!pagination.contains("REF-1000"));
    }

    #[test]
    fn test_document_badge_rendering_from_metadata() {
        let registry = ComponentRegistry::load_from_layout(&LAYOUTS_DIR, "matter", None);
        let page = DocPage {
            slug: "getting-started/installation".to_string(),
            source_path: "getting-started/10.installation.md".to_string(),
            format: DocFormat::Markdown,
            metadata: HashMap::from([
                ("title".to_string(), serde_json::json!("Installation")),
                ("badge".to_string(), serde_json::json!("Chapter 1")),
            ]),
            reference: None,
            toc: Vec::new(),
            ast: vec![DocNode::Heading {
                level: 1,
                text: "Installation".to_string(),
                children: vec![InlineNode::Text {
                    content: "Installation".to_string(),
                }],
                anchor: "installation".to_string(),
            }],
        };
        let site = DocSiteAst {
            version: "1.0.0".to_string(),
            title: Some("Documentation".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![page],
        };
        let pages = HtmlRenderer::render_site(
            &site,
            "<content></content>",
            "",
            "",
            "",
            &registry,
            false,
            false,
            None,
            None,
            None,
            None,
            &seo(&site),
        );
        let html = &pages["getting-started/installation/index.html"];
        assert!(html.contains("<span class=\"document-badge\">Chapter 1</span>"));
    }

    #[test]
    fn navigation_groups_pages_by_route_and_marks_the_active_group() {
        let page = |slug: &str, title: &str| DocPage {
            slug: slug.to_string(),
            source_path: if matches!(slug, "builtins" | "esh") {
                format!("{slug}/index.md")
            } else {
                format!("{slug}.rst")
            },
            format: DocFormat::ReStructuredText,
            metadata: HashMap::from([(
                "title".to_string(),
                serde_json::Value::String(title.to_string()),
            )]),
            reference: None,
            toc: vec![],
            ast: vec![],
        };
        let ordered_page = |slug: &str, title: &str, source_path: &str, nav_order: i64| {
            let mut page = page(slug, title);
            page.source_path = source_path.to_string();
            page.metadata.insert(
                "nav_order".to_string(),
                serde_json::Value::Number(nav_order.into()),
            );
            page
        };
        let site = DocSiteAst {
            version: "1.0".to_string(),
            title: Some("Docs".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![
                page("reference/cli", "CLI"),
                page("beta/second", "Second"),
                page("index", "Docs"),
                page("guides/workspaces", "Workspaces"),
                page("getting-started/install", "Install"),
                ordered_page(
                    "esh/runtime-reference",
                    "ESH Runtime Reference",
                    "esh/100.runtime-reference.md",
                    0,
                ),
                ordered_page("esh", "ESH", "esh/00.index.md", 100),
                ordered_page(
                    "esh/writing-scripts",
                    "Writing ESH scripts",
                    "esh/10.writing-scripts.md",
                    100,
                ),
                page("builtins", "Built-ins"),
                page("alpha/first", "First"),
                page("contributing/development", "Development"),
                page("reference/actions", "Actions"),
            ],
        };

        let html = HtmlRenderer::render_nav_for_page(&site, "reference/cli");

        assert!(!html.contains("nav-home"));
        assert!(html.contains("data-nav-group=\"getting-started\""));
        assert!(html.contains("class=\"nav-group-toggle\""));
        assert!(html.contains("id=\"nav-group-reference\""));
        assert!(html.contains("data-active-group=\"true\""));
        assert!(html.contains("aria-current=\"page\""));

        let position = |needle: &str| html.find(needle).unwrap();
        assert!(position("nav-group-getting-started") < position("nav-group-guides"));
        assert!(position("nav-group-guides") < position("nav-group-esh"));
        assert!(position("nav-group-esh") < position("nav-group-builtins"));
        assert!(position("nav-group-builtins") < position("nav-group-reference"));
        assert!(position("nav-group-reference") < position("nav-group-contributing"));
        assert!(position("nav-group-contributing") < position("nav-group-alpha"));
        assert!(position("nav-group-alpha") < position("nav-group-beta"));
        assert!(position(">Overview</span>") < position(">Writing ESH scripts</span>"));
        assert!(
            position(">Writing ESH scripts</span>") < position(">ESH Runtime Reference</span>")
        );
        assert!(position(">Actions</span>") < position(">CLI</span>"));
    }

    #[test]
    fn multi_nested_hierarchical_navigation_tree_renders_subgroups_and_indented_items() {
        let page = |slug: &str, title: &str| DocPage {
            slug: slug.to_string(),
            source_path: format!("{slug}.md"),
            format: DocFormat::Markdown,
            metadata: HashMap::from([(
                "title".to_string(),
                serde_json::Value::String(title.to_string()),
            )]),
            reference: None,
            toc: vec![],
            ast: vec![],
        };

        let site = DocSiteAst {
            version: "1.0".to_string(),
            title: Some("Nested Docs".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![
                page("book", "The Book"),
                page("book/installation", "Installation"),
                page("book/advanced", "Advanced Topics"),
                page("book/advanced/concurrency", "Concurrency"),
                page("book/advanced/memory/allocators", "Custom Allocators"),
            ],
        };

        let html = HtmlRenderer::render_nav_for_page(&site, "book/advanced/concurrency");

        assert!(html.contains("data-nav-group=\"book\""));
        assert!(html.contains("data-nav-subgroup=\"book/advanced\""));
        assert!(html.contains("data-nav-subgroup=\"book/advanced/memory\""));
        assert!(html.contains("class=\"nav-subgroup-toggle\""));
        assert!(html.contains("style=\"--nav-level: 2;\""));
        assert!(html.contains("style=\"--nav-level: 3;\""));
        assert!(html.contains("active\" aria-current=\"page\""));
        assert!(html.contains(">Overview</span>"));
        assert!(html.contains(">Concurrency</span>"));
        assert!(html.contains(">Custom Allocators</span>"));

        // Subgroups must be placed at the bottom of the list after leaf pages
        let pos_installation = html.find(">Installation</span>").unwrap();
        let pos_advanced_subgroup = html.find(">Advanced Topics</span>").unwrap();
        assert!(pos_installation < pos_advanced_subgroup);
    }

    #[test]
    fn breadcrumbs_link_to_the_section_and_keep_page_titles_contextual() {
        let page = |slug: &str, source_path: &str, title: &str| DocPage {
            slug: slug.to_string(),
            source_path: source_path.to_string(),
            format: DocFormat::Markdown,
            metadata: HashMap::from([(
                "title".to_string(),
                serde_json::Value::String(title.to_string()),
            )]),
            reference: None,
            toc: vec![],
            ast: vec![],
        };
        let site = DocSiteAst {
            version: "1.0".to_string(),
            title: Some("Docs".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![
                page("index", "index.md", "Docs"),
                page("esh", "esh/00.index.md", "ESH"),
                page(
                    "esh/control-flow-and-failures",
                    "esh/30.control-flow-and-failures.md",
                    "Control flow and failures",
                ),
            ],
        };

        let breadcrumbs = HtmlRenderer::render_breadcrumbs(&site, &site.pages[2]);
        assert!(breadcrumbs.contains("<a href=\"../../index.html\">Home</a>"));
        assert!(breadcrumbs.contains("<a href=\"../../esh/index.html\">ESH</a>"));
        assert!(breadcrumbs.ends_with("<span>Control flow and failures</span>"));
    }

    #[test]
    fn section_index_precedes_formal_references_in_navigation_and_pagination() {
        let page = |slug: &str, title: &str, reference: Option<FormalReference>| DocPage {
            slug: slug.to_string(),
            source_path: if slug == "builtins" {
                "builtins/index.rst".to_string()
            } else {
                format!("{slug}.rst")
            },
            format: DocFormat::ReStructuredText,
            metadata: HashMap::from([(
                "title".to_string(),
                serde_json::Value::String(title.to_string()),
            )]),
            reference,
            toc: vec![],
            ast: vec![],
        };
        let mut site = DocSiteAst {
            version: "1.0".to_string(),
            title: Some("Docs".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![
                page(
                    "builtins/version-bumper-action",
                    "Version bumper action",
                    Some(formal_reference("REF-5140", 5140)),
                ),
                page("builtins", "Built-in actions", None),
                page(
                    "builtins/archive-action",
                    "Archive action",
                    Some(formal_reference("REF-5110", 5110)),
                ),
            ],
        };

        crate::v1::reference::ReferenceCatalog::prepare(&mut site).unwrap();
        assert_eq!(
            site.pages
                .iter()
                .map(|page| page.slug.as_str())
                .collect::<Vec<_>>(),
            vec![
                "builtins",
                "builtins/archive-action",
                "builtins/version-bumper-action",
            ]
        );

        let navigation = HtmlRenderer::render_nav_for_page(&site, "builtins/archive-action");
        assert!(
            navigation.find(">Built-in actions</span>").unwrap()
                < navigation.find(">Archive action</span>").unwrap()
        );

        let overview_pagination = HtmlRenderer::render_pagination(&site, "builtins");
        assert!(
            overview_pagination.contains("<span class=\"pagination-title\">Archive action</span>")
        );
        assert!(!overview_pagination.contains("Previous</span>"));

        let action_pagination = HtmlRenderer::render_pagination(&site, "builtins/archive-action");
        assert!(
            action_pagination.contains("<span class=\"pagination-title\">Built-in actions</span>")
        );
        assert!(
            action_pagination
                .contains("<span class=\"pagination-title\">Version bumper action</span>")
        );
    }

    #[test]
    fn test_shell_template_custom_html_tags_and_legacy_comments() {
        let custom_shell = r#"<!DOCTYPE html>
<html lang="<page-language></page-language>">
<head>
    <page-title></page-title>
    <page-meta></page-meta>
    <css></css>
</head>
<body>
    <site-brand></site-brand>
    <sidebar-nav></sidebar-nav>
    <content></content>
</body>
</html>"#;

        let template = ShellTemplate::parse(custom_shell);
        let slots = PageSlots {
            title: "Test Page - Doc",
            site_title: "Doc",
            site_brand: "Short Doc",
            brand_logo: "",
            home_href: "./index.html",
            meta: "<link rel=\"canonical\" href=\"https://example.com/\">",
            favicons: "",
            font_preloads: "",
            icons_sprite: "",
            topbar: "",
            assets_prefix: "./assets/",
            depth_prefix: "./",
            css: "body { margin: 0; }",
            nav: "<ul><li>Item</li></ul>",
            breadcrumbs: "",
            content: "<p>Hello World</p>",
            toc: "",
            inline_toc: "",
            toc_rail: "",
            document_toc: "",
            pagination: "",
            search_data: "",
            search_js: "",
            component_js: "",
            live_reload: "",
            page_kind: "document",
            page_language: "en",
            json_ld: "",
        };

        let rendered = template.render(&slots);
        assert!(rendered.contains("<title>Test Page - Doc</title>"));
        assert!(rendered.contains("Short Doc"));
        assert!(rendered.contains("<html lang=\"en\">"));
        assert!(rendered.contains("<link rel=\"canonical\" href=\"https://example.com/\">"));
        assert!(rendered.contains("<style>body { margin: 0; }</style>"));
        assert!(rendered.contains("<ul><li>Item</li></ul>"));
        assert!(rendered.contains("<p>Hello World</p>"));

        // Verify legacy comments still work
        let legacy_shell = "<title><!-- evin:slot:title --></title><style><!-- evin:slot:css --></style><div><!-- evin:slot:content --></div>";
        let legacy_template = ShellTemplate::parse(legacy_shell);
        let legacy_rendered = legacy_template.render(&slots);
        assert!(legacy_rendered.contains("<title>Test Page - Doc</title>"));
        assert!(legacy_rendered.contains("<style>body { margin: 0; }</style>"));
        assert!(legacy_rendered.contains("<div><p>Hello World</p></div>"));

        let legacy_language =
            ShellTemplate::parse("<html lang=\"fr\"><body></body></html>").render(&slots);
        assert!(legacy_language.contains("<html lang=\"en\">"));
    }

    #[test]
    fn css_slot_supports_both_link_and_style_elements() {
        let shell = "<head><css></css></head><body></body>";
        let template = ShellTemplate::parse(shell);

        let mut slots = PageSlots {
            title: "",
            site_title: "",
            site_brand: "",
            brand_logo: "",
            home_href: "",
            meta: "",
            favicons: "",
            font_preloads: "",
            icons_sprite: "",
            topbar: "",
            assets_prefix: "",
            depth_prefix: "",
            css: "<link rel=\"stylesheet\" href=\"assets/theme.css\">",
            nav: "",
            breadcrumbs: "",
            content: "",
            toc: "",
            inline_toc: "",
            toc_rail: "",
            document_toc: "",
            pagination: "",
            search_data: "",
            search_js: "",
            component_js: "",
            live_reload: "",
            page_kind: "document",
            page_language: "en",
            json_ld: "",
        };

        let link_rendered = template.render(&slots);
        assert!(link_rendered.contains("<link rel=\"stylesheet\" href=\"assets/theme.css\">"));
        assert!(!link_rendered.contains("<style>"));

        slots.css = "<style>\nbody { color: red; }\n</style>";
        let style_rendered = template.render(&slots);
        assert!(style_rendered.contains("<style>\nbody { color: red; }\n</style>"));
        assert!(!style_rendered.contains("<style><style>"));
    }

    #[test]
    fn topbar_shell_slot_must_be_present_once_when_enabled() {
        assert!(validate_topbar_shell("<body><topbar></topbar></body>", true).is_ok());
        assert!(validate_topbar_shell("<body></body>", true).is_err());
        assert!(validate_topbar_shell("<topbar></topbar><topbar></topbar>", true).is_err());
        assert!(validate_topbar_shell("<body></body>", false).is_ok());
    }

    #[test]
    fn document_title_uses_the_site_name_for_the_start_page() {
        assert_eq!(
            HtmlRenderer::document_title("EVIN", "EVIN Documentation", true),
            "EVIN Documentation"
        );
        assert_eq!(
            HtmlRenderer::document_title("Get started", "EVIN Documentation", false),
            "Get started — EVIN Documentation"
        );
    }

    #[test]
    fn matter_shell_places_the_outline_in_inline_and_rail_slots() {
        let site = DocSiteAst {
            version: "1.0".to_string(),
            title: Some("Static Docs".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![DocPage {
                slug: "guides/setup".to_string(),
                source_path: "guides/setup.md".to_string(),
                format: DocFormat::Markdown,
                metadata: HashMap::new(),
                reference: None,
                toc: vec![TocEntry {
                    level: 2,
                    text: "Install".to_string(),
                    anchor: "install".to_string(),
                }],
                ast: vec![DocNode::Heading {
                    level: 2,
                    text: "Install".to_string(),
                    children: vec![InlineNode::Text {
                        content: "Install".to_string(),
                    }],
                    anchor: "install".to_string(),
                }],
            }],
        };
        let shell = include_str!("../../layouts/matter/shell.html");
        let registry = ComponentRegistry::load_from_layout(&LAYOUTS_DIR, "matter", None);
        let pages = HtmlRenderer::render_site(
            &site,
            shell,
            "",
            "",
            "",
            &registry,
            false,
            false,
            None,
            None,
            None,
            None,
            &seo(&site),
        );
        let html = pages.get("guides/setup/index.html").unwrap();

        assert!(html.contains("class=\"toc-rail\" aria-label=\"On this page\""));
        assert!(html.contains("class=\"inline-toc\""));
        assert_eq!(html.matches("toc-link toc-level-2").count(), 2);
        assert!(!html.contains("tocModal"));
        assert!(!html.contains("tocToggle"));
    }

    #[test]
    fn legacy_document_toc_slot_keeps_the_combined_outline_markup() {
        let site = DocSiteAst {
            version: "1.0".to_string(),
            title: Some("Static Docs".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![DocPage {
                slug: "guide".to_string(),
                source_path: "guide.rst".to_string(),
                format: DocFormat::ReStructuredText,
                metadata: HashMap::new(),
                reference: None,
                toc: vec![TocEntry {
                    level: 2,
                    text: "Overview".to_string(),
                    anchor: "overview".to_string(),
                }],
                ast: vec![],
            }],
        };
        let registry = ComponentRegistry::load_from_layout(&LAYOUTS_DIR, "matter", None);
        let pages = HtmlRenderer::render_site(
            &site,
            "<main><document-toc></document-toc></main>",
            "",
            "",
            "",
            &registry,
            false,
            false,
            None,
            None,
            None,
            None,
            &seo(&site),
        );
        let html = pages.get("guide/index.html").unwrap();

        assert!(html.contains("class=\"compact-toc\""));
        assert!(html.contains("class=\"toc-rail\""));
        assert_eq!(html.matches("toc-link toc-level-2").count(), 2);
    }

    #[test]
    fn outline_slots_are_empty_for_pages_without_headings() {
        let site = DocSiteAst {
            version: "1.0".to_string(),
            title: Some("Static Docs".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![DocPage {
                slug: "guide".to_string(),
                source_path: "guide.rst".to_string(),
                format: DocFormat::ReStructuredText,
                metadata: HashMap::new(),
                reference: None,
                toc: vec![],
                ast: vec![],
            }],
        };
        let registry = ComponentRegistry::load_from_layout(&LAYOUTS_DIR, "matter", None);
        let pages = HtmlRenderer::render_site(
            &site,
            "<main><inline-toc></inline-toc><toc-rail></toc-rail></main>",
            "",
            "",
            "",
            &registry,
            false,
            false,
            None,
            None,
            None,
            None,
            &seo(&site),
        );
        let html = pages.get("guide/index.html").unwrap();

        assert_eq!(html, "<main></main>");
    }

    #[test]
    fn start_page_uses_specialized_semantic_markup() {
        let site = DocSiteAst {
            version: "1.0".to_string(),
            title: Some("Docs".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![
                DocPage {
                    slug: "index".to_string(),
                    source_path: "index.md".to_string(),
                    format: DocFormat::Markdown,
                    metadata: HashMap::new(),
                    reference: None,
                    toc: vec![TocEntry {
                        level: 2,
                        text: "Get started".to_string(),
                        anchor: "get-started".to_string(),
                    }],
                    ast: vec![],
                },
                DocPage {
                    slug: "guide".to_string(),
                    source_path: "guide.md".to_string(),
                    format: DocFormat::Markdown,
                    metadata: HashMap::new(),
                    reference: None,
                    toc: vec![],
                    ast: vec![],
                },
            ],
        };
        let model = StartPageModel {
            title: "Docs".to_string(),
            lead: "A concise introduction.".to_string(),
            search_prompts: vec![],
            topbar: None,
            presentation: None,
            spotlight: Some(crate::v1::start_page::StartPageSpotlight {
                after_sections: 1,
                content: crate::v1::presentation::Spotlight {
                    variant: "holo".to_string(),
                    badge: Some("New".to_string()),
                    title: "Build <safely>".to_string(),
                    description: "A focused invitation.".to_string(),
                    action: crate::v1::presentation::PresentationAction {
                        label: "Start now".to_string(),
                        href: "/guide".to_string(),
                    },
                },
            }),
            features: vec![crate::v1::start_page::StartPageFeature {
                after_sections: 0,
                title: "Inspect the runtime".to_string(),
                description: vec![InlineNode::Text {
                    content: "Use the portable shell.".to_string(),
                }],
                actions: vec![crate::v1::start_page::StartPageFeatureAction {
                    label: "Read the guide".to_string(),
                    destination: FooterDestination::Internal("guide".to_string()),
                }],
                aside: StartPageFeatureAside::Code {
                    language: Some("esh".to_string()),
                    title: None,
                    code: "echo feature".to_string(),
                },
            }],
            sections: vec![crate::v1::start_page::StartPageSection {
                title: "Get started".to_string(),
                paragraphs: vec![vec![
                    InlineNode::Text {
                        content: "Learn the ".to_string(),
                    },
                    InlineNode::Link {
                        href: "guide".to_string(),
                        title: None,
                        content: "essentials".to_string(),
                        code: false,
                    },
                ]],
                media: Some(StartPageMedia {
                    kind: MediaKind::Image,
                    src: "https://example.com/guide.png".to_string(),
                    alt: "Guide overview".to_string(),
                    title: Some("Guide image".to_string()),
                    poster: None,
                }),
                cards: vec![crate::v1::start_page::StartPageCard {
                    title: "Guide".to_string(),
                    description: "Learn the essentials.".to_string(),
                    target_slug: "guide".to_string(),
                }],
            }],
            footer: None,
        };
        let shell = "<body data-page-kind=\"<page-kind></page-kind>\"><breadcrumbs></breadcrumbs><inline-toc></inline-toc><content></content><pagination></pagination><toc-rail></toc-rail><document-toc></document-toc></body>";
        let registry = ComponentRegistry::load_from_layout(&LAYOUTS_DIR, "matter", None);
        let pages = HtmlRenderer::render_site(
            &site,
            shell,
            "",
            "",
            "",
            &registry,
            false,
            false,
            None,
            None,
            None,
            Some(&model),
            &seo(&site),
        );
        let html = pages.get("index.html").unwrap();
        assert!(html.contains("data-page-kind=\"start\""));
        assert!(html.contains("class=\"start-page\""));
        assert!(!html.contains("start-hero-ornaments"));
        assert!(html.contains("aria-label=\"Documentation topics\""));
        assert!(html.contains("class=\"start-card-collection\""));
        assert!(html.contains("class=\"start-section-layout has-media\""));
        assert!(html.contains("class=\"start-section-media\""));
        assert!(html.contains("src=\"https://example.com/guide.png\""));
        assert!(html.contains("href=\"./guide/index.html\">essentials</a>"));
        assert!(
            html.contains("aria-labelledby=\"start-card-1-1-title start-card-1-1-description\"")
        );
        assert!(html.contains("href=\"./guide/index.html\""));
        assert!(
            html.contains("class=\"start-spotlight presentation-surface presentation--spotlight")
        );
        assert!(html.contains("data-presentation-variant=\"holo\""));
        assert!(html.contains("data-presentation-role=\"spotlight\""));
        assert!(html.contains("data-presentation-scale=\"feature\""));
        assert!(html.contains("Build &lt;safely&gt;"));
        assert!(html.contains("href=\"./guide/index.html\">"));
        assert!(html.contains("class=\"start-feature start-feature--right\""));
        assert!(html.contains("Inspect the runtime"));
        assert!(html.contains("class=\"start-feature-action\""));
        assert!(html.contains("echo"));
        assert!(html.contains("feature"));
        assert!(!html.contains("Home"));
        assert!(!html.contains("toc-rail"));
        assert!(!html.contains("inline-toc"));
        assert!(!html.contains("compact-toc"));
    }

    #[test]
    fn start_page_renders_optional_footer_component_with_safe_external_links() {
        let site = DocSiteAst {
            version: "1.0".to_string(),
            title: Some("Docs".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![DocPage {
                slug: "index".to_string(),
                source_path: "index.md".to_string(),
                format: DocFormat::Markdown,
                metadata: HashMap::new(),
                reference: None,
                toc: vec![],
                ast: vec![],
            }],
        };
        let model = StartPageModel {
            title: "Docs".to_string(),
            lead: "A concise introduction.".to_string(),
            search_prompts: vec![],
            topbar: None,
            presentation: None,
            spotlight: None,
            features: vec![],
            sections: vec![crate::v1::start_page::StartPageSection {
                title: "Get started".to_string(),
                paragraphs: vec![],
                media: None,
                cards: vec![],
            }],
            footer: Some(crate::v1::start_page::StartPageFooter {
                description: "A fuller project description.".to_string(),
                columns: vec![crate::v1::start_page::StartPageFooterColumn {
                    title: "Project".to_string(),
                    links: vec![crate::v1::start_page::StartPageFooterLink {
                        title: "Source".to_string(),
                        description: Some("Browse the source.".to_string()),
                        destination: crate::v1::start_page::FooterDestination::External(
                            "https://example.com/source".to_string(),
                        ),
                    }],
                }],
            }),
        };
        let registry = ComponentRegistry::load_from_layout(&LAYOUTS_DIR, "matter", None);
        let pages = HtmlRenderer::render_site(
            &site,
            "<body><content></content></body>",
            "",
            "",
            "",
            &registry,
            false,
            false,
            None,
            None,
            None,
            Some(&model),
            &seo(&site),
        );
        let html = pages.get("index.html").unwrap();

        assert!(html.contains("<footer class=\"start-footer\""));
        assert!(html.contains("class=\"start-footer-layout\""));
        assert!(html.contains("class=\"start-footer-column\""));
        assert!(
            html.contains(
                "href=\"https://example.com/source\" target=\"_blank\" rel=\"noreferrer\""
            )
        );
        assert!(!html.contains("rel=\"noreferrer\"\""));
        assert!(html.contains("Browse the source."));
        assert!(html.contains("A fuller project description."));
    }

    #[test]
    fn test_render_svgbob_and_mermaid_diagrams() {
        use evin_doc::ast::{DiagramKind, DocNode};

        let site = DocSiteAst {
            version: "1".to_string(),
            title: Some("Diagram Docs".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![DocPage {
                slug: "diagrams".to_string(),
                source_path: "diagrams.md".to_string(),
                format: DocFormat::Markdown,
                metadata: HashMap::new(),
                reference: None,
                toc: vec![],
                ast: vec![
                    DocNode::Diagram {
                        kind: DiagramKind::Svgbob,
                        code: "+---+ ---> +---+\n| A |      | B |\n+---+      +---+".to_string(),
                        title: Some("Svgbob Flow".to_string()),
                        caption: Some("Figure 1: Architecture".to_string()),
                        alt: Some("Architecture flowchart".to_string()),
                        rendered_mermaid: None,
                    },
                    DocNode::Diagram {
                        kind: DiagramKind::Mermaid,
                        code: "sequenceDiagram\nClient->>Server: Ping".to_string(),
                        title: Some("Mermaid Sequence".to_string()),
                        caption: Some("Figure 2: Protocol".to_string()),
                        alt: Some("Protocol sequence".to_string()),
                        rendered_mermaid: Some(evin_doc::ast::RenderedMermaid {
                            svg: "<svg id=\"sequence-diagram\"></svg>".to_string(),
                        }),
                    },
                ],
            }],
        };

        let registry = ComponentRegistry::load_from_layout(&LAYOUTS_DIR, "matter", None);
        let pages = HtmlRenderer::render_site(
            &site,
            "<body><content></content><!-- evin:slot:component_js --></body>",
            "",
            "",
            "",
            &registry,
            false,
            false,
            None,
            None,
            None,
            None,
            &seo(&site),
        );
        let html = pages.get("diagrams/index.html").unwrap();

        // 1. Verify Svgbob compiled inline <svg> directly into HTML with alt metadata, and without polluting <style> or backdrop <rect>
        assert!(html.contains("class=\"docs-diagram-figure diagram-figure doc-svgbob\""));
        assert!(html.contains("aria-label=\"Architecture flowchart\" role=\"img\""));
        assert!(html.contains("<svg"));
        assert!(!html.contains("<style"));
        assert!(!html.contains("class=\"backdrop\""));
        assert!(html.contains("Svgbob Flow"));
        assert!(html.contains("Figure 1: Architecture"));

        // 2. Verify offline Mermaid SVG, source, metadata, and controls.
        assert!(html.contains("class=\"docs-diagram-figure diagram-figure doc-mermaid\""));
        assert!(html.contains("aria-label=\"Protocol sequence\" role=\"img\""));
        assert!(html.contains("class=\"mermaid-source\" hidden>sequenceDiagram"));
        assert!(html.contains("<div class=\"mermaid-svg-target\"><svg id=\"sequence-diagram\""));
        assert!(html.contains("Mermaid Sequence"));
        assert!(html.contains("Figure 2: Protocol"));
        assert!(html.contains("diagram:copy"));
        assert!(html.contains("diagram:download"));
        for forbidden in [
            ["mermaid", ".esm"].concat(),
            ["window", ".mermaid"].concat(),
            ["js", "delivr"].concat(),
        ] {
            assert!(!html.contains(&forbidden));
        }

        // 3. Verify CSS rules for Mermaid background transparency
        let mut used = std::collections::HashSet::new();
        used.insert("diagram_mermaid".to_string());
        used.insert("diagram_svgbob".to_string());
        let css = registry.collect_css(&used);
        assert!(css.contains(".doc-mermaid"));
        assert!(css.contains(".mermaid-svg-target"));
        assert!(css.contains("transparent !important"));
    }

    #[test]
    fn test_diagram_toolbar_icons_in_sprite() {
        let requested_icons = crate::v1::default_icons();
        assert!(requested_icons.contains("zoom-in"));
        assert!(requested_icons.contains("zoom-out"));
        assert!(requested_icons.contains("refresh"));

        let sprite =
            crate::v1::icons::IconSpriteBuilder::build_sprite_sheet(&requested_icons, None, None);
        assert!(sprite.contains("id=\"icon-zoom-in\""));
        assert!(sprite.contains("id=\"icon-zoom-out\""));
        assert!(sprite.contains("id=\"icon-refresh\""));
    }

    #[test]
    fn test_render_mathml_inline_and_block() {
        // Inline Math
        let inline_html =
            HtmlRenderer::render_math_to_html("E = mc^2", latex2mathml::DisplayStyle::Inline);
        assert!(inline_html.starts_with("<span class=\"math-inline docs-math-inline\">"));
        assert!(
            inline_html
                .contains("<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"inline\">")
        );
        assert!(inline_html.ends_with("</math></span>"));
        // Ensure no double nested <math> tags
        assert_eq!(inline_html.matches("<math").count(), 1);

        // Block Math
        let block_html = HtmlRenderer::render_math_to_html(
            r"\int_0^1 x^2 dx",
            latex2mathml::DisplayStyle::Block,
        );
        assert!(block_html.starts_with("<div class=\"math-block docs-math-block\">"));
        assert!(
            block_html
                .contains("<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"block\">")
        );
        assert!(block_html.ends_with("</math></div>"));
        assert_eq!(block_html.matches("<math").count(), 1);
    }

    #[test]
    fn test_render_mathml_fallback_and_html_escaping() {
        let malicious_tex = r"\invalidUnclosed{<script>alert('xss')</script>";
        let fallback_html =
            HtmlRenderer::render_math_to_html(malicious_tex, latex2mathml::DisplayStyle::Block);
        assert!(fallback_html.starts_with("<div class=\"math-block docs-math-block\"><code class=\"math-fallback docs-math-fallback math-fallback-block docs-math-fallback-block\">"));
        assert!(fallback_html.contains("&lt;script&gt;alert('xss')&lt;/script&gt;"));
        assert!(!fallback_html.contains("<script>"));
        assert!(fallback_html.ends_with("</code></div>"));

        let inline_fallback =
            HtmlRenderer::render_math_to_html(malicious_tex, latex2mathml::DisplayStyle::Inline);
        assert!(inline_fallback.starts_with("<code class=\"math-fallback docs-math-fallback\">"));
        assert!(inline_fallback.ends_with("</code>"));
    }

    #[test]
    fn test_render_code_block_surface_and_actions() {
        let site = DocSiteAst {
            version: "1".to_string(),
            title: Some("CodeBlock Test".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![DocPage {
                slug: "code".to_string(),
                source_path: "code.md".to_string(),
                format: DocFormat::Markdown,
                metadata: HashMap::new(),
                reference: None,
                toc: vec![],
                ast: vec![DocNode::CodeBlock {
                    language: Some("rust".to_string()),
                    code: "fn main() {\n    println!(\"Hello\");\n}".to_string(),
                    title: Some("Main <entry>".to_string()),
                }],
            }],
        };

        let registry = ComponentRegistry::load_from_layout(&LAYOUTS_DIR, "matter", None);
        let pages = HtmlRenderer::render_site(
            &site,
            "<body><content></content></body>",
            "",
            "",
            "",
            &registry,
            false,
            false,
            None,
            None,
            None,
            None,
            &seo(&site),
        );
        let html = pages.get("code/index.html").unwrap();

        assert!(html.contains("class=\"code-surface\" data-language=\"rust\""));
        assert!(html.contains("class=\"code-header\""));
        assert!(html.contains("class=\"code-title\">Main &lt;entry&gt;</span>"));
        assert!(html.contains("class=\"lang-label\">Rust</span>"));
        assert!(html.contains("btn-download"));
        assert!(html.contains("data-extension=\"rs\""));
        assert!(html.contains("btn-copy"));
        assert!(html.contains("class=\"code-scroll\""));
        assert!(html.contains("println!"));
    }

    #[test]
    fn headings_render_an_inline_accessible_permalink() {
        let site = DocSiteAst {
            version: "1".to_string(),
            title: Some("Heading Test".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![DocPage {
                slug: "heading".to_string(),
                source_path: "heading.md".to_string(),
                format: DocFormat::Markdown,
                metadata: HashMap::new(),
                reference: None,
                toc: vec![],
                ast: vec![DocNode::Heading {
                    level: 2,
                    text: "A linked section".to_string(),
                    anchor: "a-linked-section".to_string(),
                    children: vec![InlineNode::Text {
                        content: "A linked section".to_string(),
                    }],
                }],
            }],
        };

        let registry = ComponentRegistry::load_from_layout(&LAYOUTS_DIR, "matter", None);
        let pages = HtmlRenderer::render_site(
            &site,
            "<body><content></content></body>",
            "",
            "",
            "",
            &registry,
            false,
            false,
            None,
            None,
            None,
            None,
            &seo(&site),
        );
        let html = pages.get("heading/index.html").unwrap();

        assert!(html.contains("<span class=\"heading-content\">A linked section</span>"));
        assert!(html.contains("href=\"#a-linked-section\" class=\"heading-permalink\""));
        assert!(html.contains("aria-label=\"Link to this section\""));
        assert!(html.contains("href=\"#icon-link\""));
        assert!(!html.contains("class=\"anchor-link\""));
    }

    #[test]
    fn test_dynamic_css_tree_shaking_precision() {
        let plain_site = DocSiteAst {
            version: "1".to_string(),
            title: Some("Plain Site".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![DocPage {
                slug: "index".to_string(),
                source_path: "index.md".to_string(),
                format: DocFormat::Markdown,
                metadata: HashMap::new(),
                reference: None,
                toc: vec![],
                ast: vec![
                    DocNode::Heading {
                        level: 1,
                        text: "Hello World".to_string(),
                        anchor: "hello".to_string(),
                        children: vec![InlineNode::Text {
                            content: "Hello World".to_string(),
                        }],
                    },
                    DocNode::Paragraph {
                        children: vec![InlineNode::Text {
                            content: "Simple text content without diagrams or math.".to_string(),
                        }],
                    },
                ],
            }],
        };

        let registry = ComponentRegistry::load_from_layout(&LAYOUTS_DIR, "matter", None);
        let used = HtmlRenderer::collect_site_used_components(&plain_site, None);

        // Heading and paragraph are used
        assert!(used.contains("heading"));
        assert!(used.contains("paragraph"));

        // Optional components MUST NOT be present
        assert!(!used.contains("diagram_mermaid"));
        assert!(!used.contains("diagram_svgbob"));
        assert!(!used.contains("math"));
        assert!(!used.contains("start_page"));
        assert!(!used.contains("callout"));
        assert!(!used.contains("table"));
        assert!(!used.contains("toc_block"));
        assert!(!used.contains("pagination"));

        let emitted_css = registry.collect_css(&used);
        assert!(!emitted_css.contains(".diagram-figure"));
        assert!(!emitted_css.contains(".math-block"));
        assert!(!emitted_css.contains(".start-hero"));
        assert!(!emitted_css.contains(".m3-directive"));
        assert!(!emitted_css.contains(".m3-table"));
        assert!(!emitted_css.contains(".m3-toc-block"));
        assert!(!emitted_css.contains(".doc-pagination"));
    }

    #[test]
    fn test_dynamic_js_tree_shaking_precision() {
        let plain_site = DocSiteAst {
            version: "1".to_string(),
            title: Some("Minimal Docs".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![DocPage {
                slug: "overview".to_string(),
                source_path: "overview.md".to_string(),
                format: DocFormat::Markdown,
                metadata: HashMap::new(),
                reference: None,
                toc: vec![],
                ast: vec![
                    DocNode::Heading {
                        level: 1,
                        text: "Overview".to_string(),
                        anchor: "overview".to_string(),
                        children: vec![InlineNode::Text {
                            content: "Overview".to_string(),
                        }],
                    },
                    DocNode::Paragraph {
                        children: vec![InlineNode::Text {
                            content: "Simple text content without diagrams or code.".to_string(),
                        }],
                    },
                ],
            }],
        };

        let registry = ComponentRegistry::load_from_layout(&LAYOUTS_DIR, "matter", None);
        let used = HtmlRenderer::collect_site_used_components(&plain_site, None);

        let emitted_js = registry.collect_js(&used);
        // Without code blocks, diagrams, or start page, optional JS MUST be empty
        assert!(!emitted_js.contains("initHeroParticles"));
        assert!(!emitted_js.contains("initMermaid"));
        assert!(!emitted_js.contains("code:copy"));
        assert!(!emitted_js.contains("code:download"));
        assert!(!emitted_js.contains("tape:toggle-playback"));
        assert!(!emitted_js.contains("tape:copy"));

        // Now test site with code block and mermaid diagram
        let rich_site = DocSiteAst {
            version: "1".to_string(),
            title: Some("Rich Docs".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![DocPage {
                slug: "code-diagram".to_string(),
                source_path: "code.md".to_string(),
                format: DocFormat::Markdown,
                metadata: HashMap::new(),
                reference: None,
                toc: vec![],
                ast: vec![
                    DocNode::CodeBlock {
                        language: Some("rust".to_string()),
                        code: "fn main() {}".to_string(),
                        title: None,
                    },
                    DocNode::Diagram {
                        kind: evin_doc::ast::DiagramKind::Mermaid,
                        code: "graph TD; A-->B;".to_string(),
                        title: None,
                        alt: None,
                        caption: None,
                        rendered_mermaid: None,
                    },
                ],
            }],
        };

        let rich_used = HtmlRenderer::collect_site_used_components(&rich_site, None);
        assert!(rich_used.contains("code_block"));
        assert!(rich_used.contains("diagram_mermaid"));
        assert!(!rich_used.contains("start_page"));
        assert!(!rich_used.contains("diagram_svgbob"));

        let rich_js = registry.collect_js(&rich_used);
        assert!(rich_js.contains("code:copy"));
        assert!(rich_js.contains("code:download"));
        assert!(rich_js.contains("diagram:copy"));
        assert!(rich_js.contains("diagram:download"));
        assert!(!rich_js.contains(&["init", "Mermaid"].concat()));
        assert!(!rich_js.contains("initHeroParticles"));

        let tape_site = DocSiteAst {
            version: "1".to_string(),
            title: Some("Tape Docs".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![DocPage {
                slug: "tape".to_string(),
                source_path: "tape.md".to_string(),
                format: DocFormat::Markdown,
                metadata: HashMap::new(),
                reference: None,
                toc: vec![],
                ast: vec![DocNode::Tape {
                    script: "type \"pwd\"\nenter".to_string(),
                    config: Default::default(),
                    rendered_svg: None,
                }],
            }],
        };
        let tape_used = HtmlRenderer::collect_site_used_components(&tape_site, None);
        let tape_js = registry.collect_js(&tape_used);
        assert!(tape_used.contains("tape"));
        assert!(tape_js.contains("tape:toggle-playback"));
        assert!(tape_js.contains("tape:copy"));
    }

    #[test]
    fn metadata_expansion_emits_og_twitter_generator_json_ld_and_sequence_links() {
        let mut index_meta = HashMap::new();
        index_meta.insert(
            "title".to_string(),
            serde_json::Value::String("Home Page".to_string()),
        );
        index_meta.insert(
            "description".to_string(),
            serde_json::Value::String("The site home page description.".to_string()),
        );
        index_meta.insert(
            "image".to_string(),
            serde_json::Value::String("https://example.com/custom-og.png".to_string()),
        );

        let mut guide_meta = HashMap::new();
        guide_meta.insert(
            "title".to_string(),
            serde_json::Value::String("Guide Page".to_string()),
        );
        guide_meta.insert(
            "description".to_string(),
            serde_json::Value::String("A comprehensive guide page description.".to_string()),
        );
        guide_meta.insert(
            "author".to_string(),
            serde_json::Value::String("Alice Developer".to_string()),
        );
        guide_meta.insert(
            "updated".to_string(),
            serde_json::Value::String("2026-07-20".to_string()),
        );

        let mut ref_meta = HashMap::new();
        ref_meta.insert(
            "title".to_string(),
            serde_json::Value::String("Reference Page".to_string()),
        );

        let site = DocSiteAst {
            version: "1.0".to_string(),
            title: Some("Evin Documentation".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![
                DocPage {
                    slug: "index".to_string(),
                    source_path: "index.md".to_string(),
                    format: DocFormat::Markdown,
                    metadata: index_meta,
                    reference: None,
                    toc: vec![],
                    ast: vec![],
                },
                DocPage {
                    slug: "guide".to_string(),
                    source_path: "guide.md".to_string(),
                    format: DocFormat::Markdown,
                    metadata: guide_meta,
                    reference: None,
                    toc: vec![],
                    ast: vec![],
                },
                DocPage {
                    slug: "reference".to_string(),
                    source_path: "reference.md".to_string(),
                    format: DocFormat::Markdown,
                    metadata: ref_meta,
                    reference: None,
                    toc: vec![],
                    ast: vec![],
                },
            ],
        };

        let seo = SeoCatalog::build(&site, "https://example.com/docs/", "en").unwrap();
        let assets = ResolvedAssets::default();
        let registry = ComponentRegistry::empty();
        let shell = "<head><page-title></page-title><page-meta></page-meta><json-ld></json-ld></head><body><content></content></body>";
        let index = SiteIndex::build(&site).unwrap();

        let context = PreparedRenderContext::new(
            &site,
            index,
            &assets,
            shell,
            "",
            "",
            "",
            &registry,
            false,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            &seo,
            Some("https://example.com/global-social.png"),
        );

        let outputs = HtmlRenderer::render_prepared(&context);

        // 1. Start page (index.html)
        let index_html = &outputs["index.html"];
        assert!(index_html.contains("<meta name=\"generator\" content=\"evin\">"));
        assert!(
            index_html.contains("<meta property=\"og:url\" content=\"https://example.com/docs/\">")
        );
        assert!(index_html.contains("<meta property=\"og:type\" content=\"website\">"));
        assert!(
            index_html.contains("<meta property=\"og:site_name\" content=\"Evin Documentation\">")
        );
        assert!(index_html.contains("<meta property=\"og:title\" content=\"Home Page\">"));
        assert!(index_html.contains(
            "<meta property=\"og:description\" content=\"The site home page description.\">"
        ));
        assert!(index_html.contains(
            "<meta property=\"og:image\" content=\"https://example.com/custom-og.png\">"
        ));
        assert!(index_html.contains("<meta name=\"twitter:card\" content=\"summary\">"));
        assert!(index_html.contains("<meta name=\"twitter:title\" content=\"Home Page\">"));
        assert!(index_html.contains(
            "<meta name=\"twitter:image\" content=\"https://example.com/custom-og.png\">"
        ));
        assert!(index_html.contains("\"@type\": \"WebSite\""));
        assert!(index_html.contains("\"name\": \"Evin Documentation\""));

        // 2. Guide page (guide/index.html)
        let guide_html = &outputs["guide/index.html"];
        assert!(
            guide_html
                .contains("<link rel=\"canonical\" href=\"https://example.com/docs/guide/\">")
        );
        assert!(guide_html.contains(
            "<meta property=\"og:image\" content=\"https://example.com/global-social.png\">"
        ));
        assert!(guide_html.contains(
            "<meta name=\"twitter:image\" content=\"https://example.com/global-social.png\">"
        ));
        assert!(guide_html.contains("<link rel=\"prev\" href=\"https://example.com/docs/\">"));
        assert!(
            guide_html.contains("<link rel=\"next\" href=\"https://example.com/docs/reference/\">")
        );
        assert!(guide_html.contains("\"@type\": \"TechArticle\""));
        assert!(guide_html.contains("\"headline\": \"Guide Page\""));
        assert!(guide_html.contains("\"dateModified\": \"2026-07-20\""));
        assert!(guide_html.contains("\"name\": \"Alice Developer\""));
    }
}
