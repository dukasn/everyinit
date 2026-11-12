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
use crate::planner::compiler::compile_script;
use crate::runtime::context::ExecutionContext;
use crate::runtime::path_presentation::present;
use crate::runtime::traits::{BoxFuture, Executable, ScopeManager};
use std::env;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::{ffi::CStr, mem::MaybeUninit};

pub fn os<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let flag = args.first().map(|s| s.as_str()).unwrap_or("-a");
        let output = match flag {
            "-n" | "-s" | "--name" | "--os" => env::consts::OS.to_string(),
            "-m" | "--arch" | "--machine" => env::consts::ARCH.to_string(),
            "-f" | "--family" => env::consts::FAMILY.to_string(),
            "-a" | "--all" | "--info" => format!("{}-{}", env::consts::OS, env::consts::ARCH),
            other => {
                ctx.write_stderr(
                    format!(
                        "esh: os: unrecognized option '{}' (usage: os [-n|--name] [-m|--arch] [-f|--family] [-a|--all])\n",
                        other
                    )
                    .as_bytes(),
                );
                return Ok(1);
            }
        };

        ctx.write_stdout(format!("{}\n", output).as_bytes());
        Ok(0)
    })
}

pub fn uname<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut selected = [false; 5];
        let mut options = true;
        for arg in args {
            if options && arg == "--" {
                options = false;
                continue;
            }
            if options && arg.starts_with('-') && arg.len() > 1 {
                for option in arg[1..].chars() {
                    match option {
                        'a' => selected = [true; 5],
                        's' => selected[0] = true,
                        'n' => selected[1] = true,
                        'r' => selected[2] = true,
                        'v' => selected[3] = true,
                        'm' => selected[4] = true,
                        _ => {
                            ctx.write_stderr(
                                format!("esh: uname: unsupported option '-{}'\n", option)
                                    .as_bytes(),
                            );
                            return Ok(2);
                        }
                    }
                }
            } else {
                ctx.write_stderr(b"esh: uname: unexpected operand\n");
                return Ok(2);
            }
        }
        if !selected.iter().any(|enabled| *enabled) {
            selected[0] = true;
        }

        let fields = match uname_fields(ctx) {
            Ok(fields) => fields,
            Err(error) => {
                ctx.write_stderr(format!("esh: uname: {}\n", error).as_bytes());
                return Ok(1);
            }
        };
        let output = fields
            .iter()
            .zip(selected)
            .filter_map(|(field, enabled)| (enabled && !field.is_empty()).then_some(field.as_str()))
            .collect::<Vec<_>>()
            .join(" ");

        ctx.write_stdout(format!("{}\n", output).as_bytes());
        Ok(0)
    })
}

#[cfg(unix)]
fn uname_fields(_ctx: &ExecutionContext) -> Result<[String; 5], String> {
    let mut data = MaybeUninit::<libc::utsname>::uninit();
    let result = unsafe { libc::uname(data.as_mut_ptr()) };
    if result != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let data = unsafe { data.assume_init() };
    let field = |value: &[libc::c_char]| unsafe {
        CStr::from_ptr(value.as_ptr())
            .to_string_lossy()
            .into_owned()
    };
    Ok([
        field(&data.sysname),
        field(&data.nodename),
        field(&data.release),
        field(&data.version),
        field(&data.machine),
    ])
}

#[cfg(not(unix))]
fn uname_fields(ctx: &ExecutionContext) -> Result<[String; 5], String> {
    let nodename = ctx
        .get_var("COMPUTERNAME")
        .or_else(|| ctx.get_var("HOSTNAME"))
        .unwrap_or_default();
    Ok([
        env::consts::OS.to_owned(),
        nodename,
        String::new(),
        String::new(),
        env::consts::ARCH.to_owned(),
    ])
}

pub fn which<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: which: missing binary operand\n");
            return Ok(1);
        }

        let path_var = ctx.get_var("PATH").unwrap_or_default();
        let path_dirs: Vec<PathBuf> = env::split_paths(&path_var).collect();

        let mut all_found = true;

        for bin_name in args {
            let mut found = false;

            // Direct executable check if path contains separator
            if bin_name.contains('/') || (cfg!(windows) && bin_name.contains('\\')) {
                let p = Path::new(bin_name);
                if p.is_file() {
                    ctx.write_stdout(format!("{}\n", present(p)).as_bytes());
                    found = true;
                }
            } else {
                for dir in &path_dirs {
                    let candidate = dir.join(bin_name);
                    if candidate.is_file() {
                        ctx.write_stdout(format!("{}\n", present(&candidate)).as_bytes());
                        found = true;
                        break;
                    }
                    #[cfg(windows)]
                    {
                        for ext in &[".exe", ".cmd", ".bat", ".ps1"] {
                            let with_ext = dir.join(format!("{}{}", bin_name, ext));
                            if with_ext.is_file() {
                                ctx.write_stdout(format!("{}\n", present(&with_ext)).as_bytes());
                                found = true;
                                break;
                            }
                        }
                        if found {
                            break;
                        }
                    }
                }
            }

            if !found {
                all_found = false;
            }
        }

        if all_found { Ok(0) } else { Ok(1) }
    })
}

pub fn env<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let usage = |ctx: &mut ExecutionContext, message: &str| {
            ctx.write_stderr(format!("esh: env: {message}\n").as_bytes());
        };
        let mut sub_ctx = ctx.clone();
        let mut assignments = Vec::new();
        let mut index = 0;
        let mut options = true;
        while index < args.len() {
            let arg = args[index].as_str();
            if options && arg == "--" {
                options = false;
                index += 1;
            } else if options && (arg == "-i" || arg == "--ignore-environment") {
                let exported: Vec<String> =
                    sub_ctx.exported_var_names().map(str::to_owned).collect();
                for key in exported {
                    sub_ctx.unset_var(&key);
                }
                index += 1;
            } else if options && (arg == "-u" || arg == "--unset") {
                index += 1;
                if index == args.len() {
                    usage(ctx, "option requires an argument -- 'u'");
                    return Ok(2);
                }
                sub_ctx.unset_var(&args[index]);
                index += 1;
            } else if options && arg.starts_with("--unset=") {
                sub_ctx.unset_var(&arg[8..]);
                index += 1;
            } else if let Some((key, value)) = arg.split_once('=') {
                if key.is_empty() || key.contains(|c: char| c == '=' || c.is_whitespace()) {
                    usage(ctx, "invalid variable name");
                    return Ok(2);
                }
                assignments.push((key.to_owned(), value.to_owned()));
                options = false;
                index += 1;
            } else {
                break;
            }
        }
        for (key, value) in assignments {
            sub_ctx.set_env(&key, &value);
        }
        if index == args.len() {
            let mut vars: Vec<(String, String)> = sub_ctx
                .env_vars()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            vars.sort_by(|a, b| a.0.cmp(&b.0));
            for (key, value) in vars {
                ctx.write_stdout(format!("{key}={value}\n").as_bytes());
            }
            return Ok(0);
        }
        if options && args[index].starts_with('-') {
            usage(ctx, &format!("unrecognized option '{}'", args[index]));
            return Ok(2);
        }
        // The utility and each argument are passed as literal words to ESH;
        // quoting prevents argument boundaries from becoming shell syntax.
        // `command` also prevents shell functions from intercepting the utility
        // operand while retaining ESH's in-process builtin support.
        let mut sub_script = String::from("command -- ");
        for (offset, arg) in args[index..].iter().enumerate() {
            if offset > 0 {
                sub_script.push(' ');
            }
            sub_script.push('\'');
            for ch in arg.chars() {
                if ch == '\'' {
                    sub_script.push_str("'\\''");
                } else {
                    sub_script.push(ch);
                }
            }
            sub_script.push('\'');
        }
        let plan = compile_script(&sub_script)
            .map_err(|e| ExecError::Custom(format!("env: parse error: {}", e)))?;
        plan.execute(&mut sub_ctx).await
    })
}
