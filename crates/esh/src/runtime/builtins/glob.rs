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

pub fn glob<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: glob: usage: glob <pattern...>\n");
            return Ok(1);
        }

        let mut matches = std::collections::BTreeSet::new();
        let cwd = ctx.cwd().to_path_buf();
        let match_options = glob::MatchOptions {
            require_literal_leading_dot: true,
            ..glob::MatchOptions::new()
        };

        for pat in args {
            let recursive_pattern = if pat.ends_with("/**") {
                format!("{}/*", pat)
            } else {
                pat.to_string()
            };
            let full_pattern = if std::path::Path::new(&recursive_pattern).is_absolute() {
                recursive_pattern
            } else {
                cwd.join(&recursive_pattern).to_string_lossy().to_string()
            };

            match glob::glob_with(&full_pattern, match_options) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        // Make path relative to cwd if possible for clean script consumption
                        if let Ok(rel) = entry.strip_prefix(&cwd) {
                            matches.insert(present(rel));
                        } else {
                            matches.insert(present(&entry));
                        }
                    }
                }
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: glob: invalid pattern '{}': {}\n", pat, e).as_bytes(),
                    );
                    return Ok(2);
                }
            }
        }

        if matches.is_empty() {
            return Ok(1);
        }

        for m in &matches {
            ctx.write_stdout(format!("{}\n", m).as_bytes());
        }

        Ok(0)
    })
}
