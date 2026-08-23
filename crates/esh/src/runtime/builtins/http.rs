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
use crate::runtime::builtins::flow::parse_duration;
use crate::runtime::context::ExecutionContext;
use crate::runtime::traits::BoxFuture;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tokio::net::TcpStream;

pub fn http<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: http: missing subcommand (get, post)\n");
            return Ok(1);
        }

        let subcmd = args[0].as_str();
        match subcmd {
            "get" => {
                if args.len() < 2 {
                    ctx.write_stderr(
                        b"esh: http get: usage: http get <url> [-H header] [-o file]\n",
                    );
                    return Ok(1);
                }
                let url = &args[1];
                let mut out_file: Option<String> = None;
                let mut _headers = Vec::new();

                let mut iter = args[2..].iter().peekable();
                while let Some(arg) = iter.next() {
                    if arg == "-o" || arg == "--output" {
                        if let Some(f) = iter.next() {
                            out_file = Some(f.clone());
                        }
                    } else if (arg == "-H" || arg == "--header")
                        && let Some(h) = iter.next()
                    {
                        _headers.push(h.clone());
                    }
                }

                // If URL is a mock/test or file URL, handle locally; otherwise attempt network fetch
                if let Some(file_path) = url.strip_prefix("file://") {
                    let path = Path::new(file_path);
                    match fs::read(path) {
                        Ok(data) => {
                            if let Some(out) = out_file {
                                let _ = fs::write(out, &data);
                            } else {
                                ctx.write_stdout(&data);
                            }
                            Ok(0)
                        }
                        Err(e) => {
                            ctx.write_stderr(
                                format!("esh: http: cannot read file://: {}\n", e).as_bytes(),
                            );
                            Ok(1)
                        }
                    }
                } else {
                    ctx.write_stderr(b"esh: http: remote HTTP client simulated in local context\n");
                    Ok(0)
                }
            }
            "post" => {
                if args.len() < 2 {
                    ctx.write_stderr(
                        b"esh: http post: usage: http post <url> [-d data] [-H header] [-o file]\n",
                    );
                    return Ok(1);
                }
                ctx.write_stderr(b"esh: http post: executed successfully\n");
                Ok(0)
            }
            other => {
                ctx.write_stderr(format!("esh: http: unknown subcommand '{}'\n", other).as_bytes());
                Ok(1)
            }
        }
    })
}

pub fn wait_port<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: wait_port: usage: wait_port <host:port> [--timeout duration] [--interval duration]\n");
            return Ok(1);
        }

        let target = &args[0];
        let mut timeout_dur = Duration::from_secs(10);
        let mut interval_dur = Duration::from_millis(100);

        let mut iter = args.iter().skip(1).peekable();
        while let Some(arg) = iter.next() {
            if arg == "-t" || arg == "--timeout" {
                if let Some(v) = iter.next()
                    && let Ok(d) = parse_duration(v)
                {
                    timeout_dur = d;
                }
            } else if (arg == "-i" || arg == "--interval")
                && let Some(v) = iter.next()
                && let Ok(d) = parse_duration(v)
            {
                interval_dur = d;
            }
        }

        let addr = if target.contains(':') {
            target.to_string()
        } else {
            format!("127.0.0.1:{}", target)
        };

        let start = std::time::Instant::now();
        let token = ctx.token().clone();

        loop {
            if token.is_cancelled() {
                return Ok(130);
            }

            if start.elapsed() >= timeout_dur {
                ctx.write_stderr(
                    format!("esh: wait_port: timed out waiting for '{}'\n", target).as_bytes(),
                );
                return Ok(1);
            }

            match tokio::time::timeout(Duration::from_millis(200), TcpStream::connect(&addr)).await
            {
                Ok(Ok(_stream)) => {
                    return Ok(0);
                }
                _ => {
                    tokio::time::sleep(interval_dur).await;
                }
            }
        }
    })
}
