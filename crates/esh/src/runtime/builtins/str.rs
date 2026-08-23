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

fn get_input_string(ctx: &mut ExecutionContext, arg: Option<&str>) -> String {
    if let Some(s) = arg {
        if s == "-" {
            if let Some(buf) = ctx.take_stdin() {
                String::from_utf8_lossy(&buf).to_string()
            } else {
                String::new()
            }
        } else {
            s.to_string()
        }
    } else if let Some(buf) = ctx.take_stdin() {
        String::from_utf8_lossy(&buf).to_string()
    } else {
        String::new()
    }
}

pub fn str<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: str: usage: str [-t|--trim] [-u|--upper] [-l|--lower] [-r|--replace from to] [-s|--split delim] [-c|--contains needle] [--starts-with pfx] [--ends-with sfx] [--len] [string|-]\n");
            return Ok(1);
        }

        let flag = args[0].as_str();
        match flag {
            "-t" | "--trim" => {
                let input = get_input_string(ctx, args.get(1).map(String::as_str));
                let trimmed = input.trim();
                ctx.write_stdout(format!("{}\n", trimmed).as_bytes());
                Ok(0)
            }
            "-u" | "--upper" => {
                let input = get_input_string(ctx, args.get(1).map(String::as_str));
                let upper = input.to_uppercase();
                ctx.write_stdout(format!("{}\n", upper).as_bytes());
                Ok(0)
            }
            "-l" | "--lower" => {
                let input = get_input_string(ctx, args.get(1).map(String::as_str));
                let lower = input.to_lowercase();
                ctx.write_stdout(format!("{}\n", lower).as_bytes());
                Ok(0)
            }
            "-r" | "--replace" => {
                if args.len() < 3 {
                    ctx.write_stderr(b"esh: str -r: usage: str -r <from> <to> [string|-]\n");
                    return Ok(1);
                }
                let from = &args[1];
                let to = &args[2];
                let input = get_input_string(ctx, args.get(3).map(String::as_str));
                let replaced = input.replace(from, to);
                ctx.write_stdout(format!("{}\n", replaced).as_bytes());
                Ok(0)
            }
            "-s" | "--split" => {
                if args.len() < 2 {
                    ctx.write_stderr(b"esh: str -s: usage: str -s <delimiter> [string|-]\n");
                    return Ok(1);
                }
                let delimiter = &args[1];
                let input = get_input_string(ctx, args.get(2).map(String::as_str));
                for part in input.split(delimiter) {
                    ctx.write_stdout(format!("{}\n", part).as_bytes());
                }
                Ok(0)
            }
            "-c" | "--contains" => {
                if args.len() < 2 {
                    ctx.write_stderr(b"esh: str -c: usage: str -c <needle> [string|-]\n");
                    return Ok(1);
                }
                let needle = &args[1];
                let input = get_input_string(ctx, args.get(2).map(String::as_str));
                if input.contains(needle) { Ok(0) } else { Ok(1) }
            }
            "--starts-with" => {
                if args.len() < 2 {
                    ctx.write_stderr(
                        b"esh: str --starts-with: usage: str --starts-with <prefix> [string|-]\n",
                    );
                    return Ok(1);
                }
                let prefix = &args[1];
                let input = get_input_string(ctx, args.get(2).map(String::as_str));
                if input.starts_with(prefix) {
                    Ok(0)
                } else {
                    Ok(1)
                }
            }
            "--ends-with" => {
                if args.len() < 2 {
                    ctx.write_stderr(
                        b"esh: str --ends-with: usage: str --ends-with <suffix> [string|-]\n",
                    );
                    return Ok(1);
                }
                let suffix = &args[1];
                let input = get_input_string(ctx, args.get(2).map(String::as_str));
                if input.ends_with(suffix) {
                    Ok(0)
                } else {
                    Ok(1)
                }
            }
            "--len" => {
                let input = get_input_string(ctx, args.get(1).map(String::as_str));
                let len = input.trim_end_matches('\n').chars().count();
                ctx.write_stdout(format!("{}\n", len).as_bytes());
                Ok(0)
            }
            other => {
                ctx.write_stderr(format!("esh: str: unrecognized option '{}'\n", other).as_bytes());
                Ok(1)
            }
        }
    })
}
