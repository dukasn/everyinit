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
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
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
        if raw_path == "/dev/null" || raw_path.eq_ignore_ascii_case("nul") {
            return Ok(Self::Null);
        }

        let p = Path::new(raw_path);
        let path = if raw_path.is_empty() {
            // Joining an empty relative path to the working directory would
            // turn an empty filename into the directory itself. Keep the
            // empty path so the OS reports it as a missing target.
            PathBuf::new()
        } else if p.is_absolute() {
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
                options.write(true);
                if *append {
                    options.append(true);
                }

                let mut file = options.open(path).map_err(|e| {
                    ExecError::IoError(format!("Cannot open '{}': {}", path.display(), e))
                })?;
                if !append {
                    file.seek(SeekFrom::Start(0)).map_err(|e| {
                        ExecError::IoError(format!("Cannot seek '{}': {}", path.display(), e))
                    })?;
                }
                file.write_all(data).map_err(|e| {
                    ExecError::IoError(format!("Cannot write to '{}': {}", path.display(), e))
                })?;
                Ok(())
            }
        }
    }

    pub fn prepare_for_write(&self, noclobber: bool) -> Result<(), ExecError> {
        let Self::File { path, append } = self else {
            return Ok(());
        };

        let mut options = OpenOptions::new();
        options.write(true);
        if *append {
            options.create(true).append(true);
        } else if noclobber {
            options.create_new(true);
        } else {
            options.create(true).truncate(true);
        }
        options.open(path).map_err(|e| {
            let reason = if noclobber {
                "Cannot overwrite existing target"
            } else {
                "Cannot open target"
            };
            ExecError::IoError(format!("{} '{}': {}", reason, path.display(), e))
        })?;
        Ok(())
    }
}
