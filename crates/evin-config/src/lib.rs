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

pub mod analysis;
pub mod environment;
pub mod models;
pub mod parser;
pub mod semantic;
pub mod source;
pub mod workspace;

pub use parser::EverInitConfig;
pub use parser::load_workspace_tasks;
pub use semantic::{WorkspaceSemanticAnalysis, WorkspaceSemanticIndex, WorkspaceSnapshot};
