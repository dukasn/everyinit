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

//! Portable terminal-tape syntax, virtual terminal state, and SVG emission.

pub mod dsl;
pub mod screen;
pub mod svg;

pub use dsl::{TapeCommand, TapeConfig, TapeRuntime, TapeScript, parse_tape_fence_info};
pub use screen::{CellAttributes, TerminalCell, TerminalColor, TerminalFrame, VirtualScreen};
pub use svg::{IBM_PLEX_MONO_METRICS, TerminalMetrics, render_svg};
