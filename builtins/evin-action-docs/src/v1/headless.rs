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

mod config;
use crate::v1::ast::{DocFormat, DocPage, DocSiteAst};
use config::validate;
pub use config::{HeadlessConfig, HeadlessI18n, VersionSource, validate_config};
use evin_action_sdk::RunnerContext;
use evin_diagnostic::{Diagnostic, IssueCode};
use evin_doc::SourceResolver;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct Variant {
    version: String,
    locale: String,
    requested_locale: String,
    content_locale: String,
    is_fallback: bool,
    page: DocPage,
    page_file: String,
    route: String,
    assets: crate::v1::assets::ResolvedAssets,
    asset_key: String,
}

#[derive(Debug, Serialize)]
struct PageArtifact<'a> {
    schema: &'static str,
    id: &'a str,
    version: &'a str,
    locale: &'a str,
    content_locale: &'a str,
    source_path: &'a str,
    route: &'a str,
    navigation_order: Option<u64>,
    is_fallback: bool,
    page: &'a DocPage,
}

#[derive(Debug, Serialize)]
struct RouteRecord {
    path: String,
    page_id: String,
    page_file: String,
    version: String,
    locale: String,
    content_locale: String,
    is_fallback: bool,
    navigation_order: Option<u64>,
    counterparts: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Serialize)]
struct Manifest {
    schema: &'static str,
    format_version: u32,
    latest_version: String,
    default_locale: String,
    versions: Vec<VersionRecord>,
    locales: Vec<String>,
    routes: Vec<RouteRecord>,
    assets: &'static str,
    search_indexes: Vec<crate::v1::search::SearchArtifactRecord>,
    diagnostics: Vec<BuildDiagnostic>,
}

#[derive(Debug, Serialize)]
struct BuildDiagnostic {
    severity: &'static str,
    code: &'static str,
    message: String,
    version: String,
    locale: String,
    route: String,
}

#[derive(Debug, Serialize)]
struct VersionRecord {
    id: String,
    latest: bool,
}

fn diagnostic(code: IssueCode) -> Diagnostic {
    evin_locales::diagnostic(code)
}

fn absolute(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn parse_source(
    root: &Path,
    config: &HeadlessConfig,
    source_dir: &Path,
) -> Result<DocSiteAst, Diagnostic> {
    let source_root = absolute(root, source_dir);
    let files = SourceResolver::resolve_all(&source_root, &config.include, &config.exclude)
        .map_err(|issue| {
            diagnostic(IssueCode::new("docs-source-discovery-failed").arg("error", issue.message))
        })?;
    let mut loaded = Vec::with_capacity(files.len());
    for file in files {
        let content = std::fs::read_to_string(&file.absolute_path).map_err(|error| {
            diagnostic(
                IssueCode::new("docs-source-read-failed")
                    .arg("path", file.absolute_path.display().to_string())
                    .arg("error", error.to_string()),
            )
            .with_file_location(file.relative_path.to_string_lossy(), None, None)
        })?;
        loaded.push((file, content));
    }
    let inputs = loaded
        .iter()
        .map(|(file, content)| evin_doc::DocumentInput {
            slug: file.slug.clone(),
            source_path: file.relative_path.to_string_lossy().replace('\\', "/"),
            format: if file.is_rst {
                DocFormat::ReStructuredText
            } else {
                DocFormat::Markdown
            },
            is_start_page: matches!(
                file.relative_path.to_str(),
                Some("index.md") | Some("index.rst")
            ),
            content,
        })
        .collect::<Vec<_>>();
    let prefix = config.reference_prefix.trim().to_ascii_uppercase();
    let mut site = evin_doc::compile_site(
        inputs,
        evin_doc::CompileOptions {
            extract_frontmatter: config.extract_frontmatter,
            reference_prefix: prefix,
            site_version: "1.0".to_string(),
        },
        None,
    )
    .map_err(|issue| {
        let mut diag = Diagnostic::new(
            evin_locales::t!("docs-source-parse-failed", error = issue.message),
            IssueCode::new(issue.code),
        );
        if let Some(path) = issue.source_path {
            diag = diag.with_file_location(path, None, None);
        }
        diag
    })?;
    if config.strict_filenames {
        validate_strict_filenames(&site)?;
    }
    // Ensure parser output is the canonical, renderer-free data model.
    for page in &mut site.pages {
        clear_rendered_fields(&mut page.ast);
    }
    Ok(site)
}

fn validate_strict_filenames(site: &DocSiteAst) -> Result<(), Diagnostic> {
    let filename =
        regex::Regex::new(r"^([0-9]+)\.([a-z0-9]+(?:-[a-z0-9]+)*)(?:\.([1-9][0-9]{3,}))?$")
            .expect("valid strict filename pattern");
    let mut orders = HashSet::new();
    for page in &site.pages {
        if matches!(page.source_path.as_str(), "index.md" | "index.rst") {
            if page.reference.is_some() {
                return Err(strict_filename_error(
                    &page.source_path,
                    evin_locales::t!("docs-strict-root-index-reference"),
                ));
            }
            continue;
        }
        let path = Path::new(&page.source_path);
        if !matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("md") | Some("rst")
        ) {
            return Err(strict_filename_error(
                &page.source_path,
                evin_locales::t!("docs-strict-filename-extension"),
            ));
        }
        let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        else {
            return Err(strict_filename_error(
                &page.source_path,
                evin_locales::t!("docs-strict-filename-requires-section"),
            ));
        };
        let section = parent.to_string_lossy().replace('\\', "/");
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            return Err(strict_filename_error(
                &page.source_path,
                evin_locales::t!("docs-strict-filename-invalid-utf8"),
            ));
        };
        let Some(captures) = filename.captures(stem) else {
            return Err(strict_filename_error(
                &page.source_path,
                evin_locales::t!("docs-strict-filename-format"),
            ));
        };
        let order = captures[1].parse::<u64>().map_err(|_| {
            strict_filename_error(
                &page.source_path,
                evin_locales::t!("docs-strict-navigation-order-too-large"),
            )
        })?;
        let slug = &captures[2];
        let reference_digits = captures.get(3).map(|capture| capture.as_str());
        let is_section_index = slug == "index";
        if is_section_index != (order == 0)
            || (is_section_index && (&captures[1] != "00" || reference_digits.is_some()))
        {
            return Err(strict_filename_error(
                &page.source_path,
                evin_locales::t!("docs-strict-section-index-name"),
            ));
        }
        if !orders.insert((section.clone(), order)) {
            return Err(strict_filename_error(
                &page.source_path,
                evin_locales::t!(
                    "docs-strict-navigation-order-duplicate",
                    order = order,
                    section = section
                ),
            ));
        }
        match (reference_digits, page.reference.as_ref()) {
            (Some(digits), Some(reference)) if digits == reference.digits => {}
            (Some(_), Some(_)) => {
                return Err(strict_filename_error(
                    &page.source_path,
                    evin_locales::t!("docs-strict-reference-digits-mismatch"),
                ));
            }
            (Some(_), None) => {
                return Err(strict_filename_error(
                    &page.source_path,
                    evin_locales::t!("docs-strict-reference-suffix-invalid"),
                ));
            }
            (None, Some(_)) => {
                return Err(strict_filename_error(
                    &page.source_path,
                    evin_locales::t!("docs-strict-reference-suffix-required"),
                ));
            }
            (None, None) => {}
        }
    }
    Ok(())
}

fn strict_filename_error(source_path: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(message, IssueCode::new("invalid-filename")).with_file_location(
        source_path.to_string(),
        None,
        None,
    )
}

fn clear_rendered_fields(nodes: &mut [crate::v1::ast::DocNode]) {
    for node in nodes {
        match node {
            crate::v1::ast::DocNode::Tape { rendered_svg, .. } => *rendered_svg = None,
            crate::v1::ast::DocNode::Diagram {
                rendered_mermaid, ..
            } => *rendered_mermaid = None,
            crate::v1::ast::DocNode::Admonition { children, .. }
            | crate::v1::ast::DocNode::Directive { body: children, .. }
            | crate::v1::ast::DocNode::LspSymbol { children, .. } => {
                clear_rendered_fields(children)
            }
            crate::v1::ast::DocNode::List { items, .. } => {
                for item in items {
                    clear_rendered_fields(item);
                }
            }
            crate::v1::ast::DocNode::TaskList { items } => {
                for item in items {
                    clear_rendered_fields(&mut item.body);
                }
            }
            crate::v1::ast::DocNode::DefinitionList { items } => {
                for item in items {
                    clear_rendered_fields(&mut item.body);
                }
            }
            crate::v1::ast::DocNode::OptionList { items } => {
                for item in items {
                    clear_rendered_fields(&mut item.body);
                }
            }
            crate::v1::ast::DocNode::FieldList { items } => {
                for item in items {
                    clear_rendered_fields(&mut item.body);
                }
            }
            crate::v1::ast::DocNode::Stepper { steps } => {
                for item in steps {
                    clear_rendered_fields(&mut item.body);
                }
            }
            _ => {}
        }
    }
}

fn slug_route(slug: &str) -> String {
    if slug == "index" {
        "/".into()
    } else {
        let encoded = slug
            .split('/')
            .map(encode_route_segment)
            .collect::<Vec<_>>()
            .join("/");
        format!("/{encoded}")
    }
}

fn encode_route_segment(segment: &str) -> String {
    if matches!(segment, "." | "..") {
        return segment.bytes().map(|byte| format!("%{byte:02X}")).collect();
    }
    segment
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn public_route(
    slug: &str,
    version: &str,
    latest: &str,
    locale: &str,
    default_locale: &str,
) -> String {
    let mut segments = Vec::new();
    if version != latest {
        segments.push(version.to_string());
    }
    if locale != default_locale {
        segments.push(locale.to_string());
    }
    let page = slug_route(slug);
    let prefix = segments.join("/");
    match (prefix.is_empty(), page == "/") {
        (true, _) => page,
        (false, true) => format!("/{prefix}"),
        (false, false) => format!("/{prefix}{page}"),
    }
}

fn page_id(version: &str, locale: &str, slug: &str) -> String {
    opaque_id("page", &(version, locale, slug))
}

fn asset_id(version: &str, locale: &str) -> String {
    opaque_id("asset", &(version, locale))
}

fn opaque_id(prefix: &str, identity: &impl Serialize) -> String {
    let identity = serde_json::to_vec(identity).expect("generated identity is serializable");
    let digest = Sha256::digest(identity);
    format!(
        "{prefix}-{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Diagnostic> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            diagnostic(
                IssueCode::new("docs-output-directory-create-failed")
                    .arg("path", parent.display().to_string())
                    .arg("error", error.to_string()),
            )
        })?;
    }
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        diagnostic(IssueCode::new("docs-data-serialization-failed").arg("error", error.to_string()))
    })?;
    bytes.push(b'\n');
    std::fs::write(path, bytes).map_err(|error| {
        diagnostic(
            IssueCode::new("docs-output-write-failed")
                .arg("path", path.display().to_string())
                .arg("error", error.to_string()),
        )
    })
}

fn build_variants(
    root: &Path,
    config: &HeadlessConfig,
    logger: &evin_action_sdk::Logger,
) -> Result<(Vec<String>, String, Vec<Variant>), Diagnostic> {
    let (versions, latest) = if config.versions.is_empty() {
        (
            vec![VersionSource {
                id: "latest".into(),
                source_dir: None,
            }],
            "latest".to_string(),
        )
    } else {
        (
            config.versions.clone(),
            config.latest_version.clone().expect("validated"),
        )
    };
    let default_locale = config
        .i18n
        .as_ref()
        .map(|i18n| i18n.default_locale.clone())
        .unwrap_or_else(|| "en".into());
    let locales = config
        .i18n
        .as_ref()
        .map(|i18n| i18n.locales.clone())
        .unwrap_or_else(|| vec![default_locale.clone()]);
    let mut variants = Vec::new();
    for version in &versions {
        let version_root = version.source_dir.as_deref().unwrap_or(&config.source_dir);
        let mut sites = HashMap::<String, (DocSiteAst, crate::v1::assets::ResolvedAssets)>::new();
        for locale in &locales {
            let locale_root = if config.i18n.is_some() {
                version_root.join(locale)
            } else {
                version_root.to_path_buf()
            };
            let site = parse_source(root, config, &locale_root)?;
            if locale == &default_locale && site.pages.is_empty() {
                return Err(diagnostic(
                    IssueCode::new("docs-default-locale-pages-missing")
                        .arg("locale", locale)
                        .arg("path", absolute(root, &locale_root).display().to_string()),
                ));
            }
            let source_root = absolute(root, &locale_root);
            let assets =
                crate::v1::assets::ResolvedAssets::build(&source_root, &config.asset_dir, &site)?;
            if config.validate_links {
                let report =
                    crate::v1::validator::LinkValidator::validate_with_assets(&site, &assets);
                for issue in report.diagnostics {
                    if issue.severity == evin_diagnostic::Severity::Error {
                        return Err(issue);
                    }
                    logger.diagnostic(issue);
                }
            }
            sites.insert(locale.clone(), (site, assets));
        }
        let (base, base_assets) = sites
            .get(&default_locale)
            .expect("default locale is configured");
        for locale in &locales {
            let (localized, localized_assets) = sites.get(locale).expect("locale site was parsed");
            let translated = localized
                .pages
                .iter()
                .map(|page| (page.slug.as_str(), page))
                .collect::<HashMap<_, _>>();
            for base_page in &base.pages {
                let (page, content_locale, is_fallback, assets) =
                    if let Some(page) = translated.get(base_page.slug.as_str()) {
                        (
                            (*page).clone(),
                            locale.as_str(),
                            false,
                            localized_assets.clone(),
                        )
                    } else {
                        (
                            base_page.clone(),
                            default_locale.as_str(),
                            locale != &default_locale,
                            base_assets.clone(),
                        )
                    };
                let route = public_route(&page.slug, &version.id, &latest, locale, &default_locale);
                let id = page_id(&version.id, locale, &page.slug);
                let asset_key = asset_id(&version.id, content_locale);
                variants.push(Variant {
                    version: version.id.clone(),
                    locale: locale.clone(),
                    requested_locale: locale.clone(),
                    content_locale: content_locale.to_string(),
                    is_fallback,
                    page,
                    page_file: format!("pages/{id}.json"),
                    route,
                    assets,
                    asset_key,
                });
            }
            for page in &localized.pages {
                if !base
                    .pages
                    .iter()
                    .any(|base_page| base_page.slug == page.slug)
                {
                    let route =
                        public_route(&page.slug, &version.id, &latest, locale, &default_locale);
                    let id = page_id(&version.id, locale, &page.slug);
                    let asset_key = asset_id(&version.id, locale);
                    variants.push(Variant {
                        version: version.id.clone(),
                        locale: locale.clone(),
                        requested_locale: locale.clone(),
                        content_locale: locale.clone(),
                        is_fallback: false,
                        page: page.clone(),
                        page_file: format!("pages/{id}.json"),
                        route,
                        assets: localized_assets.clone(),
                        asset_key,
                    });
                }
            }
        }
    }
    variants.sort_by(|left, right| {
        (&left.version, &left.locale, &left.page.slug).cmp(&(
            &right.version,
            &right.locale,
            &right.page.slug,
        ))
    });
    let mut routes = HashSet::new();
    for variant in &variants {
        let canonical_route = if variant.route.len() > 1 {
            variant.route.trim_end_matches('/').to_string()
        } else {
            variant.route.clone()
        };
        if !routes.insert(canonical_route.clone()) {
            return Err(diagnostic(
                IssueCode::new("docs-generated-route-ambiguous").arg("route", canonical_route),
            ));
        }
    }
    Ok((locales, latest, variants))
}

fn publish(
    root: &Path,
    config: &HeadlessConfig,
    logger: &evin_action_sdk::Logger,
) -> Result<(PathBuf, usize), Diagnostic> {
    let (locales, latest, variants) = build_variants(root, config, logger)?;
    for variant in variants.iter().filter(|variant| variant.is_fallback) {
        logger.diagnostic(
            Diagnostic::new(
                evin_locales::t!(
                    "docs-translation-missing",
                    requested_locale = variant.requested_locale.as_str(),
                    page = variant.page.slug.as_str(),
                    content_locale = variant.content_locale.as_str()
                ),
                IssueCode::new("missing-translation"),
            )
            .with_severity(evin_diagnostic::Severity::Warning)
            .with_file_location(variant.page.source_path.clone(), None, None),
        );
    }
    let default_locale = config
        .i18n
        .as_ref()
        .map(|i18n| i18n.default_locale.as_str())
        .unwrap_or("en");
    let out = absolute(root, &config.out);
    let mut page_ids = HashMap::new();
    for variant in &variants {
        let id = page_id(&variant.version, &variant.locale, &variant.page.slug);
        page_ids.insert(
            (
                variant.version.clone(),
                variant.locale.clone(),
                variant.page.slug.clone(),
            ),
            id,
        );
    }
    let mut written_assets = HashSet::new();
    for variant in &variants {
        let id = page_id(&variant.version, &variant.locale, &variant.page.slug);
        write_json(
            &out.join(&variant.page_file),
            &PageArtifact {
                schema: "evin.docs.page/v1",
                id: &id,
                version: &variant.version,
                locale: &variant.requested_locale,
                content_locale: &variant.content_locale,
                source_path: &variant.page.source_path,
                route: &variant.route,
                navigation_order: evin_doc::SourceResolver::navigation_order(Path::new(
                    &variant.page.source_path,
                ))
                .ok()
                .flatten(),
                is_fallback: variant.is_fallback,
                page: &variant.page,
            },
        )?;
        if written_assets.insert(variant.asset_key.clone()) {
            let asset_root = out.join("assets").join(&variant.asset_key);
            variant.assets.write_to(&asset_root)?;
        }
    }
    let mut search_indexes = Vec::new();
    let search_groups = variants
        .iter()
        .map(|variant| (variant.version.clone(), variant.locale.clone()))
        .collect::<BTreeSet<_>>();
    for (version, locale) in &search_groups {
        let matching = variants
            .iter()
            .filter(|variant| variant.version == *version && variant.locale == *locale)
            .map(|variant| {
                (
                    variant.route.as_str(),
                    &variant.page,
                    page_id(version, locale, &variant.page.slug),
                )
            })
            .collect::<Vec<_>>();
        let binary = crate::v1::search::build_binary(&matching);
        let relative_path = format!("search/{version}/{locale}.bin");
        let search_path = out.join(&relative_path);
        if let Some(parent) = search_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                diagnostic(
                    IssueCode::new("docs-output-directory-create-failed")
                        .arg("path", parent.display().to_string())
                        .arg("error", error.to_string()),
                )
            })?;
        }
        std::fs::write(&search_path, &binary).map_err(|error| {
            diagnostic(
                IssueCode::new("docs-search-index-write-failed")
                    .arg("path", search_path.display().to_string())
                    .arg("error", error.to_string()),
            )
        })?;
        let digest = Sha256::digest(&binary);
        search_indexes.push(crate::v1::search::SearchArtifactRecord {
            version: version.clone(),
            locale: locale.clone(),
            path: relative_path,
            byte_length: binary.len() as u64,
            sha256: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
        });
    }
    let mut route_records = Vec::new();
    for variant in &variants {
        let id = page_id(&variant.version, &variant.locale, &variant.page.slug);
        let mut counterparts = BTreeMap::new();
        for version in variants
            .iter()
            .map(|candidate| candidate.version.as_str())
            .collect::<HashSet<_>>()
        {
            for locale in variants
                .iter()
                .map(|candidate| candidate.locale.as_str())
                .collect::<HashSet<_>>()
            {
                let counterpart = page_ids
                    .get(&(
                        version.to_string(),
                        locale.to_string(),
                        variant.page.slug.clone(),
                    ))
                    .cloned();
                counterparts.insert(format!("{version}/{locale}"), counterpart);
            }
        }
        route_records.push(RouteRecord {
            path: variant.route.clone(),
            page_id: id,
            page_file: variant.page_file.clone(),
            version: variant.version.clone(),
            locale: variant.requested_locale.clone(),
            content_locale: variant.content_locale.clone(),
            is_fallback: variant.is_fallback,
            navigation_order: evin_doc::SourceResolver::navigation_order(Path::new(
                &variant.page.source_path,
            ))
            .ok()
            .flatten(),
            counterparts,
        });
    }
    route_records.sort_by(|left, right| left.path.cmp(&right.path));
    let mut version_records = variants
        .iter()
        .map(|variant| variant.version.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    version_records.sort();
    let versions = version_records
        .into_iter()
        .map(|id| VersionRecord {
            latest: id == latest,
            id,
        })
        .collect();
    write_json(
        &out.join("manifest.json"),
        &Manifest {
            schema: "evin.docs.manifest/v1",
            format_version: 1,
            latest_version: latest,
            default_locale: default_locale.to_string(),
            versions,
            locales,
            routes: route_records,
            assets: "assets.json",
            search_indexes,
            diagnostics: variants
                .iter()
                .filter(|variant| variant.is_fallback)
                .map(|variant| BuildDiagnostic {
                    severity: "warning",
                    code: "missing-translation",
                    message: evin_locales::t!(
                        "docs-translation-missing",
                        requested_locale = variant.requested_locale.as_str(),
                        page = variant.page.slug.as_str(),
                        content_locale = variant.content_locale.as_str()
                    ),
                    version: variant.version.clone(),
                    locale: variant.requested_locale.clone(),
                    route: variant.route.clone(),
                })
                .collect(),
        },
    )?;
    let mut asset_map = BTreeMap::<String, BTreeMap<String, String>>::new();
    for variant in &variants {
        let id = page_id(&variant.version, &variant.locale, &variant.page.slug);
        let prefix = format!("assets/{}/", variant.asset_key);
        let entries = variant
            .assets
            .manifest_entries()
            .into_iter()
            .map(|(source, path)| (source, format!("{prefix}{path}")))
            .collect();
        asset_map.insert(id, entries);
    }
    write_json(&out.join("assets.json"), &asset_map)?;
    Ok((out, variants.len()))
}

pub async fn execute(
    ctx: &RunnerContext,
    with: Option<evin_flow::SpannedValue>,
) -> Result<(), Diagnostic> {
    let config: HeadlessConfig = ctx.load_config(with.as_ref()).map_err(|error| {
        diagnostic(IssueCode::new("docs-invalid-config").arg("error", error.message))
    })?;
    validate(&config).map_err(diagnostic)?;
    let root = ctx.fs.workspace_root();
    // Build to an isolated location for dry runs so no configured output files are touched.
    if ctx.mode == evin_action_sdk::PipelineMode::DryRun {
        let temp = tempfile::tempdir().map_err(|error| {
            diagnostic(
                IssueCode::new("docs-temporary-output-prepare-failed")
                    .arg("error", error.to_string()),
            )
        })?;
        let mut preview = config.clone();
        preview.out = temp.path().join("docs-data");
        let (_, page_count) = publish(root, &preview, &ctx.log)?;
        let manifest = absolute(root, &config.out).join("manifest.json");
        ctx.log.plan(evin_action_sdk::activity::DocsPlanBuild {
            output: manifest.display().to_string(),
        });
        ctx.state
            .set_output("manifest_path", &manifest.to_string_lossy());
        ctx.state.set_output("page_count", &page_count.to_string());
        return Ok(());
    }
    let (out, page_count) = publish(root, &config, &ctx.log)?;
    let manifest = out.join("manifest.json");
    ctx.state
        .set_output("manifest_path", &manifest.to_string_lossy());
    ctx.state.set_output("page_count", &page_count.to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_fluent_diagnostic(diagnostic: &Diagnostic, key: &str, expected_message: String) {
        assert_eq!(diagnostic.code.key, key);
        assert_eq!(
            evin_locales::render_issue(&diagnostic.code),
            expected_message
        );
        assert_eq!(diagnostic.title, expected_message);
    }

    fn logger() -> evin_action_sdk::Logger {
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default());
        evin_action_sdk::Logger::new("docs-test".into(), tx, secrets)
    }

    fn runner_context(root: &Path) -> RunnerContext {
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default());
        let logger = evin_action_sdk::Logger::new("docs-test".into(), tx.clone(), secrets.clone());
        let git = std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
            root.to_path_buf(),
            evin_action_sdk::PipelineMode::Normal,
            logger,
        ));
        evin_action_sdk::RunnerContextBuilder::new(
            "docs-test".into(),
            tx,
            root.to_path_buf(),
            git,
            secrets,
        )
        .build()
    }

    #[tokio::test]
    async fn invalid_runtime_config_uses_fluent_diagnostic() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = runner_context(temp.path());
        let with = evin_flow::parse_yaml("unknown: true\n").unwrap();
        let config_error =
            evin_action_sdk::parse_config::<HeadlessConfig>(Some(&with)).unwrap_err();

        let error = execute(&ctx, Some(with)).await.unwrap_err();
        assert_fluent_diagnostic(
            &error,
            "docs-invalid-config",
            evin_locales::t!("docs-invalid-config", error = config_error.message),
        );
    }

    #[test]
    fn missing_default_locale_pages_use_a_fluent_issue_code() {
        let temp = tempfile::tempdir().unwrap();
        let source_root = temp.path().join("docs");
        std::fs::create_dir_all(&source_root).unwrap();
        let with = evin_flow::parse_yaml("source_dir: docs\n").unwrap();
        let config: HeadlessConfig = evin_action_sdk::parse_config(Some(&with)).unwrap();

        let error = build_variants(temp.path(), &config, &logger()).unwrap_err();
        assert_eq!(error.code.key, "docs-default-locale-pages-missing");
        let locale = &error.code.args[0].1;
        let path = &error.code.args[1].1;
        assert_fluent_diagnostic(
            &error,
            "docs-default-locale-pages-missing",
            evin_locales::t!(
                "docs-default-locale-pages-missing",
                locale = locale,
                path = path
            ),
        );
    }

    #[test]
    fn overlapping_version_route_uses_a_fluent_issue_code() {
        let temp = tempfile::tempdir().unwrap();
        let latest_docs = temp.path().join("docs/latest/v1");
        let older_docs = temp.path().join("docs/v1");
        std::fs::create_dir_all(&latest_docs).unwrap();
        std::fs::create_dir_all(&older_docs).unwrap();
        std::fs::write(latest_docs.join("guide.md"), "# Latest guide\n").unwrap();
        std::fs::write(older_docs.join("guide.md"), "# Older guide\n").unwrap();
        let with = evin_flow::parse_yaml(
            "versions:\n  - id: latest\n    source_dir: docs/latest\n  - id: v1\n    source_dir: docs/v1\nlatest_version: latest\n",
        )
        .unwrap();
        let config: HeadlessConfig = evin_action_sdk::parse_config(Some(&with)).unwrap();

        let error = build_variants(temp.path(), &config, &logger()).unwrap_err();
        let route = &error.code.args[0].1;
        assert_fluent_diagnostic(
            &error,
            "docs-generated-route-ambiguous",
            evin_locales::t!("docs-generated-route-ambiguous", route = route),
        );
    }

    #[test]
    fn source_discovery_failure_uses_a_fluent_issue_code() {
        let temp = tempfile::tempdir().unwrap();
        let source_root = temp.path().join("docs");
        std::fs::create_dir_all(&source_root).unwrap();
        std::fs::write(
            source_root.join("999999999999999999999999.guide.md"),
            "Guide\n",
        )
        .unwrap();
        let with = evin_flow::parse_yaml("source_dir: docs\n").unwrap();
        let config: HeadlessConfig = evin_action_sdk::parse_config(Some(&with)).unwrap();

        let error = build_variants(temp.path(), &config, &logger()).unwrap_err();
        assert_eq!(error.code.key, "docs-source-discovery-failed");
        let detail = &error.code.args[0].1;
        assert_fluent_diagnostic(
            &error,
            "docs-source-discovery-failed",
            evin_locales::t!("docs-source-discovery-failed", error = detail),
        );
    }

    #[test]
    fn search_index_write_failure_uses_a_fluent_issue_code() {
        let temp = tempfile::tempdir().unwrap();
        let source_root = temp.path().join("docs");
        std::fs::create_dir_all(&source_root).unwrap();
        std::fs::write(source_root.join("guide.md"), "# Guide\n").unwrap();
        let blocked_index = temp.path().join("dist/docs-data/search/latest/en.bin");
        std::fs::create_dir_all(&blocked_index).unwrap();
        let with = evin_flow::parse_yaml("source_dir: docs\n").unwrap();
        let config: HeadlessConfig = evin_action_sdk::parse_config(Some(&with)).unwrap();

        let error = publish(temp.path(), &config, &logger()).unwrap_err();
        assert_eq!(error.code.key, "docs-search-index-write-failed");
        let path = &error.code.args[0].1;
        let detail = &error.code.args[1].1;
        assert_fluent_diagnostic(
            &error,
            "docs-search-index-write-failed",
            evin_locales::t!(
                "docs-search-index-write-failed",
                path = path,
                error = detail
            ),
        );
    }

    #[test]
    fn source_read_failure_uses_a_fluent_issue_code() {
        let temp = tempfile::tempdir().unwrap();
        let source_root = temp.path().join("docs");
        std::fs::create_dir_all(&source_root).unwrap();
        std::fs::write(source_root.join("guide.md"), [0xff]).unwrap();
        let with = evin_flow::parse_yaml("source_dir: docs\n").unwrap();
        let config: HeadlessConfig = evin_action_sdk::parse_config(Some(&with)).unwrap();

        let error = parse_source(temp.path(), &config, &config.source_dir).unwrap_err();
        let path = error
            .code
            .args
            .iter()
            .find(|(name, _)| *name == "path")
            .unwrap()
            .1
            .clone();
        let detail = error
            .code
            .args
            .iter()
            .find(|(name, _)| *name == "error")
            .unwrap()
            .1
            .clone();
        assert_fluent_diagnostic(
            &error,
            "docs-source-read-failed",
            evin_locales::t!("docs-source-read-failed", path = path, error = detail),
        );
    }

    #[test]
    fn output_directory_failure_uses_a_fluent_issue_code() {
        let temp = tempfile::tempdir().unwrap();
        let blocker = temp.path().join("blocked");
        std::fs::write(&blocker, "file").unwrap();
        let output = blocker.join("docs.json");

        let error = write_json(&output, &serde_json::json!({})).unwrap_err();
        let detail = std::fs::create_dir_all(&blocker).unwrap_err().to_string();
        assert_fluent_diagnostic(
            &error,
            "docs-output-directory-create-failed",
            evin_locales::t!(
                "docs-output-directory-create-failed",
                path = blocker.display().to_string(),
                error = detail
            ),
        );
    }

    #[test]
    fn output_file_failure_uses_a_fluent_issue_code() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("docs.json");
        std::fs::create_dir(&output).unwrap();

        let error = write_json(&output, &serde_json::json!({})).unwrap_err();
        let detail = std::fs::write(&output, b"{}").unwrap_err().to_string();
        assert_fluent_diagnostic(
            &error,
            "docs-output-write-failed",
            evin_locales::t!(
                "docs-output-write-failed",
                path = output.display().to_string(),
                error = detail
            ),
        );
    }

    #[test]
    fn serialization_failure_uses_a_fluent_issue_code() {
        struct FailingSerialization;

        impl serde::Serialize for FailingSerialization {
            fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                Err(serde::ser::Error::custom("test serialization failure"))
            }
        }

        let temp = tempfile::tempdir().unwrap();
        let error = write_json(&temp.path().join("docs.json"), &FailingSerialization).unwrap_err();
        let detail = &error.code.args[0].1;
        assert_fluent_diagnostic(
            &error,
            "docs-data-serialization-failed",
            evin_locales::t!("docs-data-serialization-failed", error = detail),
        );
    }
}
