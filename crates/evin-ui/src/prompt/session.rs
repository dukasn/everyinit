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

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use crossterm::terminal;
use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct RawModeGuard {
    active: bool,
}

impl RawModeGuard {
    pub fn new() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self { active: true })
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = terminal::disable_raw_mode();
        }
    }
}

pub trait PromptSession: Send {
    fn write_str(&mut self, s: &str) -> io::Result<()>;
    fn flush(&mut self) -> io::Result<()>;
    fn poll_event(&mut self, timeout: Option<Duration>) -> io::Result<Option<Event>>;
    fn is_tty(&self) -> bool;
}

pub struct TtySession {
    _raw_guard: Option<RawModeGuard>,
    writer: Box<dyn Write + Send>,
    is_tty: bool,
}

impl TtySession {
    pub fn try_new() -> io::Result<Self> {
        let tty_path = if cfg!(windows) { "CONOUT$" } else { "/dev/tty" };
        let (writer, is_tty): (Box<dyn Write + Send>, bool) =
            if let Ok(f) = std::fs::OpenOptions::new().write(true).open(tty_path) {
                (Box::new(f), true)
            } else if console::Term::stderr().is_term() {
                (Box::new(io::stderr()), true)
            } else {
                (Box::new(io::sink()), false)
            };

        let raw_guard = if is_tty {
            RawModeGuard::new().ok()
        } else {
            None
        };

        Ok(Self {
            _raw_guard: raw_guard,
            writer,
            is_tty,
        })
    }
}

impl PromptSession for TtySession {
    fn write_str(&mut self, s: &str) -> io::Result<()> {
        self.writer.write_all(s.as_bytes())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    fn poll_event(&mut self, timeout: Option<Duration>) -> io::Result<Option<Event>> {
        if !self.is_tty {
            return Ok(None);
        }

        let poll_dur = timeout.unwrap_or(Duration::from_millis(50));
        if event::poll(poll_dur)? {
            Ok(Some(event::read()?))
        } else if timeout.is_some() {
            Ok(None)
        } else {
            // No timeout, keep polling until an event arrives
            loop {
                if event::poll(Duration::from_millis(100))? {
                    return Ok(Some(event::read()?));
                }
            }
        }
    }

    fn is_tty(&self) -> bool {
        self.is_tty
    }
}

#[derive(Clone, Default)]
pub struct MockPromptSession {
    events: VecDeque<Event>,
    output: Arc<Mutex<Vec<u8>>>,
}

impl MockPromptSession {
    pub fn new(events: Vec<Event>) -> Self {
        Self {
            events: VecDeque::from(events),
            output: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn from_chars(chars: &str) -> Self {
        let mut events = Vec::new();
        for c in chars.chars() {
            if c == '\n' || c == '\r' {
                events.push(Event::Key(KeyEvent {
                    code: KeyCode::Enter,
                    modifiers: KeyModifiers::NONE,
                    kind: KeyEventKind::Press,
                    state: KeyEventState::NONE,
                }));
            } else {
                events.push(Event::Key(KeyEvent {
                    code: KeyCode::Char(c),
                    modifiers: KeyModifiers::NONE,
                    kind: KeyEventKind::Press,
                    state: KeyEventState::NONE,
                }));
            }
        }
        Self::new(events)
    }

    pub fn push_event(&mut self, event: Event) {
        self.events.push_back(event);
    }

    pub fn output_string(&self) -> String {
        String::from_utf8_lossy(&self.output.lock().unwrap()).to_string()
    }
}

impl PromptSession for MockPromptSession {
    fn write_str(&mut self, s: &str) -> io::Result<()> {
        self.output.lock().unwrap().extend_from_slice(s.as_bytes());
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn poll_event(&mut self, _timeout: Option<Duration>) -> io::Result<Option<Event>> {
        Ok(self.events.pop_front())
    }

    fn is_tty(&self) -> bool {
        true
    }
}
