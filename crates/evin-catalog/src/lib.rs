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

//! Catalog v1 data shared by documentation producers and embedded consumers.
//!
//! This crate intentionally knows nothing about Markdown, LSP, or executable
//! handlers. The v1 `handler` JSON member is retained as an opaque consumer
//! binding so existing artifacts remain byte-compatible.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CATALOG_VERSION_V1: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogEntry {
    pub name: String,
    pub synopsis: String,
    pub summary: String,
    pub category: String,
    #[serde(default, rename = "handler")]
    pub binding: String,
    pub hover_markdown: String,
    pub hover_plaintext: String,
    pub terminal_ansi: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogExport {
    pub version: u32,
    pub generator: String,
    pub generated_at: String,
    pub symbols: BTreeMap<String, CatalogEntry>,
    #[serde(default)]
    pub keywords: BTreeMap<String, CatalogEntry>,
}

impl CatalogExport {
    pub fn from_json(input: &str) -> Result<Self, CatalogError> {
        let catalog: Self = serde_json::from_str(input)?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn validate(&self) -> Result<(), CatalogError> {
        if self.version != CATALOG_VERSION_V1 {
            return Err(CatalogError::UnsupportedVersion(self.version));
        }
        validate_entries("symbols", &self.symbols)?;
        validate_entries("keywords", &self.keywords)?;
        for name in self.symbols.keys() {
            if self.keywords.contains_key(name) {
                return Err(CatalogError::DuplicateName(name.clone()));
            }
        }
        Ok(())
    }
}

fn validate_entries(
    collection: &'static str,
    entries: &BTreeMap<String, CatalogEntry>,
) -> Result<(), CatalogError> {
    for (key, entry) in entries {
        if key != &entry.name {
            return Err(CatalogError::NameMismatch {
                collection,
                key: key.clone(),
                name: entry.name.clone(),
            });
        }
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("invalid catalog JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported catalog version {0}; expected catalog v1")]
    UnsupportedVersion(u32),
    #[error("catalog {collection} entry {key:?} has mismatched name {name:?}")]
    NameMismatch {
        collection: &'static str,
        key: String,
        name: String,
    },
    #[error("catalog name {0:?} appears as both a symbol and a keyword")]
    DuplicateName(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaticCatalogEntry {
    pub name: &'static str,
    pub synopsis: &'static str,
    pub summary: &'static str,
    pub category: &'static str,
    pub binding: &'static str,
    pub doc_markdown: &'static str,
    pub doc_plaintext: &'static str,
    pub doc_ansi: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaticCatalog {
    pub version: u32,
    pub generator: &'static str,
    pub generated_at: &'static str,
    pub symbols: &'static [StaticCatalogEntry],
    pub keywords: &'static [StaticCatalogEntry],
}

impl StaticCatalog {
    #[inline]
    pub fn find_symbol(&self, name: &str) -> Option<&'static StaticCatalogEntry> {
        self.symbols.iter().find(|entry| entry.name == name)
    }

    #[inline]
    pub fn find_keyword(&self, name: &str) -> Option<&'static StaticCatalogEntry> {
        self.keywords.iter().find(|entry| entry.name == name)
    }
}

pub mod codegen {
    use super::{CatalogEntry, CatalogError, CatalogExport};
    use std::fmt::Write;

    /// Produces deterministic Rust source containing a borrowed static catalog.
    pub fn emit_static_catalog(catalog: &CatalogExport) -> Result<String, CatalogError> {
        catalog.validate()?;

        let mut out = String::new();
        emit_entries(&mut out, "CATALOG_SYMBOLS", catalog.symbols.values());
        emit_entries(&mut out, "CATALOG_KEYWORDS", catalog.keywords.values());
        writeln!(
            out,
            "pub static CATALOG: ::evin_catalog::StaticCatalog = ::evin_catalog::StaticCatalog {{\n    version: {:?},\n    generator: {:?},\n    generated_at: {:?},\n    symbols: CATALOG_SYMBOLS,\n    keywords: CATALOG_KEYWORDS,\n}};",
            catalog.version, catalog.generator, catalog.generated_at,
        )
        .expect("writing to String cannot fail");
        Ok(out)
    }

    fn emit_entries<'a>(
        out: &mut String,
        name: &str,
        entries: impl Iterator<Item = &'a CatalogEntry>,
    ) {
        writeln!(
            out,
            "pub static {name}: &[::evin_catalog::StaticCatalogEntry] = &["
        )
        .expect("writing to String cannot fail");
        for entry in entries {
            writeln!(
                out,
                "    ::evin_catalog::StaticCatalogEntry {{\n        name: {:?},\n        synopsis: {:?},\n        summary: {:?},\n        category: {:?},\n        binding: {:?},\n        doc_markdown: {:?},\n        doc_plaintext: {:?},\n        doc_ansi: {:?},\n    }},",
                entry.name,
                entry.synopsis,
                entry.summary,
                entry.category,
                entry.binding,
                entry.hover_markdown,
                entry.hover_plaintext,
                entry.terminal_ansi,
            )
            .expect("writing to String cannot fail");
        }
        out.push_str("];\n\n");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CATALOG_VERSION_V1, CatalogEntry, CatalogError, CatalogExport, StaticCatalog,
        StaticCatalogEntry, codegen,
    };
    use std::collections::BTreeMap;

    fn entry(name: &str) -> CatalogEntry {
        CatalogEntry {
            name: name.to_owned(),
            synopsis: format!("{name} [args]"),
            summary: "Summary".to_owned(),
            category: "general".to_owned(),
            binding: format!("builtins::{name}"),
            hover_markdown: "**docs**".to_owned(),
            hover_plaintext: "docs".to_owned(),
            terminal_ansi: "\\x1b[1mdocs\\x1b[0m".to_owned(),
        }
    }

    fn catalog() -> CatalogExport {
        CatalogExport {
            version: CATALOG_VERSION_V1,
            generator: "test".to_owned(),
            generated_at: "1970-01-01T00:00:00+00:00".to_owned(),
            symbols: BTreeMap::from([("echo".to_owned(), entry("echo"))]),
            keywords: BTreeMap::from([("if".to_owned(), entry("if"))]),
        }
    }

    #[test]
    fn v1_json_round_trips_with_the_handler_wire_key() {
        let catalog = catalog();
        let json = serde_json::to_string_pretty(&catalog).unwrap();
        assert!(json.contains("\"handler\": \"builtins::echo\""));
        assert_eq!(CatalogExport::from_json(&json).unwrap(), catalog);
    }

    #[test]
    fn structural_validation_rejects_invalid_catalogs() {
        let mut invalid = catalog();
        invalid.symbols.get_mut("echo").unwrap().name = "print".to_owned();
        assert!(matches!(
            invalid.validate(),
            Err(CatalogError::NameMismatch { .. })
        ));

        let mut duplicate = catalog();
        duplicate.keywords.insert("echo".to_owned(), entry("echo"));
        assert!(matches!(
            duplicate.validate(),
            Err(CatalogError::DuplicateName(_))
        ));

        let mut unsupported = catalog();
        unsupported.version = CATALOG_VERSION_V1 + 1;
        assert!(matches!(
            unsupported.validate(),
            Err(CatalogError::UnsupportedVersion(_))
        ));
    }

    #[test]
    fn static_emission_is_deterministic_and_escapes_content() {
        let mut catalog = catalog();
        catalog.symbols.get_mut("echo").unwrap().hover_markdown = "line\n\"quoted\"".to_owned();
        let first = codegen::emit_static_catalog(&catalog).unwrap();
        assert_eq!(first, codegen::emit_static_catalog(&catalog).unwrap());
        assert!(first.contains("line\\n\\\"quoted\\\""));
        assert!(first.find("name: \"echo\"").unwrap() < first.find("name: \"if\"").unwrap());
    }

    #[test]
    fn static_catalog_looks_up_symbols_and_keywords_separately() {
        static SYMBOLS: &[StaticCatalogEntry] = &[StaticCatalogEntry {
            name: "echo",
            synopsis: "echo",
            summary: "Print",
            category: "text",
            binding: "builtins::echo",
            doc_markdown: "echo",
            doc_plaintext: "echo",
            doc_ansi: "echo",
        }];
        static KEYWORDS: &[StaticCatalogEntry] = &[StaticCatalogEntry {
            name: "if",
            synopsis: "if",
            summary: "Branch",
            category: "keyword",
            binding: "",
            doc_markdown: "if",
            doc_plaintext: "if",
            doc_ansi: "if",
        }];
        static STATIC_CATALOG: StaticCatalog = StaticCatalog {
            version: CATALOG_VERSION_V1,
            generator: "test",
            generated_at: "epoch",
            symbols: SYMBOLS,
            keywords: KEYWORDS,
        };

        assert_eq!(STATIC_CATALOG.find_symbol("echo").unwrap().summary, "Print");
        assert_eq!(STATIC_CATALOG.find_keyword("if").unwrap().summary, "Branch");
        assert!(STATIC_CATALOG.find_symbol("if").is_none());
        assert!(STATIC_CATALOG.find_keyword("echo").is_none());
    }
}
