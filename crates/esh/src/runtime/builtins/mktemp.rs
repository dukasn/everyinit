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
use tempfile::Builder;

pub fn mktemp<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut is_dir = false;
        let mut prefix = "esh_tmp_".to_string();
        let mut _keep = false;

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "-d" || arg == "--directory" {
                is_dir = true;
            } else if arg == "--keep" || arg == "--no-clean" {
                _keep = true;
            } else if arg == "-p" || arg == "--tmpdir" {
                if let Some(p) = iter.next() {
                    prefix = p.clone();
                }
            } else if !arg.starts_with('-') {
                prefix = arg.clone();
            }
        }

        if is_dir {
            match Builder::new().prefix(&prefix).tempdir() {
                Ok(dir) => {
                    let path = dir.keep();
                    ctx.write_stdout(format!("{}\n", path.to_string_lossy()).as_bytes());
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
            match Builder::new().prefix(&prefix).tempfile() {
                Ok(file) => {
                    let path = file
                        .into_temp_path()
                        .keep()
                        .map_err(|e| ExecError::IoError(e.error.to_string()))?;
                    ctx.write_stdout(format!("{}\n", path.to_string_lossy()).as_bytes());
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
