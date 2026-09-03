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
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tar::{Archive as TarArchive, Builder as TarBuilder};

fn resolve_path(ctx: &ExecutionContext, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        ctx.cwd().join(p)
    }
}

pub fn tar<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(
                b"esh: tar: usage: tar [-c|-x] [-z] [-f archive] [-C dest] [files...]\n",
            );
            return Ok(1);
        }

        let mut create = false;
        let mut extract = false;
        let mut gzip = false;
        let mut archive_file: Option<&str> = None;
        let mut dest_dir = ctx.cwd().to_path_buf();
        let mut operands = Vec::new();

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if arg.starts_with('-') && !arg.starts_with("--") {
                let chars: Vec<char> = arg[1..].chars().collect();
                let mut idx = 0;
                while idx < chars.len() {
                    match chars[idx] {
                        'c' => create = true,
                        'x' => extract = true,
                        'z' => gzip = true,
                        'C' => {
                            if let Some(d) = iter.next() {
                                dest_dir = resolve_path(ctx, d);
                            }
                        }
                        'f' => {
                            if let Some(f) = iter.next() {
                                archive_file = Some(f.as_str());
                            }
                        }
                        _ => {}
                    }
                    idx += 1;
                }
            } else if arg == "--create" {
                create = true;
            } else if arg == "--extract" {
                extract = true;
            } else if arg == "--gzip" {
                gzip = true;
            } else if arg == "-C" || arg == "--directory" {
                if let Some(d) = iter.next() {
                    dest_dir = resolve_path(ctx, d);
                }
            } else if arg == "-f" || arg == "--file" {
                if let Some(f) = iter.next() {
                    archive_file = Some(f.as_str());
                }
            } else if !arg.starts_with('-') {
                operands.push(arg.as_str());
            }
        }

        let archive_path_str = match archive_file.or_else(|| operands.first().copied()) {
            Some(f) => f,
            None => {
                ctx.write_stderr(b"esh: tar: missing archive file operand\n");
                return Ok(1);
            }
        };

        if create {
            let sources = if archive_file.is_some() {
                &operands[..]
            } else if operands.len() > 1 {
                &operands[1..]
            } else {
                &[]
            };

            let out_path = resolve_path(ctx, archive_path_str);
            if let Some(parent) = out_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            let file = match File::create(&out_path) {
                Ok(f) => f,
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: tar: cannot create '{}': {}\n", archive_path_str, e)
                            .as_bytes(),
                    );
                    return Ok(1);
                }
            };

            let is_gz =
                gzip || archive_path_str.ends_with(".tar.gz") || archive_path_str.ends_with(".tgz");
            if is_gz {
                let enc = GzEncoder::new(file, Compression::default());
                let mut builder = TarBuilder::new(enc);
                for src_str in sources {
                    let src = resolve_path(ctx, src_str);
                    if src.is_dir() {
                        let name = src.file_name().unwrap_or_default();
                        let _ = builder.append_dir_all(name, &src);
                    } else if src.is_file() {
                        let name = src.file_name().unwrap_or_default();
                        let _ = builder.append_path_with_name(&src, name);
                    }
                }
            } else {
                let mut builder = TarBuilder::new(file);
                for src_str in sources {
                    let src = resolve_path(ctx, src_str);
                    if src.is_dir() {
                        let name = src.file_name().unwrap_or_default();
                        let _ = builder.append_dir_all(name, &src);
                    } else if src.is_file() {
                        let name = src.file_name().unwrap_or_default();
                        let _ = builder.append_path_with_name(&src, name);
                    }
                }
            }
            Ok(0)
        } else if extract
            || (!create
                && (archive_path_str.ends_with(".tar")
                    || archive_path_str.ends_with(".tar.gz")
                    || archive_path_str.ends_with(".tgz")))
        {
            let archive_path = resolve_path(ctx, archive_path_str);
            if !archive_path.is_file() {
                ctx.write_stderr(
                    format!("esh: tar: file '{}' not found\n", archive_path_str).as_bytes(),
                );
                return Ok(1);
            }

            let _ = fs::create_dir_all(&dest_dir);
            let file = match File::open(&archive_path) {
                Ok(f) => f,
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: tar: cannot open '{}': {}\n", archive_path_str, e).as_bytes(),
                    );
                    return Ok(1);
                }
            };

            let is_gz =
                gzip || archive_path_str.ends_with(".tar.gz") || archive_path_str.ends_with(".tgz");
            if is_gz {
                let gz = GzDecoder::new(file);
                let mut tar = TarArchive::new(gz);
                if let Err(e) = tar.unpack(&dest_dir) {
                    ctx.write_stderr(format!("esh: tar: extraction error: {}\n", e).as_bytes());
                    return Ok(1);
                }
            } else {
                let mut tar = TarArchive::new(file);
                if let Err(e) = tar.unpack(&dest_dir) {
                    ctx.write_stderr(format!("esh: tar: extraction error: {}\n", e).as_bytes());
                    return Ok(1);
                }
            }
            Ok(0)
        } else {
            ctx.write_stderr(b"esh: tar: must specify -c (create) or -x (extract)\n");
            Ok(1)
        }
    })
}

pub fn zip<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: zip: usage: zip [-r] <archive.zip> <files...>\n");
            return Ok(1);
        }

        let mut operands = Vec::new();
        for arg in args {
            if !arg.starts_with('-') {
                operands.push(arg.as_str());
            }
        }

        if operands.len() < 2 {
            ctx.write_stderr(b"esh: zip: missing zip file or files to archive\n");
            return Ok(1);
        }

        let zip_file = resolve_path(ctx, operands[0]);
        let sources = &operands[1..];

        if let Some(parent) = zip_file.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let file = match File::create(&zip_file) {
            Ok(f) => f,
            Err(e) => {
                ctx.write_stderr(
                    format!("esh: zip: cannot create '{}': {}\n", operands[0], e).as_bytes(),
                );
                return Ok(1);
            }
        };

        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for src_str in sources {
            let src = resolve_path(ctx, src_str);
            if src.is_dir() {
                if let Err(e) = add_dir_to_zip(&mut zip, &src, &src, options) {
                    ctx.write_stderr(
                        format!("esh: zip: error adding dir '{}': {}\n", src_str, e).as_bytes(),
                    );
                    return Ok(1);
                }
            } else if src.is_file() {
                let name = src.file_name().unwrap_or_default().to_string_lossy();
                if let Err(e) = zip.start_file(name, options) {
                    ctx.write_stderr(format!("esh: zip: error: {}\n", e).as_bytes());
                    return Ok(1);
                }
                if let Ok(mut f) = File::open(&src) {
                    let mut buf = Vec::new();
                    let _ = f.read_to_end(&mut buf);
                    let _ = zip.write_all(&buf);
                }
            }
        }

        if let Err(e) = zip.finish() {
            ctx.write_stderr(format!("esh: zip: error finalizing: {}\n", e).as_bytes());
            return Ok(1);
        }

        Ok(0)
    })
}

pub fn unzip<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: unzip: usage: unzip <archive.zip> [-d destination_dir]\n");
            return Ok(1);
        }

        let mut zip_file: Option<&str> = None;
        let mut dest_dir = ctx.cwd().to_path_buf();

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "-d" || arg == "--destination" {
                if let Some(d) = iter.next() {
                    dest_dir = resolve_path(ctx, d);
                }
            } else if !arg.starts_with('-') && zip_file.is_none() {
                zip_file = Some(arg.as_str());
            }
        }

        let zip_str = match zip_file {
            Some(f) => f,
            None => {
                ctx.write_stderr(b"esh: unzip: missing zip file operand\n");
                return Ok(1);
            }
        };

        let zip_path = resolve_path(ctx, zip_str);
        if !zip_path.is_file() {
            ctx.write_stderr(format!("esh: unzip: file '{}' not found\n", zip_str).as_bytes());
            return Ok(1);
        }

        let _ = fs::create_dir_all(&dest_dir);
        let file = match File::open(&zip_path) {
            Ok(f) => f,
            Err(e) => {
                ctx.write_stderr(
                    format!("esh: unzip: cannot open '{}': {}\n", zip_str, e).as_bytes(),
                );
                return Ok(1);
            }
        };

        let mut archive = match zip::ZipArchive::new(file) {
            Ok(z) => z,
            Err(e) => {
                ctx.write_stderr(format!("esh: unzip: invalid zip file: {}\n", e).as_bytes());
                return Ok(1);
            }
        };

        if let Err(e) = archive.extract(&dest_dir) {
            ctx.write_stderr(format!("esh: unzip: extraction error: {}\n", e).as_bytes());
            return Ok(1);
        }

        Ok(0)
    })
}

pub fn archive<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.len() < 3 {
            ctx.write_stderr(
                b"esh: archive: usage: archive <tar|tar.gz|zip> <source_path> <output_archive>\n",
            );
            return Ok(1);
        }

        let format = args[0].to_lowercase();
        let src_path = resolve_path(ctx, &args[1]);
        let out_path = resolve_path(ctx, &args[2]);

        if !src_path.exists() {
            ctx.write_stderr(
                format!("esh: archive: source '{}' does not exist\n", args[1]).as_bytes(),
            );
            return Ok(1);
        }

        if let Some(parent) = out_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        match format.as_str() {
            "tar" => {
                let file = match File::create(&out_path) {
                    Ok(f) => f,
                    Err(e) => {
                        ctx.write_stderr(
                            format!("esh: archive: cannot create '{}': {}\n", args[2], e)
                                .as_bytes(),
                        );
                        return Ok(1);
                    }
                };
                let mut builder = TarBuilder::new(file);
                let res = if src_path.is_dir() {
                    builder.append_dir_all(".", &src_path)
                } else {
                    let name = src_path.file_name().unwrap_or_default();
                    builder.append_path_with_name(&src_path, name)
                };
                if let Err(e) = res {
                    ctx.write_stderr(
                        format!("esh: archive: tar compression error: {}\n", e).as_bytes(),
                    );
                    return Ok(1);
                }
            }
            "tar.gz" | "tgz" => {
                let file = match File::create(&out_path) {
                    Ok(f) => f,
                    Err(e) => {
                        ctx.write_stderr(
                            format!("esh: archive: cannot create '{}': {}\n", args[2], e)
                                .as_bytes(),
                        );
                        return Ok(1);
                    }
                };
                let enc = GzEncoder::new(file, Compression::default());
                let mut builder = TarBuilder::new(enc);
                let res = if src_path.is_dir() {
                    builder.append_dir_all(".", &src_path)
                } else {
                    let name = src_path.file_name().unwrap_or_default();
                    builder.append_path_with_name(&src_path, name)
                };
                if let Err(e) = res {
                    ctx.write_stderr(
                        format!("esh: archive: tar.gz compression error: {}\n", e).as_bytes(),
                    );
                    return Ok(1);
                }
            }
            "zip" => {
                let file = match File::create(&out_path) {
                    Ok(f) => f,
                    Err(e) => {
                        ctx.write_stderr(
                            format!("esh: archive: cannot create '{}': {}\n", args[2], e)
                                .as_bytes(),
                        );
                        return Ok(1);
                    }
                };
                let mut zip = zip::ZipWriter::new(file);
                let options = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated);

                if src_path.is_dir() {
                    if let Err(e) = add_dir_to_zip(&mut zip, &src_path, &src_path, options) {
                        ctx.write_stderr(format!("esh: archive: zip error: {}\n", e).as_bytes());
                        return Ok(1);
                    }
                } else {
                    let name = src_path.file_name().unwrap_or_default().to_string_lossy();
                    if let Err(e) = zip.start_file(name, options) {
                        ctx.write_stderr(format!("esh: archive: zip error: {}\n", e).as_bytes());
                        return Ok(1);
                    }
                    if let Ok(mut f) = File::open(&src_path) {
                        let mut buf = Vec::new();
                        let _ = f.read_to_end(&mut buf);
                        let _ = zip.write_all(&buf);
                    }
                }
                if let Err(e) = zip.finish() {
                    ctx.write_stderr(
                        format!("esh: archive: finalizing zip error: {}\n", e).as_bytes(),
                    );
                    return Ok(1);
                }
            }
            other => {
                ctx.write_stderr(
                    format!(
                        "esh: archive: unsupported format '{}' (expected tar, tar.gz, zip)\n",
                        other
                    )
                    .as_bytes(),
                );
                return Ok(1);
            }
        }

        Ok(0)
    })
}

fn add_dir_to_zip<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    base: &Path,
    current: &Path,
    options: zip::write::SimpleFileOptions,
) -> std::io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let rel_path = path.strip_prefix(base).map_err(std::io::Error::other)?;
        let name = present(rel_path);

        if path.is_dir() {
            zip.add_directory(format!("{}/", name), options)?;
            add_dir_to_zip(zip, base, &path, options)?;
        } else {
            zip.start_file(name, options)?;
            let mut f = File::open(&path)?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            zip.write_all(&buf)?;
        }
    }
    Ok(())
}

pub fn unarchive<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(
                b"esh: unarchive: usage: unarchive <archive_path> [-C destination_dir]\n",
            );
            return Ok(1);
        }

        let archive_file = resolve_path(ctx, &args[0]);
        let mut dest_dir = ctx.cwd().to_path_buf();

        let mut iter = args.iter().skip(1).peekable();
        while let Some(arg) = iter.next() {
            if (arg == "-C" || arg == "--destination")
                && let Some(d) = iter.next()
            {
                dest_dir = resolve_path(ctx, d);
            }
        }

        if !archive_file.is_file() {
            ctx.write_stderr(format!("esh: unarchive: file '{}' not found\n", args[0]).as_bytes());
            return Ok(1);
        }

        let _ = fs::create_dir_all(&dest_dir);
        let filename = archive_file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();

        if filename.ends_with(".tar.gz") || filename.ends_with(".tgz") {
            let file = match File::open(&archive_file) {
                Ok(f) => f,
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: unarchive: cannot open '{}': {}\n", args[0], e).as_bytes(),
                    );
                    return Ok(1);
                }
            };
            let gz = GzDecoder::new(file);
            let mut tar = TarArchive::new(gz);
            if let Err(e) = tar.unpack(&dest_dir) {
                ctx.write_stderr(
                    format!("esh: unarchive: tar.gz extraction error: {}\n", e).as_bytes(),
                );
                return Ok(1);
            }
        } else if filename.ends_with(".tar") {
            let file = match File::open(&archive_file) {
                Ok(f) => f,
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: unarchive: cannot open '{}': {}\n", args[0], e).as_bytes(),
                    );
                    return Ok(1);
                }
            };
            let mut tar = TarArchive::new(file);
            if let Err(e) = tar.unpack(&dest_dir) {
                ctx.write_stderr(
                    format!("esh: unarchive: tar extraction error: {}\n", e).as_bytes(),
                );
                return Ok(1);
            }
        } else if filename.ends_with(".zip") {
            let file = match File::open(&archive_file) {
                Ok(f) => f,
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: unarchive: cannot open '{}': {}\n", args[0], e).as_bytes(),
                    );
                    return Ok(1);
                }
            };
            let mut zip = match zip::ZipArchive::new(file) {
                Ok(z) => z,
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: unarchive: invalid zip archive: {}\n", e).as_bytes(),
                    );
                    return Ok(1);
                }
            };
            if let Err(e) = zip.extract(&dest_dir) {
                ctx.write_stderr(
                    format!("esh: unarchive: zip extraction error: {}\n", e).as_bytes(),
                );
                return Ok(1);
            }
        } else {
            ctx.write_stderr(
                format!(
                    "esh: unarchive: unrecognized archive format for '{}'\n",
                    args[0]
                )
                .as_bytes(),
            );
            return Ok(1);
        }

        Ok(0)
    })
}
