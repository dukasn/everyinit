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

use bumpalo::Bump;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use esh::lexer::cursor::Lexer;
use esh::parser::pratt::Parser;
use esh::planner::compiler::compile_script;
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

fn bench_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser");
    group.throughput(Throughput::Bytes(COMPLEX_SCRIPT.len() as u64));

    group.bench_function("parse_into_bumpalo_arena", |b| {
        b.iter(|| {
            let arena = Bump::new();
            let mut lexer = Lexer::new(black_box(COMPLEX_SCRIPT));
            let mut tokens = Vec::new();
            while let Ok(t) = lexer.next_token() {
                let is_eof = matches!(t.kind, esh::lexer::token::TokenKind::Eof);
                tokens.push(t);
                if is_eof {
                    break;
                }
            }
            let mut parser = Parser::new(&arena, tokens);
            let _ast = parser.parse_script().unwrap();
        });
    });

    group.bench_function("lower_ast_to_ir_plan", |b| {
        b.iter(|| {
            let _plan = compile_script(black_box(COMPLEX_SCRIPT)).unwrap();
        });
    });

    group.finish();
}

criterion_group!(benches, bench_parser);
criterion_main!(benches);
