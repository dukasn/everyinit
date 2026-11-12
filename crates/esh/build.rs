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

use evin_catalog::{CatalogExport, codegen::emit_static_catalog};
use std::env;
use std::fmt::Write;
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

    let catalog = CatalogExport::from_json(&raw_json).unwrap_or_else(|e| {
        panic!(
            "Malformed catalog artifact in {}: {e}",
            catalog_path.display()
        );
    });

    let mut out = emit_static_catalog(&catalog).expect("validated catalog must emit static Rust");
    out.push_str(
        "\npub static BUILTIN_DISPATCH_LUT: phf::Map<&'static str, BuiltinFn> = phf::phf_map! {\n",
    );
    for (name, entry) in &catalog.symbols {
        writeln!(
            out,
            "    {:?} => {},",
            name,
            handler_path(name, &entry.binding),
        )
        .expect("writing to String cannot fail");
    }
    out.push_str("};\n\n");

    fs::write(&dest_path, out).expect("Failed to write catalog_gen.rs");
}

/// ESH is the only catalog consumer that turns an opaque binding into Rust code.
fn handler_path(name: &str, binding: &str) -> String {
    if name == "[" {
        return "builtins::bracket_test".to_owned();
    }
    match binding {
        "" => match name {
            "break" => "builtins::r#break".to_owned(),
            "continue" => "builtins::r#continue".to_owned(),
            "return" => "builtins::r#return".to_owned(),
            "typeset" => "builtins::declare".to_owned(),
            other => format!("builtins::{other}"),
        },
        "builtins::typeset" => "builtins::declare".to_owned(),
        other => other.to_owned(),
    }
}
