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

use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug)]
pub(super) struct ParsedCommit {
    pub(super) commit_type: String,
    pub(super) scope: Option<String>,
    pub(super) subject: String,
    pub(super) hash: String,
}

#[derive(Debug)]
pub(super) struct ReleaseBlock {
    pub(super) version: String,
    pub(super) commits: Vec<ParsedCommit>,
}

pub(super) fn parse_header(
    subject: &str,
    hash: &str,
    parsed: Option<evin_repository_policy::models::ParsedCommit>,
) -> Option<ParsedCommit> {
    if let Some(p) = parsed {
        return Some(ParsedCommit {
            commit_type: p.commit_type,
            scope: p.scope,
            subject: p.subject,
            hash: hash.to_string(),
        });
    }

    static HEADER_REGEX: OnceLock<Regex> = OnceLock::new();
    let default_regex = HEADER_REGEX.get_or_init(|| {
        Regex::new(
            r"^(?<type>[a-zA-Z]+)(?:\((?<scope>[a-zA-Z0-9-]+)\))?(?<breaking>!)?: (?<subject>.+)$",
        )
        .expect("commit header regex is valid")
    });

    if let Some(captures) = default_regex.captures(subject) {
        let commit_type = captures.name("type")?.as_str().to_string();
        let scope = captures.name("scope").map(|s| s.as_str().to_string());
        let clean_subject = captures.name("subject")?.as_str().to_string();

        Some(ParsedCommit {
            commit_type,
            scope,
            subject: clean_subject,
            hash: hash.to_string(),
        })
    } else {
        None
    }
}
