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

/// Metadata definition for an in-process built-in function.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinInfo {
    pub name: &'static str,
    pub synopsis: &'static str,
    pub summary: &'static str,
    pub doc_markdown: &'static str,
    pub doc_plaintext: &'static str,
    pub doc_ansi: &'static str,
    pub handler: BuiltinFn,
}

pub type BuiltinFn = for<'a> fn(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>>;

/// Metadata definition for a shell control-flow keyword.
#[derive(Debug, Clone, Copy)]
pub struct KeywordInfo {
    pub name: &'static str,
    pub synopsis: &'static str,
    pub summary: &'static str,
    pub doc_markdown: &'static str,
    pub doc_plaintext: &'static str,
    pub doc_ansi: &'static str,
}

// Include compile-time generated catalog from build.rs
include!(concat!(env!("OUT_DIR"), "/catalog_gen.rs"));

#[inline(always)]
pub fn find_builtin(name: &str) -> Option<&'static BuiltinInfo> {
    BUILTIN_CATALOG.iter().find(|b| b.name == name)
}

#[inline(always)]
pub fn find_keyword(name: &str) -> Option<&'static KeywordInfo> {
    KEYWORD_CATALOG.iter().find(|k| k.name == name)
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
