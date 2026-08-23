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

use super::{PromptResult, PromptSession, TtySession, format_prompt_success};
use crate::render::{SYMBOL_FRAME_BAR, SYMBOL_POINTER, SYMBOL_PROMPT, Theme};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use std::io;
use std::time::Duration;

pub struct ConfirmPrompt<'a> {
    prompt: &'a str,
    default: Option<bool>,
    timeout: Option<Duration>,
}

pub fn confirm(prompt: &str) -> ConfirmPrompt<'_> {
    ConfirmPrompt {
        prompt,
        default: None,
        timeout: None,
    }
}

impl<'a> ConfirmPrompt<'a> {
    pub fn default_confirm(mut self, default: bool) -> Self {
        self.default = Some(default);
        self
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    pub fn interact(self) -> io::Result<PromptResult<bool>> {
        let mut session = TtySession::try_new()?;
        self.interact_with(&mut session)
    }

    pub fn interact_with<S: ?Sized + PromptSession>(
        self,
        session: &mut S,
    ) -> io::Result<PromptResult<bool>> {
        let theme = Theme::default();
        let frame = theme
            .frame_color
            .apply_to(format!("{}  {}", SYMBOL_FRAME_BAR, SYMBOL_PROMPT))
            .to_string();

        let hint = match self.default {
            Some(true) => theme.frame_color.apply_to("[Y/n]").to_string(),
            Some(false) => theme.frame_color.apply_to("[y/N]").to_string(),
            None => theme.frame_color.apply_to("[y/n]").to_string(),
        };
        let pointer = theme.frame_color.apply_to(SYMBOL_POINTER).to_string();

        let header = format!("{} {} {} {} ", frame, self.prompt, hint, pointer);
        session.write_str("\x1B[2K\r")?;
        session.write_str(&header)?;
        session.flush()?;

        let final_val = loop {
            match session.poll_event(self.timeout)? {
                Some(Event::Key(key)) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        session.write_str("\n\r")?;
                        session.flush()?;
                        return Ok(PromptResult::Cancelled);
                    }
                    KeyCode::Esc => {
                        session.write_str("\n\r")?;
                        session.flush()?;
                        return Ok(PromptResult::Cancelled);
                    }
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        break true;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') => {
                        break false;
                    }
                    KeyCode::Enter => {
                        if let Some(def) = self.default {
                            break def;
                        }
                    }
                    _ => {}
                },
                None if self.timeout.is_some() => {
                    session.write_str("\n\r")?;
                    session.flush()?;
                    return Ok(PromptResult::TimedOut);
                }
                _ => {}
            }
        };

        session.write_str("\x1B[2K\r")?;
        let res_str = if final_val { "yes" } else { "no" };
        let success = format_prompt_success(&theme, self.prompt, res_str);
        session.write_str(&success)?;
        session.flush()?;

        Ok(PromptResult::Selected(final_val))
    }
}
