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

pub fn date<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut is_utc = false;
        let mut explicit_date: Option<&str> = None;
        let mut format_str: Option<&str> = None;

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "-u" || arg == "--utc" || arg == "--universal" {
                is_utc = true;
            } else if arg == "-d" || arg == "--date" {
                if let Some(d) = iter.next() {
                    explicit_date = Some(d.as_str());
                }
            } else if let Some(d) = arg.strip_prefix("--date=") {
                explicit_date = Some(d);
            } else if let Some(f) = arg.strip_prefix('+') {
                format_str = Some(f);
            }
        }

        let dt_utc: DateTime<Utc> = if let Some(date_spec) = explicit_date {
            if let Some(epoch_str) = date_spec.strip_prefix('@') {
                let secs: i64 = epoch_str.parse().unwrap_or(0);
                Utc.timestamp_opt(secs, 0).single().unwrap_or_else(Utc::now)
            } else if let Ok(secs) = date_spec.parse::<i64>() {
                Utc.timestamp_opt(secs, 0).single().unwrap_or_else(Utc::now)
            } else if let Ok(parsed) = DateTime::parse_from_rfc3339(date_spec) {
                parsed.with_timezone(&Utc)
            } else {
                Utc::now()
            }
        } else {
            Utc::now()
        };

        let output = match (is_utc, format_str) {
            (true, Some("%s")) | (false, Some("%s")) => dt_utc.timestamp().to_string(),
            (true, Some(fmt)) => dt_utc.format(fmt).to_string(),
            (false, Some(fmt)) => {
                let local_dt = dt_utc.with_timezone(&Local);
                local_dt.format(fmt).to_string()
            }
            (true, None) => dt_utc.to_rfc3339(),
            (false, None) => {
                let local_dt = dt_utc.with_timezone(&Local);
                local_dt.format("%a %b %e %H:%M:%S %Z %Y").to_string()
            }
        };

        ctx.write_stdout(format!("{}\n", output).as_bytes());
        Ok(0)
    })
}
