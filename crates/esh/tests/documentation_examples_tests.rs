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

use esh::Engine;

fn esh_blocks(source: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = None;

    for line in source.lines() {
        if current.is_none() && line.trim() == "```esh" {
            current = Some(Vec::new());
            continue;
        }
        if line.trim() == "```" {
            if let Some(lines) = current.take() {
                blocks.push(lines.join("\n"));
            }
            continue;
        }
        if let Some(lines) = &mut current {
            lines.push(line);
        }
    }

    assert!(current.is_none(), "unclosed ESH code fence");
    blocks
}

#[test]
fn public_esh_examples_compile() {
    let engine = Engine::new();
    let documents = [
        (
            "writing ESH scripts",
            include_str!("../../../docs/book/esh/10.first-script.md"),
        ),
        (
            "ESH data, files, and HTTP",
            include_str!("../../../docs/book/esh/20.work-with-project-data.md"),
        ),
        (
            "ESH control flow and failures",
            include_str!("../../../docs/book/esh/30.control-flow-and-failures.md"),
        ),
        (
            "ESH parallel automation",
            include_str!("../../../docs/book/esh/40.parallel-work.md"),
        ),
        (
            "ESH automation recipes",
            include_str!("../../../docs/book/esh/50.automation-recipes.md"),
        ),
        (
            "ESH Language Reference",
            include_str!("../../../docs/reference/esh/90.language-reference.3000.md"),
        ),
    ];

    for (document, source) in documents {
        let blocks = esh_blocks(source);
        assert!(!blocks.is_empty(), "{document} has no ESH examples");
        for (index, block) in blocks.iter().enumerate() {
            engine.compile(block).unwrap_or_else(|error| {
                panic!(
                    "{document}, ESH example {} does not compile: {error}",
                    index + 1
                )
            });
        }
    }
}

#[tokio::test]
async fn json_project_data_example_runs_as_documented() {
    let engine = Engine::new();
    let documents = [
        (
            "ESH data guide",
            include_str!("../../../docs/book/esh/20.work-with-project-data.md"),
        ),
        (
            "ESH runtime reference",
            include_str!("../../../docs/reference/esh/100.runtime-reference.3010.md"),
        ),
    ];

    for (document, source) in documents {
        let block = esh_blocks(source)
            .into_iter()
            .find(|block| {
                block.starts_with("PACKAGE='{\"name\":\"everyinit\"")
                    && block.contains("json -g downloads.bytes")
            })
            .unwrap_or_else(|| panic!("{document} JSON example must be present"));
        let mut ctx = esh::ExecutionContext::new();
        ctx.enable_capture();
        let status = engine
            .eval_str(&block, &mut ctx)
            .await
            .unwrap_or_else(|error| panic!("{document} JSON example failed: {error}"));

        assert_eq!(status, 0, "{document} JSON example status");
        assert_eq!(
            ctx.get_captured_stdout().unwrap(),
            "everyinit 1.2.3 ships 2048 bytes\n",
            "{document} JSON example output"
        );
        assert_eq!(
            ctx.get_captured_stderr().unwrap(),
            "",
            "{document} JSON example stderr"
        );
    }
}
