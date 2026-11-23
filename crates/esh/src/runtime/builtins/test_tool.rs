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

fn is_executable(path: &Path) -> bool {
    if !path.exists() {
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

fn has_permission_bits(path: &Path, bits: u32) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & bits != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        if !path.exists() {
            return false;
        }
        if bits == 0o222 {
            std::fs::metadata(path)
                .map(|metadata| !metadata.permissions().readonly())
                .unwrap_or(false)
        } else {
            true
        }
    }
}

fn has_special_mode_bit(path: &Path, bit: u32) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & bit != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        let _ = (path, bit);
        false
    }
}

#[cfg(unix)]
fn has_file_type(path: &Path, predicate: impl FnOnce(&std::fs::FileType) -> bool) -> bool {
    std::fs::metadata(path)
        .map(|metadata| predicate(&metadata.file_type()))
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn has_file_type(_path: &Path, _predicate: impl FnOnce(&std::fs::FileType) -> bool) -> bool {
    false
}

fn resolve_path(ctx: &ExecutionContext, path: &str) -> PathBuf {
    if path.is_empty() {
        return PathBuf::new();
    }
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        ctx.cwd().join(path)
    }
}

fn file_predicate(ctx: &ExecutionContext, op: &str, arg: &str) -> Option<bool> {
    let path = resolve_path(ctx, arg);
    Some(match op {
        "-e" | "-a" => path.exists(),
        "-f" => path.is_file(),
        "-d" => path.is_dir(),
        "-x" => is_executable(&path),
        "-r" => has_permission_bits(&path, 0o444),
        "-w" => has_permission_bits(&path, 0o222),
        "-g" => has_special_mode_bit(&path, 0o2000),
        "-u" => has_special_mode_bit(&path, 0o4000),
        "-b" => has_file_type(&path, |kind| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileTypeExt;
                kind.is_block_device()
            }
            #[cfg(not(unix))]
            {
                let _ = kind;
                false
            }
        }),
        "-c" => has_file_type(&path, |kind| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileTypeExt;
                kind.is_char_device()
            }
            #[cfg(not(unix))]
            {
                let _ = kind;
                false
            }
        }),
        "-p" => has_file_type(&path, |kind| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileTypeExt;
                kind.is_fifo()
            }
            #[cfg(not(unix))]
            {
                let _ = kind;
                false
            }
        }),
        "-S" => has_file_type(&path, |kind| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileTypeExt;
                kind.is_socket()
            }
            #[cfg(not(unix))]
            {
                let _ = kind;
                false
            }
        }),
        "-s" => std::fs::metadata(&path)
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false),
        "-L" | "-h" => std::fs::symlink_metadata(&path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false),
        _ => return None,
    })
}

fn unary(ctx: &ExecutionContext, op: &str, arg: &str) -> Option<bool> {
    match op {
        "-z" => Some(arg.is_empty()),
        "-n" => Some(!arg.is_empty()),
        "!" => Some(arg.is_empty()),
        _ => file_predicate(ctx, op, arg),
    }
}

fn integer_pair(left: &str, right: &str) -> Result<(i64, i64), String> {
    let left_number = left
        .parse::<i64>()
        .map_err(|_| format!("expected an integer, got '{left}'"))?;
    let right_number = right
        .parse::<i64>()
        .map_err(|_| format!("expected an integer, got '{right}'"))?;
    Ok((left_number, right_number))
}

fn binary(ctx: &ExecutionContext, left: &str, op: &str, right: &str) -> Result<bool, String> {
    Ok(match op {
        "=" | "==" => left == right,
        "!=" => left != right,
        "<" => left < right,
        ">" => left > right,
        "-eq" => {
            let (left, right) = integer_pair(left, right)?;
            left == right
        }
        "-ne" => {
            let (left, right) = integer_pair(left, right)?;
            left != right
        }
        "-lt" => {
            let (left, right) = integer_pair(left, right)?;
            left < right
        }
        "-le" => {
            let (left, right) = integer_pair(left, right)?;
            left <= right
        }
        "-gt" => {
            let (left, right) = integer_pair(left, right)?;
            left > right
        }
        "-ge" => {
            let (left, right) = integer_pair(left, right)?;
            left >= right
        }
        "-nt" => {
            let left_metadata = std::fs::metadata(resolve_path(ctx, left)).ok();
            let right_metadata = std::fs::metadata(resolve_path(ctx, right)).ok();
            match (left_metadata, right_metadata) {
                (Some(left), Some(right)) => left
                    .modified()
                    .ok()
                    .zip(right.modified().ok())
                    .is_some_and(|(left, right)| left > right),
                (Some(_), None) => true,
                _ => false,
            }
        }
        "-ot" => {
            let left_metadata = std::fs::metadata(resolve_path(ctx, left)).ok();
            let right_metadata = std::fs::metadata(resolve_path(ctx, right)).ok();
            match (left_metadata, right_metadata) {
                (Some(left), Some(right)) => left
                    .modified()
                    .ok()
                    .zip(right.modified().ok())
                    .is_some_and(|(left, right)| left < right),
                (None, Some(_)) => true,
                _ => false,
            }
        }
        "-ef" => same_file(&resolve_path(ctx, left), &resolve_path(ctx, right)),
        _ => return Err(format!("unknown binary operator '{op}'")),
    })
}

fn same_file(left: &Path, right: &Path) -> bool {
    same_file::is_same_file(left, right).unwrap_or(false)
}

struct TestParser<'a, 'ctx> {
    tokens: &'a [&'a str],
    position: usize,
    ctx: &'ctx ExecutionContext,
}

impl TestParser<'_, '_> {
    fn parse_or(&mut self) -> Result<bool, String> {
        let mut value = self.parse_and()?;
        while self.peek() == Some("-o") {
            self.position += 1;
            let right = self.parse_and()?;
            value = value || right;
        }
        Ok(value)
    }

    fn parse_and(&mut self) -> Result<bool, String> {
        let mut value = self.parse_not()?;
        while self.peek() == Some("-a") {
            self.position += 1;
            let right = self.parse_not()?;
            value = value && right;
        }
        Ok(value)
    }

    fn parse_not(&mut self) -> Result<bool, String> {
        let remaining = self.tokens.len().saturating_sub(self.position);
        let three_argument_binary = remaining == 3
            && self
                .tokens
                .get(self.position + 1)
                .is_some_and(|operator| is_three_argument_binary(operator));
        if self.peek() == Some("!") && remaining > 1 && !three_argument_binary {
            self.position += 1;
            return Ok(!self.parse_not()?);
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<bool, String> {
        let remaining = self.tokens.len().saturating_sub(self.position);
        // POSIX resolves the three-argument binary form before interpreting
        // parentheses, since either parenthesis may be an ordinary operand.
        if remaining == 3 {
            let left = self.tokens[self.position];
            let op = self.tokens[self.position + 1];
            let right = self.tokens[self.position + 2];
            if is_binary_operator(op) || matches!(op, "-a" | "-o") {
                self.position += 3;
                if op == "-a" {
                    return Ok(!left.is_empty() && !right.is_empty());
                }
                if op == "-o" {
                    return Ok(!left.is_empty() || !right.is_empty());
                }
                return binary(self.ctx, left, op, right);
            }

            // In the XSI three-argument form, parentheses surround one
            // operand. Resolve that arity before interpreting a leading
            // unary-looking token as an operator with a missing operand.
            if left == "(" && right == ")" {
                self.position += 3;
                return Ok(!op.is_empty());
            }
        }

        if remaining == 1 && self.peek() == Some("(") {
            self.position += 1;
            return Ok(true);
        }

        if self.peek() == Some("(") {
            self.position += 1;
            let value = self.parse_or()?;
            if self.peek() != Some(")") {
                return Err("missing ')'".to_string());
            }
            self.position += 1;
            return Ok(value);
        }

        if remaining == 0 {
            return Err("expected an expression".to_string());
        }

        if remaining >= 2 && self.peek() == Some("-t") {
            let binary_expression = remaining >= 3
                && self.tokens.get(self.position + 1).is_some_and(|operator| {
                    is_binary_operator(operator)
                        || (remaining == 3 && matches!(*operator, "-a" | "-o"))
                });
            if !binary_expression {
                return Err("terminal predicate '-t' is unsupported".to_string());
            }
        }

        if remaining >= 3 {
            let left = self.tokens[self.position];
            let op = self.tokens[self.position + 1];
            let right = self.tokens[self.position + 2];
            if is_binary_operator(op) || (remaining == 3 && matches!(op, "-a" | "-o")) {
                self.position += 3;
                if op == "-a" {
                    return Ok(!left.is_empty() && !right.is_empty());
                }
                if op == "-o" {
                    return Ok(!left.is_empty() || !right.is_empty());
                }
                return binary(self.ctx, left, op, right);
            }
            if op.starts_with('-') && !matches!(op, "-a" | "-o") {
                return Err(format!("unknown binary operator '{op}'"));
            }
        }

        if remaining >= 2
            && let Some(value) = unary(
                self.ctx,
                self.tokens[self.position],
                self.tokens[self.position + 1],
            )
        {
            self.position += 2;
            return Ok(value);
        }

        self.position += 1;
        Ok(!self.tokens[self.position - 1].is_empty())
    }

    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.position).copied()
    }
}

fn is_three_argument_binary(op: &str) -> bool {
    is_binary_operator(op) || matches!(op, "-a" | "-o")
}

fn is_binary_operator(op: &str) -> bool {
    matches!(
        op,
        "=" | "=="
            | "!="
            | "<"
            | ">"
            | "-eq"
            | "-ne"
            | "-lt"
            | "-le"
            | "-gt"
            | "-ge"
            | "-nt"
            | "-ot"
            | "-ef"
    )
}

pub fn test<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let tokens: Vec<&str> = args.iter().map(String::as_str).collect();
        if tokens.is_empty() {
            return Ok(1);
        }

        let result = {
            let mut parser = TestParser {
                tokens: &tokens,
                position: 0,
                ctx,
            };
            let value = parser.parse_or();
            value.and_then(|result| {
                if parser.position == tokens.len() {
                    Ok(result)
                } else {
                    Err(format!("unexpected operand '{}'", tokens[parser.position]))
                }
            })
        };

        match result {
            Ok(true) => Ok(0),
            Ok(false) => Ok(1),
            Err(message) => {
                ctx.write_stderr(format!("esh: test: {message}\n").as_bytes());
                Ok(2)
            }
        }
    })
}

pub fn bracket_test<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.last().map(String::as_str) != Some("]") {
            ctx.write_stderr(b"esh: [: missing closing ']'\n");
            return Ok(2);
        }
        test(&args[..args.len() - 1], ctx).await
    })
}
