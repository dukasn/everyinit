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

use crate::v1::ast::MediaKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartPageModel {
    pub search_prompts: Vec<String>,
}

impl Default for StartPageModel {
    fn default() -> Self {
        Self {
            search_prompts: vec!["Search".to_string()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartPageMedia {
    pub kind: MediaKind,
    pub src: String,
    pub alt: String,
    pub title: Option<String>,
    pub poster: Option<String>,
}
