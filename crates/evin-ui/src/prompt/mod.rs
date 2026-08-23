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

use crate::render::{
    SYMBOL_FRAME_BAR, SYMBOL_POINTER, SYMBOL_PROMPT, SYMBOL_REMARK, SYMBOL_SUCCESS, Theme,
};

pub mod confirm;
pub mod input;
pub mod multiselect;
pub mod select;
pub mod session;

pub use confirm::{ConfirmPrompt, confirm};
pub use input::{InputPrompt, SecretPrompt, input, secret};
pub use multiselect::{MultiSelectPrompt, multiselect};
pub use select::{SelectPrompt, select};
pub use session::{MockPromptSession, PromptSession, RawModeGuard, TtySession};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptResult<T> {
    Selected(T),
    Cancelled,
    TimedOut,
    Error(String),
}

impl<T> PromptResult<T> {
    pub fn is_cancelled(&self) -> bool {
        matches!(self, PromptResult::Cancelled)
    }

    pub fn is_timed_out(&self) -> bool {
        matches!(self, PromptResult::TimedOut)
    }

    pub fn ok(self) -> Option<T> {
        match self {
            PromptResult::Selected(v) => Some(v),
            _ => None,
        }
    }
}

pub(crate) fn format_prompt_header(theme: &Theme, prompt: &str, default: Option<&str>) -> String {
    let frame = theme
        .frame_color
        .apply_to(format!("{}  {}", SYMBOL_FRAME_BAR, SYMBOL_PROMPT))
        .to_string();
    let mut s = format!("{} {} ", frame, prompt);

    if let Some(d) = default {
        let def_str = theme
            .frame_color
            .apply_to(format!("(default: {})", d))
            .to_string();
        s.push_str(&format!("{} ", def_str));
    }

    let pointer = theme.frame_color.apply_to(SYMBOL_POINTER).to_string();
    s.push_str(&format!("{} ", pointer));
    s
}

pub(crate) fn format_prompt_success(theme: &Theme, prompt: &str, result: &str) -> String {
    let frame = theme
        .frame_color
        .apply_to(format!("{}  ", SYMBOL_FRAME_BAR))
        .to_string();
    let check = theme.success_color.apply_to(SYMBOL_SUCCESS).to_string();
    let dot = theme.frame_color.apply_to(SYMBOL_REMARK).to_string();
    let res = theme.success_color.apply_to(result).to_string();

    format!("{}{} {} {} {}\n\r", frame, check, prompt, dot, res)
}
