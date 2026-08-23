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

use super::parser::{TemplateParser, Token};
use std::collections::HashMap;

/// Renders a basic string template containing `{key}` variables by replacing them
/// with the provided values.
///
/// Missing variables are left unreplaced.
pub fn render(template: &str, values: &HashMap<&str, &str>) -> String {
    let parser = TemplateParser::new("{", "}", "^\\{\\}");
    let tokens = parser.parse(template);
    let mut result = String::new();

    for token in tokens {
        match token {
            Token::Literal(lit) => result.push_str(&lit),
            Token::Variable(var) => {
                if let Some(val) = values.get(var.as_str()) {
                    result.push_str(val);
                } else {
                    result.push_str(&format!("{{{}}}", var));
                }
            }
        }
    }
    result
}
