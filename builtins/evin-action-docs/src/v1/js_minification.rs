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

/// Lexical tokens for JavaScript compression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsToken {
    /// Identifier or keyword (e.g. `const`, `return`, `window`, `function`)
    Identifier(String),
    /// Number literal (e.g. `10`, `0.5`, `0xff`)
    Number(String),
    /// String literal (including quotes, e.g. `"hello"`, `'world'`)
    StringLiteral(String),
    /// Template literal (e.g. `` `hello ${name}` ``)
    TemplateLiteral(String),
    /// Regular expression literal (e.g. `/[a-z]+/gi`)
    RegExpLiteral(String),
    /// Punctuator or operator (e.g. `=`, `+`, `(`, `)`, `{`, `}`, `;`)
    Punctuator(String),
}

/// Advanced streaming JavaScript minifier.
/// Strips comments, compacts punctuators and operators, eliminates redundant
/// semicolons, and preserves string and RegExp literals.
pub fn minify_js(input: &str) -> String {
    let tokens = tokenize_js(input);
    emit_minified_js(&tokens)
}

fn is_identifier_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_identifier_part(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

fn tokenize_js(input: &str) -> Vec<JsToken> {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut tokens = Vec::new();

    while i < len {
        let ch = chars[i];

        // 1. Whitespace
        if ch.is_whitespace() {
            i += 1;
            continue;
        }

        // 2. Comments
        if ch == '/' && i + 1 < len {
            if chars[i + 1] == '/' {
                i += 2;
                while i < len && chars[i] != '\n' && chars[i] != '\r' {
                    i += 1;
                }
                continue;
            }
            if chars[i + 1] == '*' {
                i += 2;
                while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i = (i + 2).min(len);
                continue;
            }
        }

        // 3. String literals (single or double quote)
        if ch == '\'' || ch == '"' {
            let quote = ch;
            let mut s = String::new();
            s.push(quote);
            i += 1;
            while i < len {
                let c = chars[i];
                s.push(c);
                if c == '\\' && i + 1 < len {
                    s.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if c == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            tokens.push(JsToken::StringLiteral(s));
            continue;
        }

        // 4. Template literals (`...`)
        if ch == '`' {
            let mut s = String::new();
            s.push('`');
            i += 1;
            let mut depth = 0;
            while i < len {
                let c = chars[i];
                s.push(c);
                if c == '\\' && i + 1 < len {
                    s.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if c == '$' && i + 1 < len && chars[i + 1] == '{' {
                    s.push('{');
                    depth += 1;
                    i += 2;
                    continue;
                }
                if c == '}' && depth > 0 {
                    depth -= 1;
                    i += 1;
                    continue;
                }
                if c == '`' && depth == 0 {
                    i += 1;
                    break;
                }
                i += 1;
            }
            tokens.push(JsToken::TemplateLiteral(s));
            continue;
        }

        // 5. RegExp Literal vs Division Slash Disambiguation
        if ch == '/' {
            let is_regex = match tokens.last() {
                None => true,
                Some(JsToken::Punctuator(p)) => !matches!(p.as_str(), ")" | "]"),
                Some(JsToken::Identifier(id)) => matches!(
                    id.as_str(),
                    "return"
                        | "typeof"
                        | "instanceof"
                        | "yield"
                        | "await"
                        | "throw"
                        | "delete"
                        | "void"
                        | "case"
                        | "in"
                        | "of"
                ),
                _ => false,
            };

            if is_regex {
                let mut reg = String::new();
                reg.push('/');
                i += 1;
                let mut in_char_class = false;
                while i < len {
                    let c = chars[i];
                    reg.push(c);
                    if c == '\\' && i + 1 < len {
                        reg.push(chars[i + 1]);
                        i += 2;
                        continue;
                    }
                    if c == '[' {
                        in_char_class = true;
                    } else if c == ']' {
                        in_char_class = false;
                    } else if c == '/' && !in_char_class {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                // Collect regex flags: g, i, m, s, u, y, d
                while i < len && is_identifier_part(chars[i]) {
                    reg.push(chars[i]);
                    i += 1;
                }
                tokens.push(JsToken::RegExpLiteral(reg));
                continue;
            }
        }

        // 6. Number literal
        if ch.is_ascii_digit() || (ch == '.' && i + 1 < len && chars[i + 1].is_ascii_digit()) {
            let mut num = String::new();
            while i < len
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '.' || chars[i] == '_')
            {
                num.push(chars[i]);
                i += 1;
            }
            tokens.push(JsToken::Number(num));
            continue;
        }

        // 7. Identifier or Keyword
        if is_identifier_start(ch) {
            let mut id = String::new();
            while i < len && is_identifier_part(chars[i]) {
                id.push(chars[i]);
                i += 1;
            }
            tokens.push(JsToken::Identifier(id));
            continue;
        }

        // 8. Multi-char and Single-char Punctuators
        let remaining = &chars[i..];
        if remaining.len() >= 3 {
            let tri: String = remaining[..3].iter().collect();
            if matches!(
                tri.as_str(),
                "===" | "!==" | ">>>" | "**=" | "&&=" | "||=" | "??=" | "..." | "<<=" | ">>="
            ) {
                tokens.push(JsToken::Punctuator(tri));
                i += 3;
                continue;
            }
        }
        if remaining.len() >= 2 {
            let pair: String = remaining[..2].iter().collect();
            if matches!(
                pair.as_str(),
                "==" | "!="
                    | "<="
                    | ">="
                    | "=>"
                    | "&&"
                    | "||"
                    | "??"
                    | "?."
                    | "++"
                    | "--"
                    | "+="
                    | "-="
                    | "*="
                    | "/="
                    | "%="
                    | "&="
                    | "|="
                    | "^="
                    | "<<"
                    | ">>"
                    | "**"
            ) {
                tokens.push(JsToken::Punctuator(pair));
                i += 2;
                continue;
            }
        }

        tokens.push(JsToken::Punctuator(ch.to_string()));
        i += 1;
    }

    tokens
}

fn emit_minified_js(tokens: &[JsToken]) -> String {
    let mut out = String::new();

    for (idx, token) in tokens.iter().enumerate() {
        let prev = if idx > 0 {
            Some(&tokens[idx - 1])
        } else {
            None
        };

        // Determine if space is required before current token
        if let Some(prev_tok) = prev {
            match (prev_tok, token) {
                (JsToken::Identifier(_), JsToken::Identifier(_))
                | (JsToken::Identifier(_), JsToken::Number(_))
                | (JsToken::Number(_), JsToken::Identifier(_)) => {
                    out.push(' ');
                }
                (JsToken::Punctuator(p1), JsToken::Punctuator(p2)) => {
                    if (p1.ends_with('+') && p2.starts_with('+'))
                        || (p1.ends_with('-') && p2.starts_with('-'))
                    {
                        out.push(' ');
                    }
                }
                (JsToken::Identifier(id), JsToken::RegExpLiteral(_)) => {
                    if matches!(
                        id.as_str(),
                        "return"
                            | "typeof"
                            | "instanceof"
                            | "yield"
                            | "await"
                            | "throw"
                            | "delete"
                            | "void"
                            | "case"
                            | "in"
                            | "of"
                    ) {
                        out.push(' ');
                    }
                }
                _ => {}
            }
        }

        // Optimize redundant semicolon before closing brace
        if let JsToken::Punctuator(p) = token {
            if p == "}" && out.ends_with(';') {
                out.pop();
            }
            if p == ";" && out.ends_with(';') {
                continue;
            }
        }

        match token {
            JsToken::Identifier(s) => out.push_str(s),
            JsToken::Number(s) => out.push_str(s),
            JsToken::StringLiteral(s) => out.push_str(s),
            JsToken::TemplateLiteral(s) => out.push_str(s),
            JsToken::RegExpLiteral(s) => out.push_str(s),
            JsToken::Punctuator(s) => out.push_str(s),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minify_js_comments_and_whitespace() {
        let input = r#"
            // Top comment
            const x = 10; /* Inline comment */
            function test(a, b) {
                // Return result
                return a + b;
            }
        "#;
        let minified = minify_js(input);
        assert!(!minified.contains("// Top comment"));
        assert!(!minified.contains("Inline comment"));
        assert_eq!(minified, "const x=10;function test(a,b){return a+b}");
    }

    #[test]
    fn test_minify_js_preserves_strings_and_urls() {
        let input = r#"
            const url = "https://example.com/api";
            const msg = 'Hello // world /* test */';
            const tmpl = `Template ${url}`;
        "#;
        let minified = minify_js(input);
        assert!(minified.contains("https://example.com/api"));
        assert!(minified.contains("Hello // world /* test */"));
        assert!(minified.contains("Template ${url}"));
    }

    #[test]
    fn test_minify_js_regex_and_signs() {
        let input = r#"
            const pattern = /[.*+?^${}()|[\]\\]/g;
            const unary = a + +b;
            const sub = c - -d;
            if (pattern.test(str)) { return true; }
        "#;
        let minified = minify_js(input);
        assert!(minified.contains(r"/[.*+?^${}()|[\]\\]/g"));
        assert!(minified.contains("a+ +b"));
        assert!(minified.contains("c- -d"));
        assert!(minified.contains("return true"));
    }
}
