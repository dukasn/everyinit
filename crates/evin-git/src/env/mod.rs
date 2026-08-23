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

pub mod mock;
pub mod native;

use crate::types::{GitHook, HookError};

pub trait HookEnvironment {
    fn read_hook(
        &self,
        hook: &GitHook,
    ) -> impl std::future::Future<Output = Result<Option<String>, HookError>> + Send;
    fn write_hook(
        &self,
        hook: &GitHook,
        content: &str,
    ) -> impl std::future::Future<Output = Result<(), HookError>> + Send;
    fn make_executable(
        &self,
        hook: &GitHook,
    ) -> impl std::future::Future<Output = Result<(), HookError>> + Send;
    fn is_executable(
        &self,
        hook: &GitHook,
    ) -> impl std::future::Future<Output = Result<bool, HookError>> + Send;
}
