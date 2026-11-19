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

use crate::error::ExecError;
use crate::planner::compiler::compile_script;
use crate::runtime::context::{ExecutionContext, TrapSignal};
use crate::runtime::traits::{BoxFuture, Executable, ScopeManager};
use std::path::{Path, PathBuf};
use std::time::Duration;

const ESCAPED_PRINTF_PERCENT: char = '\u{f0000}';
const PRINTF_BYTE_MARKER_BASE: u32 = 0xf1000;

pub fn parse_duration(s: &str) -> Result<Duration, ExecError> {
    let s = s.trim();
    if let Some(num_str) = s.strip_suffix("ms") {
        let ms: u64 = num_str
            .parse()
            .map_err(|_| ExecError::Custom(format!("Invalid millisecond duration: '{}'", s)))?;
        return Ok(Duration::from_millis(ms));
    }
    if let Some(num_str) = s.strip_suffix('s') {
        let secs: f64 = num_str
            .parse()
            .map_err(|_| ExecError::Custom(format!("Invalid seconds duration: '{}'", s)))?;
        return checked_duration(secs, "seconds", s);
    }
    if let Some(num_str) = s.strip_suffix('m') {
        let mins: f64 = num_str
            .parse()
            .map_err(|_| ExecError::Custom(format!("Invalid minutes duration: '{}'", s)))?;
        return checked_duration(mins * 60.0, "minutes", s);
    }
    if let Some(num_str) = s.strip_suffix('h') {
        let hours: f64 = num_str
            .parse()
            .map_err(|_| ExecError::Custom(format!("Invalid hours duration: '{}'", s)))?;
        return checked_duration(hours * 3600.0, "hours", s);
    }
    if let Ok(secs) = s.parse::<f64>() {
        return checked_duration(secs, "seconds", s);
    }
    Err(ExecError::Custom(format!(
        "Unrecognized duration format: '{}'",
        s
    )))
}

fn checked_duration(seconds: f64, unit: &str, original: &str) -> Result<Duration, ExecError> {
    Duration::try_from_secs_f64(seconds)
        .map_err(|_| ExecError::Custom(format!("Invalid {unit} duration: '{original}'")))
}

pub fn unescape_string(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('n') => {
                    chars.next();
                    out.push('\n');
                }
                Some('t') => {
                    chars.next();
                    out.push('\t');
                }
                Some('r') => {
                    chars.next();
                    out.push('\r');
                }
                Some('\\') => {
                    chars.next();
                    out.push('\\');
                }
                Some('e') => {
                    chars.next();
                    out.push('\x1b');
                }
                Some('a') => {
                    chars.next();
                    out.push('\x07');
                }
                Some('b') => {
                    chars.next();
                    out.push('\x08');
                }
                Some('f') => {
                    chars.next();
                    out.push('\x0C');
                }
                Some('v') => {
                    chars.next();
                    out.push('\x0B');
                }
                Some('0') => {
                    chars.next();
                    let mut octal = String::new();
                    while let Some(&d) = chars.peek() {
                        if ('0'..='7').contains(&d) && octal.len() < 3 {
                            octal.push(d);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if !octal.is_empty() {
                        if let Ok(code) = u32::from_str_radix(&octal, 8)
                            && let Some(ch) = char::from_u32(code)
                        {
                            out.push(ch);
                        }
                    } else {
                        out.push('\0');
                    }
                }
                Some('x') => {
                    chars.next();
                    let mut hex = String::new();
                    while let Some(&d) = chars.peek() {
                        if d.is_ascii_hexdigit() && hex.len() < 2 {
                            hex.push(d);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if let Ok(code) = u32::from_str_radix(&hex, 16)
                        && let Some(ch) = char::from_u32(code)
                    {
                        out.push(ch);
                    }
                }
                Some(&other) => {
                    chars.next();
                    out.push(other);
                }
                None => {
                    out.push('\\');
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn format_c_style(fmt: &str, args: &[String]) -> String {
    restore_printf_bytes_as_latin1(&format_c_style_with_status(fmt, args).0)
}

fn restore_printf_bytes_as_latin1(output: &str) -> String {
    output
        .chars()
        .map(|ch| {
            let code = ch as u32;
            if (PRINTF_BYTE_MARKER_BASE + 0x80..=PRINTF_BYTE_MARKER_BASE + 0xff).contains(&code) {
                char::from_u32(code - PRINTF_BYTE_MARKER_BASE).unwrap_or(ch)
            } else {
                ch
            }
        })
        .collect()
}

fn printf_output_bytes(output: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(output.len());
    for ch in output.chars() {
        let code = ch as u32;
        if (PRINTF_BYTE_MARKER_BASE + 0x80..=PRINTF_BYTE_MARKER_BASE + 0xff).contains(&code) {
            bytes.push((code - PRINTF_BYTE_MARKER_BASE) as u8);
        } else {
            let mut encoded = [0; 4];
            bytes.extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
        }
    }
    bytes
}

fn collect_printf_spec(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    let mut spec = String::new();
    while chars
        .peek()
        .is_some_and(|ch| matches!(ch, '-' | '+' | ' ' | '#' | '0'))
    {
        spec.push(chars.next().unwrap());
    }
    if chars.peek() == Some(&'*') {
        spec.push(chars.next().unwrap());
    } else {
        while chars.peek().is_some_and(char::is_ascii_digit) {
            spec.push(chars.next().unwrap());
        }
    }
    if chars.peek() == Some(&'.') {
        spec.push(chars.next().unwrap());
        if chars.peek() == Some(&'*') {
            spec.push(chars.next().unwrap());
        } else {
            while chars.peek().is_some_and(char::is_ascii_digit) {
                spec.push(chars.next().unwrap());
            }
        }
    }
    if matches!(chars.peek(), Some('h' | 'l')) {
        let modifier = chars.next().unwrap();
        spec.push(modifier);
        if chars.peek() == Some(&modifier) {
            spec.push(chars.next().unwrap());
        }
    } else if chars
        .peek()
        .is_some_and(|ch| matches!(ch, 'j' | 'z' | 't' | 'L'))
    {
        spec.push(chars.next().unwrap());
    }
    spec.push(chars.next()?);
    Some(spec)
}

fn format_c_style_with_status(fmt: &str, args: &[String]) -> (String, bool, Vec<String>) {
    let unescaped_fmt = unescape_printf_format(fmt);
    let mut out = String::with_capacity(unescaped_fmt.len());
    let mut chars = unescaped_fmt.chars().peekable();
    let mut arg_idx = 0;
    let mut diagnostics = Vec::new();

    while let Some(c) = chars.next() {
        if c == '%' {
            if chars.peek() == Some(&'%') {
                chars.next();
                out.push('%');
                continue;
            }

            let Some(spec) = collect_printf_spec(&mut chars) else {
                diagnostics.push("esh: printf: incomplete format conversion\n".to_string());
                break;
            };

            let mut parsed = parse_printf_spec(&spec);
            if !matches!(
                parsed.conversion,
                's' | 'b'
                    | 'c'
                    | 'd'
                    | 'i'
                    | 'o'
                    | 'u'
                    | 'x'
                    | 'X'
                    | 'a'
                    | 'A'
                    | 'f'
                    | 'F'
                    | 'e'
                    | 'E'
                    | 'g'
                    | 'G'
            ) {
                diagnostics.push(format!(
                    "esh: printf: invalid conversion specifier %{}\n",
                    parsed.conversion
                ));
                continue;
            }
            if parsed.width_from_arg {
                let width = args.get(arg_idx).map_or(0, |value| {
                    parse_printf_or_zero(value, true, false, &mut diagnostics)
                });
                arg_idx += 1;
                if width < 0 {
                    if !parsed.flags.contains('-') {
                        parsed.flags.push('-');
                    }
                    parsed.width = Some(width.unsigned_abs().min(usize::MAX as u64) as usize);
                } else {
                    parsed.width = Some(width as usize);
                }
            }
            if parsed.precision_from_arg {
                let precision = args.get(arg_idx).map_or(0, |value| {
                    parse_printf_or_zero(value, true, false, &mut diagnostics)
                });
                arg_idx += 1;
                parsed.precision = (precision >= 0).then_some(precision as usize);
            }
            let arg_present = args.get(arg_idx).is_some();
            let arg_val = args.get(arg_idx).map(|s| s.as_str()).unwrap_or("");
            arg_idx += 1;

            let formatted = match parsed.conversion {
                's' => {
                    let value = parsed.precision.map_or_else(
                        || arg_val.to_string(),
                        |limit| arg_val.chars().take(limit).collect(),
                    );
                    apply_string_width(value, &parsed)
                }
                'b' => {
                    let (value, stop) = unescape_printf_b(arg_val);
                    if stop {
                        let value = parsed
                            .precision
                            .map_or(value.clone(), |limit| value.chars().take(limit).collect());
                        out.push_str(&apply_string_width(value, &parsed));
                        return (out.replace(ESCAPED_PRINTF_PERCENT, "%"), true, diagnostics);
                    }
                    apply_string_width(
                        parsed
                            .precision
                            .map_or(value.clone(), |limit| value.chars().take(limit).collect()),
                        &parsed,
                    )
                }
                'd' | 'i' => format_integer(
                    parse_printf_or_zero(arg_val, arg_present, false, &mut diagnostics),
                    false,
                    10,
                    &parsed,
                ),
                'o' | 'u' | 'x' | 'X' => {
                    let radix = match parsed.conversion {
                        'o' => 8,
                        'x' => 16,
                        'X' => 16,
                        _ => 10,
                    };
                    format_integer(
                        parse_printf_or_zero(arg_val, arg_present, true, &mut diagnostics),
                        true,
                        radix,
                        &parsed,
                    )
                }
                'a' | 'A' | 'f' | 'F' | 'e' | 'E' | 'g' | 'G' => {
                    let num: f64 = if !arg_present {
                        0.0
                    } else if let Some((value, valid)) = parse_printf_float(arg_val) {
                        if float_operand_out_of_range(arg_val, value) {
                            diagnostics.push(printf_range_diagnostic(arg_val));
                        } else if !valid {
                            diagnostics.push(printf_numeric_diagnostic(arg_val));
                        }
                        value
                    } else {
                        diagnostics.push(printf_numeric_diagnostic(arg_val));
                        0.0
                    };
                    if matches!(parsed.conversion, 'a' | 'A') {
                        format_hex_float(num, &parsed)
                    } else {
                        format_float(num, &parsed)
                    }
                }
                'c' => {
                    apply_string_width(arg_val.chars().next().unwrap_or('\0').to_string(), &parsed)
                }
                _ => arg_val.to_string(),
            };
            out.push_str(&formatted);
        } else {
            out.push(c);
        }
    }
    (out.replace(ESCAPED_PRINTF_PERCENT, "%"), false, diagnostics)
}

fn printf_numeric_diagnostic(value: &str) -> String {
    format!("esh: printf: {value}: expected numeric value\n")
}

fn printf_range_diagnostic(value: &str) -> String {
    format!("esh: printf: {value}: numeric result out of range\n")
}

fn parse_printf_or_zero(
    value: &str,
    present: bool,
    unsigned: bool,
    diagnostics: &mut Vec<String>,
) -> i64 {
    if !present {
        return 0;
    }
    let (number, valid) = parse_printf_integer(value, unsigned);
    if !valid {
        diagnostics.push(printf_numeric_diagnostic(value));
    }
    number
}

fn parse_printf_float(value: &str) -> Option<(f64, bool)> {
    let value = value.trim_start();
    if let Ok(number) = value.parse::<f64>() {
        return Some((number, true));
    }
    if let Some((number, end)) = parse_hex_printf_float(value) {
        return Some((number, value[end..].is_empty()));
    }
    value
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(value.len()))
        .filter_map(|end| value[..end].parse::<f64>().ok().map(|number| (end, number)))
        .max_by_key(|(end, _)| *end)
        .map(|(end, number)| (number, value[end..].is_empty()))
}

fn float_operand_out_of_range(operand: &str, value: f64) -> bool {
    let operand = operand.trim().trim_start_matches(['+', '-']);
    if operand.to_ascii_lowercase().starts_with("inf")
        || operand.to_ascii_lowercase().starts_with("nan")
    {
        return false;
    }
    if value.is_infinite() {
        return true;
    }

    let mantissa = operand
        .split_once(['e', 'E', 'p', 'P'])
        .map_or(operand, |(mantissa, _)| mantissa);
    let nonzero_mantissa = mantissa
        .strip_prefix("0x")
        .or_else(|| mantissa.strip_prefix("0X"))
        .unwrap_or(mantissa)
        .chars()
        .filter_map(|ch| ch.to_digit(16))
        .any(|digit| digit != 0);
    nonzero_mantissa && (value == 0.0 || value.abs() < f64::MIN_POSITIVE)
}

fn parse_hex_printf_float(value: &str) -> Option<(f64, usize)> {
    let bytes = value.as_bytes();
    let mut index = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let negative = bytes.first() == Some(&b'-');
    if bytes.get(index) != Some(&b'0') || !matches!(bytes.get(index + 1), Some(b'x' | b'X')) {
        return None;
    }
    index += 2;

    let mut significand = 0.0;
    let mut digits = 0usize;
    while let Some(byte) = bytes.get(index).copied()
        && let Some(digit) = (byte as char).to_digit(16)
    {
        significand = significand * 16.0 + f64::from(digit);
        digits += 1;
        index += 1;
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let mut place = 1.0 / 16.0;
        while let Some(byte) = bytes.get(index).copied()
            && let Some(digit) = (byte as char).to_digit(16)
        {
            significand += f64::from(digit) * place;
            place /= 16.0;
            digits += 1;
            index += 1;
        }
    }
    if digits == 0 || !matches!(bytes.get(index), Some(b'p' | b'P')) {
        return None;
    }
    index += 1;

    let exponent_start = index;
    if matches!(bytes.get(index), Some(b'+' | b'-')) {
        index += 1;
    }
    let exponent_digits_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if index == exponent_digits_start {
        return None;
    }
    let exponent_text = &value[exponent_start..index];
    let exponent = exponent_text.parse::<i32>().unwrap_or_else(|_| {
        if exponent_text.starts_with('-') {
            i32::MIN
        } else {
            i32::MAX
        }
    });
    let result = significand * 2.0f64.powi(exponent);
    Some((if negative { -result } else { result }, index))
}

fn format_float(value: f64, spec: &PrintfSpec) -> String {
    let conversion = spec.conversion;
    let uppercase = matches!(conversion, 'F' | 'E' | 'G');
    let precision = spec.precision.unwrap_or(6);
    let mut formatted = if value.is_nan() {
        if uppercase { "NAN" } else { "nan" }.to_string()
    } else if value.is_infinite() {
        if value.is_sign_negative() {
            if uppercase { "-INF" } else { "-inf" }.to_string()
        } else if uppercase {
            "INF".to_string()
        } else {
            "inf".to_string()
        }
    } else {
        match conversion {
            'e' | 'E' => {
                let mut output = normalize_exponent(format!("{value:.precision$e}"), uppercase);
                if spec.flags.contains('#') && !output.contains('.') {
                    let exponent = output.find(['e', 'E']).unwrap_or(output.len());
                    output.insert(exponent, '.');
                }
                output
            }
            'g' | 'G' => {
                format_general_float(value, precision, spec.flags.contains('#'), uppercase)
            }
            _ => {
                let mut output = format!("{value:.precision$}");
                if spec.flags.contains('#') && !output.contains('.') {
                    output.push('.');
                }
                output
            }
        }
    };

    if value.is_nan() && value.is_sign_negative() {
        formatted.insert(0, '-');
    } else if !value.is_sign_negative() {
        if spec.flags.contains('+') {
            formatted.insert(0, '+');
        } else if spec.flags.contains(' ') {
            formatted.insert(0, ' ');
        }
    }
    apply_numeric_width_with_zero(formatted, spec, !value.is_nan() && !value.is_infinite())
}

fn format_hex_float(value: f64, spec: &PrintfSpec) -> String {
    let uppercase = spec.conversion == 'A';
    let precision = spec.precision;
    let negative = value.is_sign_negative();
    let (body, prefix) = if value.is_nan() {
        (if uppercase { "NAN" } else { "nan" }.to_string(), "")
    } else if value.is_infinite() {
        (if uppercase { "INF" } else { "inf" }.to_string(), "")
    } else {
        let bits = value.abs().to_bits();
        let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
        let fraction = bits & ((1u64 << 52) - 1);
        let (mut leading, mut exponent) = if bits == 0 {
            ('0', 0)
        } else if exponent_bits == 0 {
            ('0', -1022)
        } else {
            ('1', exponent_bits - 1023)
        };
        let mut digits = format!("{fraction:013x}");
        let count = precision.unwrap_or_else(|| digits.trim_end_matches('0').len());
        if count < 13 {
            let discarded = &digits[count..];
            let first = discarded.as_bytes()[0];
            let rest_nonzero = discarded.as_bytes()[1..].iter().any(|digit| *digit != b'0');
            let previous_odd = if count == 0 {
                leading == '1'
            } else {
                (digits.as_bytes()[count - 1] & 1) != 0
            };
            let round_up = first > b'8' || (first == b'8' && (rest_nonzero || previous_odd));
            digits.truncate(count);
            if round_up {
                let mut bytes = digits.into_bytes();
                let mut carry = true;
                for byte in bytes.iter_mut().rev() {
                    let digit = (*byte as char).to_digit(16).unwrap_or(0);
                    if digit < 15 {
                        *byte = b"0123456789abcdef"[(digit + 1) as usize];
                        carry = false;
                        break;
                    }
                    *byte = b'0';
                }
                if carry {
                    // Rounding a normalized significand up changes 1.ffff to 2.0.
                    // Keep the equivalent hexadecimal value by adjusting its exponent.
                    if leading == '1' {
                        exponent += 1;
                    } else {
                        leading = '1';
                    }
                    bytes = vec![b'0'; count];
                }
                digits = String::from_utf8(bytes).unwrap_or_default();
            }
        } else {
            digits.push_str(&"0".repeat(count - 13));
        }
        let radix = if uppercase { 'P' } else { 'p' };
        let marker = if uppercase { "0X" } else { "0x" };
        let mantissa = if !digits.is_empty() || spec.flags.contains('#') {
            format!(
                "{leading}.{}",
                if uppercase {
                    digits.to_uppercase()
                } else {
                    digits
                }
            )
        } else {
            leading.to_string()
        };
        (format!("{mantissa}{radix}{exponent:+}"), marker)
    };
    let sign = if negative {
        "-"
    } else if spec.flags.contains('+') {
        "+"
    } else if spec.flags.contains(' ') {
        " "
    } else {
        ""
    };
    let mut formatted = format!("{sign}{prefix}{body}");
    let zero_padding = !value.is_nan() && !value.is_infinite();
    if zero_padding && spec.flags.contains('0') && !spec.flags.contains('-') {
        let padding = spec.width.unwrap_or(0).saturating_sub(formatted.len());
        let sign_len = usize::from(formatted.starts_with(['-', '+', ' ']));
        let prefix_end =
            if formatted[sign_len..].starts_with("0x") || formatted[sign_len..].starts_with("0X") {
                sign_len + 2
            } else {
                sign_len
            };
        formatted.insert_str(prefix_end, &"0".repeat(padding));
    } else {
        formatted = apply_numeric_width_with_zero(formatted, spec, zero_padding);
    }
    formatted
}

fn format_general_float(value: f64, precision: usize, alternate: bool, uppercase: bool) -> String {
    let precision = precision.max(1);
    let scientific = format!("{value:.digits$e}", digits = precision - 1);
    let exponent = scientific
        .split_once('e')
        .and_then(|(_, exponent)| exponent.parse::<i32>().ok())
        .unwrap_or(0);
    let use_scientific = exponent < -4 || exponent >= precision as i32;

    let mut output = if use_scientific {
        normalize_exponent(scientific, uppercase)
    } else {
        let fractional_digits = (precision as i32 - exponent - 1).max(0) as usize;
        format!("{value:.fractional_digits$}")
    };
    if !alternate {
        if let Some((mantissa, exponent)) = output.split_once(['e', 'E']) {
            let mut mantissa = mantissa.to_string();
            trim_fractional_zeros(&mut mantissa);
            let marker = if uppercase { 'E' } else { 'e' };
            output = format!("{mantissa}{marker}{exponent}");
        } else {
            trim_fractional_zeros(&mut output);
        }
    } else if !output.contains('.') {
        if let Some(exponent) = output.find(['e', 'E']) {
            output.insert(exponent, '.');
        } else {
            output.push('.');
        }
    }
    output
}

fn normalize_exponent(mut output: String, uppercase: bool) -> String {
    if let Some((mantissa, exponent)) = output.split_once('e') {
        let exponent = exponent.parse::<i32>().unwrap_or(0);
        let marker = if uppercase { 'E' } else { 'e' };
        output = format!("{mantissa}{marker}{exponent:+03}");
    }
    output
}

fn trim_fractional_zeros(output: &mut String) {
    if !output.contains('.') {
        return;
    }
    while output.ends_with('0') {
        output.pop();
    }
    if output.ends_with('.') {
        output.pop();
    }
}

#[derive(Default)]
struct PrintfSpec {
    conversion: char,
    flags: String,
    length: String,
    width: Option<usize>,
    width_from_arg: bool,
    precision: Option<usize>,
    precision_from_arg: bool,
}

fn parse_printf_spec(spec: &str) -> PrintfSpec {
    let mut chars = spec.chars().peekable();
    let mut parsed = PrintfSpec {
        conversion: spec.chars().last().unwrap_or('s'),
        ..PrintfSpec::default()
    };
    while chars
        .peek()
        .is_some_and(|ch| matches!(ch, '-' | '+' | ' ' | '#' | '0'))
    {
        parsed.flags.push(chars.next().unwrap());
    }
    if chars.peek() == Some(&'*') {
        chars.next();
        parsed.width_from_arg = true;
    } else {
        let mut width = String::new();
        while chars.peek().is_some_and(char::is_ascii_digit) {
            width.push(chars.next().unwrap());
        }
        parsed.width = width.parse::<usize>().ok();
    }
    if chars.peek() == Some(&'.') {
        chars.next();
        if chars.peek() == Some(&'*') {
            chars.next();
            parsed.precision_from_arg = true;
        } else {
            let mut precision = String::new();
            while chars.peek().is_some_and(char::is_ascii_digit) {
                precision.push(chars.next().unwrap());
            }
            parsed.precision = Some(precision.parse::<usize>().unwrap_or(0));
        }
    }
    if matches!(chars.peek(), Some('h' | 'l')) {
        let modifier = chars.next().unwrap();
        parsed.length.push(modifier);
        if chars.peek() == Some(&modifier) {
            parsed.length.push(chars.next().unwrap());
        }
    } else if chars
        .peek()
        .is_some_and(|ch| matches!(ch, 'j' | 'z' | 't' | 'L'))
    {
        parsed.length.push(chars.next().unwrap());
    }
    parsed
}

fn apply_string_width(mut value: String, spec: &PrintfSpec) -> String {
    let padding = spec
        .width
        .unwrap_or(0)
        .saturating_sub(value.chars().count());
    if spec.flags.contains('-') {
        value.extend(std::iter::repeat_n(' ', padding));
        value
    } else {
        format!("{}{}", " ".repeat(padding), value)
    }
}

fn apply_numeric_width(value: String, spec: &PrintfSpec) -> String {
    apply_numeric_width_with_zero(value, spec, true)
}

fn apply_numeric_width_with_zero(value: String, spec: &PrintfSpec, zero_padding: bool) -> String {
    let padding = spec.width.unwrap_or(0).saturating_sub(value.len());
    if spec.flags.contains('-') {
        format!("{value}{}", " ".repeat(padding))
    } else if zero_padding && spec.flags.contains('0') {
        let sign_len = usize::from(value.starts_with(['-', '+', ' ']));
        format!(
            "{}{}{}",
            &value[..sign_len],
            "0".repeat(padding),
            &value[sign_len..]
        )
    } else {
        format!("{}{}", " ".repeat(padding), value)
    }
}

fn format_integer(value: i64, unsigned: bool, radix: u32, spec: &PrintfSpec) -> String {
    let negative = !unsigned && value < 0;
    let mut digits = if unsigned {
        let magnitude = value as u64;
        match radix {
            8 => format!("{magnitude:o}"),
            16 if spec.conversion == 'X' => format!("{magnitude:X}"),
            16 => format!("{magnitude:x}"),
            _ => magnitude.to_string(),
        }
    } else {
        let magnitude = value.unsigned_abs();
        match radix {
            8 => format!("{magnitude:o}"),
            16 if spec.conversion == 'X' => format!("{magnitude:X}"),
            16 => format!("{magnitude:x}"),
            _ => magnitude.to_string(),
        }
    };
    if spec.precision == Some(0) && value == 0 {
        digits.clear();
    }
    if let Some(precision) = spec.precision
        && precision > digits.len()
    {
        digits = format!("{}{}", "0".repeat(precision - digits.len()), digits);
    }
    let prefix = if spec.flags.contains('#') {
        match radix {
            8 if !digits.starts_with('0') => "0",
            16 if value != 0 && spec.conversion == 'X' => "0X",
            16 if value != 0 => "0x",
            _ => "",
        }
    } else {
        ""
    };
    let sign = if negative {
        "-"
    } else if !unsigned && spec.flags.contains('+') {
        "+"
    } else if !unsigned && spec.flags.contains(' ') {
        " "
    } else {
        ""
    };
    let value = format!("{sign}{prefix}{digits}");
    if spec.precision.is_none() && spec.flags.contains('0') && !spec.flags.contains('-') {
        let padding = spec.width.unwrap_or(0).saturating_sub(value.len());
        return format!("{sign}{prefix}{}{digits}", "0".repeat(padding));
    }
    apply_numeric_width(value, spec)
}

fn unescape_printf_b(value: &str) -> (String, bool) {
    let mut escaped = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            escaped.push(ch);
            continue;
        }
        let Some(next) = chars.next() else {
            escaped.push('\\');
            break;
        };
        if next == 'c' {
            return (unescape_string(&escaped), true);
        }
        if next == '0' {
            let mut octal = String::new();
            while octal.len() < 3
                && chars
                    .peek()
                    .is_some_and(|digit| ('0'..='7').contains(digit))
            {
                octal.push(chars.next().unwrap());
            }
            let byte = u16::from_str_radix(&octal, 8).unwrap_or_default() as u8;
            if byte >= 0x80 {
                escaped.push(char::from_u32(PRINTF_BYTE_MARKER_BASE + u32::from(byte)).unwrap());
            } else {
                escaped.push(char::from(byte));
            }
        } else {
            escaped.push('\\');
            escaped.push(next);
        }
    }
    (unescape_string(&escaped), false)
}

fn unescape_printf_format(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }
        let Some(next) = chars.next() else {
            output.push('\\');
            break;
        };
        if ('0'..='7').contains(&next) {
            let mut digits = String::from(next);
            while digits.len() < 3
                && chars
                    .peek()
                    .is_some_and(|digit| ('0'..='7').contains(digit))
            {
                digits.push(chars.next().unwrap());
            }
            let value = u32::from_str_radix(&digits, 8).unwrap_or_default();
            // Check the encoded byte so values above 0xff that wrap to `%`
            // remain literal instead of becoming format conversions.
            let byte = value as u8;
            if byte == b'%' {
                output.push(ESCAPED_PRINTF_PERCENT);
            } else {
                // POSIX printf octal escapes emit a byte. Values above 0xff
                // therefore wrap to their low eight bits instead of becoming
                // UTF-8 encodings of a Unicode scalar.
                let decoded = if byte >= 0x80 {
                    char::from_u32(PRINTF_BYTE_MARKER_BASE + u32::from(byte)).unwrap()
                } else {
                    char::from(byte)
                };
                output.push(decoded);
            }
            continue;
        }

        let escaped = format!("\\{next}");
        let decoded = unescape_string(&escaped);
        if decoded == "%" {
            output.push(ESCAPED_PRINTF_PERCENT);
        } else {
            output.push_str(&decoded);
        }
    }
    output
}

fn parse_printf_integer(value: &str, unsigned: bool) -> (i64, bool) {
    if let Some(quoted) = value.strip_prefix('\'').or_else(|| value.strip_prefix('"')) {
        return quoted
            .chars()
            .next()
            .map_or((0, false), |ch| (ch as i64, true));
    }

    let total_len = value.len();
    let leading_len = total_len - value.trim_start().len();
    let value = &value[leading_len..];
    let (negative, value, sign_offset) = match value.as_bytes().first() {
        Some(b'-') => (true, &value[1..], leading_len + 1),
        Some(b'+') => (false, &value[1..], leading_len + 1),
        _ => (false, value, leading_len),
    };
    let (radix, digits, digit_offset) = if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        (16, hex, sign_offset + value.len() - hex.len())
    } else if value.len() > 1 && value.starts_with('0') {
        (8, &value[1..], sign_offset + 1)
    } else {
        (10, value, sign_offset)
    };
    let digit_count = digits
        .char_indices()
        .take_while(|(_, ch)| ch.is_digit(radix))
        .map(|(index, ch)| index + ch.len_utf8())
        .last()
        .unwrap_or(0);
    if digit_count == 0 {
        return (0, false);
    }
    let number_digits = &digits[..digit_count];
    let magnitude = u128::from_str_radix(number_digits, radix).unwrap_or(u128::MAX);
    let lexical_valid = digit_offset + digit_count == total_len;
    if unsigned {
        let overflow = magnitude > u64::MAX as u128;
        if overflow {
            return (u64::MAX as i64, false);
        }
        let magnitude = magnitude.min(u64::MAX as u128) as u64;
        let converted = if negative {
            0u64.wrapping_sub(magnitude)
        } else {
            magnitude
        };
        (converted as i64, lexical_valid && !overflow)
    } else {
        let limit = if negative {
            1u128 << 63
        } else {
            i64::MAX as u128
        };
        let overflow = magnitude > limit;
        let magnitude = magnitude.min(limit);
        let converted = if negative {
            if magnitude == 1u128 << 63 {
                i64::MIN
            } else {
                -(magnitude as i64)
            }
        } else {
            magnitude as i64
        };
        (converted, lexical_valid && !overflow)
    }
}

fn format_argument_count(fmt: &str) -> usize {
    let unescaped_fmt = unescape_printf_format(fmt);
    let mut chars = unescaped_fmt.chars().peekable();
    let mut count = 0;

    while let Some(c) = chars.next() {
        if c != '%' {
            continue;
        }
        if chars.peek() == Some(&'%') {
            chars.next();
            continue;
        }

        let Some(spec) = collect_printf_spec(&mut chars) else {
            break;
        };
        let parsed = parse_printf_spec(&spec);
        if matches!(
            parsed.conversion,
            's' | 'b'
                | 'c'
                | 'd'
                | 'i'
                | 'o'
                | 'u'
                | 'x'
                | 'X'
                | 'a'
                | 'A'
                | 'f'
                | 'F'
                | 'e'
                | 'E'
                | 'g'
                | 'G'
        ) {
            count +=
                1 + usize::from(parsed.width_from_arg) + usize::from(parsed.precision_from_arg);
        }
    }

    count
}

pub fn echo<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut newline = true;
        let mut enable_escapes = false;
        let mut format_string: Option<String> = None;

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.peek() {
            if *arg == "--" {
                iter.next();
                break;
            } else if *arg == "-n" {
                iter.next();
                newline = false;
            } else if *arg == "-e" {
                iter.next();
                enable_escapes = true;
            } else if *arg == "-E" {
                iter.next();
                enable_escapes = false;
            } else if *arg == "-ne" || *arg == "-en" {
                iter.next();
                newline = false;
                enable_escapes = true;
            } else if *arg == "-f" {
                iter.next();
                if let Some(fmt) = iter.next() {
                    format_string = Some(fmt.clone());
                } else {
                    ctx.write_stderr(b"esh: echo: -f requires format string\n");
                    return Ok(1);
                }
            } else if let Some(fmt) = arg.strip_prefix("-f") {
                iter.next();
                format_string = Some(fmt.to_string());
            } else {
                break;
            }
        }

        let remaining: Vec<String> = iter.cloned().collect();

        let output = if let Some(fmt) = format_string {
            format_c_style(&fmt, &remaining)
        } else {
            let raw = remaining.join(" ");
            if enable_escapes {
                unescape_string(&raw)
            } else {
                raw
            }
        };

        if newline {
            ctx.write_stdout(format!("{}\n", output).as_bytes());
        } else {
            ctx.write_stdout(output.as_bytes());
        }

        Ok(0)
    })
}

pub fn printf<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: printf: missing format operand\n");
            return Ok(2);
        }
        let fmt = &args[0];
        let rest = &args[1..];
        let argument_count = format_argument_count(fmt);
        let (output, diagnostics) = if argument_count == 0 || rest.is_empty() {
            let (output, _, diagnostics) = format_c_style_with_status(fmt, rest);
            (output, diagnostics)
        } else {
            let mut output = String::new();
            let mut diagnostics = Vec::new();
            for chunk in rest.chunks(argument_count) {
                let (formatted, stop, mut chunk_diagnostics) =
                    format_c_style_with_status(fmt, chunk);
                output.push_str(&formatted);
                diagnostics.append(&mut chunk_diagnostics);
                if stop {
                    break;
                }
            }
            (output, diagnostics)
        };
        ctx.write_stdout(&printf_output_bytes(&output));
        for diagnostic in &diagnostics {
            ctx.write_stderr(diagnostic.as_bytes());
        }
        Ok(i32::from(!diagnostics.is_empty()))
    })
}

fn parse_status_arg(name: &str, args: &[String]) -> Result<i32, String> {
    if args.len() > 1 {
        return Err(format!("{name}: expected zero or one integer argument"));
    }
    let Some(raw) = args.first() else {
        return Ok(0);
    };
    let code = raw
        .parse::<i32>()
        .map_err(|_| format!("{name}: expected an integer status, got '{raw}'"))?;
    if !(0..=255).contains(&code) {
        return Err(format!("{name}: status must be between 0 and 255"));
    }
    Ok(code)
}

pub fn exit<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let code = if args.is_empty() {
            ctx.last_exit()
        } else {
            match parse_status_arg("exit", args) {
                Ok(code) => code,
                Err(message) => {
                    ctx.write_stderr(format!("esh: {message}\n").as_bytes());
                    return Ok(2);
                }
            }
        };
        ctx.request_exit(code);
        Ok(code)
    })
}

pub fn r#return<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if !ctx.in_function() && !ctx.in_source() {
            ctx.write_stderr(b"esh: return: only meaningful in a function or sourced file\n");
            return Ok(2);
        }
        let code = if args.is_empty() {
            ctx.last_exit()
        } else {
            match parse_status_arg("return", args) {
                Ok(code) => code,
                Err(message) => {
                    ctx.write_stderr(format!("esh: {message}\n").as_bytes());
                    return Ok(2);
                }
            }
        };
        ctx.request_return(code);
        Ok(code)
    })
}

fn parse_loop_levels(name: &str, args: &[String]) -> Result<usize, String> {
    if args.len() > 1 {
        return Err(format!(
            "{name}: expected zero or one positive integer argument"
        ));
    }

    let Some(raw) = args.first() else {
        return Ok(1);
    };
    let digits = raw.strip_prefix('+').unwrap_or(raw);
    let levels = digits
        .parse::<usize>()
        .map_err(|_| format!("{name}: expected a positive integer, got '{raw}'"))?;
    if levels == 0 {
        return Err(format!("{name}: loop level must be greater than zero"));
    }
    Ok(levels)
}

pub fn r#break<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let levels = match parse_loop_levels("break", args) {
            Ok(levels) => levels,
            Err(message) => {
                ctx.write_stderr(format!("esh: {message}\n").as_bytes());
                return Ok(2);
            }
        };
        if !ctx.in_loop() {
            ctx.write_stderr(b"esh: break: only meaningful in a `for', `while', or `until' loop\n");
            return Ok(2);
        }
        ctx.trigger_break(levels);
        Ok(0)
    })
}

pub fn r#continue<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let levels = match parse_loop_levels("continue", args) {
            Ok(levels) => levels,
            Err(message) => {
                ctx.write_stderr(format!("esh: {message}\n").as_bytes());
                return Ok(2);
            }
        };
        if !ctx.in_loop() {
            ctx.write_stderr(
                b"esh: continue: only meaningful in a `for', `while', or `until' loop\n",
            );
            return Ok(2);
        }
        ctx.trigger_continue(levels);
        Ok(0)
    })
}

pub fn export<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let args = args.iter();
        if args.len() == 1 && args.as_slice()[0] == "-p" {
            let mut names = ctx
                .exported_var_names()
                .filter(|name| is_shell_identifier(name))
                .map(str::to_string)
                .collect::<Vec<_>>();
            names.sort_unstable();
            let entries = names
                .into_iter()
                .map(|name| {
                    let value = ctx.get_var(&name);
                    (name, value)
                })
                .collect::<Vec<_>>();
            for (name, value) in entries {
                if let Some(value) = value {
                    let quoted = value.replace('\'', "'\\''");
                    ctx.write_stdout(format!("export {name}='{quoted}'\n").as_bytes());
                } else {
                    ctx.write_stdout(format!("export {name}\n").as_bytes());
                }
            }
            return Ok(0);
        }
        let mut parsing_options = true;
        for arg in args {
            if parsing_options && arg == "--" {
                parsing_options = false;
                continue;
            }
            if parsing_options && arg.starts_with('-') {
                ctx.write_stderr(format!("esh: export: unsupported option: {arg}\n").as_bytes());
                return Ok(2);
            }
            let (key, val) = if let Some((key, val)) = arg.split_once('=') {
                (key, Some(val))
            } else {
                (arg.as_str(), None)
            };
            if !is_shell_identifier(key) {
                ctx.write_stderr(format!("esh: export: invalid identifier: {key}\n").as_bytes());
                return Ok(2);
            }
            if let Some(val) = val {
                ctx.set_exported_var(key, val.to_string());
            } else {
                ctx.export_var(key);
            }
        }
        Ok(0)
    })
}

fn is_shell_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'_'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

pub fn unset<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut mode = None;
        let mut names = Vec::new();
        let mut parsing_options = true;
        for arg in args {
            if parsing_options && arg == "--" {
                parsing_options = false;
                continue;
            }
            if parsing_options && arg.starts_with('-') && arg.len() > 1 {
                for option in arg[1..].chars() {
                    let next_mode = match option {
                        'f' => UnsetMode::Function,
                        'v' => UnsetMode::Variable,
                        _ => {
                            ctx.write_stderr(
                                format!("esh: unset: unsupported option '-{option}'\n").as_bytes(),
                            );
                            return Ok(2);
                        }
                    };
                    if mode.is_some_and(|current| current != next_mode) {
                        ctx.write_stderr(b"esh: unset: -f and -v cannot be combined\n");
                        return Ok(2);
                    }
                    mode = Some(next_mode);
                }
                continue;
            }
            parsing_options = false;
            names.push(arg.as_str());
        }

        let mode = mode.unwrap_or(UnsetMode::Variable);
        let mut status = 0;
        for name in names {
            if !is_shell_identifier(name) {
                ctx.write_stderr(format!("esh: unset: invalid identifier: {name}\n").as_bytes());
                status = 2;
                continue;
            }
            match mode {
                UnsetMode::Variable => ctx.unset_var(name),
                UnsetMode::Function => ctx.unset_function(name),
            }
        }
        Ok(status)
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UnsetMode {
    Variable,
    Function,
}

pub fn shift<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.len() > 1 {
            ctx.write_stderr(b"esh: shift: expected zero or one non-negative integer argument\n");
            return Ok(2);
        }
        let count = if let Some(raw) = args.first() {
            match raw.strip_prefix('+').unwrap_or(raw).parse::<usize>() {
                Ok(count) => count,
                Err(_) => {
                    ctx.write_stderr(
                        format!("esh: shift: expected a non-negative integer, got '{raw}'\n")
                            .as_bytes(),
                    );
                    return Ok(2);
                }
            }
        } else {
            1
        };
        let available = ctx.positional_args().len();
        if count > available {
            ctx.write_stderr(
                format!(
                    "esh: shift: requested {count} positional parameters, but only {available} are set\n"
                )
                .as_bytes(),
            );
            return Ok(2);
        }
        ctx.shift_positional_args(count);
        Ok(0)
    })
}

pub fn colon<'a>(
    _args: &'a [String],
    _ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async { Ok(0) })
}

pub fn true_command<'a>(
    _args: &'a [String],
    _ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async { Ok(0) })
}

pub fn false_command<'a>(
    _args: &'a [String],
    _ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async { Ok(1) })
}

pub fn dot<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let Some(source_name) = args.first() else {
            ctx.write_stderr(b"esh: .: expected a file operand\n");
            return Ok(2);
        };
        let source_path = resolve_dot_path(source_name, ctx);
        let source = match std::fs::read_to_string(&source_path) {
            Ok(source) => source,
            Err(error) => {
                ctx.write_stderr(
                    format!("esh: .: cannot read '{}': {error}\n", source_path.display())
                        .as_bytes(),
                );
                return Ok(1);
            }
        };
        let plan = match compile_script(&source) {
            Ok(plan) => plan,
            Err(error) => {
                ctx.write_stderr(format!("esh: .: parse error: {error}\n").as_bytes());
                return Ok(2);
            }
        };

        let previous_args = ctx.positional_args_with_zero().to_vec();
        if args.len() > 1 {
            let mut source_args = vec![previous_args[0].clone()];
            source_args.extend_from_slice(&args[1..]);
            ctx.set_positional_args(source_args);
        }

        // A sourced file is its own `return` boundary, while all other shell
        // state remains in the caller's execution context.
        ctx.enter_source();
        let result = plan.execute(ctx).await;
        let return_code = ctx.take_return();
        ctx.exit_source();
        ctx.set_positional_args(previous_args);
        match result {
            Ok(status) => Ok(return_code.unwrap_or(status)),
            Err(error) => Err(error),
        }
    })
}

fn resolve_dot_path(source_name: &str, ctx: &ExecutionContext) -> PathBuf {
    let source_path = Path::new(source_name);
    if source_path.is_absolute() || source_name.contains('/') {
        return if source_path.is_absolute() {
            source_path.to_path_buf()
        } else {
            ctx.cwd().join(source_path)
        };
    }

    if let Some(path) = ctx.get_var("PATH") {
        for directory in std::env::split_paths(&path) {
            let directory = if directory.is_absolute() {
                directory
            } else {
                ctx.cwd().join(directory)
            };
            let candidate = directory.join(source_path);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    ctx.cwd().join(source_path)
}

pub fn set<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args == ["-"] {
            ctx.options_mut().xtrace = false;
            return Ok(0);
        }

        let mut iter = args.iter().peekable();
        let mut positional = Vec::new();
        let mut parsing_flags = true;
        let mut pending_options = ctx.options().clone();

        while let Some(arg) = iter.next() {
            if parsing_flags && arg == "--" {
                parsing_flags = false;
                continue;
            }

            if parsing_flags && (arg.starts_with('-') || arg.starts_with('+')) && arg.len() > 1 {
                let enable = arg.starts_with('-');
                let flag_chars: &str = &arg[1..];

                if flag_chars == "o" {
                    if let Some(opt_name) = iter.next() {
                        if !set_named_option(&mut pending_options, opt_name, enable) {
                            ctx.write_stderr(
                                format!("esh: set: unknown option '{opt_name}'\n").as_bytes(),
                            );
                            return Ok(2);
                        }
                    } else if enable {
                        ctx.write_stdout(format_set_options(&pending_options).as_bytes());
                    } else {
                        ctx.write_stdout(format_set_replay_options(&pending_options).as_bytes());
                    }
                    continue;
                }

                for ch in flag_chars.chars() {
                    if ch == 'o' {
                        let Some(opt_name) = iter.next() else {
                            ctx.write_stderr(b"esh: set: -o requires an option name\n");
                            return Ok(2);
                        };
                        if !set_named_option(&mut pending_options, opt_name, enable) {
                            ctx.write_stderr(
                                format!("esh: set: unknown option '{opt_name}'\n").as_bytes(),
                            );
                            return Ok(2);
                        }
                        continue;
                    }
                    if !set_short_option(&mut pending_options, ch, enable) {
                        ctx.write_stderr(format!("esh: set: unknown option '{ch}'\n").as_bytes());
                        return Ok(2);
                    }
                }
                continue;
            }

            if parsing_flags {
                parsing_flags = false;
            }
            positional.push(arg.clone());
        }

        *ctx.options_mut() = pending_options;
        if !positional.is_empty() || args.iter().any(|a| a == "--") {
            ctx.set_positional_params(&positional);
        }

        Ok(0)
    })
}

fn set_short_option(
    options: &mut crate::runtime::context::ShellOptions,
    flag: char,
    enable: bool,
) -> bool {
    match flag {
        'a' => options.allexport = enable,
        'e' => options.errexit = enable,
        'u' => options.nounset = enable,
        'x' => options.xtrace = enable,
        'f' => options.noglob = enable,
        'C' => options.noclobber = enable,
        _ => return false,
    }
    true
}

fn set_named_option(
    options: &mut crate::runtime::context::ShellOptions,
    name: &str,
    enable: bool,
) -> bool {
    match name {
        "allexport" | "a" => options.allexport = enable,
        "errexit" | "e" => options.errexit = enable,
        "nounset" | "u" => options.nounset = enable,
        "xtrace" | "x" => options.xtrace = enable,
        "pipefail" => options.pipefail = enable,
        "noglob" | "f" => options.noglob = enable,
        "noclobber" | "C" => options.noclobber = enable,
        _ => return false,
    }
    true
}

fn format_set_options(options: &crate::runtime::context::ShellOptions) -> String {
    let names = [
        "allexport",
        "errexit",
        "noglob",
        "nounset",
        "pipefail",
        "xtrace",
        "noclobber",
    ];
    names
        .iter()
        .map(|name| {
            let enabled = match *name {
                "allexport" => options.allexport,
                "errexit" => options.errexit,
                "noglob" => options.noglob,
                "nounset" => options.nounset,
                "pipefail" => options.pipefail,
                "xtrace" => options.xtrace,
                "noclobber" => options.noclobber,
                _ => false,
            };
            format!("{name} {}\n", if enabled { "on" } else { "off" })
        })
        .collect()
}

fn format_set_replay_options(options: &crate::runtime::context::ShellOptions) -> String {
    let names = [
        "allexport",
        "errexit",
        "noglob",
        "nounset",
        "pipefail",
        "xtrace",
        "noclobber",
    ];
    names
        .iter()
        .map(|name| {
            let enabled = match *name {
                "allexport" => options.allexport,
                "errexit" => options.errexit,
                "noglob" => options.noglob,
                "nounset" => options.nounset,
                "pipefail" => options.pipefail,
                "xtrace" => options.xtrace,
                "noclobber" => options.noclobber,
                _ => false,
            };
            format!("set {}o {name}\n", if enabled { "-" } else { "+" })
        })
        .collect()
}

pub fn sleep<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.len() > 1 {
            ctx.write_stderr(b"esh: sleep: expected exactly one duration argument\n");
            return Ok(2);
        }
        if args.is_empty() {
            ctx.write_stderr(b"esh: sleep: missing duration argument (e.g. 500ms, 2s, 1m)\n");
            return Ok(1);
        }

        let dur = match parse_duration(&args[0]) {
            Ok(d) => d,
            Err(e) => {
                ctx.write_stderr(format!("esh: sleep: {}\n", e).as_bytes());
                return Ok(1);
            }
        };

        let token = ctx.token().clone();
        tokio::select! {
            biased;
            _ = token.cancelled() => {
                ctx.record_failure("sleep", 0, 130, Some("Sleep interrupted".to_string()));
                Ok(130)
            }
            _ = tokio::time::sleep(dur) => Ok(0),
        }
    })
}

pub fn timeout<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.len() < 2 {
            ctx.write_stderr(b"esh: timeout: usage: timeout <duration> <command...>\n");
            return Ok(1);
        }

        let dur = match parse_duration(&args[0]) {
            Ok(d) => d,
            Err(e) => {
                ctx.write_stderr(format!("esh: timeout: {}\n", e).as_bytes());
                return Ok(1);
            }
        };

        let sub_script = args[1..].join(" ");
        let plan = match compile_script(&sub_script) {
            Ok(p) => p,
            Err(e) => {
                ctx.write_stderr(format!("esh: timeout: parse error: {}\n", e).as_bytes());
                return Ok(2);
            }
        };

        match tokio::time::timeout(dur, plan.execute(ctx)).await {
            Ok(res) => res,
            Err(_) => {
                ctx.write_stderr(
                    format!("esh: timeout: command timed out after {:?}\n", dur).as_bytes(),
                );
                ctx.record_failure("timeout", 0, 124, Some("Command timed out".to_string()));
                Ok(124)
            }
        }
    })
}

pub fn retry<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(
                b"esh: retry: usage: retry [-n attempts] [-d delay] [-b factor] [--] <command...>\n",
            );
            return Ok(2);
        }

        let mut attempts = 3usize;
        let mut delay = Duration::from_millis(500);
        let mut factor = 1.0f64;
        let mut cmd_start_idx = 0usize;

        let mut index = 0;
        while index < args.len() {
            let arg = &args[index];
            if arg == "--" {
                index += 1;
                cmd_start_idx = index;
                break;
            }
            match arg.as_str() {
                "-n" | "--attempts" => {
                    index += 1;
                    let Some(value) = args.get(index) else {
                        ctx.write_stderr(b"esh: retry: -n requires a positive attempt count\n");
                        return Ok(2);
                    };
                    attempts = match value.parse::<usize>() {
                        Ok(count) if count > 0 => count,
                        _ => {
                            ctx.write_stderr(
                                format!("esh: retry: invalid attempt count: {value}\n").as_bytes(),
                            );
                            return Ok(2);
                        }
                    };
                }
                "-d" | "--delay" => {
                    index += 1;
                    let Some(value) = args.get(index) else {
                        ctx.write_stderr(b"esh: retry: -d requires a valid delay\n");
                        return Ok(2);
                    };
                    delay = match parse_duration(value) {
                        Ok(delay) => delay,
                        Err(error) => {
                            ctx.write_stderr(format!("esh: retry: {error}\n").as_bytes());
                            return Ok(2);
                        }
                    };
                }
                "-b" | "--backoff" => {
                    index += 1;
                    let Some(value) = args.get(index) else {
                        ctx.write_stderr(b"esh: retry: -b requires a finite non-negative factor\n");
                        return Ok(2);
                    };
                    factor = match value.parse::<f64>() {
                        Ok(factor) if factor.is_finite() && factor >= 0.0 => factor,
                        _ => {
                            ctx.write_stderr(
                                format!("esh: retry: invalid backoff factor: {value}\n").as_bytes(),
                            );
                            return Ok(2);
                        }
                    };
                }
                _ if arg.starts_with('-') => {
                    ctx.write_stderr(format!("esh: retry: unsupported option: {arg}\n").as_bytes());
                    return Ok(2);
                }
                _ => {
                    cmd_start_idx = index;
                    break;
                }
            }
            index += 1;
            cmd_start_idx = index;
        }

        if cmd_start_idx >= args.len() {
            ctx.write_stderr(b"esh: retry: missing command to execute\n");
            return Ok(2);
        }

        let sub_script = args[cmd_start_idx..].join(" ");
        let plan = match compile_script(&sub_script) {
            Ok(p) => p,
            Err(e) => {
                ctx.write_stderr(format!("esh: retry: parse error: {}\n", e).as_bytes());
                return Ok(2);
            }
        };

        let mut last_code = 0;
        let mut current_delay = delay;

        for attempt in 1..=attempts {
            if ctx.is_cancelled() {
                return Ok(130);
            }

            match plan.execute(ctx).await {
                Ok(0) => return Ok(0),
                Ok(code) => {
                    last_code = code;
                }
                Err(e) => {
                    last_code = e.exit_code();
                }
            }

            if attempt < attempts {
                tokio::time::sleep(current_delay).await;
                match Duration::try_from_secs_f64(current_delay.as_secs_f64() * factor) {
                    Ok(next_delay) => current_delay = next_delay,
                    Err(_) => {
                        ctx.write_stderr(b"esh: retry: backoff delay is out of range\n");
                        return Ok(2);
                    }
                }
            }
        }

        Ok(last_code)
    })
}

pub fn wait<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        // ESH has no fire-and-forget process jobs. Structured `&|` branches
        // have already joined before execution reaches `wait`.
        if let Some(invalid) = args.iter().find(|operand| {
            operand.is_empty() || !operand.bytes().all(|byte| byte.is_ascii_digit())
        }) {
            ctx.write_stderr(
                format!("esh: wait: unsupported process identifier '{invalid}'\n").as_bytes(),
            );
            return Ok(2);
        }

        if !args.is_empty() {
            // Every syntactically valid PID is unknown to ESH's execution
            // environment because it does not create asynchronous job PIDs.
            return Ok(127);
        }

        // Yield cooperatively even though there are no pending jobs to wait
        // for, matching the successful no-operand shell builtin contract.
        tokio::task::yield_now().await;
        Ok(0)
    })
}

pub fn trap<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            for signal in [TrapSignal::Exit, TrapSignal::Err, TrapSignal::Interrupt] {
                if let Some(action) = ctx.get_trap_action(signal) {
                    write_trap_listing(ctx, signal, &action);
                }
            }
            return Ok(0);
        }

        let listing_signals: Option<Vec<TrapSignal>> =
            args.iter().map(|value| TrapSignal::parse(value)).collect();
        if let Some(signals) = listing_signals {
            for signal in signals {
                if let Some(action) = ctx.get_trap_action(signal) {
                    write_trap_listing(ctx, signal, &action);
                }
            }
            return Ok(0);
        }

        if args.len() < 2 {
            ctx.write_stderr(b"esh: trap: usage: trap <action> <SIGNAL...>\n");
            return Ok(2);
        }
        let handler_str = &args[0];
        let mut signals = Vec::with_capacity(args.len() - 1);
        for sig_str in &args[1..] {
            match TrapSignal::parse(sig_str) {
                Some(signal) => signals.push(signal),
                None => {
                    ctx.write_stderr(format!("esh: trap: unknown signal '{sig_str}'\n").as_bytes());
                    return Ok(2);
                }
            }
        }

        if handler_str == "-" {
            for signal in signals {
                ctx.clear_trap(signal);
            }
            return Ok(0);
        }

        let plan = match compile_script(handler_str) {
            Ok(plan) => plan,
            Err(error) => {
                ctx.write_stderr(
                    format!("esh: trap: invalid handler script: {error}\n").as_bytes(),
                );
                return Ok(2);
            }
        };

        for signal in signals {
            // An empty action is an installed no-op handler (ignore), while `-`
            // removes the handler and restores ESH's default behavior.
            ctx.set_trap(signal, plan.clone(), handler_str.clone());
        }
        Ok(0)
    })
}

fn write_trap_listing(ctx: &mut ExecutionContext, signal: TrapSignal, action: &str) {
    let quoted = action.replace('\'', "'\\''");
    let signal = match signal {
        TrapSignal::Exit => "EXIT",
        TrapSignal::Err => "ERR",
        TrapSignal::Interrupt => "INT",
    };
    ctx.write_stdout(format!("trap '{}' {}\n", quoted, signal).as_bytes());
}

pub fn time<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: time: usage: time [-p] <command...>\n");
            return Ok(1);
        }

        let mut posix = false;
        let mut cmd_start = 0;
        for (i, arg) in args.iter().enumerate() {
            if arg == "-p" || arg == "--portability" {
                posix = true;
            } else if !arg.starts_with('-') {
                cmd_start = i;
                break;
            }
        }

        let sub_script = args[cmd_start..].join(" ");
        let plan = match compile_script(&sub_script) {
            Ok(p) => p,
            Err(e) => {
                ctx.write_stderr(format!("esh: time: parse error: {}\n", e).as_bytes());
                return Ok(2);
            }
        };

        let start = std::time::Instant::now();
        let res = plan.execute(ctx).await;
        let elapsed = start.elapsed();

        if posix {
            ctx.write_stderr(format!("real {:.3}\n", elapsed.as_secs_f64()).as_bytes());
        } else {
            ctx.write_stderr(format!("\nreal\t{:.3}s\n", elapsed.as_secs_f64()).as_bytes());
        }

        res
    })
}

pub fn assert<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: assert: missing assertion command\n");
            return Ok(1);
        }

        let (expression, message) = if let Some(separator) = args.iter().position(|arg| arg == "--")
        {
            (&args[..separator], Some(args[separator + 1..].join(" ")))
        } else if args.first().is_some_and(|arg| arg == "[") {
            if let Some(closing) = args.iter().position(|arg| arg == "]") {
                let message = (closing + 1 < args.len()).then(|| args[closing + 1..].join(" "));
                (&args[..=closing], message)
            } else {
                (args, None)
            }
        } else {
            (args, None)
        };
        let sub_script = expression.join(" ");
        let plan = match compile_script(&sub_script) {
            Ok(p) => p,
            Err(e) => {
                ctx.write_stderr(format!("esh: assert: parse error: {}\n", e).as_bytes());
                return Ok(2);
            }
        };

        match plan.execute(ctx).await {
            Ok(0) => Ok(0),
            Ok(code) => {
                let detail = message
                    .as_deref()
                    .filter(|message| !message.is_empty())
                    .unwrap_or(&sub_script);
                ctx.write_stderr(
                    format!("esh: assertion failed: {} (exit code {})\n", detail, code).as_bytes(),
                );
                Ok(1)
            }
            Err(e) => {
                let detail = message
                    .as_deref()
                    .filter(|message| !message.is_empty())
                    .unwrap_or(&sub_script);
                ctx.write_stderr(format!("esh: assertion failed: {}: {}\n", detail, e).as_bytes());
                Ok(1)
            }
        }
    })
}

pub fn local<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        for arg in args {
            if let Some((k, v)) = arg.split_once('=') {
                ctx.set_local_var(k, v.to_string());
            } else {
                ctx.set_local_var(arg, String::new());
            }
        }
        Ok(0)
    })
}

pub fn read<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut raw = false;
        let mut prompt: Option<&str> = None;
        let mut delimiter: u8 = b'\n';
        let mut timeout_dur: Option<std::time::Duration> = None;
        let mut default_val: Option<String> = None;
        let mut defaults_var: Option<String> = None;
        let mut is_secret = false;
        let mut mask_char: Option<char> = None;
        let mut is_confirm = false;
        let mut is_menu = false;
        let mut array_target: Option<String> = None;
        let mut positional_args = Vec::new();

        let mut iter = args.iter().peekable();
        let mut parsing_options = true;
        while let Some(arg) = iter.next() {
            if parsing_options && arg == "--" {
                parsing_options = false;
                continue;
            }
            if !parsing_options {
                positional_args.push(arg.clone());
                continue;
            }
            match arg.as_str() {
                "-r" => raw = true,
                "-s" | "--silent" => is_secret = true,
                "-c" | "--confirm" => is_confirm = true,
                "--menu" => is_menu = true,
                "-p" => {
                    let Some(p) = iter.next() else {
                        ctx.write_stderr(b"esh: read: -p requires a prompt\n");
                        return Ok(2);
                    };
                    prompt = Some(p.as_str());
                }
                "-d" => {
                    let Some(d) = iter.next() else {
                        ctx.write_stderr(b"esh: read: -d requires a delimiter\n");
                        return Ok(2);
                    };
                    // POSIX.1-2024 defines an empty -d operand as a NUL
                    // delimiter. ESH uses the first byte when the operand
                    // contains multiple bytes, whose POSIX behavior is
                    // unspecified.
                    delimiter = d.as_bytes().first().copied().unwrap_or(0);
                }
                "-t" => {
                    let Some(t) = iter.next() else {
                        ctx.write_stderr(b"esh: read: -t requires a duration\n");
                        return Ok(2);
                    };
                    if let Ok(duration) = parse_duration(t) {
                        timeout_dur = Some(duration);
                    } else {
                        ctx.write_stderr(b"esh: read: invalid timeout duration\n");
                        return Ok(2);
                    }
                }
                "-m" | "--mask" => {
                    let Some(m) = iter.next().filter(|value| !value.is_empty()) else {
                        ctx.write_stderr(b"esh: read: -m requires a mask character\n");
                        return Ok(2);
                    };
                    mask_char = m.chars().next();
                }
                "-a" => {
                    if let Some(a) = iter.next() {
                        array_target = Some(a.clone());
                    } else {
                        ctx.write_stderr(b"esh: read: -a requires an argument\n");
                        return Ok(2);
                    }
                }
                "--default" => {
                    let Some(d) = iter.next() else {
                        ctx.write_stderr(b"esh: read: --default requires a value\n");
                        return Ok(2);
                    };
                    default_val = Some(d.clone());
                }
                "--defaults" => {
                    let Some(d) = iter.next() else {
                        ctx.write_stderr(b"esh: read: --defaults requires a variable name\n");
                        return Ok(2);
                    };
                    defaults_var = Some(d.clone());
                }
                other if other.starts_with("-p") && other.len() > 2 => {
                    prompt = Some(&other[2..]);
                }
                other if other.starts_with("-d") && other.len() > 2 => {
                    delimiter = other[2..].bytes().next().unwrap_or(0);
                }
                other if other.starts_with("-t") && other.len() > 2 => {
                    if let Ok(d) = parse_duration(&other[2..]) {
                        timeout_dur = Some(d);
                    } else {
                        ctx.write_stderr(b"esh: read: invalid timeout duration\n");
                        return Ok(2);
                    }
                }
                other if other.starts_with("-m") && other.len() > 2 => {
                    mask_char = other[2..].chars().next();
                }
                other if other.starts_with('-') && other.len() > 1 && !other.starts_with("--") => {
                    for ch in other.chars().skip(1) {
                        match ch {
                            'r' => raw = true,
                            's' => is_secret = true,
                            'c' => is_confirm = true,
                            _ => {
                                ctx.write_stderr(
                                    format!("esh: read: unknown option '-{}'\n", ch).as_bytes(),
                                );
                                return Ok(2);
                            }
                        }
                    }
                }
                other if other.starts_with("--") => {
                    ctx.write_stderr(format!("esh: read: unknown option '{other}'\n").as_bytes());
                    return Ok(2);
                }
                _ => {
                    positional_args.push(arg.clone());
                }
            }
        }

        // Validate mutually exclusive / invalid option combinations
        if is_secret && default_val.is_some() {
            ctx.write_stderr(b"esh: read: --default cannot be used with --silent\n");
            return Ok(2);
        }
        if defaults_var.is_some() && !is_menu {
            ctx.write_stderr(b"esh: read: --defaults is only valid with --menu\n");
            return Ok(2);
        }
        if default_val.is_some() && defaults_var.is_some() {
            ctx.write_stderr(b"esh: read: --default and --defaults are mutually exclusive\n");
            return Ok(2);
        }
        if let Some(name) = array_target
            .as_deref()
            .filter(|name| !is_shell_identifier(name))
        {
            ctx.write_stderr(format!("esh: read: invalid identifier: {name}\n").as_bytes());
            return Ok(2);
        }
        if let Some(name) = defaults_var
            .as_deref()
            .filter(|name| !is_shell_identifier(name))
        {
            ctx.write_stderr(format!("esh: read: invalid identifier: {name}\n").as_bytes());
            return Ok(2);
        }
        if is_confirm || is_secret {
            let target = positional_args
                .first()
                .map(String::as_str)
                .unwrap_or("REPLY");
            if !is_shell_identifier(target) {
                ctx.write_stderr(format!("esh: read: invalid identifier: {target}\n").as_bytes());
                return Ok(2);
            }
        }

        // ==========================================
        // 1. MENU MODE
        // ==========================================
        if is_menu {
            let mut menu_items: Vec<(String, String)> = Vec::new();
            let target_var: String;

            // Check if stdin has piped data for menu options
            let piped_stdin = ctx.take_stdin();
            if let Some(stdin_bytes) = piped_stdin {
                target_var = if let Some(ref arr_t) = array_target {
                    arr_t.clone()
                } else if let Some(first_pos) = positional_args.first() {
                    first_pos.clone()
                } else {
                    "REPLY".to_string()
                };
                if !is_shell_identifier(&target_var) {
                    ctx.set_stdin(stdin_bytes);
                    ctx.write_stderr(
                        format!("esh: read: invalid identifier: {target_var}\n").as_bytes(),
                    );
                    return Ok(2);
                }
                let text = String::from_utf8_lossy(&stdin_bytes);
                for line in text.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        menu_items.push((trimmed.to_string(), trimmed.to_string()));
                    }
                }
            } else {
                // Menu source is positional
                let (source_spec, tgt) = if let Some(ref arr_t) = array_target {
                    if let Some(src) = positional_args.first() {
                        (src.clone(), arr_t.clone())
                    } else {
                        ctx.write_stderr(b"esh: read: missing menu source\n");
                        return Ok(2);
                    }
                } else if positional_args.len() >= 2 {
                    (positional_args[0].clone(), positional_args[1].clone())
                } else if positional_args.len() == 1 {
                    (positional_args[0].clone(), "REPLY".to_string())
                } else {
                    ctx.write_stderr(b"esh: read: missing menu source and target\n");
                    return Ok(2);
                };

                target_var = tgt;
                if !is_shell_identifier(&target_var) {
                    ctx.write_stderr(
                        format!("esh: read: invalid identifier: {target_var}\n").as_bytes(),
                    );
                    return Ok(2);
                }

                // Check if source_spec is variable or inline
                if !source_spec.contains(' ')
                    && !source_spec.contains('\t')
                    && !source_spec.contains('=')
                {
                    // Variable reference
                    if let Some(shell_val) = ctx.get_shell_val(&source_spec) {
                        match shell_val {
                            crate::runtime::context::ShellValue::Map(map) => {
                                let mut sorted_keys: Vec<String> = map.keys().cloned().collect();
                                sorted_keys.sort();
                                for k in sorted_keys {
                                    let label = map.get(&k).cloned().unwrap_or_default();
                                    menu_items.push((k, label));
                                }
                            }
                            crate::runtime::context::ShellValue::Array(arr) => {
                                for item in arr {
                                    menu_items.push((item.clone(), item.clone()));
                                }
                            }
                            crate::runtime::context::ShellValue::String(_) => {
                                ctx.write_stderr(
                                    format!(
                                        "esh: read: variable '{}' is not an array or map\n",
                                        source_spec
                                    )
                                    .as_bytes(),
                                );
                                return Ok(2);
                            }
                        }
                    } else {
                        ctx.write_stderr(
                            format!("esh: read: variable '{}' not found\n", source_spec).as_bytes(),
                        );
                        return Ok(2);
                    }
                } else {
                    // Inline specification
                    match parse_inline_menu_spec(&source_spec) {
                        Ok(items) => menu_items = items,
                        Err(e) => {
                            ctx.write_stderr(format!("esh: read: {}\n", e).as_bytes());
                            return Ok(2);
                        }
                    }
                }
            }

            if menu_items.is_empty() {
                ctx.write_stderr(b"esh: read: menu source contains no selectable items\n");
                return Ok(2);
            }

            let prompt_text = prompt.unwrap_or("Select choice");
            let mut session = ctx.take_interactive_session();

            if !session.is_tty() {
                ctx.write_stderr(b"esh: read: interactive terminal required\n");
                return Ok(2);
            }

            if array_target.is_some() {
                // Multi-selection menu
                let mut pre_defaults = Vec::new();
                if let Some(ref def_var) = defaults_var {
                    if let Some(shell_val) = ctx.get_shell_val(def_var) {
                        match shell_val {
                            crate::runtime::context::ShellValue::Array(arr) => {
                                let mut seen = std::collections::HashSet::new();
                                for k in arr {
                                    if !seen.insert(k.clone()) {
                                        ctx.write_stderr(
                                            b"esh: read: duplicate key in menu --defaults array\n",
                                        );
                                        return Ok(2);
                                    }
                                    if !menu_items.iter().any(|(item_k, _)| *item_k == *k) {
                                        ctx.write_stderr(
                                            format!("esh: read: default key '{}' not in menu\n", k)
                                                .as_bytes(),
                                        );
                                        return Ok(2);
                                    }
                                    pre_defaults.push(k);
                                }
                            }
                            _ => {
                                ctx.write_stderr(
                                    b"esh: read: --defaults must name an array variable for multi-select menu\n",
                                );
                                return Ok(2);
                            }
                        }
                    } else {
                        ctx.write_stderr(
                            format!("esh: read: defaults variable '{}' not found\n", def_var)
                                .as_bytes(),
                        );
                        return Ok(2);
                    }
                }

                let items_ref: Vec<(&str, &str)> = menu_items
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                let defs_ref: Vec<&str> = pre_defaults.iter().map(|s| s.as_str()).collect();

                let mut prompt_builder = evin_ui::prompt::multiselect(prompt_text)
                    .items(&items_ref)
                    .defaults(&defs_ref);
                if let Some(dur) = timeout_dur {
                    prompt_builder = prompt_builder.timeout(dur);
                }

                let result = prompt_builder
                    .interact_with(&mut *session)
                    .map_err(|e| ExecError::IoError(e.to_string()))?;

                match result {
                    evin_ui::prompt::PromptResult::Selected(selected_vals) => {
                        ctx.set_scoped_val(
                            &target_var,
                            crate::runtime::context::ShellValue::Array(selected_vals),
                        );
                        return Ok(0);
                    }
                    evin_ui::prompt::PromptResult::Cancelled => {
                        return Ok(130);
                    }
                    evin_ui::prompt::PromptResult::TimedOut => {
                        return Ok(1);
                    }
                    evin_ui::prompt::PromptResult::Error(e) => {
                        ctx.write_stderr(format!("esh: read: {}\n", e).as_bytes());
                        return Ok(2);
                    }
                }
            } else {
                // Single-selection menu
                let mut single_default: Option<String> = None;
                if let Some(ref def_var) = defaults_var {
                    if let Some(val) = ctx.get_var(def_var) {
                        if !menu_items.iter().any(|(k, _)| *k == val) {
                            ctx.write_stderr(
                                format!("esh: read: default key '{}' not in menu\n", val)
                                    .as_bytes(),
                            );
                            return Ok(2);
                        }
                        single_default = Some(val);
                    } else {
                        ctx.write_stderr(
                            format!("esh: read: defaults variable '{}' not found\n", def_var)
                                .as_bytes(),
                        );
                        return Ok(2);
                    }
                }

                let items_ref: Vec<(&str, &str)> = menu_items
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();

                let mut prompt_builder = evin_ui::prompt::select(prompt_text).items(&items_ref);
                if let Some(ref d) = single_default {
                    prompt_builder = prompt_builder.default_key(d.as_str());
                }
                if let Some(dur) = timeout_dur {
                    prompt_builder = prompt_builder.timeout(dur);
                }

                let result = prompt_builder
                    .interact_with(&mut *session)
                    .map_err(|e| ExecError::IoError(e.to_string()))?;

                match result {
                    evin_ui::prompt::PromptResult::Selected(selected_val) => {
                        ctx.set_scoped_var(&target_var, selected_val);
                        return Ok(0);
                    }
                    evin_ui::prompt::PromptResult::Cancelled => {
                        return Ok(130);
                    }
                    evin_ui::prompt::PromptResult::TimedOut => {
                        return Ok(1);
                    }
                    evin_ui::prompt::PromptResult::Error(e) => {
                        ctx.write_stderr(format!("esh: read: {}\n", e).as_bytes());
                        return Ok(2);
                    }
                }
            }
        }

        // ==========================================
        // 2. CONFIRMATION MODE (-c / --confirm)
        // ==========================================
        if is_confirm {
            let target_var = positional_args
                .first()
                .cloned()
                .unwrap_or_else(|| "REPLY".to_string());
            let prompt_text = prompt.unwrap_or("Are you sure?");

            let default_bool = default_val.as_deref().map(|d| {
                d.eq_ignore_ascii_case("y")
                    || d.eq_ignore_ascii_case("yes")
                    || d.eq_ignore_ascii_case("true")
                    || d == "1"
            });

            let mut session = ctx.take_interactive_session();
            if !session.is_tty() {
                ctx.write_stderr(b"esh: read: interactive terminal required\n");
                return Ok(2);
            }

            let mut prompt_builder = evin_ui::prompt::confirm(prompt_text);
            if let Some(def) = default_bool {
                prompt_builder = prompt_builder.default_confirm(def);
            }
            if let Some(dur) = timeout_dur {
                prompt_builder = prompt_builder.timeout(dur);
            }

            let result = prompt_builder
                .interact_with(&mut *session)
                .map_err(|e| ExecError::IoError(e.to_string()))?;

            match result {
                evin_ui::prompt::PromptResult::Selected(confirmed) => {
                    let val_str = if confirmed { "yes" } else { "no" };
                    ctx.set_scoped_var(&target_var, val_str.to_string());
                    return if confirmed { Ok(0) } else { Ok(1) };
                }
                evin_ui::prompt::PromptResult::Cancelled => {
                    return Ok(130);
                }
                evin_ui::prompt::PromptResult::TimedOut => {
                    return Ok(1);
                }
                evin_ui::prompt::PromptResult::Error(e) => {
                    ctx.write_stderr(format!("esh: read: {}\n", e).as_bytes());
                    return Ok(2);
                }
            }
        }

        // ==========================================
        // 3. SECRET MODE (-s / --silent)
        // ==========================================
        if is_secret {
            let target_var = positional_args
                .first()
                .cloned()
                .unwrap_or_else(|| "REPLY".to_string());
            let prompt_text = prompt.unwrap_or("Enter secret");

            let mut session = ctx.take_interactive_session();
            if !session.is_tty() {
                ctx.write_stderr(b"esh: read: interactive terminal required\n");
                return Ok(2);
            }

            let mut prompt_builder = evin_ui::prompt::secret(prompt_text);
            if let Some(m) = mask_char {
                prompt_builder = prompt_builder.mask(m);
            }
            if let Some(dur) = timeout_dur {
                prompt_builder = prompt_builder.timeout(dur);
            }

            let result = prompt_builder
                .interact_with(&mut *session)
                .map_err(|e| ExecError::IoError(e.to_string()))?;

            match result {
                evin_ui::prompt::PromptResult::Selected(secret_val) => {
                    ctx.set_scoped_var(&target_var, secret_val);
                    return Ok(0);
                }
                evin_ui::prompt::PromptResult::Cancelled => {
                    return Ok(130);
                }
                evin_ui::prompt::PromptResult::TimedOut => {
                    return Ok(1);
                }
                evin_ui::prompt::PromptResult::Error(e) => {
                    ctx.write_stderr(format!("esh: read: {}\n", e).as_bytes());
                    return Ok(2);
                }
            }
        }

        // ==========================================
        // 4. STANDARD TEXT INPUT / POSIX READ
        // ==========================================
        if let Some(p) = prompt {
            ctx.write_stderr(p.as_bytes());
        }

        let var_names = if positional_args.is_empty() {
            vec!["REPLY".to_string()]
        } else {
            positional_args
        };

        if let Some(name) = var_names.iter().find(|name| !is_shell_identifier(name)) {
            ctx.write_stderr(format!("esh: read: invalid identifier: {name}\n").as_bytes());
            return Ok(2);
        }

        let read_future = async {
            match ctx.take_stdin() {
                Some(buf) => {
                    let mut line_bytes = Vec::new();
                    let mut rest_bytes = Vec::new();
                    let mut found_delim = false;

                    let mut i = 0;
                    while i < buf.len() {
                        let b = buf[i];
                        if !found_delim {
                            if !raw && b == b'\\' && i + 1 < buf.len() {
                                let next_b = buf[i + 1];
                                if next_b == b'\n' {
                                    // Line continuation: skip both \ and \n
                                    i += 2;
                                    continue;
                                } else {
                                    line_bytes.push(b'\\');
                                    line_bytes.push(next_b);
                                    i += 2;
                                    continue;
                                }
                            }

                            if b == delimiter {
                                found_delim = true;
                            } else {
                                line_bytes.push(b);
                            }
                        } else {
                            rest_bytes.push(b);
                        }
                        i += 1;
                    }

                    ctx.set_stdin(rest_bytes);
                    if line_bytes.is_empty() && !found_delim {
                        return Err(ExecError::Custom("EOF".to_string()));
                    }
                    Ok((
                        String::from_utf8_lossy(&line_bytes).to_string(),
                        found_delim,
                        false,
                    ))
                }
                None => {
                    #[cfg(unix)]
                    {
                        // SAFETY: isatty only inspects the standard input fd.
                        if unsafe { libc::isatty(libc::STDIN_FILENO) == 1 } {
                            let (line, found, timed_out, rest) = read_tty_line_with_timeout(
                                raw,
                                delimiter,
                                timeout_dur,
                                ctx.token().clone(),
                            )
                            .await?;
                            if !rest.is_empty() {
                                ctx.set_stdin(rest);
                            }
                            Ok((line, found, timed_out))
                        } else if let Some(timeout) = timeout_dur {
                            read_stdin_line_with_timeout(raw, delimiter, timeout).await
                        } else {
                            let mut stdin = tokio::io::stdin();
                            read_async_line(&mut stdin, raw, delimiter, None).await
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        let mut stdin = tokio::io::stdin();
                        read_async_line(&mut stdin, raw, delimiter, timeout_dur).await
                    }
                }
            }
        };

        let line_res = read_future.await;

        let (line, found_delimiter, timed_out) = match (line_res, default_val) {
            (Ok((line, found_delimiter, timed_out)), Some(default))
                if line.is_empty() && !timed_out =>
            {
                (default, found_delimiter, false)
            }
            (Ok((line, found_delimiter, timed_out)), _) => (line, found_delimiter, timed_out),
            (Err(_), Some(default)) => (default, true, false),
            (Err(_), None) => return Ok(1), // EOF or Error
        };

        if timed_out && line.is_empty() {
            return Ok(1);
        }

        let ifs = ctx.get_var("IFS").unwrap_or_else(|| " \t\n".to_string());
        for (var, value) in split_read_values(&line, &ifs, &var_names, raw) {
            ctx.set_scoped_var(&var, value);
        }

        Ok(if found_delimiter && !timed_out { 0 } else { 1 })
    })
}

async fn read_async_line<R>(
    reader: &mut R,
    raw: bool,
    delimiter: u8,
    timeout: Option<std::time::Duration>,
) -> Result<(String, bool, bool), ExecError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;

    let deadline = timeout.map(|duration| tokio::time::Instant::now() + duration);
    let mut line_bytes = Vec::new();
    let mut found_delimiter = false;
    let mut escaping = false;
    let mut timed_out = false;
    let mut byte = [0u8; 1];

    loop {
        let count = if let Some(deadline) = deadline {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                timed_out = true;
                break;
            }
            match tokio::time::timeout_at(deadline, reader.read(&mut byte)).await {
                Ok(result) => result.map_err(|error| ExecError::IoError(error.to_string()))?,
                Err(_) => {
                    timed_out = true;
                    break;
                }
            }
        } else {
            reader
                .read(&mut byte)
                .await
                .map_err(|error| ExecError::IoError(error.to_string()))?
        };

        if count == 0 {
            if escaping {
                line_bytes.push(b'\\');
            }
            break;
        }

        let current = byte[0];
        if !raw && escaping {
            if current != b'\n' {
                line_bytes.push(b'\\');
                line_bytes.push(current);
            }
            escaping = false;
        } else if !raw && current == b'\\' {
            escaping = true;
        } else if current == delimiter {
            found_delimiter = true;
            break;
        } else {
            line_bytes.push(current);
        }
    }

    if line_bytes.is_empty() && !found_delimiter && !timed_out {
        return Err(ExecError::Custom("EOF".to_string()));
    }
    if timed_out && escaping {
        line_bytes.push(b'\\');
    }

    Ok((
        String::from_utf8_lossy(&line_bytes).to_string(),
        found_delimiter,
        timed_out,
    ))
}

#[cfg(unix)]
async fn read_tty_line_with_timeout(
    raw: bool,
    delimiter: u8,
    timeout: Option<std::time::Duration>,
    token: tokio_util::sync::CancellationToken,
) -> Result<(String, bool, bool, Vec<u8>), ExecError> {
    tokio::task::spawn_blocking(move || read_tty_line_blocking(raw, delimiter, timeout, token))
        .await
        .map_err(|error| ExecError::IoError(error.to_string()))?
}

#[cfg(unix)]
fn read_tty_line_blocking(
    raw: bool,
    delimiter: u8,
    timeout: Option<std::time::Duration>,
    token: tokio_util::sync::CancellationToken,
) -> Result<(String, bool, bool, Vec<u8>), ExecError> {
    use std::time::Instant;

    let deadline = timeout.map(|duration| Instant::now() + duration);
    let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
    // SAFETY: termios points to writable storage for tcgetattr.
    let canonical = unsafe {
        libc::tcgetattr(libc::STDIN_FILENO, termios.as_mut_ptr()) == 0
            && termios.assume_init().c_lflag & libc::ICANON != 0
    };
    let mut line = Vec::new();
    let mut remainder = Vec::new();
    let mut escaping = false;
    let mut found_delimiter = false;
    let mut buffer = [0u8; 4096];

    loop {
        if token.is_cancelled() {
            return Err(ExecError::Custom("Terminal read cancelled".to_string()));
        }
        let wait_ms = if let Some(deadline) = deadline {
            let now = Instant::now();
            if now >= deadline {
                if escaping {
                    line.push(b'\\');
                }
                return Ok((
                    String::from_utf8_lossy(&line).into_owned(),
                    found_delimiter,
                    true,
                    remainder,
                ));
            }
            deadline
                .saturating_duration_since(now)
                .as_millis()
                .saturating_add(1)
                .clamp(1, 50) as i32
        } else {
            50
        };
        let mut descriptor = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: descriptor points to one initialized pollfd for the call.
        let ready = unsafe { libc::poll(&mut descriptor, 1, wait_ms) };
        if ready == 0 {
            continue;
        }
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(ExecError::IoError(error.to_string()));
        }

        // SAFETY: buffer is writable and poll reported stdin readiness.
        let count =
            unsafe { libc::read(libc::STDIN_FILENO, buffer.as_mut_ptr().cast(), buffer.len()) };
        if count < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(ExecError::IoError(error.to_string()));
        }
        if count == 0 {
            break;
        }

        let count = count as usize;
        let mut ended_line = false;
        for (index, current) in buffer[..count].iter().copied().enumerate() {
            if !raw && escaping {
                if current != b'\n' {
                    line.push(b'\\');
                    line.push(current);
                }
                escaping = false;
            } else if !raw && current == b'\\' {
                escaping = true;
            } else if current == delimiter {
                found_delimiter = true;
                remainder.extend_from_slice(&buffer[index + 1..count]);
                break;
            } else {
                line.push(current);
            }
            if current == b'\n' {
                ended_line = true;
            }
        }
        if found_delimiter {
            break;
        }

        // In canonical mode, VEOF after text makes read(2) return a short
        // buffer without a newline. Enter completes a line with newline.
        if canonical && count < buffer.len() && !ended_line {
            if escaping {
                line.push(b'\\');
            }
            break;
        }
    }

    if line.is_empty() && !found_delimiter {
        return Err(ExecError::Custom("EOF".to_string()));
    }
    Ok((
        String::from_utf8_lossy(&line).into_owned(),
        found_delimiter,
        false,
        remainder,
    ))
}

#[cfg(unix)]
async fn read_stdin_line_with_timeout(
    raw: bool,
    delimiter: u8,
    timeout: std::time::Duration,
) -> Result<(String, bool, bool), ExecError> {
    tokio::task::spawn_blocking(move || read_stdin_line_blocking(raw, delimiter, timeout))
        .await
        .map_err(|error| ExecError::IoError(error.to_string()))?
}

#[cfg(unix)]
fn read_stdin_line_blocking(
    raw: bool,
    delimiter: u8,
    timeout: std::time::Duration,
) -> Result<(String, bool, bool), ExecError> {
    use std::time::Instant;

    let deadline = Instant::now() + timeout;
    let mut line_bytes = Vec::new();
    let mut found_delimiter = false;
    let mut escaping = false;
    let mut byte = [0u8; 1];

    loop {
        let now = Instant::now();
        if now >= deadline {
            if escaping {
                line_bytes.push(b'\\');
            }
            return Ok((
                String::from_utf8_lossy(&line_bytes).to_string(),
                found_delimiter,
                true,
            ));
        }
        let remaining = deadline.saturating_duration_since(now);
        let timeout_ms = remaining
            .as_millis()
            .saturating_add(u128::from(
                !remaining.subsec_nanos().is_multiple_of(1_000_000),
            ))
            .clamp(1, i32::MAX as u128) as i32;
        let mut descriptor = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: descriptor points to one initialized pollfd for the duration
        // of the call; the OS only reads its fd/events and writes revents.
        let poll_result = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
        if poll_result == 0 {
            if escaping {
                line_bytes.push(b'\\');
            }
            return Ok((
                String::from_utf8_lossy(&line_bytes).to_string(),
                found_delimiter,
                true,
            ));
        }
        if poll_result < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(ExecError::IoError(error.to_string()));
        }

        // SAFETY: byte is a writable one-byte buffer and stdin is an open file
        // descriptor. poll reported readiness before this synchronous read.
        let count = unsafe { libc::read(libc::STDIN_FILENO, byte.as_mut_ptr().cast(), byte.len()) };
        if count < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(ExecError::IoError(error.to_string()));
        }
        if count == 0 {
            if escaping {
                line_bytes.push(b'\\');
            }
            break;
        }

        let current = byte[0];
        if !raw && escaping {
            if current != b'\n' {
                line_bytes.push(b'\\');
                line_bytes.push(current);
            }
            escaping = false;
        } else if !raw && current == b'\\' {
            escaping = true;
        } else if current == delimiter {
            found_delimiter = true;
            break;
        } else {
            line_bytes.push(current);
        }
    }

    if line_bytes.is_empty() && !found_delimiter {
        return Err(ExecError::Custom("EOF".to_string()));
    }
    Ok((
        String::from_utf8_lossy(&line_bytes).to_string(),
        found_delimiter,
        false,
    ))
}

fn split_read_values(line: &str, ifs: &str, names: &[String], raw: bool) -> Vec<(String, String)> {
    let mut assignments = Vec::with_capacity(names.len());
    if names.is_empty() {
        return assignments;
    }

    let chars: Vec<char> = line.chars().collect();
    let ifs_chars: Vec<char> = ifs.chars().collect();
    let is_ifs = |ch: char| ifs_chars.contains(&ch);
    let is_ifs_whitespace = |ch: char| is_ifs(ch) && matches!(ch, ' ' | '\t' | '\n');
    let escaped = |index: usize| {
        if raw {
            return false;
        }
        let mut slashes = 0;
        let mut cursor = index;
        while cursor > 0 && chars[cursor - 1] == '\\' {
            slashes += 1;
            cursor -= 1;
        }
        slashes % 2 == 1
    };
    let unescape = |slice: &[char]| {
        if raw {
            return slice.iter().collect();
        }
        let mut result = String::new();
        let mut cursor = 0;
        while cursor < slice.len() {
            if slice[cursor] == '\\' && cursor + 1 < slice.len() {
                cursor += 1;
            }
            result.push(slice[cursor]);
            cursor += 1;
        }
        result
    };
    let mut position = 0;

    for name in names.iter().take(names.len().saturating_sub(1)) {
        while position < chars.len() && is_ifs_whitespace(chars[position]) && !escaped(position) {
            position += 1;
        }

        if position == chars.len() {
            assignments.push((name.clone(), String::new()));
            continue;
        }

        let mut consumed_delimiter = false;
        if is_ifs(chars[position]) && !escaped(position) && !is_ifs_whitespace(chars[position]) {
            assignments.push((name.clone(), String::new()));
            position += 1;
            consumed_delimiter = true;
        } else {
            let start = position;
            while position < chars.len() && (!is_ifs(chars[position]) || escaped(position)) {
                position += 1;
            }
            assignments.push((name.clone(), unescape(&chars[start..position])));
        }

        if !consumed_delimiter
            && position < chars.len()
            && is_ifs_whitespace(chars[position])
            && !escaped(position)
        {
            while position < chars.len() && is_ifs_whitespace(chars[position]) && !escaped(position)
            {
                position += 1;
            }
            if position < chars.len()
                && is_ifs(chars[position])
                && !escaped(position)
                && !is_ifs_whitespace(chars[position])
            {
                position += 1;
            }
        } else if !consumed_delimiter
            && position < chars.len()
            && is_ifs(chars[position])
            && !escaped(position)
        {
            position += 1;
        }
        while position < chars.len() && is_ifs_whitespace(chars[position]) && !escaped(position) {
            position += 1;
        }
    }

    let last_name = names.last().unwrap();
    while position < chars.len() && is_ifs_whitespace(chars[position]) && !escaped(position) {
        position += 1;
    }
    let mut end = chars.len();
    while end > position && is_ifs_whitespace(chars[end - 1]) && !escaped(end - 1) {
        end -= 1;
    }
    assignments.push((last_name.clone(), unescape(&chars[position..end])));
    assignments
}

fn parse_inline_menu_spec(spec: &str) -> Result<Vec<(String, String)>, String> {
    let mut items = Vec::new();
    let chars: Vec<char> = spec.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }

        let key_start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' {
            i += 1;
        }
        if i >= chars.len() || chars[i] != '=' {
            let key: String = chars[key_start..i].iter().collect();
            items.push((key.clone(), key));
            continue;
        }

        let key: String = chars[key_start..i].iter().collect();
        i += 1; // skip '='

        if i >= chars.len() {
            items.push((key, String::new()));
            break;
        }

        let mut label = String::new();
        if chars[i] == '\'' {
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\'
                    && i + 1 < chars.len()
                    && (chars[i + 1] == '\'' || chars[i + 1] == '\\')
                {
                    label.push(chars[i + 1]);
                    i += 2;
                } else if chars[i] == '\'' {
                    i += 1;
                    break;
                } else {
                    label.push(chars[i]);
                    i += 1;
                }
            }
        } else if chars[i] == '"' {
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\'
                    && i + 1 < chars.len()
                    && (chars[i + 1] == '"' || chars[i + 1] == '\\')
                {
                    label.push(chars[i + 1]);
                    i += 2;
                } else if chars[i] == '"' {
                    i += 1;
                    break;
                } else {
                    label.push(chars[i]);
                    i += 1;
                }
            }
        } else {
            while i < chars.len() && !chars[i].is_whitespace() {
                label.push(chars[i]);
                i += 1;
            }
        }

        items.push((key, label));
    }

    Ok(items)
}

pub fn declare<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut is_assoc_map = false;
        let mut is_indexed_array = false;
        let mut is_export = false;
        let mut parsing_options = true;

        let mut names_and_values = Vec::new();

        for arg in args {
            if parsing_options && arg == "--" {
                parsing_options = false;
                continue;
            }
            if parsing_options && arg.starts_with('-') {
                let supported = match arg.as_str() {
                    "-A" => {
                        is_assoc_map = true;
                        true
                    }
                    "-a" => {
                        is_indexed_array = true;
                        true
                    }
                    "-x" => {
                        is_export = true;
                        true
                    }
                    _ => false,
                };
                if !supported {
                    ctx.write_stderr(
                        format!("esh: declare: unsupported option '{arg}'\n").as_bytes(),
                    );
                    return Ok(2);
                }
            } else {
                parsing_options = false;
                names_and_values.push(arg.as_str());
            }
        }

        if is_export && (is_assoc_map || is_indexed_array) {
            ctx.write_stderr(b"esh: declare: -x cannot be combined with -a or -A\n");
            return Ok(2);
        }

        for item in names_and_values {
            if let Some((k, v)) = item.split_once('=') {
                if !is_shell_identifier(k) {
                    ctx.write_stderr(format!("esh: declare: invalid identifier: {k}\n").as_bytes());
                    return Ok(2);
                }
                if is_assoc_map {
                    ctx.set_scoped_val(
                        k,
                        crate::runtime::context::ShellValue::Map(rustc_hash::FxHashMap::default()),
                    );
                } else if is_indexed_array {
                    ctx.set_scoped_val(
                        k,
                        crate::runtime::context::ShellValue::Array(vec![v.to_string()]),
                    );
                } else {
                    if is_export {
                        ctx.set_exported_var(k, v.to_string());
                    } else {
                        ctx.set_scoped_var(k, v.to_string());
                    }
                }
            } else {
                if !is_shell_identifier(item) {
                    ctx.write_stderr(
                        format!("esh: declare: invalid identifier: {item}\n").as_bytes(),
                    );
                    return Ok(2);
                }
                if is_assoc_map {
                    ctx.set_scoped_val(
                        item,
                        crate::runtime::context::ShellValue::Map(rustc_hash::FxHashMap::default()),
                    );
                } else if is_indexed_array {
                    ctx.set_scoped_val(
                        item,
                        crate::runtime::context::ShellValue::Array(Vec::new()),
                    );
                } else {
                    if is_export {
                        ctx.export_var(item);
                    } else {
                        ctx.set_scoped_var(item, String::new());
                    }
                }
            }
        }

        Ok(0)
    })
}

#[cfg(test)]
mod read_timeout_tests {
    use super::read_async_line;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn timed_read_returns_partial_input_without_consuming_it_as_a_complete_line() {
        let (mut reader, mut writer) = tokio::io::duplex(32);
        writer.write_all(b"partial").await.unwrap();

        let result = read_async_line(&mut reader, true, b'\n', Some(Duration::from_millis(5)))
            .await
            .unwrap();

        assert_eq!(result, ("partial".to_string(), false, true));
    }
}
