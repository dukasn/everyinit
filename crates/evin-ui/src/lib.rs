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

pub mod diagnostics;
pub mod pipeline;
pub mod prompt;
pub mod render;

// Re-exports for stable external API paths
pub mod backend {
    pub use crate::render::{OutputBackend, StderrBackend};
}

pub mod event {
    pub use crate::pipeline::event::UIEvent;
}

pub mod mode {
    pub use crate::render::Mode;
}

pub mod layout {
    pub use crate::render::layout::Layout;
}

pub mod theme {
    pub use crate::render::Theme;
}

pub mod log {
    pub use crate::render::logger::Logger;
}

pub mod spinner {
    pub use crate::render::spinner::Spinner;
}

pub mod diagnostic {
    pub use crate::diagnostics::snippet;
    pub use crate::diagnostics::{error, print_diagnostic, render_diagnostic};
}

pub mod utils {
    pub use crate::render::{
        dim, format_duration, format_module_paths, highlight, suggestion, summary, summary_text,
        wrap_text,
    };
}

pub mod constants {
    pub use crate::render::*;
}
