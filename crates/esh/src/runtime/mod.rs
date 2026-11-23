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

pub mod arith;
pub mod builtins;
pub(crate) mod char_class;
pub mod context;
pub mod exec;
#[path = "path.rs"]
pub(crate) mod path_presentation;
pub mod streams;
pub mod traits;

pub use arith::eval_arithmetic_expr;
pub use builtins::*;
pub use context::ExecutionContext;
pub use exec::expand_word;
pub use streams::StreamTarget;
pub use traits::{BoxFuture, BuiltinCommand, Executable, ScopeManager};
