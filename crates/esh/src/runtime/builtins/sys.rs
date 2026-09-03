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
        let flag = args.first().map(|s| s.as_str()).unwrap_or("-s");
        let output = match flag {
            "-s" | "--kernel-name" => env::consts::OS.to_string(),
            "-m" | "--machine" => env::consts::ARCH.to_string(),
            "-a" | "--all" => format!(
                "{} {} {}",
                env::consts::OS,
                env::consts::ARCH,
                env::consts::FAMILY
            ),
            other => {
                ctx.write_stderr(
                    format!("esh: uname: unrecognized option '{}'\n", other).as_bytes(),
                );
                return Ok(1);
            }
        };

        ctx.write_stdout(format!("{}\n", output).as_bytes());
        Ok(0)
    })
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
        if args.is_empty() {
            let mut vars: Vec<(String, String)> = ctx
                .env_vars()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            vars.sort_by(|a, b| a.0.cmp(&b.0));
            for (k, v) in vars {
                ctx.write_stdout(format!("{}={}\n", k, v).as_bytes());
            }
            return Ok(0);
        }

        // Check for `env -u KEY <cmd...>` or `env KEY=VAL <cmd...>`
        let mut unset_keys = Vec::new();
        let mut set_keys = Vec::new();
        let mut cmd_start = args.len();

        let mut iter = args.iter().enumerate().peekable();
        while let Some((idx, arg)) = iter.next() {
            if arg == "-u" || arg == "--unset" {
                if let Some((_, val)) = iter.next() {
                    unset_keys.push(val.as_str());
                }
            } else if let Some((k, v)) = arg.split_once('=') {
                set_keys.push((k, v));
            } else {
                cmd_start = idx;
                break;
            }
        }

        if cmd_start < args.len() {
            let mut sub_ctx = ctx.clone();
            for k in unset_keys {
                sub_ctx.set_env(k, "");
            }
            for (k, v) in set_keys {
                sub_ctx.set_env(k, v);
            }
            let sub_script = args[cmd_start..].join(" ");
            let plan = compile_script(&sub_script)
                .map_err(|e| ExecError::Custom(format!("env: parse error: {}", e)))?;
            return plan.execute(&mut sub_ctx).await;
        }

        Ok(0)
    })
}
