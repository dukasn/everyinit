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
use chrono::{DateTime, Local, TimeZone, Utc};
use std::fmt::Write as _;

pub fn date<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut is_utc = false;
        let mut explicit_date: Option<&str> = None;
        let mut format_str: Option<&str> = None;

        let mut index = 0;
        while index < args.len() {
            let arg = args[index].as_str();
            if arg == "-u" || arg == "--utc" || arg == "--universal" {
                is_utc = true;
            } else if arg == "-d" || arg == "--date" {
                if explicit_date.is_some() {
                    ctx.write_stderr(b"esh: date: date may be specified only once\n");
                    return Ok(2);
                }
                index += 1;
                let Some(date) = args.get(index) else {
                    ctx.write_stderr(b"esh: date: -d requires a date\n");
                    return Ok(2);
                };
                explicit_date = Some(date);
            } else if let Some(d) = arg.strip_prefix("--date=") {
                if explicit_date.is_some() {
                    ctx.write_stderr(b"esh: date: date may be specified only once\n");
                    return Ok(2);
                }
                explicit_date = Some(d);
            } else if let Some(f) = arg.strip_prefix('+') {
                if format_str.is_some() {
                    ctx.write_stderr(b"esh: date: format may be specified only once\n");
                    return Ok(2);
                }
                format_str = Some(f);
            } else if arg.starts_with('-') {
                ctx.write_stderr(format!("esh: date: unsupported option '{}'\n", arg).as_bytes());
                return Ok(2);
            } else {
                ctx.write_stderr(
                    b"esh: date: setting the system clock is unsupported; use -d to format a timestamp\n",
                );
                return Ok(2);
            }
            index += 1;
        }

        let dt_utc: DateTime<Utc> = if let Some(date_spec) = explicit_date {
            let parsed = date_spec
                .strip_prefix('@')
                .unwrap_or(date_spec)
                .parse::<i64>()
                .ok()
                .and_then(|seconds| Utc.timestamp_opt(seconds, 0).single())
                .or_else(|| {
                    DateTime::parse_from_rfc3339(date_spec)
                        .ok()
                        .map(|date| date.with_timezone(&Utc))
                });
            let Some(parsed) = parsed else {
                ctx.write_stderr(
                    format!(
                        "esh: date: invalid date '{}'; expected epoch seconds or RFC-3339\n",
                        date_spec
                    )
                    .as_bytes(),
                );
                return Ok(2);
            };
            parsed
        } else {
            Utc::now()
        };

        let format = format_str.unwrap_or("%a %b %e %H:%M:%S %Z %Y");
        let output = if format == "%s" {
            dt_utc.timestamp().to_string()
        } else if is_utc {
            match format_date(&dt_utc, format) {
                Ok(output) => output,
                Err(()) => {
                    ctx.write_stderr(b"esh: date: invalid format string\n");
                    return Ok(2);
                }
            }
        } else {
            match format_date(&dt_utc.with_timezone(&Local), format) {
                Ok(output) => output,
                Err(()) => {
                    ctx.write_stderr(b"esh: date: invalid format string\n");
                    return Ok(2);
                }
            }
        };

        ctx.write_stdout(format!("{}\n", output).as_bytes());
        Ok(0)
    })
}

fn format_date<T: TimeZone>(date: &DateTime<T>, format: &str) -> Result<String, ()>
where
    T::Offset: std::fmt::Display,
{
    let format = normalize_posix_modifiers(format);
    let mut output = String::new();
    write!(&mut output, "{}", date.format(&format)).map_err(|_| ())?;
    Ok(output)
}

fn normalize_posix_modifiers(format: &str) -> String {
    let mut chars = format.chars().peekable();
    let mut normalized = String::with_capacity(format.len());

    while let Some(ch) = chars.next() {
        if ch != '%' {
            normalized.push(ch);
            continue;
        }

        normalized.push('%');
        let Some(next) = chars.next() else {
            break;
        };
        if next != 'E' && next != 'O' {
            normalized.push(next);
            continue;
        }

        let Some(conversion) = chars.next() else {
            normalized.push(next);
            break;
        };
        let supported = match next {
            'E' => matches!(conversion, 'c' | 'C' | 'x' | 'X' | 'y' | 'Y'),
            'O' => matches!(
                conversion,
                'd' | 'e' | 'H' | 'I' | 'm' | 'M' | 'S' | 'u' | 'U' | 'V' | 'w' | 'W' | 'y'
            ),
            _ => false,
        };
        if supported {
            normalized.push(conversion);
        } else {
            normalized.push(next);
            normalized.push(conversion);
        }
    }

    normalized
}
