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

use tokio::process::Command;

/// Creates a `Command` for `git` with safe defaults to prevent it from
/// spawning a pager or prompting for credentials in non-interactive contexts.
pub fn git_cmd() -> Command {
    let mut cmd = Command::new("git");
    cmd.env("GIT_PAGER", "cat").env("GIT_TERMINAL_PROMPT", "0");
    cmd
}
