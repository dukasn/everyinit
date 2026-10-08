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
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Evin's workspace-aware Language Server Protocol adapter.

mod catalog;
mod dispatcher;
mod handlers;
mod protocol;
mod server;
#[cfg(test)]
mod tests;
mod transport;

#[cfg(test)]
use catalog::CONFIG_CATALOG;
pub use server::EvinLspServer;
pub use transport::run_lsp_server;
