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

pub fn duration<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut _is_ms = false;
        let mut _is_sec = false;
        let mut _human = true;
        let mut raw_val: Option<&str> = None;

        for arg in args {
            if arg == "-h" || arg == "--human" {
                _human = true;
            } else if arg == "--ms" {
                _is_ms = true;
            } else if arg == "--seconds" || arg == "-s" {
                _is_sec = true;
            } else if !arg.starts_with('-') {
                raw_val = Some(arg.as_str());
            }
        }

        let input_str = if let Some(v) = raw_val {
            v.to_string()
        } else if let Some(buf) = ctx.take_stdin() {
            String::from_utf8_lossy(&buf).trim().to_string()
        } else {
            ctx.write_stderr(b"esh: duration: missing value operand\n");
            return Ok(1);
        };

        let secs: f64 = if input_str.ends_with("ms") {
            let ms: f64 = input_str
                .trim_end_matches("ms")
                .trim()
                .parse()
                .unwrap_or(0.0);
            ms / 1000.0
        } else if input_str.ends_with('s') {
            input_str
                .trim_end_matches('s')
                .trim()
                .parse()
                .unwrap_or(0.0)
        } else if input_str.ends_with('m') {
            let mins: f64 = input_str
                .trim_end_matches('m')
                .trim()
                .parse()
                .unwrap_or(0.0);
            mins * 60.0
        } else {
            input_str.parse().unwrap_or(0.0)
        };

        let formatted = format_human_duration(secs);
        ctx.write_stdout(format!("{}\n", formatted).as_bytes());
        Ok(0)
    })
}

fn format_human_duration(total_secs: f64) -> String {
    if (0.0..0.001).contains(&total_secs) {
        return "0s".to_string();
    }
    if total_secs < 1.0 {
        let ms = (total_secs * 1000.0).round() as u64;
        return format!("{}ms", ms);
    }

    let whole_secs = total_secs.floor() as u64;
    let hours = whole_secs / 3600;
    let mins = (whole_secs % 3600) / 60;
    let secs = whole_secs % 60;

    if hours > 0 {
        if mins > 0 && secs > 0 {
            format!("{}h {}m {}s", hours, mins, secs)
        } else if mins > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{}h", hours)
        }
    } else if mins > 0 {
        if secs > 0 {
            format!("{}m {}s", mins, secs)
        } else {
            format!("{}m", mins)
        }
    } else {
        format!("{}s", whole_secs)
    }
}
