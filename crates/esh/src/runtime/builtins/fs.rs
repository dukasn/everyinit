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
use std::collections::VecDeque;
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

pub fn pwd<'a>(
    _args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let cwd_str = present(ctx.cwd());
        ctx.write_stdout(format!("{}\n", cwd_str).as_bytes());
        Ok(0)
    })
}

pub fn cd<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let target = if let Some(first) = args.first() {
            first.as_str()
        } else {
            "~"
        };

        let path = if target == "~" {
            std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| ".".to_string())
        } else {
            target.to_string()
        };

        match ctx.set_cwd(&path) {
            Ok(_) => Ok(0),
            Err(e) => {
                ctx.write_stderr(format!("esh: cd: {}\n", e).as_bytes());
                Ok(1)
            }
        }
    })
}

pub fn mkdir<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut parents = false;
        let mut paths = Vec::new();

        for arg in args {
            if arg == "-p" || arg == "--parents" {
                parents = true;
            } else if !arg.starts_with('-') {
                paths.push(arg.as_str());
            }
        }

        if paths.is_empty() {
            ctx.write_stderr(b"esh: mkdir: missing operand\n");
            return Ok(1);
        }

        for path_str in paths {
            let path = resolve_path(ctx, path_str);
            let res = if parents {
                fs::create_dir_all(&path)
            } else {
                fs::create_dir(&path)
            };

            if let Err(e) = res {
                ctx.write_stderr(
                    format!(
                        "esh: mkdir: cannot create directory '{}': {}\n",
                        path_str, e
                    )
                    .as_bytes(),
                );
                return Ok(1);
            }
        }

        Ok(0)
    })
}

pub fn rm<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut recursive = false;
        let mut force = false;
        let mut paths = Vec::new();

        for arg in args {
            if arg == "-r" || arg == "-R" || arg == "--recursive" {
                recursive = true;
            } else if arg == "-f" || arg == "--force" {
                force = true;
            } else if arg == "-rf" || arg == "-fr" {
                recursive = true;
                force = true;
            } else if !arg.starts_with('-') {
                paths.push(arg.as_str());
            }
        }

        if paths.is_empty() {
            if force {
                return Ok(0);
            }
            ctx.write_stderr(b"esh: rm: missing operand\n");
            return Ok(1);
        }

        for path_str in paths {
            let path = resolve_path(ctx, path_str);
            if !path.exists() {
                if !force {
                    ctx.write_stderr(
                        format!(
                            "esh: rm: cannot remove '{}': No such file or directory\n",
                            path_str
                        )
                        .as_bytes(),
                    );
                    return Ok(1);
                }
                continue;
            }

            let res = if path.is_dir() {
                if recursive {
                    fs::remove_dir_all(&path)
                } else {
                    ctx.write_stderr(
                        format!("esh: rm: cannot remove '{}': Is a directory\n", path_str)
                            .as_bytes(),
                    );
                    return Ok(1);
                }
            } else {
                fs::remove_file(&path)
            };

            if let Err(e) = res
                && !force
            {
                ctx.write_stderr(
                    format!("esh: rm: cannot remove '{}': {}\n", path_str, e).as_bytes(),
                );
                return Ok(1);
            }
        }

        Ok(0)
    })
}

pub fn cp<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut recursive = false;
        let mut paths = Vec::new();

        for arg in args {
            if arg == "-r" || arg == "-R" || arg == "--recursive" {
                recursive = true;
            } else if !arg.starts_with('-') {
                paths.push(arg.as_str());
            }
        }

        if paths.len() < 2 {
            ctx.write_stderr(b"esh: cp: missing destination file operand\n");
            return Ok(1);
        }

        let src = resolve_path(ctx, paths[0]);
        let dst = resolve_path(ctx, paths[1]);

        let res = if src.is_dir() {
            if !recursive {
                ctx.write_stderr(
                    format!(
                        "esh: cp: -r not specified; omitting directory '{}'\n",
                        paths[0]
                    )
                    .as_bytes(),
                );
                return Ok(1);
            }
            copy_dir_all(&src, &dst)
        } else {
            let final_dst = if dst.is_dir() {
                dst.join(src.file_name().unwrap_or_default())
            } else {
                dst
            };
            fs::copy(&src, &final_dst).map(|_| ())
        };

        match res {
            Ok(_) => Ok(0),
            Err(e) => {
                ctx.write_stderr(
                    format!(
                        "esh: cp: cannot copy '{}' to '{}': {}\n",
                        paths[0], paths[1], e
                    )
                    .as_bytes(),
                );
                Ok(1)
            }
        }
    })
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

pub fn mv<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let paths: Vec<&str> = args
            .iter()
            .filter(|a| !a.starts_with('-'))
            .map(|s| s.as_str())
            .collect();

        if paths.len() < 2 {
            ctx.write_stderr(b"esh: mv: missing destination file operand\n");
            return Ok(1);
        }

        let src = resolve_path(ctx, paths[0]);
        let dst = resolve_path(ctx, paths[1]);

        let final_dst = if dst.is_dir() {
            dst.join(src.file_name().unwrap_or_default())
        } else {
            dst
        };

        match fs::rename(&src, &final_dst) {
            Ok(_) => Ok(0),
            Err(e) => {
                ctx.write_stderr(
                    format!(
                        "esh: mv: cannot move '{}' to '{}': {}\n",
                        paths[0], paths[1], e
                    )
                    .as_bytes(),
                );
                Ok(1)
            }
        }
    })
}

pub fn touch<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: touch: missing file operand\n");
            return Ok(1);
        }

        for path_str in args {
            let path = resolve_path(ctx, path_str);
            if path.exists() {
                let _ = fs::OpenOptions::new().write(true).open(&path);
            } else {
                if let Err(e) = fs::File::create(&path) {
                    ctx.write_stderr(
                        format!("esh: touch: cannot touch '{}': {}\n", path_str, e).as_bytes(),
                    );
                    return Ok(1);
                }
            }
        }

        Ok(0)
    })
}

pub fn cat<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            if let Some(buf) = ctx.take_stdin() {
                ctx.write_stdout(&buf);
            }
            return Ok(0);
        }

        for path_str in args {
            let path = resolve_path(ctx, path_str);
            match fs::read(&path) {
                Ok(data) => ctx.write_stdout(&data),
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: cat: cannot open '{}': {}\n", path_str, e).as_bytes(),
                    );
                    return Ok(1);
                }
            }
        }

        Ok(0)
    })
}

pub fn head<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut count = 10usize;
        let mut files = Vec::new();

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "-n" || arg == "--lines" {
                if let Some(val) = iter.next()
                    && let Ok(n) = val.parse::<usize>()
                {
                    count = n;
                }
            } else if !arg.starts_with('-') {
                files.push(arg.as_str());
            }
        }

        let process_text = |text: &str, ctx: &mut ExecutionContext| {
            for (idx, line) in text.lines().enumerate() {
                if idx >= count {
                    break;
                }
                ctx.write_stdout(format!("{}\n", line).as_bytes());
            }
        };

        if files.is_empty() {
            if let Some(buf) = ctx.take_stdin() {
                let s = String::from_utf8_lossy(&buf);
                process_text(&s, ctx);
            }
            return Ok(0);
        }

        for path_str in files {
            let path = resolve_path(ctx, path_str);
            match fs::read_to_string(&path) {
                Ok(content) => process_text(&content, ctx),
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: head: cannot read '{}': {}\n", path_str, e).as_bytes(),
                    );
                    return Ok(1);
                }
            }
        }

        Ok(0)
    })
}

pub fn tail<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut count = 10usize;
        let mut files = Vec::new();

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "-n" || arg == "--lines" {
                if let Some(val) = iter.next()
                    && let Ok(n) = val.parse::<usize>()
                {
                    count = n;
                }
            } else if !arg.starts_with('-') {
                files.push(arg.as_str());
            }
        }

        let process_text = |text: &str, ctx: &mut ExecutionContext| {
            let mut ring: VecDeque<&str> = VecDeque::with_capacity(count);
            for line in text.lines() {
                if ring.len() >= count {
                    ring.pop_front();
                }
                ring.push_back(line);
            }
            for line in ring {
                ctx.write_stdout(format!("{}\n", line).as_bytes());
            }
        };

        if files.is_empty() {
            if let Some(buf) = ctx.take_stdin() {
                let s = String::from_utf8_lossy(&buf);
                process_text(&s, ctx);
            }
            return Ok(0);
        }

        for path_str in files {
            let path = resolve_path(ctx, path_str);
            match fs::read_to_string(&path) {
                Ok(content) => process_text(&content, ctx),
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: tail: cannot read '{}': {}\n", path_str, e).as_bytes(),
                    );
                    return Ok(1);
                }
            }
        }

        Ok(0)
    })
}

pub fn stat<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: stat: missing operand\n");
            return Ok(1);
        }

        let mut format_spec: Option<&str> = None;
        let mut files = Vec::new();

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "-c" || arg == "--format" {
                if let Some(f) = iter.next() {
                    format_spec = Some(f.as_str());
                }
            } else if let Some(f) = arg.strip_prefix("-c") {
                format_spec = Some(f);
            } else if let Some(f) = arg.strip_prefix("--format=") {
                format_spec = Some(f);
            } else if !arg.starts_with('-') {
                files.push(arg.as_str());
            }
        }

        if files.is_empty() {
            ctx.write_stderr(b"esh: stat: missing operand\n");
            return Ok(1);
        }

        let mut all_ok = true;
        for path_str in files {
            let path = resolve_path(ctx, path_str);
            match fs::metadata(&path) {
                Ok(meta) => {
                    if let Some(fmt) = format_spec {
                        let mut out = fmt.to_string();
                        out = out.replace("%s", &meta.len().to_string());
                        out = out.replace("%n", path_str);
                        let file_type = if meta.is_dir() {
                            "directory"
                        } else {
                            "regular file"
                        };
                        out = out.replace("%F", file_type);
                        ctx.write_stdout(format!("{}\n", out).as_bytes());
                    } else {
                        let file_type = if meta.is_dir() {
                            "directory"
                        } else {
                            "regular file"
                        };
                        ctx.write_stdout(
                            format!(
                                "  File: {}\n  Size: {}\tType: {}\n",
                                path_str,
                                meta.len(),
                                file_type
                            )
                            .as_bytes(),
                        );
                    }
                }
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: stat: cannot stat '{}': {}\n", path_str, e).as_bytes(),
                    );
                    all_ok = false;
                }
            }
        }

        if all_ok { Ok(0) } else { Ok(1) }
    })
}
