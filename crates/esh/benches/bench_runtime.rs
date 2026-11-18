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

use criterion::{Criterion, criterion_group, criterion_main};
use esh::{Engine, ExecutionContext};
use std::hint::black_box;
use tempfile::tempdir;

fn bench_runtime(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut group = c.benchmark_group("runtime");

    group.bench_function("plan_cache_hit_eval", |b| {
        let engine = Engine::new();
        let script = "str -u 'hello world' && str -l 'EVIN RUNTIME'";

        b.iter(|| {
            rt.block_on(async {
                let mut ctx = ExecutionContext::new();
                ctx.enable_capture();
                let _ = engine.eval_str(black_box(script), &mut ctx).await.unwrap();
            });
        });
    });

    group.bench_function("in_process_data_and_path_eval", |b| {
        let engine = Engine::new();
        let script = r#"
            path -d /usr/local/bin/esh
            path -b /usr/local/bin/esh
            json -g pkg.version '{"pkg": {"version": "1.0.0"}}'
        "#;

        b.iter(|| {
            rt.block_on(async {
                let mut ctx = ExecutionContext::new();
                ctx.enable_capture();
                let _ = engine.eval_str(black_box(script), &mut ctx).await.unwrap();
            });
        });
    });

    group.bench_function("in_process_fs_operations", |b| {
        let tmp = tempdir().unwrap();
        let engine = Engine::new();
        let script = "mkdir -p ./sub1/sub2 && touch ./sub1/sub2/file.txt";

        b.iter(|| {
            rt.block_on(async {
                let mut ctx = ExecutionContext::new();
                ctx.enable_capture();
                let _ = ctx.set_cwd(tmp.path());
                let _ = engine.eval_str(black_box(script), &mut ctx).await.unwrap();
            });
        });
    });

    group.finish();
}

criterion_group!(benches, bench_runtime);
criterion_main!(benches);
