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
use base64::prelude::*;
use md5::Md5;
use sha2::digest::Digest;
use sha2::{Sha256, Sha512};
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

pub fn sha256sum<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut check_manifest: Option<&str> = None;
        let mut files = Vec::new();
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            if arg == "-c" || arg == "--check" {
                let Some(manifest) = iter.next() else {
                    ctx.write_stderr(b"esh: sha256sum: option requires an argument -- 'c'\n");
                    return Ok(2);
                };
                if check_manifest.replace(manifest.as_str()).is_some() {
                    ctx.write_stderr(b"esh: sha256sum: only one checksum file may be checked\n");
                    return Ok(2);
                }
            } else if arg.starts_with('-') && arg != "-" {
                ctx.write_stderr(
                    format!("esh: sha256sum: unrecognized option '{}'\n", arg).as_bytes(),
                );
                return Ok(2);
            } else {
                files.push(arg.as_str());
            }
        }

        if let Some(manifest) = check_manifest {
            if !files.is_empty() {
                ctx.write_stderr(
                    b"esh: sha256sum: --check cannot be combined with file operands\n",
                );
                return Ok(2);
            }
            return check_sha256_manifest(manifest, ctx);
        }

        if files.is_empty() {
            if let Some(buf) = ctx.take_stdin() {
                let mut hasher = Sha256::new();
                hasher.update(&buf);
                let hash = hex_encode(&hasher.finalize());
                ctx.write_stdout(format!("{}  -\n", hash).as_bytes());
                return Ok(0);
            }
            return Ok(1);
        }

        let mut all_ok = true;
        for path_str in files {
            if path_str == "-" {
                if let Some(buf) = ctx.take_stdin() {
                    let mut hasher = Sha256::new();
                    hasher.update(&buf);
                    let hash = hex_encode(&hasher.finalize());
                    ctx.write_stdout(format!("{}  -\n", hash).as_bytes());
                }
                continue;
            }

            let path = resolve_path(ctx, path_str);
            match fs::read(&path) {
                Ok(bytes) => {
                    let mut hasher = Sha256::new();
                    hasher.update(&bytes);
                    let hash = hex_encode(&hasher.finalize());
                    ctx.write_stdout(format!("{}  {}\n", hash, path_str).as_bytes());
                }
                Err(e) => {
                    ctx.write_stderr(format!("esh: sha256sum: {}: {}\n", path_str, e).as_bytes());
                    all_ok = false;
                }
            }
        }

        if all_ok { Ok(0) } else { Ok(1) }
    })
}

pub fn md5sum<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            if let Some(buf) = ctx.take_stdin() {
                let mut hasher = Md5::new();
                hasher.update(&buf);
                let hash = hex_encode(&hasher.finalize());
                ctx.write_stdout(format!("{}  -\n", hash).as_bytes());
                return Ok(0);
            }
            return Ok(1);
        }

        let mut all_ok = true;
        for path_str in args {
            if path_str.starts_with('-') && path_str != "-" {
                continue;
            }
            if path_str == "-" {
                if let Some(buf) = ctx.take_stdin() {
                    let mut hasher = Md5::new();
                    hasher.update(&buf);
                    let hash = hex_encode(&hasher.finalize());
                    ctx.write_stdout(format!("{}  -\n", hash).as_bytes());
                }
                continue;
            }

            let path = resolve_path(ctx, path_str);
            match fs::read(&path) {
                Ok(bytes) => {
                    let mut hasher = Md5::new();
                    hasher.update(&bytes);
                    let hash = hex_encode(&hasher.finalize());
                    ctx.write_stdout(format!("{}  {}\n", hash, path_str).as_bytes());
                }
                Err(e) => {
                    ctx.write_stderr(format!("esh: md5sum: {}: {}\n", path_str, e).as_bytes());
                    all_ok = false;
                }
            }
        }

        if all_ok { Ok(0) } else { Ok(1) }
    })
}

pub fn hash<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: hash: usage: hash [-a|--algo sha256|sha512|md5|crc32] [--sha256] [--sha512] [--md5] [--crc32] [file|-]\n");
            return Ok(1);
        }

        let mut algo = String::new();
        let mut operands = Vec::new();

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "-a" || arg == "--algo" || arg == "--algorithm" {
                if let Some(a) = iter.next() {
                    algo = a.to_lowercase();
                }
            } else if let Some(a) = arg.strip_prefix("--algo=") {
                algo = a.to_lowercase();
            } else if arg == "--sha256" || arg == "--256" {
                algo = "sha256".to_string();
            } else if arg == "--sha512" || arg == "--512" {
                algo = "sha512".to_string();
            } else if arg == "--md5" {
                algo = "md5".to_string();
            } else if arg == "--crc32" {
                algo = "crc32".to_string();
            } else if !arg.starts_with('-') || arg == "-" {
                operands.push(arg.as_str());
            } else {
                ctx.write_stderr(format!("esh: hash: unrecognized option '{}'\n", arg).as_bytes());
                return Ok(1);
            }
        }

        if algo.is_empty() {
            ctx.write_stderr(b"esh: hash: missing algorithm option (-a sha256|sha512|md5|crc32)\n");
            return Ok(1);
        }

        let data: Vec<u8> = if !operands.is_empty() {
            let target = operands[0];
            if target == "-" {
                ctx.take_stdin().unwrap_or_default()
            } else {
                let path = resolve_path(ctx, target);
                if path.is_file() {
                    match fs::read(&path) {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            ctx.write_stderr(
                                format!("esh: hash: cannot read '{}': {}\n", target, e).as_bytes(),
                            );
                            return Ok(1);
                        }
                    }
                } else {
                    target.as_bytes().to_vec()
                }
            }
        } else if let Some(buf) = ctx.take_stdin() {
            buf
        } else {
            ctx.write_stderr(b"esh: hash: missing input operand\n");
            return Ok(1);
        };

        let result = match algo.as_str() {
            "sha256" | "256" => {
                let mut hasher = Sha256::new();
                hasher.update(&data);
                hex_encode(&hasher.finalize())
            }
            "sha512" | "512" => {
                let mut hasher = Sha512::new();
                hasher.update(&data);
                hex_encode(&hasher.finalize())
            }
            "md5" => {
                let mut hasher = Md5::new();
                hasher.update(&data);
                hex_encode(&hasher.finalize())
            }
            "crc32" => {
                let mut hasher = crc32fast::Hasher::new();
                hasher.update(&data);
                format!("{:08x}", hasher.finalize())
            }
            other => {
                ctx.write_stderr(format!("esh: hash: unsupported algorithm '{}' (expected sha256, sha512, md5, crc32)\n", other).as_bytes());
                return Ok(1);
            }
        };

        ctx.write_stdout(format!("{}\n", result).as_bytes());
        Ok(0)
    })
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn check_sha256_manifest(manifest_str: &str, ctx: &mut ExecutionContext) -> Result<i32, ExecError> {
    let manifest_path = resolve_path(ctx, manifest_str);
    let manifest = match fs::read_to_string(&manifest_path) {
        Ok(manifest) => manifest,
        Err(error) => {
            ctx.write_stderr(format!("esh: sha256sum: {}: {}\n", manifest_str, error).as_bytes());
            return Ok(1);
        }
    };

    let mut all_ok = true;
    for line in manifest.lines() {
        let Some((expected, remainder)) = line.split_once(char::is_whitespace) else {
            ctx.write_stderr(
                format!(
                    "esh: sha256sum: {}: improperly formatted checksum line\n",
                    manifest_str
                )
                .as_bytes(),
            );
            all_ok = false;
            continue;
        };
        let filename = remainder
            .trim_start()
            .strip_prefix('*')
            .unwrap_or(remainder.trim_start());
        if expected.len() != 64
            || !expected.bytes().all(|byte| byte.is_ascii_hexdigit())
            || filename.is_empty()
        {
            ctx.write_stderr(
                format!(
                    "esh: sha256sum: {}: improperly formatted checksum line\n",
                    manifest_str
                )
                .as_bytes(),
            );
            all_ok = false;
            continue;
        }

        let path = resolve_path(ctx, filename);
        let actual = match fs::read(&path) {
            Ok(bytes) => {
                let mut hasher = Sha256::new();
                hasher.update(bytes);
                hex_encode(&hasher.finalize())
            }
            Err(error) => {
                ctx.write_stderr(format!("esh: sha256sum: {}: {}\n", filename, error).as_bytes());
                all_ok = false;
                continue;
            }
        };

        if actual.eq_ignore_ascii_case(expected) {
            ctx.write_stdout(format!("{}: OK\n", filename).as_bytes());
        } else {
            ctx.write_stdout(format!("{}: FAILED\n", filename).as_bytes());
            all_ok = false;
        }
    }

    if all_ok { Ok(0) } else { Ok(1) }
}

pub fn base64<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut decode = false;
        let mut operands = Vec::new();

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "-d" || arg == "-D" || arg == "--decode" || arg == "decode" {
                decode = true;
            } else if arg == "encode" {
                decode = false;
            } else if arg == "-f" || arg == "--file" {
                if let Some(f) = iter.next() {
                    operands.push(f.as_str());
                }
            } else if !arg.starts_with('-') {
                operands.push(arg.as_str());
            }
        }

        let data: Vec<u8> = if !operands.is_empty() {
            let target = operands[0];
            let path = resolve_path(ctx, target);
            if path.is_file() {
                match fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        ctx.write_stderr(
                            format!("esh: base64: cannot read file '{}': {}\n", target, e)
                                .as_bytes(),
                        );
                        return Ok(1);
                    }
                }
            } else {
                target.as_bytes().to_vec()
            }
        } else if let Some(buf) = ctx.take_stdin() {
            buf
        } else {
            return Ok(0);
        };

        if decode {
            let input_str = String::from_utf8_lossy(&data).trim().to_string();
            match BASE64_STANDARD.decode(input_str.as_bytes()) {
                Ok(decoded) => {
                    ctx.write_stdout(&decoded);
                    Ok(0)
                }
                Err(e) => {
                    ctx.write_stderr(format!("esh: base64: decode error: {}\n", e).as_bytes());
                    Ok(1)
                }
            }
        } else {
            let encoded = BASE64_STANDARD.encode(&data);
            ctx.write_stdout(format!("{}\n", encoded).as_bytes());
            Ok(0)
        }
    })
}
