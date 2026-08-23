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
use crate::runtime::context::ExecutionContext;
use std::future::Future;
use std::pin::Pin;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Trait implemented by any executable unit in the esh engine
pub trait Executable: Send + Sync {
    fn execute<'a>(
        &'a self,
        ctx: &'a mut ExecutionContext,
    ) -> BoxFuture<'a, Result<i32, ExecError>>;
}

/// Trait implemented by every virtual built-in command
pub trait BuiltinCommand: Send + Sync {
    fn name(&self) -> &'static str;
    fn run<'a>(
        &'a self,
        args: &'a [String],
        ctx: &'a mut ExecutionContext,
    ) -> BoxFuture<'a, Result<i32, ExecError>>;
}

/// Variable environment & scope management contract
pub trait ScopeManager {
    fn push_scope(&mut self);
    fn pop_scope(&mut self) -> Result<(), ExecError>;
    fn set_local(&mut self, key: &str, val: String);
    fn get_var(&self, key: &str) -> Option<String>;
}
