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
            ctx.write_stderr(b"esh: getopts: missing arguments\n");
            return Ok(1);
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

    let optind_str = ctx.get_var("OPTIND").unwrap_or_else(|| "1".to_string());
    let mut optind = optind_str.parse::<usize>().unwrap_or(1);

    let positional = ctx.positional_args().to_vec();
    if optind > positional.len() {
        return Ok(1);
    }

    let current_arg = &positional[optind - 1];
    if !current_arg.starts_with('-') || current_arg == "-" || current_arg == "--" {
        return Ok(1);
    }

    let flag_char = current_arg.chars().nth(1).unwrap_or('?');
    let opt_pos = optstring.find(flag_char);

    match opt_pos {
        Some(pos) => {
            let requires_arg = optstring.as_bytes().get(pos + 1) == Some(&b':');
            ctx.set_local(varname, flag_char.to_string());

            if requires_arg {
                if current_arg.len() > 2 {
                    let optarg = &current_arg[2..];
                    ctx.set_local("OPTARG", optarg.to_string());
                    optind += 1;
                } else if optind < positional.len() {
                    let optarg = &positional[optind];
                    ctx.set_local("OPTARG", optarg.to_string());
                    optind += 2;
                } else {
                    ctx.set_local(varname, "?".to_string());
                    ctx.write_stderr(
                        format!(
                            "esh: getopts: option requires an argument -- {}\n",
                            flag_char
                        )
                        .as_bytes(),
                    );
                    return Ok(1);
                }
            } else {
                optind += 1;
            }

            ctx.set_local("OPTIND", optind.to_string());
            Ok(0)
        }
        None => {
            ctx.set_local(varname, "?".to_string());
            ctx.write_stderr(format!("esh: getopts: illegal option -- {}\n", flag_char).as_bytes());
            ctx.set_local("OPTIND", (optind + 1).to_string());
            Ok(1)
        }
    }
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
                ctx.set_local(key, val.to_string());
            } else if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                ctx.set_local(stripped, args[i + 1].clone());
                i += 1;
            } else {
                ctx.set_local(stripped, "true".to_string());
            }
        } else if let Some(stripped) = arg.strip_prefix('-') {
            if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                ctx.set_local(stripped, args[i + 1].clone());
                i += 1;
            } else {
                ctx.set_local(stripped, "true".to_string());
            }
        }
        i += 1;
    }

    Ok(0)
}
