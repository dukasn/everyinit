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

pub fn numfmt<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut to_mode: Option<&str> = None;
        let mut from_mode: Option<&str> = None;
        let mut operands = Vec::new();

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if let Some(val) = arg.strip_prefix("--to=") {
                to_mode = Some(val);
            } else if arg == "--to" {
                if let Some(v) = iter.next() {
                    to_mode = Some(v.as_str());
                }
            } else if let Some(val) = arg.strip_prefix("--from=") {
                from_mode = Some(val);
            } else if arg == "--from" {
                if let Some(v) = iter.next() {
                    from_mode = Some(v.as_str());
                }
            } else if !arg.starts_with('-') {
                operands.push(arg.as_str());
            }
        }

        let process_value = |val_str: &str, ctx: &mut ExecutionContext| {
            let trimmed = val_str.trim();
            if let Some(from) = from_mode {
                let bytes = parse_human_bytes(trimmed, from.eq_ignore_ascii_case("iec"));
                ctx.write_stdout(format!("{}\n", bytes).as_bytes());
            } else if let Some(to) = to_mode {
                let raw: u64 = trimmed.parse().unwrap_or(0);
                let formatted = format_human_bytes(raw, to.eq_ignore_ascii_case("iec"));
                ctx.write_stdout(format!("{}\n", formatted).as_bytes());
            } else {
                ctx.write_stdout(format!("{}\n", trimmed).as_bytes());
            }
        };

        if operands.is_empty() {
            if let Some(buf) = ctx.take_stdin() {
                let s = String::from_utf8_lossy(&buf);
                for line in s.lines() {
                    process_value(line, ctx);
                }
            }
            return Ok(0);
        }

        for op in operands {
            process_value(op, ctx);
        }

        Ok(0)
    })
}

fn parse_human_bytes(s: &str, is_iec: bool) -> u64 {
    let s = s.trim().to_uppercase();
    let base: u64 = if is_iec { 1024 } else { 1000 };

    if let Some(num_part) = s.strip_suffix("G") {
        let val: f64 = num_part.trim().parse().unwrap_or(0.0);
        (val * (base.pow(3) as f64)).round() as u64
    } else if let Some(num_part) = s.strip_suffix("GB") {
        let val: f64 = num_part.trim().parse().unwrap_or(0.0);
        (val * (base.pow(3) as f64)).round() as u64
    } else if let Some(num_part) = s.strip_suffix("M") {
        let val: f64 = num_part.trim().parse().unwrap_or(0.0);
        (val * (base.pow(2) as f64)).round() as u64
    } else if let Some(num_part) = s.strip_suffix("MB") {
        let val: f64 = num_part.trim().parse().unwrap_or(0.0);
        (val * (base.pow(2) as f64)).round() as u64
    } else if let Some(num_part) = s.strip_suffix("K") {
        let val: f64 = num_part.trim().parse().unwrap_or(0.0);
        (val * (base as f64)).round() as u64
    } else if let Some(num_part) = s.strip_suffix("KB") {
        let val: f64 = num_part.trim().parse().unwrap_or(0.0);
        (val * (base as f64)).round() as u64
    } else if let Some(num_part) = s.strip_suffix("B") {
        num_part.trim().parse().unwrap_or(0)
    } else {
        s.parse().unwrap_or(0)
    }
}

fn format_human_bytes(bytes: u64, is_iec: bool) -> String {
    let base: f64 = if is_iec { 1024.0 } else { 1000.0 };
    let fbytes = bytes as f64;

    if fbytes >= base * base * base {
        let gb = fbytes / (base * base * base);
        if gb.fract() == 0.0 {
            format!("{:.0}G", gb)
        } else {
            format!("{:.1}G", gb)
        }
    } else if fbytes >= base * base {
        let mb = fbytes / (base * base);
        if mb.fract() == 0.0 {
            format!("{:.0}M", mb)
        } else {
            format!("{:.1}M", mb)
        }
    } else if fbytes >= base {
        let kb = fbytes / base;
        if kb.fract() == 0.0 {
            format!("{:.0}K", kb)
        } else {
            format!("{:.1}K", kb)
        }
    } else {
        format!("{}B", bytes)
    }
}
