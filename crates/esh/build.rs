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

use evin_doc::ast::LspCatalogExport;
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR must be set");
    let dest_path = Path::new(&out_dir).join("catalog_gen.rs");

    let catalog_path = Path::new("assets/builtins-catalog.json");
    println!("cargo:rerun-if-changed={}", catalog_path.display());

    if !catalog_path.exists() {
        panic!(
            "\n\n========================================================================\n\
             [FATAL ERROR] Pre-compiled documentation catalog not found: {}\n\
             ESH requires 'assets/builtins-catalog.json' for zero-fallback .rodata baking.\n\
             Run 'cargo test -p evin-action-docs' or 'evin run docs:build-lsp' to generate.\n\
             ========================================================================\n\n",
            catalog_path.display()
        );
    }

    let raw_json = fs::read_to_string(catalog_path).unwrap_or_else(|e| {
        panic!("Failed to read {}: {e}", catalog_path.display());
    });

    let catalog: LspCatalogExport = serde_json::from_str(&raw_json).unwrap_or_else(|e| {
        panic!(
            "Malformed catalog artifact in {}: {e}",
            catalog_path.display()
        );
    });

    let mut out = String::new();

    // 1. Generate BUILTIN_CATALOG static slice
    out.push_str("pub static BUILTIN_CATALOG: &[BuiltinInfo] = &[\n");
    for (name, meta) in &catalog.symbols {
        let handler = match meta.handler.as_str() {
            "" => match name.as_str() {
                "break" => "builtins::r#break".to_string(),
                "continue" => "builtins::r#continue".to_string(),
                "return" => "builtins::r#return".to_string(),
                "[" => "builtins::test".to_string(),
                "typeset" => "builtins::declare".to_string(),
                other => format!("builtins::{}", other),
            },
            "builtins::typeset" => "builtins::declare".to_string(),
            other => other.to_string(),
        };

        out.push_str(&format!(
            "    BuiltinInfo {{\n        name: {:?},\n        synopsis: {:?},\n        summary: {:?},\n        doc_markdown: {:?},\n        doc_plaintext: {:?},\n        doc_ansi: {:?},\n        handler: {},\n    }},\n",
            name, meta.synopsis, meta.summary, meta.hover_markdown, meta.hover_plaintext, meta.terminal_ansi, handler
        ));
    }
    out.push_str("];\n\n");

    // 2. Generate KEYWORD_CATALOG static slice
    out.push_str("pub static KEYWORD_CATALOG: &[KeywordInfo] = &[\n");
    for (name, meta) in &catalog.keywords {
        out.push_str(&format!(
            "    KeywordInfo {{\n        name: {:?},\n        synopsis: {:?},\n        summary: {:?},\n        doc_markdown: {:?},\n        doc_plaintext: {:?},\n        doc_ansi: {:?},\n    }},\n",
            name, meta.synopsis, meta.summary, meta.hover_markdown, meta.hover_plaintext, meta.terminal_ansi
        ));
    }
    out.push_str("];\n\n");

    // 3. Generate BUILTIN_DISPATCH_LUT phf_map
    out.push_str(
        "pub static BUILTIN_DISPATCH_LUT: phf::Map<&'static str, BuiltinFn> = phf::phf_map! {\n",
    );
    for (name, meta) in &catalog.symbols {
        let handler = match meta.handler.as_str() {
            "" => match name.as_str() {
                "break" => "builtins::r#break".to_string(),
                "continue" => "builtins::r#continue".to_string(),
                "return" => "builtins::r#return".to_string(),
                "[" => "builtins::test".to_string(),
                "typeset" => "builtins::declare".to_string(),
                other => format!("builtins::{}", other),
            },
            "builtins::typeset" => "builtins::declare".to_string(),
            other => other.to_string(),
        };
        out.push_str(&format!("    {:?} => {},\n", name, handler));
    }
    out.push_str("};\n\n");

    fs::write(&dest_path, out).expect("Failed to write catalog_gen.rs");
}
