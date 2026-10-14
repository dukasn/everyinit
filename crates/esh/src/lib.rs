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

//! `esh` is Evin's deterministic shell language runtime for project automation.
//!
//! The language and integration contracts are defined in
//! [`LANGUAGE_SPECIFICATION.md`](../LANGUAGE_SPECIFICATION.md).

pub mod cache;
pub mod catalog;
pub mod engine;
pub mod error;
pub mod lexer;
pub mod lsp;
pub mod parser;
pub mod planner;
pub mod runtime;
pub mod syntax;

pub use cache::PlanCache;
pub use engine::Engine;
pub use error::{EshError, ExecError, LexError, ParseError};
pub use lexer::token::{Span, Token, TokenKind};
pub use lsp::{LspServer, run_lsp_server};
pub use planner::ir::{ExecutionPlan, ExecutionStage, PlanNode};
pub use runtime::context::ExecutionContext;
pub use runtime::traits::{BuiltinCommand, Executable, ScopeManager};
pub use syntax::{highlight_ansi, highlight_html, highlight_html_classed};

/// The version of the language specification this crate implements.
pub const LANGUAGE_SPECIFICATION_VERSION: &str = "1.0.0-draft";
