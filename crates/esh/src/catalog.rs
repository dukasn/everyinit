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

use crate::error::ExecError;
use crate::runtime::builtins;
use crate::runtime::context::ExecutionContext;
use crate::runtime::traits::BoxFuture;
use evin_catalog::StaticCatalogEntry;

pub type BuiltinFn = for<'a> fn(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>>;

pub type BuiltinInfo = StaticCatalogEntry;
pub type KeywordInfo = StaticCatalogEntry;

// Include compile-time generated catalog from build.rs
include!(concat!(env!("OUT_DIR"), "/catalog_gen.rs"));

pub use CATALOG_KEYWORDS as KEYWORD_CATALOG;
pub use CATALOG_SYMBOLS as BUILTIN_CATALOG;

#[inline(always)]
pub fn find_builtin(name: &str) -> Option<&'static BuiltinInfo> {
    CATALOG.find_symbol(name)
}

#[inline(always)]
pub fn find_keyword(name: &str) -> Option<&'static KeywordInfo> {
    CATALOG.find_keyword(name)
}

#[inline]
pub fn get_hover_doc(word: &str) -> Option<&'static str> {
    if let Some(b) = find_builtin(word) {
        return Some(b.doc_markdown);
    }
    if let Some(k) = find_keyword(word) {
        return Some(k.doc_markdown);
    }
    None
}

#[inline]
pub fn get_hover_doc_plain(word: &str) -> Option<&'static str> {
    if let Some(b) = find_builtin(word) {
        return Some(b.doc_plaintext);
    }
    if let Some(k) = find_keyword(word) {
        return Some(k.doc_plaintext);
    }
    None
}

#[inline]
pub fn get_ansi_doc(word: &str) -> Option<&'static str> {
    if let Some(b) = find_builtin(word) {
        return Some(b.doc_ansi);
    }
    if let Some(k) = find_keyword(word) {
        return Some(k.doc_ansi);
    }
    None
}

#[inline(always)]
pub fn resolve_builtin(name: &str) -> Option<BuiltinFn> {
    BUILTIN_DISPATCH_LUT.get(name).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use evin_catalog::{CatalogEntry, CatalogExport, StaticCatalogEntry};

    #[test]
    fn embedded_catalog_matches_the_checked_in_v1_artifact() {
        let raw = include_str!("../assets/builtins-catalog.json");
        let expected = CatalogExport::from_json(raw).expect("checked-in catalog must be valid");

        assert_eq!(serde_json::to_string_pretty(&expected).unwrap(), raw);

        assert_eq!(CATALOG.version, expected.version);
        assert_eq!(CATALOG.generator, expected.generator);
        assert_eq!(CATALOG.generated_at, expected.generated_at);
        assert_entries(BUILTIN_CATALOG, &expected.symbols);
        assert_entries(KEYWORD_CATALOG, &expected.keywords);
    }

    #[test]
    fn catalog_helpers_preserve_help_and_dispatch_contracts() {
        assert!(get_hover_doc("echo").unwrap().contains("```esh"));
        assert!(get_hover_doc_plain("if").is_some());
        assert!(get_ansi_doc("echo").is_some());
        assert!(resolve_builtin("typeset").is_some());
        assert!(resolve_builtin("[").is_some());
        assert!(resolve_builtin("not-a-builtin").is_none());
    }

    fn assert_entries(
        actual: &[StaticCatalogEntry],
        expected: &std::collections::BTreeMap<String, CatalogEntry>,
    ) {
        assert_eq!(actual.len(), expected.len());
        for (actual, (name, expected)) in actual.iter().zip(expected) {
            assert_eq!(actual.name, name);
            assert_eq!(actual.name, expected.name);
            assert_eq!(actual.synopsis, expected.synopsis);
            assert_eq!(actual.summary, expected.summary);
            assert_eq!(actual.category, expected.category);
            assert_eq!(actual.binding, expected.binding);
            assert_eq!(actual.doc_markdown, expected.hover_markdown);
            assert_eq!(actual.doc_plaintext, expected.hover_plaintext);
            assert_eq!(actual.doc_ansi, expected.terminal_ansi);
        }
    }
}
