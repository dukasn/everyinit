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
use crate::runtime::traits::BoxFuture;
use std::path::{Path, PathBuf};

fn resolve_path(ctx: &ExecutionContext, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        ctx.cwd().join(p)
    }
}

pub fn test<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        // If invoked as `[` with trailing `]`, strip trailing `]`
        let mut tokens: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        if let Some(&last) = tokens.last()
            && last == "]"
        {
            tokens.pop();
        }

        if tokens.is_empty() {
            return Ok(1);
        }

        // Single operand: true if non-empty string
        if tokens.len() == 1 {
            return if !tokens[0].is_empty() { Ok(0) } else { Ok(1) };
        }

        // Unary operators
        if tokens.len() == 2 {
            let op = tokens[0];
            let arg = tokens[1];
            let res = match op {
                "-z" => arg.is_empty(),
                "-n" => !arg.is_empty(),
                "-f" => resolve_path(ctx, arg).is_file(),
                "-d" => resolve_path(ctx, arg).is_dir(),
                "-e" => resolve_path(ctx, arg).exists(),
                "-s" => {
                    let p = resolve_path(ctx, arg);
                    p.is_file() && std::fs::metadata(&p).map(|m| m.len() > 0).unwrap_or(false)
                }
                "!" => arg.is_empty(),
                _ => false,
            };
            return if res { Ok(0) } else { Ok(1) };
        }

        // Binary operators
        if tokens.len() == 3 {
            let left = tokens[0];
            let op = tokens[1];
            let right = tokens[2];

            let res = match op {
                "=" | "==" => left == right,
                "!=" => left != right,
                "-eq" => {
                    let n1: Result<i64, _> = left.parse();
                    let n2: Result<i64, _> = right.parse();
                    n1.is_ok() && n2.is_ok() && n1.unwrap() == n2.unwrap()
                }
                "-ne" => {
                    let n1: Result<i64, _> = left.parse();
                    let n2: Result<i64, _> = right.parse();
                    n1.is_ok() && n2.is_ok() && n1.unwrap() != n2.unwrap()
                }
                "-lt" => {
                    let n1: Result<i64, _> = left.parse();
                    let n2: Result<i64, _> = right.parse();
                    n1.is_ok() && n2.is_ok() && n1.unwrap() < n2.unwrap()
                }
                "-le" => {
                    let n1: Result<i64, _> = left.parse();
                    let n2: Result<i64, _> = right.parse();
                    n1.is_ok() && n2.is_ok() && n1.unwrap() <= n2.unwrap()
                }
                "-gt" => {
                    let n1: Result<i64, _> = left.parse();
                    let n2: Result<i64, _> = right.parse();
                    n1.is_ok() && n2.is_ok() && n1.unwrap() > n2.unwrap()
                }
                "-ge" => {
                    let n1: Result<i64, _> = left.parse();
                    let n2: Result<i64, _> = right.parse();
                    n1.is_ok() && n2.is_ok() && n1.unwrap() >= n2.unwrap()
                }
                _ => false,
            };
            return if res { Ok(0) } else { Ok(1) };
        }

        // Complex 4+ operand fallback (e.g. `! -f file`)
        if tokens.len() == 4 && tokens[0] == "!" {
            let inner_args = vec![
                tokens[1].to_string(),
                tokens[2].to_string(),
                tokens[3].to_string(),
            ];
            let code = Box::pin(test(&inner_args, ctx)).await?;
            return if code == 0 { Ok(1) } else { Ok(0) };
        }

        Ok(1)
    })
}
