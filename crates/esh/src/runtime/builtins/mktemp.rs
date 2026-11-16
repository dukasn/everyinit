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

use std::path::{Path, PathBuf};

use crate::error::ExecError;
use crate::runtime::context::ExecutionContext;
use crate::runtime::path_presentation::present;
use crate::runtime::traits::BoxFuture;
use tempfile::Builder;

pub fn mktemp<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut is_dir = false;
        let mut directory: Option<PathBuf> = None;
        let mut prefix = "esh_tmp_".to_string();
        let mut suffix = String::new();
        let mut _keep = false;

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "-d" || arg == "--directory" {
                is_dir = true;
            } else if arg == "--keep" || arg == "--no-clean" {
                _keep = true;
            } else if arg == "-p" || arg == "--tmpdir" {
                if let Some(p) = iter.next() {
                    directory = Some(PathBuf::from(p));
                }
            } else if !arg.starts_with('-') {
                let template = Path::new(arg);
                if directory.is_none()
                    && let Some(parent) = template
                        .parent()
                        .filter(|path| !path.as_os_str().is_empty())
                {
                    directory = Some(parent.to_path_buf());
                }

                let name = template
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(arg);
                if let Some(first_x) = name.find('X') {
                    let after_x = &name[first_x..];
                    let run_length = after_x
                        .chars()
                        .take_while(|character| *character == 'X')
                        .count();
                    if run_length >= 3 {
                        prefix = name[..first_x].to_string();
                        suffix = name[first_x + run_length..].to_string();
                    } else {
                        prefix = name.to_string();
                    }
                } else {
                    prefix = name.to_string();
                }
            }
        }

        let mut builder = Builder::new();
        builder.prefix(&prefix).suffix(&suffix);
        if is_dir {
            let result = match directory.as_deref() {
                Some(directory) => builder.tempdir_in(directory),
                None => builder.tempdir(),
            };
            match result {
                Ok(dir) => {
                    let path = dir.keep();
                    ctx.write_stdout(format!("{}\n", present(&path)).as_bytes());
                    Ok(0)
                }
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: mktemp: cannot create directory: {}\n", e).as_bytes(),
                    );
                    Ok(1)
                }
            }
        } else {
            let result = match directory.as_deref() {
                Some(directory) => builder.tempfile_in(directory),
                None => builder.tempfile(),
            };
            match result {
                Ok(file) => {
                    let path = file
                        .into_temp_path()
                        .keep()
                        .map_err(|e| ExecError::IoError(e.error.to_string()))?;
                    ctx.write_stdout(format!("{}\n", present(&path)).as_bytes());
                    Ok(0)
                }
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: mktemp: cannot create temporary file: {}\n", e).as_bytes(),
                    );
                    Ok(1)
                }
            }
        }
    })
}
