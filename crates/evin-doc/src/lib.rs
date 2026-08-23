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

pub mod analysis;
pub mod ast;
pub mod lsp;
pub mod parser;
pub mod render;
pub mod site;
pub mod source;
pub mod tape;
pub mod validation;

pub use ast::{
    DefinitionListItem, DocFormat, DocNode, DocPage, DocSiteAst, FieldListItem, FormalReference,
    InlineNode, LspCatalogExport, LspSymbolMeta, MediaKind, NumberingKind, OptionListItem,
    StepItem, TaskListItem, TocEntry,
};
pub use lsp::build_lsp_catalog;
pub use parser::{MarkdownParseResult, MarkdownParser, RstParseResult, RstParser};
pub use render::{
    latex_to_unicode, push_escaped_html, render_ansi, render_markdown, render_plain,
    sanitize_svgbob_code,
};
pub use site::{
    AliasTarget, CompileOptions, DocIssue, DocSeverity, DocumentInput, ReferenceCatalog, SiteIndex,
    compile_site, looks_like_alias, resolve_alias, rewrite_aliases_to_routes,
};
pub use source::{ResolvedDocFile, SourceResolver};
pub use tape::{TapeCommand, TapeConfig, TapeRuntime, TapeScript, TerminalFrame, VirtualScreen};
