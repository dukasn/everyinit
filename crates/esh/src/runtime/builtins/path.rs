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
use crate::runtime::path_presentation::present;
use crate::runtime::traits::BoxFuture;
use std::path::{Path, PathBuf};

fn resolve_path(ctx: &ExecutionContext, path_str: &str) -> PathBuf {
    let p = Path::new(path_str);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        ctx.cwd().join(p)
    }
}

pub fn dirname<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: dirname: missing operand\n");
            return Ok(1);
        }

        for path_str in args {
            let p = Path::new(path_str);
            let parent = p.parent().map(present).unwrap_or_else(|| ".".to_string());
            let out = if parent.is_empty() {
                ".".to_string()
            } else {
                parent
            };
            ctx.write_stdout(format!("{}\n", out).as_bytes());
        }
        Ok(0)
    })
}

pub fn basename<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: basename: missing operand\n");
            return Ok(1);
        }

        let mut suffix: Option<&str> = None;
        let mut operands = Vec::new();

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "-s" || arg == "--suffix" {
                if let Some(s) = iter.next() {
                    suffix = Some(s.as_str());
                }
            } else if let Some(s) = arg.strip_prefix("--suffix=") {
                suffix = Some(s);
            } else if !arg.starts_with('-') || operands.is_empty() {
                operands.push(arg.as_str());
            }
        }

        if operands.is_empty() {
            ctx.write_stderr(b"esh: basename: missing operand\n");
            return Ok(1);
        }

        let target = operands[0];
        let explicit_suffix = if operands.len() > 1 && suffix.is_none() {
            Some(operands[1])
        } else {
            suffix
        };

        let p = Path::new(target);
        let mut name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| target.to_string());

        if let Some(s) = explicit_suffix
            && name.ends_with(s)
            && name.len() > s.len()
        {
            name.truncate(name.len() - s.len());
        }

        ctx.write_stdout(format!("{}\n", name).as_bytes());
        Ok(0)
    })
}

pub fn realpath<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: realpath: missing operand\n");
            return Ok(1);
        }

        let mut quiet = false;
        let mut rel_to: Option<&str> = None;
        let mut paths = Vec::new();

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "-q" || arg == "--quiet" {
                quiet = true;
            } else if arg == "--relative-to" {
                if let Some(base) = iter.next() {
                    rel_to = Some(base.as_str());
                }
            } else if let Some(base) = arg.strip_prefix("--relative-to=") {
                rel_to = Some(base);
            } else if !arg.starts_with('-') {
                paths.push(arg.as_str());
            }
        }

        if paths.is_empty() {
            ctx.write_stderr(b"esh: realpath: missing operand\n");
            return Ok(1);
        }

        let all_ok = true;
        for path_str in paths {
            let target = resolve_path(ctx, path_str);
            let canonical = target.canonicalize().unwrap_or(target);

            if let Some(base_str) = rel_to {
                let base = resolve_path(ctx, base_str);
                let base_can = base.canonicalize().unwrap_or(base);
                match pathdiff::diff_paths(&canonical, &base_can) {
                    Some(rel) => {
                        ctx.write_stdout(format!("{}\n", present(&rel)).as_bytes());
                    }
                    None => {
                        ctx.write_stdout(format!("{}\n", present(&canonical)).as_bytes());
                    }
                }
            } else {
                ctx.write_stdout(format!("{}\n", present(&canonical)).as_bytes());
            }
        }

        if all_ok || quiet { Ok(0) } else { Ok(1) }
    })
}

pub fn path<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: path: usage: path [-d|--dirname path] [-b|--basename path] [-e|--ext path] [-s|--stem path] [-a|--abs path] [-r|--rel target [base]] [-j|--join parts...] [--exists path] [--is-dir path] [--is-file path]\n");
            return Ok(1);
        }

        let flag = args[0].as_str();
        match flag {
            "-d" | "--dirname" => {
                if args.len() < 2 {
                    ctx.write_stderr(b"esh: path -d: missing path argument\n");
                    return Ok(1);
                }
                let p = Path::new(&args[1]);
                let parent = p.parent().map(present).unwrap_or_else(|| ".".to_string());
                let out = if parent.is_empty() {
                    ".".to_string()
                } else {
                    parent
                };
                ctx.write_stdout(format!("{}\n", out).as_bytes());
                Ok(0)
            }
            "-b" | "--basename" | "--filename" => {
                if args.len() < 2 {
                    ctx.write_stderr(b"esh: path -b: missing path argument\n");
                    return Ok(1);
                }
                let p = Path::new(&args[1]);
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                ctx.write_stdout(format!("{}\n", name).as_bytes());
                Ok(0)
            }
            "-e" | "--ext" => {
                if args.len() < 2 {
                    ctx.write_stderr(b"esh: path -e: missing path argument\n");
                    return Ok(1);
                }
                let p = Path::new(&args[1]);
                let ext = p
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default();
                ctx.write_stdout(format!("{}\n", ext).as_bytes());
                Ok(0)
            }
            "-s" | "--stem" => {
                if args.len() < 2 {
                    ctx.write_stderr(b"esh: path -s: missing path argument\n");
                    return Ok(1);
                }
                let p = Path::new(&args[1]);
                let stem = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                ctx.write_stdout(format!("{}\n", stem).as_bytes());
                Ok(0)
            }
            "-a" | "--abs" | "--canonical" => {
                if args.len() < 2 {
                    ctx.write_stderr(b"esh: path -a: missing path argument\n");
                    return Ok(1);
                }
                let resolved = resolve_path(ctx, &args[1]);
                let normalized = resolved.canonicalize().unwrap_or(resolved);
                ctx.write_stdout(format!("{}\n", present(&normalized)).as_bytes());
                Ok(0)
            }
            "-r" | "--rel" => {
                if args.len() < 2 {
                    ctx.write_stderr(b"esh: path -r: missing path argument\n");
                    return Ok(1);
                }
                let target = resolve_path(ctx, &args[1]);
                let base = if args.len() >= 3 {
                    resolve_path(ctx, &args[2])
                } else {
                    ctx.cwd().to_path_buf()
                };

                match pathdiff::diff_paths(&target, &base) {
                    Some(rel) => {
                        ctx.write_stdout(format!("{}\n", present(&rel)).as_bytes());
                        Ok(0)
                    }
                    None => {
                        ctx.write_stdout(format!("{}\n", present(&target)).as_bytes());
                        Ok(0)
                    }
                }
            }
            "-j" | "--join" => {
                if args.len() < 2 {
                    ctx.write_stdout(b"\n");
                    return Ok(0);
                }
                let mut p = PathBuf::new();
                for part in &args[1..] {
                    p.push(part);
                }
                ctx.write_stdout(format!("{}\n", present(&p)).as_bytes());
                Ok(0)
            }
            "--exists" => {
                if args.len() < 2 {
                    return Ok(1);
                }
                let p = resolve_path(ctx, &args[1]);
                if p.exists() { Ok(0) } else { Ok(1) }
            }
            "--is-dir" => {
                if args.len() < 2 {
                    return Ok(1);
                }
                let p = resolve_path(ctx, &args[1]);
                if p.is_dir() { Ok(0) } else { Ok(1) }
            }
            "--is-file" => {
                if args.len() < 2 {
                    return Ok(1);
                }
                let p = resolve_path(ctx, &args[1]);
                if p.is_file() { Ok(0) } else { Ok(1) }
            }
            other => {
                ctx.write_stderr(
                    format!("esh: path: unrecognized option '{}'\n", other).as_bytes(),
                );
                Ok(1)
            }
        }
    })
}

mod pathdiff {
    use std::path::{Component, Path, PathBuf};

    pub fn diff_paths(path: &Path, base: &Path) -> Option<PathBuf> {
        if path.is_absolute() != base.is_absolute() {
            if path.is_absolute() {
                Some(PathBuf::from(path))
            } else {
                None
            }
        } else {
            let mut ita = path.components();
            let mut itb = base.components();
            let mut comps: Vec<Component> = vec![];
            loop {
                match (ita.next(), itb.next()) {
                    (None, None) => break,
                    (Some(a), None) => {
                        comps.push(a);
                        comps.extend(ita.by_ref());
                        break;
                    }
                    (None, _) => comps.push(Component::ParentDir),
                    (Some(a), Some(b)) if comps.is_empty() && a == b => (),
                    (Some(a), Some(Component::CurDir)) => comps.push(a),
                    (Some(_), Some(Component::ParentDir)) => return None,
                    (Some(a), Some(_)) => {
                        comps.push(Component::ParentDir);
                        for _ in itb {
                            comps.push(Component::ParentDir);
                        }
                        comps.push(a);
                        comps.extend(ita.by_ref());
                        break;
                    }
                }
            }
            Some(comps.iter().map(|c| c.as_os_str()).collect())
        }
    }
}
