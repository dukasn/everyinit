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

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use evin_action_docs::v1::{
    assets::ResolvedAssets,
    ast::{DocFormat, DocNode, DocPage, DocSiteAst, InlineNode},
    renderer::{HtmlRenderer, PreparedRenderContext},
    seo::SeoCatalog,
    templates::ComponentRegistry,
};
use evin_doc::SiteIndex;
use std::collections::HashMap;

fn fixture_site(page_count: usize) -> DocSiteAst {
    let mut pages = Vec::with_capacity(page_count + 1);
    pages.push(page("index", "index.rst", "Documentation"));
    for number in 0..page_count {
        let slug = format!("guides/page-{number}");
        pages.push(page(&slug, &format!("guides/page-{number}.rst"), "Guide"));
    }
    DocSiteAst {
        version: "1.0".to_string(),
        title: Some("Documentation".to_string()),
        reference_prefix: "REF".to_string(),
        pages,
    }
}

fn page(slug: &str, source_path: &str, title: &str) -> DocPage {
    DocPage {
        slug: slug.to_string(),
        source_path: source_path.to_string(),
        format: DocFormat::ReStructuredText,
        metadata: HashMap::from([(
            "title".to_string(),
            serde_json::Value::String(title.to_string()),
        )]),
        reference: None,
        toc: Vec::new(),
        ast: vec![DocNode::Paragraph {
            children: vec![InlineNode::Text {
                content: "Prepared rendering benchmark content.".to_string(),
            }],
        }],
    }
}

fn publishing_benchmarks(criterion: &mut Criterion) {
    let site = fixture_site(256);
    let assets = ResolvedAssets::default();
    let registry = ComponentRegistry::empty();
    let seo = SeoCatalog::build(&site, "https://example.com/docs/", "en").unwrap();

    criterion.bench_function("prepared_render_context_large_site", |bencher| {
        bencher.iter(|| {
            black_box(PreparedRenderContext::new(
                &site,
                SiteIndex::build(&site).unwrap(),
                &assets,
                "<content></content><navigation></navigation>",
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
            ))
        })
    });

    let context = PreparedRenderContext::new(
        &site,
        SiteIndex::build(&site).unwrap(),
        &assets,
        "<content></content><navigation></navigation><pagination></pagination>",
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
    );
    criterion.bench_function("render_prepared_large_site", |bencher| {
        bencher.iter(|| black_box(HtmlRenderer::render_prepared(black_box(&context))))
    });
}

criterion_group!(benches, publishing_benchmarks);
criterion_main!(benches);
