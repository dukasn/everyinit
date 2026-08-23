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

use crate::syntax::{LspSemanticTokensEmitter, SemanticClassifier};

/// Standard LSP Semantic Token Types Legend
pub const TOKEN_TYPES: &[&str] = &[
    "keyword",   // 0
    "function",  // 1 (Commands and Builtins)
    "variable",  // 2
    "string",    // 3
    "number",    // 4
    "operator",  // 5
    "comment",   // 6
    "parameter", // 7 (Flags / Options)
];

/// Encodes an ESH script into standard LSP 5-tuple delta-encoded semantic tokens:
/// `[delta_line, delta_start_col, length, token_type, token_modifiers]`
pub fn compute_semantic_tokens(source: &str) -> Vec<u32> {
    let mut data = Vec::with_capacity(source.len() / 4);
    let mut emitter = LspSemanticTokensEmitter::new(&mut data);
    let mut classifier = SemanticClassifier::new(source);
    classifier.classify_into(&mut emitter);
    data
}
