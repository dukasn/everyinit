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

pub mod archive;
pub mod crypto;
pub mod data;
pub mod date_tool;
pub mod diff_tool;
pub mod duration_tool;
pub mod flow;
pub mod fs;
pub mod getopts;
pub mod glob;
pub mod http;
pub mod mktemp;
pub mod numfmt_tool;
pub mod path;
pub mod semver_tool;
pub mod str;
pub mod sys;
pub mod table_tool;
pub mod test_tool;
pub mod text;

pub use archive::{archive, tar, unarchive, unzip, zip};
pub use crypto::{base64, hash, md5sum, sha256sum};
pub use data::{json, yaml};
pub use date_tool::date;
pub use diff_tool::diff;
pub use duration_tool::duration;
pub use flow::{
    assert, r#break, r#continue, declare, echo, exit, export, local, printf, read, retry, r#return,
    set, shift, sleep, time, timeout, trap, wait,
};
pub use fs::{cat, cd, cp, head, mkdir, mv, pwd, rm, stat, tail, touch};
pub use getopts::getopts;
pub use glob::glob;
pub use http::{http, wait_port};
pub use mktemp::mktemp;
pub use numfmt_tool::numfmt;
pub use path::{basename, dirname, path, realpath};
pub use semver_tool::semver;
pub use str::str;
pub use sys::{env, os, uname, which};
pub use table_tool::{csv, table};
pub use test_tool::test;
pub use text::{cut, grep, sort, tr, uniq, wc};
