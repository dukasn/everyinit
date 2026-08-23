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

use std::sync::OnceLock;

/// Projects a supported LaTeX math subset to readable UTF-8 Unicode characters.
///
/// Designed for terminal CLI output (`esh help <symbol>`) and plain text LSP hover representations.
/// Unsupported or complex constructs are preserved verbatim.
pub fn latex_to_unicode(latex: &str) -> String {
    let mut s = latex.trim().to_string();

    // 1. Remove text/font wrappers with nested-brace support
    s = strip_font_wrappers(&s);

    // Environments & Delimiters
    s = s
        .replace("\\begin{cases}", "{ ")
        .replace("\\end{cases}", " }")
        .replace("\\begin{matrix}", "")
        .replace("\\end{matrix}", "")
        .replace("\\right.", "");

    // 2. Fractions with balanced-brace support
    s = convert_generic_fractions(&s);

    // 3. Multi-character Token Replacements (Ordered strictly by length descending to prevent prefix collisions)
    static TOKENS: OnceLock<Vec<(&str, &str)>> = OnceLock::new();
    let tokens = TOKENS.get_or_init(|| {
        let mut tokens: Vec<(&str, &str)> = vec![
            // Delimiters
            ("\\left(", "("),
            ("\\right)", ")"),
            ("\\left[", "["),
            ("\\right]", "]"),
            ("\\left\\{", "{"),
            ("\\right\\}", "}"),
            ("\\left|", "|"),
            ("\\right|", "|"),
            // Operators & Relations (Longest prefixes first)
            ("\\subseteq", "⊆"),
            ("\\subset", "⊂"),
            ("\\notin", "∉"),
            ("\\in", "∈"),
            ("\\oiint", "∯"),
            ("\\iiint", "∭"),
            ("\\iint", "∬"),
            ("\\oint", "∮"),
            ("\\int", "∫"),
            ("\\sum", "∑"),
            ("\\prod", "∏"),
            ("\\sqrt", "√"),
            ("\\approx", "≈"),
            ("\\equiv", "≡"),
            ("\\leq", "≤"),
            ("\\le", "≤"),
            ("\\geq", "≥"),
            ("\\ge", "≥"),
            ("\\neq", "≠"),
            ("\\ne", "≠"),
            ("\\times", "×"),
            ("\\cdot", "·"),
            ("\\rightarrow", "→"),
            ("\\leftarrow", "←"),
            ("\\to", "→"),
            ("\\infty", "∞"),
            ("\\pm", "±"),
            ("\\mp", "∓"),
            ("\\forall", "∀"),
            ("\\exists", "∃"),
            ("\\nabla", "∇"),
            ("\\partial", "∂"),
            ("\\cup", "∪"),
            ("\\cap", "∩"),
            ("\\circ", "∘"),
            ("\\bullet", "•"),
            // Greek letters (variants and longest first)
            ("\\varepsilon", "ε"),
            ("\\vartheta", "ϑ"),
            ("\\varrho", "ϱ"),
            ("\\varsigma", "ς"),
            ("\\varphi", "φ"),
            ("\\varpi", "ϖ"),
            ("\\alpha", "α"),
            ("\\beta", "β"),
            ("\\gamma", "γ"),
            ("\\delta", "δ"),
            ("\\epsilon", "ϵ"),
            ("\\zeta", "ζ"),
            ("\\eta", "η"),
            ("\\theta", "θ"),
            ("\\iota", "ι"),
            ("\\kappa", "κ"),
            ("\\lambda", "λ"),
            ("\\mu", "μ"),
            ("\\nu", "ν"),
            ("\\xi", "ξ"),
            ("\\pi", "π"),
            ("\\rho", "ρ"),
            ("\\sigma", "σ"),
            ("\\tau", "τ"),
            ("\\upsilon", "υ"),
            ("\\phi", "ϕ"),
            ("\\chi", "χ"),
            ("\\psi", "ψ"),
            ("\\omega", "ω"),
            ("\\Gamma", "Γ"),
            ("\\Delta", "Δ"),
            ("\\Theta", "Θ"),
            ("\\Lambda", "Λ"),
            ("\\Xi", "Ξ"),
            ("\\Pi", "Π"),
            ("\\Sigma", "Σ"),
            ("\\Upsilon", "Υ"),
            ("\\Phi", "Φ"),
            ("\\Psi", "Ψ"),
            ("\\Omega", "Ω"),
        ];
        // Sort strictly by token length descending to guarantee longest prefix match
        tokens.sort_by_key(|token| std::cmp::Reverse(token.0.len()));
        tokens
    });

    for &(tex, uni) in tokens {
        s = s.replace(tex, uni);
    }

    // 4. LaTeX Spacing Tokens
    s = s
        .replace("\\quad", "  ")
        .replace("\\qquad", "    ")
        .replace("\\,", " ")
        .replace("\\;", " ")
        .replace("\\!", "");

    // 5. Superscripts & Subscripts
    s = convert_scripts(&s);

    s
}

/// Given a string starting with '{', extracts the inner content up to matching '}' and the remaining slice.
fn extract_braced_content(text: &str) -> Option<(&str, &str)> {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('{') {
        return None;
    }
    let mut depth = 0;
    let mut end_idx = None;
    for (i, c) in trimmed.char_indices() {
        if c == '{' {
            depth += 1;
        } else if c == '}' {
            depth -= 1;
            if depth == 0 {
                end_idx = Some(i);
                break;
            }
        }
    }
    if let Some(idx) = end_idx {
        let inner = &trimmed[1..idx];
        let rest = &trimmed[idx + 1..];
        Some((inner, rest))
    } else {
        None
    }
}

fn strip_font_wrappers(mut text: &str) -> String {
    let mut out = String::new();
    let wrappers = [
        "\\mathbf", "\\mathrm", "\\text", "\\mathit", "\\textbf", "\\textit",
    ];

    while !text.is_empty() {
        let mut matched = false;
        for wrapper in &wrappers {
            if let Some(after_wrapper) = text.strip_prefix(wrapper)
                && let Some((inner, rest)) = extract_braced_content(after_wrapper)
            {
                out.push_str(inner);
                text = rest;
                matched = true;
                break;
            }
        }
        if !matched {
            let ch = text.chars().next().unwrap();
            out.push(ch);
            text = &text[ch.len_utf8()..];
        }
    }
    out
}

fn convert_generic_fractions(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;

    while let Some(start) = rest.find("\\frac") {
        out.push_str(&rest[..start]);
        let after_cmd = &rest[start + 5..];
        if let Some((numerator, after_num)) = extract_braced_content(after_cmd)
            && let Some((denominator, after_den)) = extract_braced_content(after_num)
        {
            let num_clean = numerator.trim();
            let den_clean = denominator.trim();
            let frac_str = match (num_clean, den_clean) {
                ("1", "2") => "½".to_string(),
                ("1", "4") => "¼".to_string(),
                ("3", "4") => "¾".to_string(),
                ("1", "3") => "⅓".to_string(),
                ("2", "3") => "⅔".to_string(),
                _ => {
                    let num_u = latex_to_unicode(num_clean);
                    let den_u = latex_to_unicode(den_clean);
                    format!("{}/{}", num_u, den_u)
                }
            };
            out.push_str(&frac_str);
            rest = after_den;
            continue;
        }
        out.push_str("\\frac");
        rest = &rest[start + 5..];
    }
    out.push_str(rest);
    out
}

fn convert_scripts(text: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '^' && i + 1 < len {
            if chars[i + 1] == '{' {
                if let Some(close_idx) = chars[i + 2..].iter().position(|&c| c == '}') {
                    let group_end = i + 2 + close_idx;
                    let inner: String = chars[i + 2..group_end].iter().collect();
                    if let Some(unicode_sup) = to_superscript_group(&inner) {
                        out.push_str(&unicode_sup);
                        i = group_end + 1;
                        continue;
                    }
                }
            } else if let Some(sup) = to_superscript_char(chars[i + 1]) {
                out.push(sup);
                i += 2;
                continue;
            }
        } else if chars[i] == '_' && i + 1 < len {
            if chars[i + 1] == '{' {
                if let Some(close_idx) = chars[i + 2..].iter().position(|&c| c == '}') {
                    let group_end = i + 2 + close_idx;
                    let inner: String = chars[i + 2..group_end].iter().collect();
                    if let Some(unicode_sub) = to_subscript_group(&inner) {
                        out.push_str(&unicode_sub);
                        i = group_end + 1;
                        continue;
                    }
                }
            } else if let Some(sub) = to_subscript_char(chars[i + 1]) {
                out.push(sub);
                i += 2;
                continue;
            }
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

fn to_superscript_group(s: &str) -> Option<String> {
    if s.chars().count() == 1 {
        to_superscript_char(s.chars().next()?).map(|c| c.to_string())
    } else if s.chars().all(|c| c.is_ascii_digit()) {
        to_superscript_string(s)
    } else {
        None
    }
}

fn to_subscript_group(s: &str) -> Option<String> {
    if s.chars().count() == 1 {
        to_subscript_char(s.chars().next()?).map(|c| c.to_string())
    } else if s.chars().all(|c| c.is_ascii_digit()) {
        to_subscript_string(s)
    } else {
        None
    }
}

fn to_superscript_char(c: char) -> Option<char> {
    match c {
        '0' => Some('⁰'),
        '1' => Some('¹'),
        '2' => Some('²'),
        '3' => Some('³'),
        '4' => Some('⁴'),
        '5' => Some('⁵'),
        '6' => Some('⁶'),
        '7' => Some('⁷'),
        '8' => Some('⁸'),
        '9' => Some('⁹'),
        '+' => Some('⁺'),
        '-' => Some('⁻'),
        '=' => Some('⁼'),
        '(' => Some('⁽'),
        ')' => Some('⁾'),
        'n' => Some('ⁿ'),
        'i' => Some('ⁱ'),
        _ => None,
    }
}

fn to_superscript_string(s: &str) -> Option<String> {
    let mut out = String::new();
    for c in s.chars() {
        {
            let sup = to_superscript_char(c)?;
            out.push(sup);
        }
    }
    Some(out)
}

fn to_subscript_char(c: char) -> Option<char> {
    match c {
        '0' => Some('₀'),
        '1' => Some('₁'),
        '2' => Some('₂'),
        '3' => Some('₃'),
        '4' => Some('₄'),
        '5' => Some('₅'),
        '6' => Some('₆'),
        '7' => Some('₇'),
        '8' => Some('₈'),
        '9' => Some('₉'),
        '+' => Some('₊'),
        '-' => Some('₋'),
        '=' => Some('₌'),
        '(' => Some('₍'),
        ')' => Some('₎'),
        'a' => Some('ₐ'),
        'e' => Some('ₑ'),
        'h' => Some('ₕ'),
        'i' => Some('ᵢ'),
        'j' => Some('ⱼ'),
        'k' => Some('ₖ'),
        'l' => Some('ₗ'),
        'm' => Some('ₘ'),
        'n' => Some('ₙ'),
        'o' => Some('ₒ'),
        'p' => Some('ₚ'),
        'r' => Some('ᵣ'),
        's' => Some('ₛ'),
        't' => Some('ₜ'),
        'u' => Some('ᵤ'),
        'v' => Some('ᵥ'),
        'x' => Some('ₓ'),
        _ => None,
    }
}

fn to_subscript_string(s: &str) -> Option<String> {
    let mut out = String::new();
    for c in s.chars() {
        {
            let sub = to_subscript_char(c)?;
            out.push(sub);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latex_to_unicode_basic() {
        assert_eq!(latex_to_unicode("E = mc^2"), "E = mc²");
        assert_eq!(latex_to_unicode("\\alpha + \\beta = \\gamma"), "α + β = γ");
        assert_eq!(latex_to_unicode("\\int_0^1 x dx"), "∫₀¹ x dx");
    }

    #[test]
    fn test_latex_to_unicode_no_prefix_collisions() {
        assert_eq!(latex_to_unicode("A \\subseteq B \\subset C"), "A ⊆ B ⊂ C");
        assert_eq!(latex_to_unicode("x \\notin S \\in T"), "x ∉ S ∈ T");
        assert_eq!(latex_to_unicode("a \\leq b \\le c"), "a ≤ b ≤ c");
        assert_eq!(latex_to_unicode("a \\geq b \\ge c"), "a ≥ b ≥ c");
        assert_eq!(latex_to_unicode("a \\neq b \\ne c"), "a ≠ b ≠ c");
        assert_eq!(latex_to_unicode("\\oiint \\iiint \\iint \\int"), "∯ ∭ ∬ ∫");
        assert_eq!(latex_to_unicode("\\varepsilon \\epsilon"), "ε ϵ");
        assert_eq!(latex_to_unicode("\\vartheta \\theta"), "ϑ θ");
    }

    #[test]
    fn test_latex_to_unicode_nested_fractions_and_braces() {
        assert_eq!(latex_to_unicode("\\frac{1}{2}"), "½");
        assert_eq!(latex_to_unicode("\\frac{a_{i}}{b}"), "aᵢ/b");
        assert_eq!(
            latex_to_unicode("\\frac{\\alpha + \\beta}{\\gamma}"),
            "α + β/γ"
        );
        assert_eq!(latex_to_unicode("\\mathbf{x_{i}}"), "xᵢ");
        assert_eq!(latex_to_unicode("\\customUnknown{x}"), "\\customUnknown{x}");
    }

    #[test]
    fn test_latex_to_unicode_grouped_exponents() {
        assert_eq!(latex_to_unicode("x^{10}"), "x¹⁰");
        assert_eq!(latex_to_unicode("a_{12}"), "a₁₂");
        // Complex non-numeric groups are preserved
        assert_eq!(latex_to_unicode("x^{n+1}"), "x^{n+1}");
    }
}
