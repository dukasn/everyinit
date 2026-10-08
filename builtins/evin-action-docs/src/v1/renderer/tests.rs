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
        workspace_html.contains("<span class=\"heading-content\">Workspace configuration</span>")
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
    assert!(position(">Writing ESH scripts</span>") < position(">ESH Runtime Reference</span>"));
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
    assert!(overview_pagination.contains("<span class=\"pagination-title\">Archive action</span>"));
    assert!(!overview_pagination.contains("Previous</span>"));

    let action_pagination = HtmlRenderer::render_pagination(&site, "builtins/archive-action");
    assert!(action_pagination.contains("<span class=\"pagination-title\">Built-in actions</span>"));
    assert!(
        action_pagination.contains("<span class=\"pagination-title\">Version bumper action</span>")
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
    let shell = include_str!("../../../layouts/matter/shell.html");
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
    assert!(html.contains("aria-labelledby=\"start-card-1-1-title start-card-1-1-description\""));
    assert!(html.contains("href=\"./guide/index.html\""));
    assert!(html.contains("class=\"start-spotlight presentation-surface presentation--spotlight"));
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
        html.contains("href=\"https://example.com/source\" target=\"_blank\" rel=\"noreferrer\"")
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
    let block_html =
        HtmlRenderer::render_math_to_html(r"\int_0^1 x^2 dx", latex2mathml::DisplayStyle::Block);
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
    assert!(index_html.contains("<meta property=\"og:site_name\" content=\"Evin Documentation\">"));
    assert!(index_html.contains("<meta property=\"og:title\" content=\"Home Page\">"));
    assert!(index_html.contains(
        "<meta property=\"og:description\" content=\"The site home page description.\">"
    ));
    assert!(
        index_html
            .contains("<meta property=\"og:image\" content=\"https://example.com/custom-og.png\">")
    );
    assert!(index_html.contains("<meta name=\"twitter:card\" content=\"summary\">"));
    assert!(index_html.contains("<meta name=\"twitter:title\" content=\"Home Page\">"));
    assert!(
        index_html.contains(
            "<meta name=\"twitter:image\" content=\"https://example.com/custom-og.png\">"
        )
    );
    assert!(index_html.contains("\"@type\": \"WebSite\""));
    assert!(index_html.contains("\"name\": \"Evin Documentation\""));

    // 2. Guide page (guide/index.html)
    let guide_html = &outputs["guide/index.html"];
    assert!(
        guide_html.contains("<link rel=\"canonical\" href=\"https://example.com/docs/guide/\">")
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
