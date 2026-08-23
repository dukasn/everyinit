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
use crate::parser::ast::RedirectionOp;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum StreamTarget {
    Inherit,
    Null,
    Stderr,
    Stdout,
    File { path: PathBuf, append: bool },
}

impl StreamTarget {
    pub fn from_redirection(
        op: RedirectionOp,
        raw_path: &str,
        cwd: &Path,
    ) -> Result<Self, ExecError> {
        let trimmed = raw_path.trim();
        if trimmed == "&2"
            || trimmed == "2"
            || (trimmed.is_empty()
                && (op == RedirectionOp::RedirectStderr
                    || op == RedirectionOp::RedirectStderrAppend))
        {
            return Ok(Self::Stderr);
        }
        if trimmed == "&1"
            || trimmed == "1"
            || (trimmed.is_empty()
                && (op == RedirectionOp::RedirectAll || op == RedirectionOp::RedirectAllAppend))
        {
            return Ok(Self::Stdout);
        }
        if trimmed == "/dev/null" || trimmed.eq_ignore_ascii_case("nul") {
            return Ok(Self::Null);
        }

        let p = Path::new(trimmed);
        let path = if p.is_absolute() {
            p.to_path_buf()
        } else {
            cwd.join(p)
        };

        let append = matches!(
            op,
            RedirectionOp::RedirectAppend
                | RedirectionOp::RedirectStderrAppend
                | RedirectionOp::RedirectAllAppend
        );

        Ok(Self::File { path, append })
    }

    pub fn write_data(&self, data: &[u8]) -> Result<(), ExecError> {
        match self {
            StreamTarget::Null | StreamTarget::Inherit => Ok(()),
            StreamTarget::Stderr => {
                let _ = std::io::stderr().write_all(data);
                Ok(())
            }
            StreamTarget::Stdout => {
                let _ = std::io::stdout().write_all(data);
                Ok(())
            }
            StreamTarget::File { path, append } => {
                let mut options = OpenOptions::new();
                options.create(true).write(true);
                if *append {
                    options.append(true);
                } else {
                    options.truncate(true);
                }

                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }

                let mut file = options.open(path).map_err(|e| {
                    ExecError::IoError(format!("Cannot open '{}': {}", path.display(), e))
                })?;
                file.write_all(data).map_err(|e| {
                    ExecError::IoError(format!("Cannot write to '{}': {}", path.display(), e))
                })?;
                Ok(())
            }
        }
    }
}
