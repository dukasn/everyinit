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

pub fn table<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut delim: Option<&str> = None;
        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "-d" || arg == "--delimiter" {
                if let Some(d) = iter.next() {
                    delim = Some(d.as_str());
                }
            } else if let Some(d) = arg.strip_prefix("-d") {
                delim = Some(d);
            }
        }

        let input = match ctx.take_stdin() {
            Some(buf) => String::from_utf8_lossy(&buf).to_string(),
            None => return Ok(0),
        };

        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut col_widths: Vec<usize> = Vec::new();

        for line in input.lines() {
            let row: Vec<String> = if let Some(d) = delim {
                line.split(d).map(|s| s.trim().to_string()).collect()
            } else {
                line.split_whitespace().map(|s| s.to_string()).collect()
            };

            for (idx, cell) in row.iter().enumerate() {
                if idx >= col_widths.len() {
                    col_widths.push(cell.len());
                } else if cell.len() > col_widths[idx] {
                    col_widths[idx] = cell.len();
                }
            }
            rows.push(row);
        }

        for row in rows {
            let mut formatted_row = Vec::new();
            for (idx, cell) in row.iter().enumerate() {
                let width = col_widths.get(idx).copied().unwrap_or(cell.len());
                formatted_row.push(format!("{:width$}", cell, width = width));
            }
            ctx.write_stdout(format!("{}\n", formatted_row.join("  ")).as_bytes());
        }

        Ok(0)
    })
}

pub fn csv<'a>(
    _args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let input = match ctx.take_stdin() {
            Some(buf) => String::from_utf8_lossy(&buf).to_string(),
            None => return Ok(0),
        };

        for line in input.lines() {
            let fields: Vec<String> = line
                .split_whitespace()
                .map(|s| {
                    if s.contains(',') || s.contains('"') {
                        format!("\"{}\"", s.replace('"', "\"\""))
                    } else {
                        s.to_string()
                    }
                })
                .collect();
            ctx.write_stdout(format!("{}\n", fields.join(",")).as_bytes());
        }

        Ok(0)
    })
}
