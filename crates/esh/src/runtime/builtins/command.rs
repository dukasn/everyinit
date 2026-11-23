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

use crate::catalog::find_keyword;
use crate::error::ExecError;
use crate::planner::dispatch::resolve_builtin;
use crate::runtime::context::ExecutionContext;
use crate::runtime::exec::execute_external_command_with_path;
use crate::runtime::traits::{BoxFuture, ScopeManager};
use std::env;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
enum QueryMode {
    Verbose,
    Brief,
}

pub fn command<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut path_override = None;
        let mut query_mode = None;
        let mut command_index = 0;

        while let Some(arg) = args.get(command_index) {
            match arg.as_str() {
                "--" => {
                    command_index += 1;
                    break;
                }
                "-p" => {
                    path_override = if cfg!(windows) {
                        None
                    } else {
                        Some("/usr/bin:/bin")
                    };
                    command_index += 1;
                }
                "-v" | "-V" => {
                    let mode = if arg == "-v" {
                        QueryMode::Brief
                    } else {
                        QueryMode::Verbose
                    };
                    if query_mode.is_some_and(|previous| previous != mode) {
                        ctx.write_stderr(b"esh: command: -v and -V cannot be combined\n");
                        return Ok(2);
                    }
                    query_mode = Some(mode);
                    command_index += 1;
                }
                _ => break,
            }
        }

        let operands = &args[command_index..];
        if let Some(mode) = query_mode {
            if operands.is_empty() {
                return Ok(0);
            }
            let mut all_found = true;
            for name in operands {
                match resolve_name(name, path_override, ctx) {
                    Some((kind, path)) => match mode {
                        QueryMode::Brief => {
                            ctx.write_stdout(
                                format!("{}\n", path.unwrap_or_else(|| name.clone())).as_bytes(),
                            );
                        }
                        QueryMode::Verbose => {
                            let description = match kind {
                                ResolvedKind::Reserved => format!("{name} is a reserved word"),
                                ResolvedKind::Function => format!("{name} is a shell function"),
                                ResolvedKind::Builtin => format!("{name} is an ESH builtin"),
                                ResolvedKind::External => {
                                    format!("{name} is {}", path.unwrap())
                                }
                            };
                            ctx.write_stdout(format!("{description}\n").as_bytes());
                        }
                    },
                    None => all_found = false,
                }
            }
            return Ok(if all_found { 0 } else { 1 });
        }

        let Some(name) = operands.first() else {
            return Ok(0);
        };
        let command_args = &operands[1..];

        if let Some(builtin) = resolve_builtin(name) {
            return builtin(command_args, ctx).await;
        }

        let path_value = path_override
            .map(str::to_string)
            .or_else(|| ctx.get_var("PATH"));
        let path = path_value.as_deref();
        if find_external(name, path, ctx).is_none() {
            if find_file(name, path, ctx).is_some() {
                ctx.write_stderr(format!("esh: command: {name}: permission denied\n").as_bytes());
                return Ok(126);
            }
            ctx.write_stderr(format!("esh: command: {name}: not found\n").as_bytes());
            return Ok(127);
        }

        match execute_external_command_with_path(name, command_args, ctx, path).await {
            Ok(status) => Ok(status),
            Err(ExecError::IoError(error)) => {
                ctx.write_stderr(
                    format!("esh: command: {name}: cannot execute: {error}\n").as_bytes(),
                );
                Ok(126)
            }
            Err(error) => Err(error),
        }
    })
}

pub fn type_builtin<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut all_found = true;
        for name in args {
            let description = if is_reserved_word(name) {
                Some(format!("{name} is a reserved word"))
            } else {
                resolve_name(name, None, ctx).map(|(kind, path)| match kind {
                    ResolvedKind::Reserved => format!("{name} is a reserved word"),
                    ResolvedKind::Function => format!("{name} is a shell function"),
                    ResolvedKind::Builtin => format!("{name} is an ESH builtin"),
                    ResolvedKind::External => format!("{name} is {}", path.unwrap()),
                })
            };
            if let Some(description) = description {
                ctx.write_stdout(format!("{description}\n").as_bytes());
            } else {
                all_found = false;
                ctx.write_stderr(format!("esh: type: {name}: not found\n").as_bytes());
            }
        }
        Ok(if all_found { 0 } else { 1 })
    })
}

fn is_reserved_word(name: &str) -> bool {
    matches!(name, "!" | "{" | "}") || find_keyword(name).is_some()
}

#[derive(Clone, Copy)]
enum ResolvedKind {
    Reserved,
    Function,
    Builtin,
    External,
}

fn resolve_name(
    name: &str,
    path_override: Option<&str>,
    ctx: &ExecutionContext,
) -> Option<(ResolvedKind, Option<String>)> {
    if is_reserved_word(name) {
        return Some((ResolvedKind::Reserved, None));
    }
    if ctx.get_function(name).is_some() {
        return Some((ResolvedKind::Function, None));
    }
    if resolve_builtin(name).is_some() {
        return Some((ResolvedKind::Builtin, None));
    }
    let path = path_override
        .map(str::to_owned)
        .or_else(|| ctx.get_var("PATH"));
    find_external(name, path.as_deref(), ctx).map(|path| {
        (
            ResolvedKind::External,
            Some(path.to_string_lossy().into_owned()),
        )
    })
}

fn find_external(name: &str, path: Option<&str>, ctx: &ExecutionContext) -> Option<PathBuf> {
    find_path_match(name, path, is_executable_candidate, ctx)
}

pub(crate) fn external_command_exists(name: &str, ctx: &ExecutionContext) -> bool {
    let path = ctx.get_var("PATH");
    find_external(name, path.as_deref(), ctx).is_some()
}

fn find_file(name: &str, path: Option<&str>, ctx: &ExecutionContext) -> Option<PathBuf> {
    find_path_match(name, path, Path::is_file, ctx)
}

fn find_path_match(
    name: &str,
    path: Option<&str>,
    predicate: impl Fn(&Path) -> bool,
    ctx: &ExecutionContext,
) -> Option<PathBuf> {
    let requested = Path::new(name);
    if requested.is_absolute() || name.contains('/') || (cfg!(windows) && name.contains('\\')) {
        let path = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            ctx.cwd().join(requested)
        };
        return predicate(&path).then_some(path);
    }

    let directories = path
        .map(|path| env::split_paths(path).collect::<Vec<_>>())
        .unwrap_or_else(|| vec![PathBuf::new()]);

    directories
        .into_iter()
        .map(|directory| {
            if directory.is_absolute() {
                directory.join(name)
            } else {
                ctx.cwd().join(directory).join(name)
            }
        })
        .find(|candidate| predicate(candidate))
}

pub(crate) fn is_executable_candidate(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        true
    }
}
