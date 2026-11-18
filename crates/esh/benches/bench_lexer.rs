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

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use esh::lexer::cursor::Lexer;
use esh::lexer::token::TokenKind;
use std::hint::black_box;

const COMPLEX_SCRIPT: &str = r#"
#!/usr/bin/env esh
export TARGET_DIR="./dist"
mkdir -p "$TARGET_DIR"

deploy() {
    getopts "$@" \
        --about "Deploys build artifacts" \
        -e, --env <target> "Target environment" default="staging" \
        <artifact> "Build artifact" || return $?
    
    echo "Deploying $artifact to $env..."
}

{
    cargo build --release &| npm run build --prefix ./web
    deploy --env production ./dist/app.tar.gz
} || {
    echo "Failed at $ESH_FAILED_CMD (exit $?)"
    exit 1
}
"#;

fn bench_lexer(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer");
    group.throughput(Throughput::Bytes(COMPLEX_SCRIPT.len() as u64));

    group.bench_function("tokenize_complex_script", |b| {
        b.iter(|| {
            let mut lexer = Lexer::new(black_box(COMPLEX_SCRIPT));
            while let Ok(token) = lexer.next_token() {
                if matches!(token.kind, TokenKind::Eof) {
                    break;
                }
            }
        });
    });

    group.bench_function("tokenize_simple_one_liner", |b| {
        let script = "mkdir -p ./build && cargo build --release &| npm test";
        b.iter(|| {
            let mut lexer = Lexer::new(black_box(script));
            while let Ok(token) = lexer.next_token() {
                if matches!(token.kind, TokenKind::Eof) {
                    break;
                }
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_lexer);
criterion_main!(benches);
