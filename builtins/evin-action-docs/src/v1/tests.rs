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

#[test]
fn defaults_are_build_friendly() {
    let config = DocsConfig::default();
    assert_eq!(config.mode, DocsMode::Build);
    assert!(!config.host);
    assert_eq!(config.source_dir, "docs");
    assert_eq!(config.asset_dir, "assets");
    assert_eq!(config.out, "dist/docs");
    assert_eq!(config.theme, "matter");
    assert_eq!(config.bundle, BundleMode::Separated);
    assert!(!config.strict_filenames);
    assert!(config.minify.unwrap_or(config.mode == DocsMode::Build));
}

#[test]
fn strict_filenames_require_the_public_documentation_contract() {
    let page = |source_path: &str, reference: Option<evin_doc::FormalReference>| ast::DocPage {
        slug: source_path.to_string(),
        source_path: source_path.to_string(),
        format: DocFormat::Markdown,
        metadata: HashMap::new(),
        reference,
        toc: vec![],
        ast: vec![],
    };
    let reference = evin_doc::FormalReference {
        id: "REF-4500".to_string(),
        digits: "4500".to_string(),
        number: 4500,
        depends_on: vec![],
        referenced_by: vec![],
    };
    let valid = DocSiteAst {
        version: "1.0".to_string(),
        title: None,
        reference_prefix: "REF".to_string(),
        pages: vec![
            page("index.md", None),
            page("guides/00.index.md", None),
            page("guides/10.workspaces.md", None),
            page(
                "reference/20.workspace-configuration.4500.md",
                Some(reference),
            ),
        ],
    };
    assert!(validate_strict_filenames(&valid).is_ok());

    let duplicate = DocSiteAst {
        pages: vec![
            page("guides/00.index.md", None),
            page("guides/01.first.md", None),
            page("guides/1.second.md", None),
        ],
        ..valid.clone()
    };
    assert_eq!(
        validate_strict_filenames(&duplicate).unwrap_err().code,
        "invalid-filename"
    );

    let legacy_reference = DocSiteAst {
        pages: vec![page(
            "reference/20.workspace-configuration-4500.md",
            Some(evin_doc::FormalReference {
                id: "REF-4500".to_string(),
                digits: "4500".to_string(),
                number: 4500,
                depends_on: vec![],
                referenced_by: vec![],
            }),
        )],
        ..valid
    };
    assert_eq!(
        validate_strict_filenames(&legacy_reference)
            .unwrap_err()
            .code,
        "invalid-filename"
    );
}

#[test]
fn site_url_is_required_and_site_settings_are_validated() {
    let missing = evin_flow::parse_yaml("title: Docs\n").unwrap();
    assert!(evin_flow::de::from_spanned_value::<DocsConfig>(missing).is_err());

    let invalid =
        evin_flow::parse_yaml("site_url: ftp://example.com/docs\nlanguage: not_a_language\n")
            .unwrap();
    let error = validate_config(Some(&invalid)).unwrap_err();
    assert!(error.message.contains("site_url"));
}

#[test]
fn bundle_mode_rejects_unrecognized_strings() {
    let invalid =
        evin_flow::parse_yaml("site_url: https://example.com/\nbundle: almost-single-file\n")
            .unwrap();
    assert!(evin_flow::de::from_spanned_value::<DocsConfig>(invalid).is_err());
}

#[test]
fn site_url_template_is_validated_after_value_resolution() {
    let templated = evin_flow::parse_yaml("site_url: ${docs_site_url}\n").unwrap();
    assert!(validate_config(Some(&templated)).is_ok());
}

#[test]
fn serve_mode_defaults_to_unminified_assets() {
    let config = DocsConfig {
        mode: DocsMode::Serve,
        ..DocsConfig::default()
    };
    assert!(!config.minify.unwrap_or(config.mode == DocsMode::Build));
}

#[test]
fn serve_host_accepts_yaml_and_routed_invocation_override() {
    let yaml = "site_url: https://example.com/\nhost: true\n";
    let value = evin_flow::parse_yaml(yaml).unwrap();
    let configured = evin_flow::de::from_spanned_value::<DocsConfig>(value).unwrap();
    assert!(configured.host);
    assert_eq!(
        resolve_serve_host(&configured, &InvocationFlags::default()),
        server::ServeHost::AllInterfaces
    );

    let default = DocsConfig::default();
    assert_eq!(
        resolve_serve_host(&default, &InvocationFlags::default()),
        server::ServeHost::Loopback
    );
    let mut host_flag = InvocationFlags::default();
    host_flag.insert("host");
    assert_eq!(
        resolve_serve_host(&default, &host_flag),
        server::ServeHost::AllInterfaces
    );
    assert_eq!(
        resolve_serve_host(&default, &InvocationFlags::default()),
        server::ServeHost::Loopback
    );

    let invalid =
        evin_flow::parse_yaml("site_url: https://example.com/\nhost: 127.0.0.1\n").unwrap();
    assert!(evin_flow::de::from_spanned_value::<DocsConfig>(invalid).is_err());
}

#[test]
fn reference_prefix_defaults_normalizes_and_validates() {
    assert_eq!(
        DocsConfig::default().normalized_reference_prefix().unwrap(),
        "REF"
    );

    let value =
        evin_flow::parse_yaml("site_url: https://example.com/docs/\nreference_prefix: rfc\n")
            .unwrap();
    let config = evin_flow::de::from_spanned_value::<DocsConfig>(value).unwrap();
    assert_eq!(config.normalized_reference_prefix().unwrap(), "RFC");

    for invalid in ["", "RFC-DOC", "RFC DOC"] {
        let config = DocsConfig {
            reference_prefix: invalid.to_string(),
            ..DocsConfig::default()
        };
        assert!(config.normalized_reference_prefix().is_err());
    }
}

#[test]
fn asset_directory_must_stay_inside_the_documentation_source() {
    let invalid =
        evin_flow::parse_yaml("site_url: https://example.com/\nasset_dir: ../outside\n").unwrap();
    let error = validate_config(Some(&invalid)).unwrap_err();
    assert!(error.message.contains("asset_dir"));
}

#[test]
fn asset_directory_can_be_resolved_from_a_template() {
    let templated =
        evin_flow::parse_yaml("site_url: ${docs_site_url}\nasset_dir: ${docs_asset_dir}\n")
            .unwrap();
    assert!(validate_config(Some(&templated)).is_ok());
}

#[test]
fn bundled_search_prioritizes_and_displays_formal_reference_ids() {
    let (_, _, javascript, _) = resolve_layout_assets(Path::new("."), "matter", None);
    assert!(javascript.contains("EXACT_REFERENCE: 240"));
    assert!(javascript.contains("reference_id"));
    assert!(javascript.contains("m3-reference-badge"));
}

#[test]
fn legacy_inputs_are_rejected() {
    let yaml = "ast: old.json\nserve: true\n";
    let value = evin_flow::parse_yaml(yaml).unwrap();
    let error = evin_flow::de::from_spanned_value::<DocsConfig>(value).unwrap_err();
    assert!(error.message.contains("unknown field"));
}

#[test]
fn source_fingerprint_tracks_docs_and_assets_but_not_layouts() {
    let temp = tempfile::tempdir().unwrap();
    let docs = temp.path().join("docs");
    let layouts = temp.path().join("layouts");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::create_dir_all(&layouts).unwrap();
    std::fs::write(docs.join("index.md"), "# One").unwrap();
    let config = DocsConfig {
        source_dir: docs.to_string_lossy().to_string(),
        layout_dir: Some(layouts.to_string_lossy().to_string()),
        ..DocsConfig::default()
    };

    let original = source_fingerprint(temp.path(), &config);
    std::fs::write(layouts.join("theme.css"), "body {}").unwrap();
    assert_eq!(source_fingerprint(temp.path(), &config), original);

    std::fs::write(docs.join("guide.rst"), "Guide\n=====").unwrap();
    assert_ne!(source_fingerprint(temp.path(), &config), original);
    std::fs::remove_file(docs.join("guide.rst")).unwrap();
    assert_eq!(source_fingerprint(temp.path(), &config), original);

    let asset = docs.join("assets/diagram.png");
    std::fs::create_dir_all(asset.parent().unwrap()).unwrap();
    std::fs::write(&asset, "image").unwrap();
    assert_ne!(source_fingerprint(temp.path(), &config), original);
    std::fs::remove_file(asset).unwrap();
    assert_eq!(source_fingerprint(temp.path(), &config), original);
}

#[test]
fn bundled_themes_support_component_tree_shaking() {
    {
        let theme = "matter";
        let (_, raw_css, _, embedded_fs) = resolve_layout_assets(Path::new("."), theme, None);
        let registry = templates::ComponentRegistry::load_from_layout(&LAYOUTS_DIR, theme, None);

        // 1. Base theme without components has 0 component dead weight
        let base_css = CssOptimizer::process(&raw_css, &HashMap::new(), &embedded_fs, false)
            .unwrap_or_else(|error| panic!("{theme} theme did not compile: {error}"));

        assert!(!base_css.contains(".start-hero"));
        assert!(!base_css.contains(".docs-diagram-container"));
        assert!(!base_css.contains(".docs-math-block"));
        assert!(!base_css.contains(".presentation-surface"));
        assert!(!base_css.contains(".docs-topbar"));
        assert!(base_css.contains("--ui-ref-space-6"));
        assert!(base_css.contains("--ui-color-background"));
        assert!(base_css.contains("--ui-layout-sidebar-width"));
        assert!(base_css.contains("#F8F6F2"));
        assert!(base_css.contains("#141210"));

        // 2. Activating components injects only the active component styles
        let mut used = HashSet::new();
        used.insert("start_page".to_string());
        used.insert("diagram_mermaid".to_string());
        used.insert("math".to_string());
        let comp_scss = registry.collect_css(&used);
        let full_css = CssOptimizer::process_with_components(
            &raw_css,
            &comp_scss,
            &HashMap::new(),
            &embedded_fs,
            false,
        )
        .unwrap();

        assert!(full_css.contains(".start-hero"));
        assert!(full_css.contains(".start-card-grid"));
        assert!(full_css.contains(".mermaid-svg-target"));
        assert!(full_css.contains(".docs-diagram-figure"));
        assert!(full_css.contains(".docs-math-block"));
    }
}

#[test]
fn matter_topbar_compiles_only_the_selected_variant() {
    let (_, raw_css, _, embedded_fs) = resolve_layout_assets(Path::new("."), "matter", None);
    let registry = templates::ComponentRegistry::load_from_layout(&LAYOUTS_DIR, "matter", None);
    let presentation_scss = registry.collect_css(&HashSet::from(["presentation".to_string()]));
    let selected = topbar::Topbar {
        id: None,
        variant: "genome".to_string(),
        badge: None,
        message: "A release is coming.".to_string(),
        action: None,
        dismissible: false,
    };
    let selected_variants = BTreeSet::from([selected.variant.clone()]);
    let selected_css = CssOptimizer::process_with_components(
        &topbar::configure_matter_scss(&raw_css, "matter", &selected_variants, true, false),
        &presentation_scss,
        &HashMap::new(),
        &embedded_fs,
        false,
    )
    .unwrap();
    assert!(selected_css.contains(".topbar--genome"));
    assert!(selected_css.contains(".presentation-surface"));
    assert!(selected_css.contains(".docs-topbar"));
    assert!(!selected_css.contains(".start-spotlight-content"));
    assert!(selected_css.contains("--ui-topbar-genome-surface"));
    assert!(!selected_css.contains(".topbar--aurora"));
    assert!(!selected_css.contains("--ui-topbar-aurora-surface"));

    let shared_css = CssOptimizer::process_with_components(
        &topbar::configure_matter_scss(
            &raw_css,
            "matter",
            &BTreeSet::from(["holo".to_string(), "geo".to_string()]),
            true,
            true,
        ),
        &presentation_scss,
        &HashMap::new(),
        &embedded_fs,
        false,
    )
    .unwrap();
    assert!(shared_css.contains(".topbar--holo"));
    assert!(shared_css.contains(".topbar--geo"));
    assert!(!shared_css.contains(".topbar--genome"));

    let spotlight_only_css = CssOptimizer::process_with_components(
        &topbar::configure_matter_scss(
            &raw_css,
            "matter",
            &BTreeSet::from(["holo".to_string()]),
            false,
            true,
        ),
        &presentation_scss,
        &HashMap::new(),
        &embedded_fs,
        false,
    )
    .unwrap();
    assert!(spotlight_only_css.contains(".start-spotlight-content"));
    assert!(spotlight_only_css.contains(".topbar--holo"));
    assert!(!spotlight_only_css.contains(".docs-topbar"));
    assert!(!spotlight_only_css.contains(".topbar-content"));

    for variant in topbar::MATTER_VARIANTS {
        let mut variant_topbar = selected.clone();
        variant_topbar.variant = variant.to_string();
        let variants = BTreeSet::from([variant_topbar.variant.clone()]);
        let css = CssOptimizer::process_with_components(
            &topbar::configure_matter_scss(&raw_css, "matter", &variants, true, false),
            &presentation_scss,
            &HashMap::new(),
            &embedded_fs,
            false,
        )
        .unwrap();
        assert!(css.contains(&format!(".topbar--{variant}")));
        assert!(css.contains("--topbar-surface"));
        assert!(css.contains(".presentation-layer-one"));
        if *variant != "genome" {
            assert!(!css.contains(".topbar--genome"));
        }
    }

    let disabled_css = CssOptimizer::process_with_components(
        &topbar::configure_matter_scss(&raw_css, "matter", &BTreeSet::new(), false, false),
        "",
        &HashMap::new(),
        &embedded_fs,
        false,
    )
    .unwrap();
    assert!(!disabled_css.contains(".docs-topbar"));
    assert!(!disabled_css.contains(".presentation-surface"));
    assert!(!disabled_css.contains("--ui-topbar-"));
}

#[test]
fn test_presentation_surfaces_decoupled_from_document_theme() {
    let (_, raw_css, _, embedded_fs) = resolve_layout_assets(Path::new("."), "matter", None);
    let registry = templates::ComponentRegistry::load_from_layout(&LAYOUTS_DIR, "matter", None);
    let presentation_scss = registry.collect_css(&HashSet::from(["presentation".to_string()]));

    for variant in topbar::MATTER_VARIANTS {
        let variants = BTreeSet::from([variant.to_string()]);
        let css = CssOptimizer::process_with_components(
            &topbar::configure_matter_scss(&raw_css, "matter", &variants, true, true),
            &presentation_scss,
            &HashMap::new(),
            &embedded_fs,
            false,
        )
        .unwrap();

        assert!(
            css.contains(&format!(".presentation-surface--{variant}")),
            "Missing presentation surface class for {variant}"
        );
        assert!(
            css.contains("--presentation-surface"),
            "Missing --presentation-surface token for {variant}"
        );
        assert!(
            css.contains("--presentation-ink"),
            "Missing --presentation-ink token for {variant}"
        );
        assert!(
            css.contains("--presentation-action-ink"),
            "Missing --presentation-action-ink token for {variant}"
        );
        assert!(
            css.contains("--presentation-badge-ink"),
            "Missing --presentation-badge-ink token for {variant}"
        );
        assert!(
            css.contains("--topbar-action-ink-hover"),
            "Missing --topbar-action-ink-hover token for {variant}"
        );
        assert!(
            css.contains("--topbar-action-font-size"),
            "Missing --topbar-action-font-size token for {variant}"
        );
        assert!(
            css.contains("--spotlight-action-font-size"),
            "Missing --spotlight-action-font-size token for {variant}"
        );
    }

    // Verify spotlight-only compilation contains complete presentation variables without topbar layout
    let spotlight_css = CssOptimizer::process_with_components(
        &topbar::configure_matter_scss(
            &raw_css,
            "matter",
            &BTreeSet::from(["brutalist".to_string()]),
            false,
            true,
        ),
        &presentation_scss,
        &HashMap::new(),
        &embedded_fs,
        false,
    )
    .unwrap();

    assert!(spotlight_css.contains(".presentation-surface--brutalist"));
    assert!(spotlight_css.contains("--presentation-surface"));
    assert!(spotlight_css.contains(".start-spotlight"));
    assert!(spotlight_css.contains("--spotlight-action-font-size"));
    assert!(spotlight_css.contains(".presentation-scale-feature"));
    assert!(!spotlight_css.contains(".docs-topbar"));
    assert!(!spotlight_css.contains(".topbar-content"));
}

#[test]
fn bundled_themes_use_only_material_window_width_boundaries() {
    let width_query =
        regex::Regex::new(r"@media\s*\(\s*(?:min|max)-width:\s*(\d+)px\s*\)").unwrap();
    let allowed = [600_u16, 840, 1200, 1600];

    {
        let theme = "matter";
        let (_, raw_css, _, embedded_fs) = resolve_layout_assets(Path::new("."), theme, None);
        let css = CssOptimizer::process(&raw_css, &HashMap::new(), &embedded_fs, false)
            .unwrap_or_else(|error| panic!("{theme} theme did not compile: {error}"));
        let boundaries: Vec<u16> = width_query
            .captures_iter(&css)
            .map(|capture| capture[1].parse::<u16>().unwrap())
            .collect();

        assert!(!boundaries.is_empty(), "{theme} emitted no width classes");
        assert!(
            boundaries.iter().all(|boundary| allowed.contains(boundary)),
            "{theme} emitted a non-canonical width boundary: {boundaries:?}"
        );
        assert!(css.contains("--ui-layout-window-width-class: compact"));
        assert!(css.contains("--ui-layout-window-height-class: compact"));
    }
}

#[test]
fn theme_overrides_win_and_shorthand_names_target_semantic_tokens() {
    let (_, raw_css, _, embedded_fs) = resolve_layout_assets(Path::new("."), "matter", None);
    let overrides = HashMap::from([
        ("ui_bg".to_string(), "#222222".to_string()),
        ("ui_color_primary".to_string(), "#eeeeee".to_string()),
    ]);
    let css = CssOptimizer::process(&raw_css, &overrides, &embedded_fs, false).unwrap();

    let override_start = css.rfind(":root {").unwrap();
    assert!(css[override_start..].contains("--ui-color-background: #222222"));
    assert!(css[override_start..].contains("--ui-color-primary: #eeeeee"));
    assert!(override_start > css.rfind("[data-theme=dark]").unwrap());
}

#[test]
fn matter_emits_and_uses_configurable_font_tokens() {
    let (_, raw_css, _, embedded_fs) = resolve_layout_assets(Path::new("."), "matter", None);
    let css = CssOptimizer::process(&raw_css, &HashMap::new(), &embedded_fs, false).unwrap();

    assert!(css.contains("--ui-typescale-font-sans: system-ui"));
    assert!(css.contains("--ui-typescale-font-monospace: \"IBM Plex Mono\", ui-monospace"));
    assert!(css.contains("font-family: var(--ui-typescale-font-sans)"));
    assert!(css.contains("font-family: var(--ui-typescale-font-monospace)"));
}

#[test]
fn matter_sidebar_backdrop_preserves_fixed_positioning_and_excludes_ripple() {
    let (_, raw_css, app_js, embedded_fs) = resolve_layout_assets(Path::new("."), "matter", None);
    let css = CssOptimizer::process(&raw_css, &HashMap::new(), &embedded_fs, false).unwrap();

    assert!(
        css.contains(".sidebar-backdrop") && css.contains("position: fixed !important"),
        "sidebar-backdrop must enforce fixed !important positioning to survive cascade overrides"
    );
    assert!(
        app_js.contains("button:not(.sidebar-backdrop)"),
        "ripple target selector must explicitly exclude sidebar-backdrop"
    );
    assert!(
        app_js.contains("button.classList.contains('sidebar-backdrop')"),
        "ripple effect function must guard against sidebar-backdrop"
    );
    assert!(
        !app_js.contains("$('#sidebarBackdrop')?.addEventListener('click'"),
        "sidebarBackdrop should not have redundant click listener"
    );
}

#[test]
fn bundled_styles_keep_ui_colors_in_the_token_layer() {
    fn inspect(dir: &include_dir::Dir<'_>, hex_color: &regex::Regex, violations: &mut Vec<String>) {
        for file in dir.files() {
            let path = file.path().to_string_lossy();
            let is_layout_source =
                path.ends_with(".scss") || path.ends_with(".html") || path.ends_with(".js");
            if !is_layout_source || path.ends_with("_tokens.scss") || path.ends_with("shell.html") {
                continue;
            }
            let Some(contents) = file.contents_utf8() else {
                continue;
            };
            for (index, line) in contents.lines().enumerate() {
                let trimmed = line.trim_start();
                let is_syntax_rule = trimmed.starts_with(".syn-");
                let has_literal_color =
                    hex_color.is_match(line) || line.contains("rgb(") || line.contains("rgba(");
                if has_literal_color && !is_syntax_rule {
                    violations.push(format!("{}:{}: {}", path, index + 1, line.trim()));
                }
            }
        }
        for child in dir.dirs() {
            inspect(child, hex_color, violations);
        }
    }

    let mut violations = Vec::new();
    let hex_color = regex::Regex::new(r"(?i)#[0-9a-f]{3,8}\b").unwrap();
    inspect(&LAYOUTS_DIR, &hex_color, &mut violations);
    assert!(
        violations.is_empty(),
        "hard-coded UI colors must live in _tokens.scss:\n{}",
        violations.join("\n")
    );
}

#[test]
fn visual_gallery_fixture_parses_validates_and_renders_markdown_components() {
    let temp_dir = tempfile::tempdir().unwrap();
    let docs_dir = temp_dir.path().join("docs");
    std::fs::create_dir_all(&docs_dir).unwrap();
    std::fs::write(
        docs_dir.join("index.md"),
        "# Test docs\n\nA valid start page.\n\n## Explore\n\n- [Gallery](/gallery) — Render the component fixture.\n",
    )
    .unwrap();
    std::fs::write(
        docs_dir.join("gallery.md"),
        include_str!("../../testdata/visual-gallery.md"),
    )
    .unwrap();

    let config = DocsConfig {
        source_dir: docs_dir.to_string_lossy().to_string(),
        out: temp_dir.path().join("site").to_string_lossy().to_string(),
        site_url: "https://example.com/".to_string(),
        ..DocsConfig::default()
    };
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
        temp_dir.path(),
    ));
    let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);
    let mut stats = DocsDebugStats::default();
    let site = parse_docs(temp_dir.path(), &config, &logger, &mut stats).unwrap();
    let output = render_docs(temp_dir.path(), &config, &site, &logger, &mut stats).unwrap();
    let html = std::fs::read_to_string(output.join("gallery/index.html")).unwrap();

    for expected in [
        "doc-svgbob",
        "doc-mermaid",
        "math-block",
        "doc-list-stepper",
        "doc-list-task",
        "doc-list-def",
        "doc-list-fields",
        "docs-callout",
    ] {
        assert!(html.contains(expected), "fixture did not render {expected}");
    }
    assert!(html.matches("class=\"mermaid-svg-target\"").count() >= 1);
    for forbidden in [
        ["mermaid", ".esm"].concat(),
        ["window", ".mermaid"].concat(),
        ["js", "delivr"].concat(),
    ] {
        assert!(!html.contains(&forbidden));
    }
}

#[test]
fn malformed_mermaid_fails_even_when_link_validation_is_disabled() {
    let temp_dir = tempfile::tempdir().unwrap();
    let docs_dir = temp_dir.path().join("docs");
    std::fs::create_dir_all(&docs_dir).unwrap();
    std::fs::write(
        docs_dir.join("index.md"),
        "# Test docs\n\nA valid start page.\n\n## Explore\n\n- [Broken](/broken) — Broken diagram.\n",
    )
    .unwrap();
    std::fs::write(
        docs_dir.join("broken.md"),
        "# Broken\n\n```mermaid \"Bad graph\"\nflowchart LR\n  A --\n```\n",
    )
    .unwrap();

    let output = temp_dir.path().join("site");
    let config = DocsConfig {
        source_dir: docs_dir.to_string_lossy().to_string(),
        out: output.to_string_lossy().to_string(),
        site_url: "https://example.com/".to_string(),
        validate_links: false,
        ..DocsConfig::default()
    };
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
        temp_dir.path(),
    ));
    let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);
    let mut stats = DocsDebugStats::default();
    let site = parse_docs(temp_dir.path(), &config, &logger, &mut stats).unwrap();
    let error = render_docs(temp_dir.path(), &config, &site, &logger, &mut stats).unwrap_err();
    let error = format!("{error:?}");

    assert!(error.contains("issue-docs-mermaid-failed"));
    assert!(error.contains("broken.md"));
    assert!(error.contains("Bad graph"));
    assert!(!output.exists());
}

#[test]
fn mermaid_is_offline_in_separated_and_single_file_bundles() {
    let temp_dir = tempfile::tempdir().unwrap();
    let docs_dir = temp_dir.path().join("docs");
    std::fs::create_dir_all(&docs_dir).unwrap();
    std::fs::write(
        docs_dir.join("index.md"),
        "# Test docs\n\nA valid start page.\n\n## Explore\n\n- [Graph](/graph) — Compiled diagram.\n",
    )
    .unwrap();
    std::fs::write(
        docs_dir.join("graph.md"),
        "# Graph\n\n```mermaid \"Offline graph\"\nflowchart LR\n  A --> B\n```\n",
    )
    .unwrap();

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
        temp_dir.path(),
    ));
    let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);

    for bundle in ["separated", "single-file"] {
        let config = DocsConfig {
            source_dir: docs_dir.to_string_lossy().to_string(),
            out: temp_dir.path().join(bundle).to_string_lossy().to_string(),
            site_url: "https://example.com/".to_string(),
            bundle: if bundle == "single-file" {
                BundleMode::SingleFile
            } else {
                BundleMode::Separated
            },
            ..DocsConfig::default()
        };
        let mut stats = DocsDebugStats::default();
        let site = parse_docs(temp_dir.path(), &config, &logger, &mut stats).unwrap();
        let output = render_docs(temp_dir.path(), &config, &site, &logger, &mut stats).unwrap();
        let html = std::fs::read_to_string(output.join("graph/index.html")).unwrap();

        assert_eq!(html.matches("class=\"mermaid-svg-target\"").count(), 1);
        assert!(html.contains("class=\"mermaid-source\" hidden>flowchart LR"));
        assert!(html.contains("<svg id=\"diagram-graph-mermaid-1\""));
        for forbidden in [
            ["mermaid", ".esm"].concat(),
            ["window", ".mermaid"].concat(),
            ["js", "delivr"].concat(),
        ] {
            assert!(!html.contains(&forbidden));
        }
    }
}

#[test]
fn tape_fences_render_as_static_code_without_running_shell_commands() {
    let temp_dir = tempfile::tempdir().unwrap();
    let docs_dir = temp_dir.path().join("docs");
    std::fs::create_dir_all(&docs_dir).unwrap();
    std::fs::write(
        docs_dir.join("index.md"),
        "# Test docs\n\nA valid start page.\n\n## Explore\n\n- [Tape](/tape) — Run a terminal tape.\n",
    )
    .unwrap();
    std::fs::write(
        docs_dir.join("tape.md"),
        "# Tape\n\n```tape title=\"Build proof\" rows=4 cols=40\ntype \"pwd\"\nenter\nwait\n```\n",
    )
    .unwrap();

    let config = DocsConfig {
        source_dir: docs_dir.to_string_lossy().to_string(),
        out: temp_dir.path().join("site").to_string_lossy().to_string(),
        site_url: "https://example.com/".to_string(),
        ..DocsConfig::default()
    };
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
        temp_dir.path(),
    ));
    let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);
    let mut stats = DocsDebugStats::default();
    let site = parse_docs(temp_dir.path(), &config, &logger, &mut stats).unwrap();
    let output = render_docs(temp_dir.path(), &config, &site, &logger, &mut stats).unwrap();
    let page = std::fs::read_to_string(output.join("tape/index.html")).unwrap();
    assert!(page.contains("Build proof"));
    assert!(page.contains("ESH"));
    assert!(page.contains("pwd"));
    assert!(!page.contains("terminal-tape-svg"));
    assert!(!page.contains("data-action=\"tape:toggle-playback\""));
    assert!(!output.join("assets/fonts").exists());
    assert!(!default_icons().contains("player-pause"));
}

#[test]
fn rebuilding_docs_prunes_removed_page_routes() {
    let temp_dir = tempfile::tempdir().unwrap();
    let docs_dir = temp_dir.path().join("docs");
    std::fs::create_dir_all(&docs_dir).unwrap();
    std::fs::write(
        docs_dir.join("index.md"),
        "# Test docs\n\nA valid start page.\n\n## Explore\n\n- [Legacy](/legacy) — Render the legacy page.\n",
    )
    .unwrap();
    std::fs::write(docs_dir.join("legacy.md"), "# Legacy\n").unwrap();

    let config = DocsConfig {
        source_dir: docs_dir.to_string_lossy().to_string(),
        out: temp_dir.path().join("site").to_string_lossy().to_string(),
        site_url: "https://example.com/".to_string(),
        ..DocsConfig::default()
    };
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
        temp_dir.path(),
    ));
    let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);

    let mut first_stats = DocsDebugStats::default();
    let first_site = parse_docs(temp_dir.path(), &config, &logger, &mut first_stats).unwrap();
    let output = render_docs(
        temp_dir.path(),
        &config,
        &first_site,
        &logger,
        &mut first_stats,
    )
    .unwrap();
    assert!(output.join("legacy/index.html").exists());

    std::fs::remove_file(docs_dir.join("legacy.md")).unwrap();
    std::fs::write(docs_dir.join("current.md"), "# Current\n").unwrap();
    std::fs::write(
        docs_dir.join("index.md"),
        "# Test docs\n\nA valid start page.\n\n## Explore\n\n- [Current](/current) — Render the current page.\n",
    )
    .unwrap();
    let mut second_stats = DocsDebugStats::default();
    let second_site = parse_docs(temp_dir.path(), &config, &logger, &mut second_stats).unwrap();
    render_docs(
        temp_dir.path(),
        &config,
        &second_site,
        &logger,
        &mut second_stats,
    )
    .unwrap();

    assert!(!output.join("legacy/index.html").exists());
    assert!(output.join("current/index.html").exists());
}

#[test]
fn brandless_docs_build_uses_neutral_defaults_without_hardcoded_brand() {
    let temp_dir = tempfile::tempdir().unwrap();
    let docs_dir = temp_dir.path().join("docs");
    std::fs::create_dir_all(&docs_dir).unwrap();
    std::fs::write(
        docs_dir.join("index.md"),
        "# Acme Corp Guide\n\nWelcome to Acme Corp documentation.\n\n## Section 1\n\n- [Page 1](/page-one) — Subpage\n",
    )
    .unwrap();
    std::fs::write(docs_dir.join("page-one.md"), "# Page 1\n\nDetails here.\n").unwrap();

    let config = DocsConfig {
        source_dir: docs_dir.to_string_lossy().to_string(),
        out: temp_dir.path().join("site").to_string_lossy().to_string(),
        site_url: "https://example.com/".to_string(),
        ..DocsConfig::default()
    };
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
        temp_dir.path(),
    ));
    let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);
    let mut stats = DocsDebugStats::default();
    let site = parse_docs(temp_dir.path(), &config, &logger, &mut stats).unwrap();
    let output = render_docs(temp_dir.path(), &config, &site, &logger, &mut stats).unwrap();

    let index_html = std::fs::read_to_string(output.join("index.html")).unwrap();
    let page_html = std::fs::read_to_string(output.join("page-one/index.html")).unwrap();

    // 1. Check neutral glyph is rendered
    assert!(
        index_html.contains("brand-fallback-glyph"),
        "index.html should have brand-fallback-glyph"
    );
    assert!(
        index_html.contains('🕮'),
        "index.html should contain neutral glyph"
    );
    assert!(
        page_html.contains("brand-fallback-glyph"),
        "page-one should have brand-fallback-glyph"
    );

    // 2. Check no hardcoded vendor branding leaked
    assert!(
        !index_html.contains("EVIN documentation footer"),
        "footer aria label should be neutral"
    );
    assert!(
        !index_html.to_lowercase().contains("dukasn"),
        "should not leak author/repo handles"
    );

    // 3. No footer section in index.md means no <footer> tag in output
    assert!(
        !index_html.contains("<footer"),
        "no footer should be rendered when not authored"
    );

    // 4. Neutral favicon was generated
    let favicon_svg = std::fs::read_to_string(output.join("assets/favicons/favicon.svg")).unwrap();
    assert!(favicon_svg.contains("<svg"), "favicon.svg should exist");
    assert!(
        !favicon_svg.contains("#1ED4BE"),
        "favicon must not contain EVIN brand teal"
    );
}

#[test]
fn custom_extension_brand_assets_render_in_output() {
    let temp_dir = tempfile::tempdir().unwrap();
    let docs_dir = temp_dir.path().join("docs");
    std::fs::create_dir_all(&docs_dir).unwrap();
    std::fs::write(
        docs_dir.join("index.md"),
        "# Acme Corp Guide\n\nWelcome to Acme Corp.\n\n## Section\n\n- [P1](/p1) — P1\n\n## Footer\n\nAcme Copyright 2026\n\n### Links\n\n- [Acme](https://acme.org)\n",
    )
    .unwrap();
    std::fs::write(docs_dir.join("p1.md"), "# P1\n").unwrap();

    // Set up conventional extension assets under .evin/actions/docs
    let ext_dir = temp_dir.path().join(".evin/actions/docs");
    std::fs::create_dir_all(&ext_dir).unwrap();
    std::fs::write(
        ext_dir.join("logo.svg"),
        "<svg id=\"acme-logo\"><circle r=\"10\"/></svg>",
    )
    .unwrap();
    std::fs::write(
        ext_dir.join("favicon.svg"),
        "<svg id=\"acme-favicon\"></svg>",
    )
    .unwrap();

    let config = DocsConfig {
        source_dir: docs_dir.to_string_lossy().to_string(),
        out: temp_dir.path().join("site").to_string_lossy().to_string(),
        site_url: "https://example.com/".to_string(),
        ..DocsConfig::default()
    };
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
        temp_dir.path(),
    ));
    let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);
    let mut stats = DocsDebugStats::default();
    let site = parse_docs(temp_dir.path(), &config, &logger, &mut stats).unwrap();
    let output = render_docs(temp_dir.path(), &config, &site, &logger, &mut stats).unwrap();

    let index_html = std::fs::read_to_string(output.join("index.html")).unwrap();

    // Custom logo rendered in the site header.
    assert!(
        index_html.contains("id=\"acme-logo\""),
        "custom logo SVG must be rendered in index.html"
    );
    assert!(
        index_html.contains("brand-logo-custom"),
        "custom logo class must be present"
    );
    assert!(!index_html.contains("<footer"));

    // Custom favicon was used
    let favicon_svg = std::fs::read_to_string(output.join("assets/favicons/favicon.svg")).unwrap();
    assert!(
        favicon_svg.contains("acme-favicon"),
        "favicon should match custom extension favicon"
    );
}
