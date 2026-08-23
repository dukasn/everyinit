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
use evin_doc::{
    CompileOptions, DocFormat, DocumentInput, SourceResolver, compile_site, latex_to_unicode,
};
use std::path::PathBuf;

fn corpus() -> Vec<(evin_doc::ResolvedDocFile, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs");
    SourceResolver::resolve_all(&root, &[], &[])
        .expect("documentation corpus resolves")
        .into_iter()
        .filter_map(|file| {
            std::fs::read_to_string(&file.absolute_path)
                .ok()
                .map(|content| (file, content))
        })
        .collect()
}

fn compiler_benchmarks(criterion: &mut Criterion) {
    let documents = corpus();
    criterion.bench_function("compile_docs_corpus", |bencher| {
        bencher.iter(|| {
            let inputs = documents.iter().map(|(file, content)| DocumentInput {
                source_path: file.relative_path.to_string_lossy().to_string(),
                slug: file.slug.clone(),
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
            });
            black_box(
                compile_site(inputs, CompileOptions::default(), Some("Evin".to_string())).unwrap(),
            );
        })
    });
    criterion.bench_function("unicode_math", |bencher| {
        bencher.iter(|| {
            black_box(latex_to_unicode(black_box(
                r"\frac{\alpha_{10}}{\sqrt{2}} + \subseteq \varepsilon",
            )))
        })
    });
}

criterion_group!(benches, compiler_benchmarks);
criterion_main!(benches);
