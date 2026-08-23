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

pub mod ansi;
pub mod html;
pub mod markdown;
pub mod plain;
pub mod svgbob_preprocessor;
pub mod unicode_math;

pub use ansi::render_ansi;
pub use html::push_escaped_html;
pub use markdown::render_markdown;
pub use plain::render_plain;
pub use svgbob_preprocessor::sanitize_svgbob_code;
pub use unicode_math::latex_to_unicode;
