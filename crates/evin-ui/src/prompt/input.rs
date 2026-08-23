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

use super::{PromptResult, PromptSession, TtySession, format_prompt_header, format_prompt_success};
use crate::render::Theme;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use std::io;
use std::time::Duration;

pub struct InputPrompt<'a> {
    prompt: &'a str,
    default: Option<&'a str>,
    timeout: Option<Duration>,
}

pub fn input(prompt: &str) -> InputPrompt<'_> {
    InputPrompt {
        prompt,
        default: None,
        timeout: None,
    }
}

impl<'a> InputPrompt<'a> {
    pub fn default_input(mut self, default: &'a str) -> Self {
        self.default = Some(default);
        self
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    pub fn interact(self) -> io::Result<PromptResult<String>> {
        let mut session = TtySession::try_new()?;
        self.interact_with(&mut session)
    }

    pub fn interact_with<S: ?Sized + PromptSession>(
        self,
        session: &mut S,
    ) -> io::Result<PromptResult<String>> {
        let theme = Theme::default();
        let mut input_buf = String::new();

        loop {
            let default_str = if input_buf.is_empty() {
                self.default
            } else {
                None
            };
            let header = format_prompt_header(&theme, self.prompt, default_str);
            // Clear current line and write header + input buffer
            session.write_str("\x1B[2K\r")?;
            session.write_str(&header)?;
            session.write_str(&input_buf)?;
            session.flush()?;

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
                    KeyCode::Char(c) => {
                        input_buf.push(c);
                    }
                    KeyCode::Backspace => {
                        input_buf.pop();
                    }
                    KeyCode::Enter => {
                        break;
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
        }

        let final_val = if input_buf.is_empty() {
            self.default.unwrap_or("").to_string()
        } else {
            input_buf
        };

        session.write_str("\x1B[2K\r")?;
        let success = format_prompt_success(&theme, self.prompt, &final_val);
        session.write_str(&success)?;
        session.flush()?;

        Ok(PromptResult::Selected(final_val))
    }
}

pub struct SecretPrompt<'a> {
    prompt: &'a str,
    mask: Option<char>,
    timeout: Option<Duration>,
}

pub fn secret(prompt: &str) -> SecretPrompt<'_> {
    SecretPrompt {
        prompt,
        mask: None,
        timeout: None,
    }
}

impl<'a> SecretPrompt<'a> {
    pub fn mask(mut self, mask: char) -> Self {
        self.mask = Some(mask);
        self
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    pub fn interact(self) -> io::Result<PromptResult<String>> {
        let mut session = TtySession::try_new()?;
        self.interact_with(&mut session)
    }

    pub fn interact_with<S: ?Sized + PromptSession>(
        self,
        session: &mut S,
    ) -> io::Result<PromptResult<String>> {
        let theme = Theme::default();
        let mut input_buf = String::new();

        loop {
            let header = format_prompt_header(&theme, self.prompt, None);
            session.write_str("\x1B[2K\r")?;
            session.write_str(&header)?;
            if let Some(m) = self.mask {
                let mask_str: String = std::iter::repeat_n(m, input_buf.len()).collect();
                session.write_str(&mask_str)?;
            }
            session.flush()?;

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
                    KeyCode::Char(c) => {
                        input_buf.push(c);
                    }
                    KeyCode::Backspace => {
                        input_buf.pop();
                    }
                    KeyCode::Enter => {
                        break;
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
        }

        let display_val = if let Some(m) = self.mask {
            std::iter::repeat_n(m, input_buf.len()).collect::<String>()
        } else {
            "******".to_string()
        };

        session.write_str("\x1B[2K\r")?;
        let success = format_prompt_success(&theme, self.prompt, &display_val);
        session.write_str(&success)?;
        session.flush()?;

        Ok(PromptResult::Selected(input_buf))
    }
}
