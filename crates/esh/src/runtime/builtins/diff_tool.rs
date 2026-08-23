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
use std::fs;
use std::path::{Path, PathBuf};

fn resolve_path(ctx: &ExecutionContext, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        ctx.cwd().join(p)
    }
}

pub fn diff<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut quiet = false;
        let mut _unified = false;
        let mut operands = Vec::new();

        for arg in args {
            if arg == "-q" || arg == "--brief" {
                quiet = true;
            } else if arg == "-u" || arg == "-U" || arg == "--unified" {
                _unified = true;
            } else if !arg.starts_with('-') {
                operands.push(arg.as_str());
            }
        }

        if operands.len() < 2 {
            ctx.write_stderr(b"esh: diff: usage: diff [-u] [-q] <file1> <file2>\n");
            return Ok(2);
        }

        let path1 = resolve_path(ctx, operands[0]);
        let path2 = resolve_path(ctx, operands[1]);

        let content1 = match fs::read_to_string(&path1) {
            Ok(c) => c,
            Err(e) => {
                ctx.write_stderr(
                    format!("esh: diff: cannot read '{}': {}\n", operands[0], e).as_bytes(),
                );
                return Ok(2);
            }
        };

        let content2 = match fs::read_to_string(&path2) {
            Ok(c) => c,
            Err(e) => {
                ctx.write_stderr(
                    format!("esh: diff: cannot read '{}': {}\n", operands[1], e).as_bytes(),
                );
                return Ok(2);
            }
        };

        if content1 == content2 {
            return Ok(0);
        }

        if quiet {
            ctx.write_stdout(
                format!("Files {} and {} differ\n", operands[0], operands[1]).as_bytes(),
            );
            return Ok(1);
        }

        // Unified diff output
        let lines1: Vec<&str> = content1.lines().collect();
        let lines2: Vec<&str> = content2.lines().collect();

        ctx.write_stdout(format!("--- {}\n+++ {}\n", operands[0], operands[1]).as_bytes());

        let max_lines = std::cmp::max(lines1.len(), lines2.len());
        for idx in 0..max_lines {
            let l1 = lines1.get(idx);
            let l2 = lines2.get(idx);
            match (l1, l2) {
                (Some(a), Some(b)) if a == b => {
                    ctx.write_stdout(format!(" {}\n", a).as_bytes());
                }
                (Some(a), Some(b)) => {
                    ctx.write_stdout(format!("-{}\n+{}\n", a, b).as_bytes());
                }
                (Some(a), None) => {
                    ctx.write_stdout(format!("-{}\n", a).as_bytes());
                }
                (None, Some(b)) => {
                    ctx.write_stdout(format!("+{}\n", b).as_bytes());
                }
                (None, None) => break,
            }
        }

        Ok(1)
    })
}
