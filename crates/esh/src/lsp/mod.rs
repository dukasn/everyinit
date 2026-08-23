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

pub mod hover;
pub mod protocol;
pub mod semantic_tokens;
pub mod server;

pub use hover::get_hover_documentation;
pub use protocol::{
    CompletionItem, Diagnostic, DocumentSymbol, Hover, JsonRpcNotification, JsonRpcRequest,
    JsonRpcResponse, Position, PublishDiagnosticsParams, Range, SemanticTokens, read_message,
    write_message,
};
pub use semantic_tokens::{TOKEN_TYPES, compute_semantic_tokens};
pub use server::{LspServer, run_lsp_server};
