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

pub mod cache;
pub mod context;
pub mod dry_run;
pub mod error;
pub mod locales;
pub mod path;
pub mod source;

pub use cache::SourceCache;
pub use context::{IoContext, IoMode};
pub use dry_run::DryRunState;
pub use error::IoError;
pub use locales::{FtlResource, load_ftl_resources};
pub use path::clean_path;
pub use source::SourceFile;
