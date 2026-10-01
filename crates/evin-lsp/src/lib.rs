// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 of the License.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Internal, language-neutral building blocks for Evin language servers.

pub mod document;
pub mod position;
pub mod protocol;

pub use document::{DocumentStore, LspDocument};
pub use position::SourcePositionIndex;
pub use protocol::{Position, Range};
