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
use crate::runtime::traits::{BoxFuture, ScopeManager};

pub fn getopts<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: getopts: usage: getopts optstring name [arg ...]\n");
            return Ok(2);
        }

        // Check if declarative mode: getopts "$@" --about ...
        // In this mode, the passed args start with positional parameters or declarative flag specifications.
        // If the first argument is a traditional optstring (e.g. "e:v"), run traditional getopts.
        if !args[0].starts_with('-')
            && !args[0].contains(' ')
            && args.len() >= 2
            && !args.iter().any(|a| a == "--about" || a.starts_with('<'))
        {
            return traditional_getopts(args, ctx);
        }

        declarative_getopts(args, ctx)
    })
}

fn traditional_getopts(args: &[String], ctx: &mut ExecutionContext) -> Result<i32, ExecError> {
    let optstring = &args[0];
    let varname = &args[1];
    if !is_identifier(varname) {
        ctx.write_stderr(format!("esh: getopts: invalid variable name: {varname}\n").as_bytes());
        return Ok(2);
    }
    if let Some(message) = validate_optstring(optstring) {
        ctx.write_stderr(format!("esh: getopts: {message}\n").as_bytes());
        return Ok(2);
    }
    let operands = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        ctx.positional_args().to_vec()
    };
    let mut optind = ctx
        .get_var("OPTIND")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1);

    ctx.unset_var("OPTARG");
    if optind > operands.len() {
        return finish_getopts(ctx, varname, optind);
    }
    let current_arg = &operands[optind - 1];
    if current_arg == "--" {
        optind += 1;
        return finish_getopts(ctx, varname, optind);
    }
    if current_arg == "-" || !current_arg.starts_with('-') {
        return finish_getopts(ctx, varname, optind);
    }

    let option_chars = current_arg[1..].chars().collect::<Vec<_>>();
    let offset = ctx.getopts_offset(optind);
    if offset >= option_chars.len() {
        ctx.set_getopts_position(optind, 0);
        return Ok(1);
    }
    let option = option_chars[offset];
    let optstring_chars = optstring.chars().collect::<Vec<_>>();
    let leading_colon = optstring_chars.first() == Some(&':');
    let found = optstring_chars
        .iter()
        .position(|candidate| *candidate == option);
    let opt_position = found.filter(|position| !leading_colon || *position != 0);
    let requires_arg =
        opt_position.and_then(|position| optstring_chars.get(position + 1)) == Some(&':');
    let next_offset = offset + 1;
    let has_cluster_tail = next_offset < option_chars.len();

    match opt_position {
        Some(_) if requires_arg => {
            if has_cluster_tail {
                ctx.set_scoped_var("OPTARG", option_chars[next_offset..].iter().collect());
                optind += 1;
            } else if optind < operands.len() {
                ctx.set_scoped_var("OPTARG", operands[optind].clone());
                optind += 2;
            } else {
                let error_value = if leading_colon { ":" } else { "?" };
                ctx.set_scoped_var(varname, error_value.to_string());
                if leading_colon {
                    ctx.set_scoped_var("OPTARG", option.to_string());
                }
                optind += 1;
                ctx.set_getopts_position(optind, 0);
                if !leading_colon {
                    ctx.write_stderr(
                        format!("esh: getopts: option requires an argument -- {option}\n")
                            .as_bytes(),
                    );
                }
                ctx.set_scoped_var("OPTIND", optind.to_string());
                return Ok(0);
            }
            ctx.set_scoped_var(varname, option.to_string());
            ctx.set_scoped_var("OPTIND", optind.to_string());
            ctx.set_getopts_position(optind, 0);
            Ok(0)
        }
        Some(_) => {
            if has_cluster_tail {
                ctx.set_getopts_position(optind, next_offset);
            } else {
                optind += 1;
                ctx.set_getopts_position(optind, 0);
                ctx.set_scoped_var("OPTIND", optind.to_string());
            }
            ctx.set_scoped_var(varname, option.to_string());
            Ok(0)
        }
        None => {
            ctx.set_scoped_var(varname, "?".to_string());
            if leading_colon {
                ctx.set_scoped_var("OPTARG", option.to_string());
            } else {
                ctx.write_stderr(format!("esh: getopts: illegal option -- {option}\n").as_bytes());
            }
            if has_cluster_tail {
                ctx.set_getopts_position(optind, next_offset);
            } else {
                optind += 1;
                ctx.set_scoped_var("OPTIND", optind.to_string());
                ctx.set_getopts_position(optind, 0);
            }
            Ok(0)
        }
    }
}

fn finish_getopts(
    ctx: &mut ExecutionContext,
    varname: &str,
    optind: usize,
) -> Result<i32, ExecError> {
    ctx.set_scoped_var(varname, "?".to_string());
    ctx.set_scoped_var("OPTIND", optind.to_string());
    ctx.set_getopts_position(optind, 0);
    Ok(1)
}

fn validate_optstring(optstring: &str) -> Option<String> {
    let mut chars = optstring.chars().peekable();
    if chars.peek() == Some(&':') {
        chars.next();
    }
    let mut seen = std::collections::HashSet::new();
    while let Some(option) = chars.next() {
        if !option.is_ascii_alphanumeric() || !seen.insert(option) {
            return Some(format!("invalid option specification: {optstring}"));
        }
        if chars.peek() == Some(&':') {
            chars.next();
        }
    }
    None
}

fn is_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'_'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn declarative_getopts(args: &[String], ctx: &mut ExecutionContext) -> Result<i32, ExecError> {
    // Declarative parser binds flags into local variables
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-h" || arg == "--help" {
            ctx.write_stdout(b"Help:\n  Usage: <command> [OPTIONS]\n");
            return Ok(0);
        }

        if let Some(stripped) = arg.strip_prefix("--") {
            if let Some(eq_idx) = stripped.find('=') {
                let key = &stripped[..eq_idx];
                let val = &stripped[eq_idx + 1..];
                ctx.set_scoped_var(key, val.to_string());
            } else if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                ctx.set_scoped_var(stripped, args[i + 1].clone());
                i += 1;
            } else {
                ctx.set_scoped_var(stripped, "true".to_string());
            }
        } else if let Some(stripped) = arg.strip_prefix('-') {
            if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                ctx.set_scoped_var(stripped, args[i + 1].clone());
                i += 1;
            } else {
                ctx.set_scoped_var(stripped, "true".to_string());
            }
        }
        i += 1;
    }

    Ok(0)
}
