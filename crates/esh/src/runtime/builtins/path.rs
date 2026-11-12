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
use std::{fs, io};

fn resolve_path(ctx: &ExecutionContext, path_str: &str) -> PathBuf {
    let p = Path::new(path_str);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        ctx.cwd().join(p)
    }
}

fn canonicalize_path(path: &Path, allow_missing_leaf: bool) -> io::Result<PathBuf> {
    match path.canonicalize() {
        Ok(path) => Ok(path),
        Err(error) if allow_missing_leaf && error.kind() == io::ErrorKind::NotFound => {
            let path_text = path.as_os_str().to_string_lossy();
            let trimmed = path_text.trim_end_matches(std::path::MAIN_SEPARATOR);
            let lookup_path = if trimmed.is_empty() {
                path
            } else {
                Path::new(trimmed)
            };

            match fs::symlink_metadata(lookup_path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    let target = fs::read_link(lookup_path)?;
                    let target_path = if target.is_absolute() {
                        target
                    } else {
                        lookup_path
                            .parent()
                            .unwrap_or_else(|| Path::new("."))
                            .join(target)
                    };
                    canonicalize_path(&target_path, true)
                }
                Ok(_) => Err(error),
                Err(metadata_error) if metadata_error.kind() == io::ErrorKind::NotFound => {
                    let leaf = lookup_path.file_name().ok_or(error)?;
                    let parent = lookup_path.parent().unwrap_or_else(|| Path::new("."));
                    let canonical_parent = parent.canonicalize()?;
                    Ok(canonical_parent.join(leaf))
                }
                Err(metadata_error) => Err(metadata_error),
            }
        }
        Err(error) => Err(error),
    }
}

fn posix_dirname(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    if path == "//" || path.chars().all(|ch| ch == '/') {
        return "/".to_string();
    }

    let trimmed = path.trim_end_matches('/');
    let Some(separator) = trimmed.rfind('/') else {
        return ".".to_string();
    };
    let parent = trimmed[..separator].trim_end_matches('/');
    if parent.is_empty() {
        "/".to_string()
    } else {
        parent.to_string()
    }
}

fn posix_basename(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    if path == "//" || path.chars().all(|ch| ch == '/') {
        return "/".to_string();
    }

    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

pub fn dirname<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let args = if args.first().is_some_and(|arg| arg == "--") {
            &args[1..]
        } else {
            args
        };
        if args.is_empty() {
            ctx.write_stderr(b"esh: dirname: missing operand\n");
            return Ok(1);
        }

        for path_str in args {
            ctx.write_stdout(format!("{}\n", posix_dirname(path_str)).as_bytes());
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

        let mut suffix = None;
        let mut multiple = false;
        let mut operands = Vec::new();
        let mut parsing_options = true;
        let mut index = 0;

        while index < args.len() {
            let arg = args[index].as_str();
            if parsing_options && arg == "--" {
                parsing_options = false;
            } else if parsing_options && (arg == "-a" || arg == "--multiple") {
                multiple = true;
            } else if parsing_options && (arg == "-s" || arg == "--suffix") {
                index += 1;
                if index == args.len() {
                    ctx.write_stderr(b"esh: basename: option requires a suffix\n");
                    return Ok(2);
                }
                suffix = Some(args[index].as_str());
                multiple = true;
            } else if parsing_options && let Some(value) = arg.strip_prefix("--suffix=") {
                suffix = Some(value);
                multiple = true;
            } else if parsing_options && arg.starts_with('-') && arg != "-" {
                ctx.write_stderr(format!("esh: basename: unknown option '{}'\n", arg).as_bytes());
                return Ok(2);
            } else {
                parsing_options = false;
                operands.push(arg);
            }
            index += 1;
        }

        if operands.is_empty() {
            ctx.write_stderr(b"esh: basename: missing operand\n");
            return Ok(1);
        }

        if !multiple && operands.len() > 2 {
            ctx.write_stderr(b"esh: basename: too many operands\n");
            return Ok(2);
        }

        let positional_suffix = if !multiple && operands.len() == 2 {
            Some(operands[1])
        } else {
            None
        };
        let suffix = suffix.or(positional_suffix);

        for target in operands
            .iter()
            .take(if multiple { operands.len() } else { 1 })
        {
            let mut name = posix_basename(target);
            if let Some(value) = suffix
                && name.ends_with(value)
                && name.len() > value.len()
            {
                name.truncate(name.len() - value.len());
            }
            ctx.write_stdout(format!("{}\n", name).as_bytes());
        }
        Ok(0)
    })
}

pub fn realpath<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut quiet = false;
        let mut rel_to: Option<&str> = None;
        let mut allow_missing_leaf = false;
        let mut paths = Vec::new();
        let mut options = true;

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if options && arg == "--" {
                options = false;
            } else if options && (arg == "-q" || arg == "--quiet") {
                quiet = true;
            } else if options && (arg == "-e" || arg == "-E") {
                allow_missing_leaf = arg == "-E";
            } else if options && arg == "--relative-to" {
                let Some(base) = iter.next() else {
                    ctx.write_stderr(b"esh: realpath: --relative-to requires a directory\n");
                    return Ok(2);
                };
                rel_to = Some(base.as_str());
            } else if options && let Some(base) = arg.strip_prefix("--relative-to=") {
                rel_to = Some(base);
            } else if options && arg.starts_with('-') {
                ctx.write_stderr(
                    format!("esh: realpath: unsupported option '{}'\n", arg).as_bytes(),
                );
                return Ok(2);
            } else {
                paths.push(arg.as_str());
            }
        }

        if paths.is_empty() {
            ctx.write_stderr(b"esh: realpath: missing operand\n");
            return Ok(1);
        }

        let canonical_base = if let Some(base_str) = rel_to {
            match resolve_path(ctx, base_str).canonicalize() {
                Ok(base) => Some(base),
                Err(error) => {
                    if !quiet {
                        ctx.write_stderr(
                            format!("esh: realpath: cannot resolve '{}': {}\n", base_str, error)
                                .as_bytes(),
                        );
                    }
                    return Ok(1);
                }
            }
        } else {
            None
        };

        let mut all_ok = true;
        for path_str in paths {
            let target = resolve_path(ctx, path_str);
            let canonical = match canonicalize_path(&target, allow_missing_leaf) {
                Ok(canonical) => canonical,
                Err(error) => {
                    all_ok = false;
                    if !quiet {
                        ctx.write_stderr(
                            format!("esh: realpath: cannot resolve '{}': {}\n", path_str, error)
                                .as_bytes(),
                        );
                    }
                    continue;
                }
            };

            if let Some(base) = canonical_base.as_ref() {
                match pathdiff::diff_paths(&canonical, base) {
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

        Ok(if all_ok { 0 } else { 1 })
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
