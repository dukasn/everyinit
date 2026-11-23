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
use crate::runtime::char_class::is_posix_space;
use crate::runtime::context::ExecutionContext;
use crate::runtime::traits::BoxFuture;
use regex::RegexBuilder;
use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

fn resolve_path(ctx: &ExecutionContext, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        ctx.cwd().join(p)
    }
}

fn read_text_inputs(
    args: &[String],
    ctx: &mut ExecutionContext,
    command: &str,
) -> Result<String, i32> {
    if args.is_empty() {
        let mut input = ctx
            .take_stdin()
            .map(|input| String::from_utf8_lossy(&input).into_owned())
            .unwrap_or_default();
        terminate_incomplete_text_record(&mut input);
        return Ok(input);
    }

    let mut input = String::new();
    for arg in args {
        if arg == "-" {
            let mut operand =
                String::from_utf8_lossy(&ctx.take_stdin().unwrap_or_default()).into_owned();
            terminate_incomplete_text_record(&mut operand);
            input.push_str(&operand);
            continue;
        }
        let path = resolve_path(ctx, arg);
        match fs::read_to_string(&path) {
            Ok(mut contents) => {
                terminate_incomplete_text_record(&mut contents);
                input.push_str(&contents);
            }
            Err(error) => {
                ctx.write_stderr(
                    format!("esh: {}: cannot read '{}': {}\n", command, arg, error).as_bytes(),
                );
                return Err(1);
            }
        }
    }
    Ok(input)
}

fn read_sort_input_runs(
    args: &[String],
    ctx: &mut ExecutionContext,
) -> Result<Vec<Vec<String>>, i32> {
    let operands = if args.is_empty() {
        vec!["-".to_string()]
    } else {
        args.to_vec()
    };
    let mut runs = Vec::with_capacity(operands.len());
    for operand in operands {
        let mut contents = if operand == "-" {
            String::from_utf8_lossy(&ctx.take_stdin().unwrap_or_default()).into_owned()
        } else {
            let path = resolve_path(ctx, &operand);
            match fs::read_to_string(&path) {
                Ok(contents) => contents,
                Err(error) => {
                    ctx.write_stderr(
                        format!("esh: sort: cannot read '{}': {}\n", operand, error).as_bytes(),
                    );
                    return Err(1);
                }
            }
        };
        terminate_incomplete_text_record(&mut contents);
        runs.push(contents.split_terminator('\n').map(str::to_owned).collect());
    }
    Ok(runs)
}

fn terminate_incomplete_text_record(input: &mut String) {
    if !input.is_empty() && !input.ends_with('\n') {
        input.push('\n');
    }
}

fn append_grep_pattern_file(
    patterns: &mut Vec<String>,
    pattern_file: &str,
    ctx: &mut ExecutionContext,
    silent_errors: bool,
) -> Result<(), i32> {
    let contents = if pattern_file == "-" {
        String::from_utf8_lossy(&ctx.take_stdin().unwrap_or_default()).into_owned()
    } else {
        let path = resolve_path(ctx, pattern_file);
        match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) => {
                if !silent_errors {
                    ctx.write_stderr(
                        format!(
                            "esh: grep: cannot read pattern file '{}': {}\n",
                            pattern_file, error
                        )
                        .as_bytes(),
                    );
                }
                return Err(2);
            }
        }
    };
    patterns.extend(contents.split_terminator('\n').map(str::to_owned));
    Ok(())
}

fn append_grep_pattern_list(patterns: &mut Vec<String>, pattern_list: &str) {
    if pattern_list.is_empty() {
        patterns.push(String::new());
    } else {
        patterns.extend(pattern_list.split_terminator('\n').map(str::to_owned));
    }
}

fn grep_basic_pattern_to_regex(pattern: &str) -> Result<String, String> {
    let mut output = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();
    let mut in_bracket = false;
    let mut bracket_first = false;
    while let Some(ch) = chars.next() {
        if in_bracket {
            if bracket_first && ch == '^' {
                output.push(ch);
                continue;
            }
            if bracket_first && ch == ']' {
                output.push(ch);
                bracket_first = false;
                continue;
            }
            if ch == '[' && matches!(chars.peek(), Some(':' | '.' | '=')) {
                let marker = chars.next().expect("peeked bracket expression marker");
                if marker != ':' {
                    return Err(format!(
                        "collating and equivalence classes ([{marker}…{marker}]) are not supported by ESH grep"
                    ));
                }
                output.push('[');
                output.push(marker);
                let mut closed = false;
                while let Some(inner) = chars.next() {
                    output.push(inner);
                    if inner == marker && chars.peek() == Some(&']') {
                        output.push(chars.next().expect("peeked bracket expression close"));
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    bracket_first = false;
                }
                continue;
            }
            output.push(ch);
            if ch == ']' {
                in_bracket = false;
            } else {
                bracket_first = false;
            }
            continue;
        }
        if ch == '[' {
            in_bracket = true;
            bracket_first = true;
            output.push(ch);
            continue;
        }
        if ch == '\\' {
            let Some(escaped) = chars.next() else {
                output.push_str("\\\\");
                break;
            };
            match escaped {
                '(' | ')' | '{' | '}' => output.push(escaped),
                '1'..='9' => {
                    return Err(format!(
                        "backreferences (\\{escaped}) are not supported by ESH grep"
                    ));
                }
                other => {
                    output.push('\\');
                    output.push(other);
                }
            }
            continue;
        }
        if matches!(ch, '(' | ')' | '{' | '}' | '+' | '?' | '|') {
            output.push('\\');
        }
        output.push(ch);
    }
    Ok(output)
}

fn grep_reject_locale_bracket_constructs(pattern: &str) -> Result<(), String> {
    let mut chars = pattern.chars().peekable();
    let mut in_bracket = false;
    let mut first = false;
    while let Some(ch) = chars.next() {
        if in_bracket {
            if first && ch == '^' {
                continue;
            }
            if first && ch == ']' {
                first = false;
                continue;
            }
            if ch == '[' && matches!(chars.peek(), Some('.' | '=')) {
                let marker = chars.next().expect("peeked bracket expression marker");
                return Err(format!(
                    "collating and equivalence classes ([{marker}…{marker}]) are not supported by ESH grep"
                ));
            }
            if ch == ']' {
                in_bracket = false;
            } else {
                first = false;
            }
        } else if ch == '[' {
            in_bracket = true;
            first = true;
        } else if ch == '\\' {
            // Escaped brackets are ordinary pattern characters.
            chars.next();
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct SortPosition {
    field: usize,
    character: usize,
}

struct SortKeySpec {
    start: SortPosition,
    end: Option<SortPosition>,
    start_blank: bool,
    end_blank: bool,
    modifiers: Vec<char>,
}

#[derive(Clone, Copy)]
struct SortModes {
    reverse: bool,
    unique: bool,
    numeric: bool,
    fold_case: bool,
    dictionary_order: bool,
    ignore_nonprinting: bool,
}

fn parse_sort_position(value: &str, is_end: bool) -> Option<(SortPosition, Vec<char>)> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if index == 0 {
        return None;
    }
    let field = value[..index].parse::<usize>().ok()?;
    if field == 0 {
        return None;
    }
    let mut character = if is_end { 0 } else { 1 };
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let digit_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if digit_start == index {
            return None;
        }
        character = value[digit_start..index].parse::<usize>().ok()?;
        if !is_end && character == 0 {
            return None;
        }
    }
    let mut modifiers = Vec::new();
    for modifier in value[index..].chars() {
        if !matches!(modifier, 'b' | 'd' | 'f' | 'i' | 'n' | 'r') {
            return None;
        }
        modifiers.push(modifier);
    }
    Some((SortPosition { field, character }, modifiers))
}

fn parse_sort_key_spec(value: &str) -> Option<SortKeySpec> {
    let mut pieces = value.split(',');
    let start_value = pieces.next()?;
    let end_value = pieces.next();
    if pieces.next().is_some() {
        return None;
    }
    let (start, start_modifiers) = parse_sort_position(start_value, false)?;
    let start_blank = start_modifiers.contains(&'b');
    let mut modifiers = start_modifiers;
    let end = if let Some(end_value) = end_value {
        let (end, end_modifiers) = parse_sort_position(end_value, true)?;
        if end.field < start.field
            || (end.field == start.field && end.character != 0 && end.character < start.character)
        {
            return None;
        }
        modifiers.extend(end_modifiers.iter().copied());
        let end_blank = end_modifiers.contains(&'b');
        Some((end, end_blank))
    } else {
        None
    };
    let (end, end_blank) = end.map_or((None, false), |(end, blank)| (Some(end), blank));
    Some(SortKeySpec {
        start,
        end,
        start_blank,
        end_blank,
        modifiers,
    })
}

fn sort_key(line: &str, spec: &SortKeySpec, separator: Option<char>, global_blank: bool) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut fields = Vec::new();
    if let Some(separator) = separator {
        let mut start = 0;
        for (index, ch) in chars.iter().enumerate() {
            if *ch == separator {
                fields.push((start, index));
                start = index + 1;
            }
        }
        fields.push((start, chars.len()));
    } else {
        let mut field_start = 0;
        let mut index = 0;
        while index < chars.len() {
            if !matches!(chars[index], ' ' | '\t') {
                index += 1;
                continue;
            }

            let separator_start = index;
            while index < chars.len() && matches!(chars[index], ' ' | '\t') {
                index += 1;
            }
            if chars[field_start..separator_start]
                .iter()
                .any(|ch| !matches!(ch, ' ' | '\t'))
            {
                fields.push((field_start, separator_start));
                field_start = separator_start;
            }
        }
        fields.push((field_start, chars.len()));
    }

    let Some(&(field_start, field_end)) = fields.get(spec.start.field - 1) else {
        return String::new();
    };
    let start_base = if global_blank || spec.start_blank {
        (field_start..field_end)
            .find(|index| !matches!(chars[*index], ' ' | '\t'))
            .unwrap_or(field_end)
    } else {
        field_start
    };
    let start = start_base
        .saturating_add(spec.start.character.saturating_sub(1))
        .min(field_end);
    let end = if let Some(end) = spec.end {
        let Some(&(end_field_start, end_field_end)) = fields.get(end.field - 1) else {
            return String::new();
        };
        if end.field < spec.start.field {
            return String::new();
        }
        if end.character == 0 {
            end_field_end
        } else {
            let end_base = if global_blank || spec.end_blank {
                (end_field_start..end_field_end)
                    .find(|index| !matches!(chars[*index], ' ' | '\t'))
                    .unwrap_or(end_field_end)
            } else {
                end_field_start
            };
            end_base.saturating_add(end.character).min(end_field_end)
        }
    } else {
        chars.len()
    };
    if end < start {
        return String::new();
    }
    chars[start..end].iter().collect()
}

pub fn sort<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut global = SortModes {
            reverse: false,
            unique: false,
            numeric: false,
            fold_case: false,
            dictionary_order: false,
            ignore_nonprinting: false,
        };
        let mut global_blank = false;
        let mut stable = false;
        let mut check_only = false;
        let mut check_silent = false;
        let mut merge_only = false;
        let mut output_file: Option<String> = None;
        let mut keys = Vec::new();
        let mut separator = None;
        let mut files = Vec::new();
        let mut options = true;
        let mut index = 0;
        while index < args.len() {
            let arg = &args[index];
            if options && arg == "--" {
                options = false;
            } else if options && (arg == "-o" || arg == "--output") {
                index += 1;
                let Some(value) = args.get(index) else {
                    ctx.write_stderr(b"esh: sort: -o requires an output file\n");
                    return Ok(2);
                };
                if output_file.replace(value.clone()).is_some() {
                    ctx.write_stderr(b"esh: sort: output file may be specified only once\n");
                    return Ok(2);
                }
            } else if options && arg.starts_with("--output=") {
                let value = arg.strip_prefix("--output=").unwrap_or_default();
                if output_file.replace(value.to_string()).is_some() {
                    ctx.write_stderr(b"esh: sort: output file may be specified only once\n");
                    return Ok(2);
                }
            } else if options && (arg == "-k" || arg == "--key") {
                index += 1;
                let Some(value) = args.get(index) else {
                    ctx.write_stderr(b"esh: sort: -k requires a key definition\n");
                    return Ok(2);
                };
                let Some(key) = parse_sort_key_spec(value) else {
                    ctx.write_stderr(
                        format!("esh: sort: invalid key definition '{}'\n", value).as_bytes(),
                    );
                    return Ok(2);
                };
                keys.push(key);
            } else if options
                && (arg.starts_with("--key=") || arg.starts_with("--field-separator="))
            {
                let (is_key, value) = if let Some(value) = arg.strip_prefix("--key=") {
                    (true, value)
                } else {
                    (
                        false,
                        arg.strip_prefix("--field-separator=").unwrap_or_default(),
                    )
                };
                if is_key {
                    if let Some(key) = parse_sort_key_spec(value) {
                        keys.push(key);
                    } else {
                        ctx.write_stderr(
                            format!("esh: sort: invalid key definition '{}'\n", value).as_bytes(),
                        );
                        return Ok(2);
                    }
                } else if value.chars().count() == 1 {
                    separator = value.chars().next();
                } else {
                    ctx.write_stderr(b"esh: sort: --field-separator requires one character\n");
                    return Ok(2);
                }
            } else if options && (arg == "-t" || arg == "--field-separator") {
                index += 1;
                let Some(value) = args.get(index).filter(|value| value.chars().count() == 1) else {
                    ctx.write_stderr(b"esh: sort: -t requires one separator character\n");
                    return Ok(2);
                };
                separator = value.chars().next();
            } else if options && (arg == "-r" || arg == "--reverse") {
                global.reverse = true;
            } else if options && (arg == "-u" || arg == "--unique") {
                global.unique = true;
            } else if options && (arg == "-s" || arg == "--stable") {
                stable = true;
            } else if options && (arg == "-c" || arg == "--check") {
                check_only = true;
            } else if options && (arg == "-C" || arg == "--check-silent") {
                check_silent = true;
            } else if options && (arg == "-m" || arg == "--merge") {
                merge_only = true;
            } else if options && (arg == "-n" || arg == "--numeric-sort") {
                global.numeric = true;
            } else if options && (arg == "-f" || arg == "--ignore-case") {
                global.fold_case = true;
            } else if options && (arg == "-d" || arg == "--dictionary-order") {
                global.dictionary_order = true;
            } else if options && (arg == "-i" || arg == "--ignore-nonprinting") {
                global.ignore_nonprinting = true;
            } else if options && (arg == "-b" || arg == "--ignore-leading-blanks") {
                global_blank = true;
            } else if options && arg.starts_with('-') && arg != "-" {
                let chars = arg[1..].char_indices().peekable();
                let mut valid = true;
                for (offset, flag) in chars {
                    match flag {
                        'r' => global.reverse = true,
                        'u' => global.unique = true,
                        's' => stable = true,
                        'c' => check_only = true,
                        'C' => check_silent = true,
                        'm' => merge_only = true,
                        'o' => {
                            let attached = &arg[1 + offset + flag.len_utf8()..];
                            let value = if !attached.is_empty() {
                                attached.to_owned()
                            } else {
                                index += 1;
                                let Some(value) = args.get(index) else {
                                    valid = false;
                                    break;
                                };
                                value.clone()
                            };
                            if output_file.replace(value).is_some() {
                                valid = false;
                            }
                            break;
                        }
                        'n' => global.numeric = true,
                        'f' => global.fold_case = true,
                        'd' => global.dictionary_order = true,
                        'i' => global.ignore_nonprinting = true,
                        'b' => global_blank = true,
                        'k' | 't' => {
                            let attached = &arg[1 + offset + flag.len_utf8()..];
                            let value = if !attached.is_empty() {
                                attached.to_owned()
                            } else {
                                index += 1;
                                let Some(value) = args.get(index) else {
                                    valid = false;
                                    break;
                                };
                                value.clone()
                            };
                            if flag == 'k' {
                                if let Some(key) = parse_sort_key_spec(&value) {
                                    keys.push(key);
                                } else {
                                    valid = false;
                                    break;
                                }
                            } else if value.chars().count() == 1 {
                                separator = value.chars().next();
                            } else {
                                valid = false;
                                break;
                            }
                            break;
                        }
                        _ => {
                            valid = false;
                            break;
                        }
                    }
                }
                if !valid {
                    ctx.write_stderr(
                        format!("esh: sort: invalid option or argument '{}'\n", arg).as_bytes(),
                    );
                    return Ok(2);
                }
            } else {
                files.push(arg.clone());
            }
            index += 1;
        }

        let modes_for = |key: &SortKeySpec| {
            let mut modes = global;
            if !key.modifiers.is_empty() {
                modes.reverse = false;
                modes.numeric = false;
                modes.fold_case = false;
                modes.dictionary_order = false;
                modes.ignore_nonprinting = false;
                for modifier in &key.modifiers {
                    match modifier {
                        'b' => {}
                        'd' => modes.dictionary_order = true,
                        'f' => modes.fold_case = true,
                        'i' => modes.ignore_nonprinting = true,
                        'n' => modes.numeric = true,
                        'r' => modes.reverse = true,
                        _ => unreachable!(),
                    }
                }
            }
            modes
        };
        let key_modes: Vec<_> = keys.iter().map(modes_for).collect();
        if key_modes
            .iter()
            .any(|m| m.numeric && (m.dictionary_order || m.ignore_nonprinting))
            || (keys.is_empty()
                && global.numeric
                && (global.dictionary_order || global.ignore_nonprinting))
        {
            ctx.write_stderr(b"esh: sort: -d/-i cannot be combined with -n\n");
            return Ok(2);
        }
        if (check_only || check_silent) && output_file.is_some() {
            ctx.write_stderr(b"esh: sort: -c cannot be combined with -o\n");
            return Ok(2);
        }
        if (check_only || check_silent) && merge_only {
            ctx.write_stderr(b"esh: sort: -c cannot be combined with -m\n");
            return Ok(2);
        }
        if check_only && check_silent {
            ctx.write_stderr(b"esh: sort: -c and -C are mutually exclusive\n");
            return Ok(2);
        }
        if (check_only || check_silent) && files.len() > 1 {
            ctx.write_stderr(b"esh: sort: -c accepts at most one input file\n");
            return Ok(2);
        }

        let compare = |left: &str, right: &str| {
            if keys.is_empty() {
                let ordering = if global.numeric {
                    compare_numeric_prefix(left, right)
                } else {
                    compare_text_key(
                        left,
                        right,
                        global.fold_case,
                        global.dictionary_order,
                        global.ignore_nonprinting,
                    )
                };
                let ordering = if global.unique || stable {
                    ordering
                } else {
                    ordering.then_with(|| left.cmp(right))
                };
                return if global.reverse {
                    ordering.reverse()
                } else {
                    ordering
                };
            }
            for (key, modes) in keys.iter().zip(&key_modes) {
                let left_key = sort_key(left, key, separator, global_blank);
                let right_key = sort_key(right, key, separator, global_blank);
                let ordering = if modes.numeric {
                    compare_numeric_prefix(&left_key, &right_key)
                } else {
                    compare_text_key(
                        &left_key,
                        &right_key,
                        modes.fold_case,
                        modes.dictionary_order,
                        modes.ignore_nonprinting,
                    )
                };
                let ordering = if modes.reverse {
                    ordering.reverse()
                } else {
                    ordering
                };
                if !ordering.is_eq() {
                    return ordering;
                }
            }
            if global.unique || stable {
                std::cmp::Ordering::Equal
            } else if global.reverse {
                left.cmp(right).reverse()
            } else {
                left.cmp(right)
            }
        };
        let mut lines: Vec<String> = if merge_only {
            let runs = match read_sort_input_runs(&files, ctx) {
                Ok(runs) => runs,
                Err(code) => return Ok(code),
            };
            let mut merged: Vec<String> = Vec::new();
            for run in runs {
                let mut left = merged.into_iter().peekable();
                let mut right = run.into_iter().peekable();
                let mut combined = Vec::new();
                loop {
                    match (left.peek(), right.peek()) {
                        (Some(left_line), Some(right_line)) => {
                            if compare(left_line, right_line).is_gt() {
                                combined.push(right.next().expect("peeked right record"));
                            } else {
                                combined.push(left.next().expect("peeked left record"));
                            }
                        }
                        (Some(_), None) => combined.extend(left.by_ref()),
                        (None, Some(_)) => combined.extend(right.by_ref()),
                        (None, None) => break,
                    }
                }
                merged = combined;
            }
            merged
        } else {
            let input = match read_text_inputs(&files, ctx, "sort") {
                Ok(input) => input,
                Err(code) => return Ok(code),
            };
            input.split_terminator('\n').map(str::to_owned).collect()
        };
        if check_only || check_silent {
            let disorder = lines.windows(2).any(|pair| {
                let ordering = compare(&pair[0], &pair[1]);
                ordering.is_gt() || (global.unique && ordering.is_eq())
            });
            if disorder && check_only {
                ctx.write_stderr(b"esh: sort: input is not in sorted order\n");
            }
            if disorder {
                return Ok(1);
            }
            return Ok(0);
        }
        lines.sort_by(|left, right| compare(left, right));
        if global.unique {
            lines.dedup_by(|right, left| compare(left, right).is_eq());
        }
        if let Some(output_file) = output_file {
            let mut output = String::new();
            for line in lines {
                output.push_str(&line);
                output.push('\n');
            }
            let path = resolve_path(ctx, &output_file);
            if let Err(error) = fs::write(&path, output.as_bytes()) {
                ctx.write_stderr(
                    format!("esh: sort: cannot write '{}': {}\n", output_file, error).as_bytes(),
                );
                return Ok(1);
            }
        } else {
            for line in lines {
                ctx.write_stdout(format!("{}\n", line).as_bytes());
            }
        }
        Ok(0)
    })
}

fn compare_text_key(
    left: &str,
    right: &str,
    fold_case: bool,
    dictionary_order: bool,
    ignore_nonprinting: bool,
) -> std::cmp::Ordering {
    let key = |line: &str| {
        let mut key = String::new();
        for ch in line.chars().filter(|ch| {
            (!dictionary_order || matches!(ch, ' ' | '\t') || ch.is_alphanumeric())
                && (!ignore_nonprinting || !ch.is_control())
        }) {
            if fold_case {
                key.extend(ch.to_uppercase());
            } else {
                key.push(ch);
            }
        }
        key
    };
    let left = key(left);
    let right = key(right);
    left.cmp(&right)
}

#[derive(Debug)]
struct DecimalKey<'a> {
    negative: bool,
    integer: &'a str,
    fraction: &'a str,
}

fn decimal_key(line: &str) -> DecimalKey<'_> {
    let bytes = line.as_bytes();
    let mut index = 0;
    while matches!(bytes.get(index), Some(b' ' | b'\t')) {
        index += 1;
    }
    let negative = matches!(bytes.get(index), Some(b'-'));
    if negative {
        index += 1;
    }

    let integer_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    let integer_end = index;
    let mut fraction_start = index;
    let mut fraction_end = index;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        fraction_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        fraction_end = index;
    }

    let mut integer = line[integer_start..integer_end].trim_start_matches('0');
    let mut fraction = &line[fraction_start..fraction_end];
    if integer.is_empty() {
        integer = "0";
    }
    fraction = fraction.trim_end_matches('0');
    let is_zero = integer == "0" && fraction.is_empty();
    DecimalKey {
        negative: negative && !is_zero,
        integer,
        fraction,
    }
}

fn compare_numeric_prefix(left: &str, right: &str) -> std::cmp::Ordering {
    let left = decimal_key(left);
    let right = decimal_key(right);
    use std::cmp::Ordering;

    match (left.negative, right.negative) {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }

    let magnitude = left
        .integer
        .len()
        .cmp(&right.integer.len())
        .then_with(|| left.integer.cmp(right.integer))
        .then_with(|| {
            let width = left.fraction.len().max(right.fraction.len());
            (0..width)
                .map(|index| {
                    let left_digit = left.fraction.as_bytes().get(index).copied().unwrap_or(b'0');
                    let right_digit = right
                        .fraction
                        .as_bytes()
                        .get(index)
                        .copied()
                        .unwrap_or(b'0');
                    left_digit.cmp(&right_digit)
                })
                .find(|ordering| !ordering.is_eq())
                .unwrap_or(Ordering::Equal)
        });
    if left.negative {
        magnitude.reverse()
    } else {
        magnitude
    }
}

pub fn uniq<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut count = false;
        let mut repeated_only = false;
        let mut unique_only = false;
        let mut skip_fields = 0usize;
        let mut skip_chars = 0usize;
        let mut files = Vec::new();
        let mut options = true;

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if options && arg == "--" {
                options = false;
            } else if options && (arg == "-c" || arg == "--count") {
                count = true;
            } else if options && (arg == "-d" || arg == "--repeated") {
                repeated_only = true;
            } else if options && (arg == "-u" || arg == "--unique") {
                unique_only = true;
            } else if options && (arg == "-f" || arg == "--skip-fields") {
                let Some(value) = iter.next() else {
                    ctx.write_stderr(b"esh: uniq: missing field count\n");
                    return Ok(2);
                };
                let Some(parsed) = parse_uniq_skip(value) else {
                    ctx.write_stderr(
                        format!("esh: uniq: invalid field count '{}'\n", value).as_bytes(),
                    );
                    return Ok(2);
                };
                skip_fields = parsed;
            } else if options && (arg == "-s" || arg == "--skip-chars") {
                let Some(value) = iter.next() else {
                    ctx.write_stderr(b"esh: uniq: missing character count\n");
                    return Ok(2);
                };
                let Some(parsed) = parse_uniq_skip(value) else {
                    ctx.write_stderr(
                        format!("esh: uniq: invalid character count '{}'\n", value).as_bytes(),
                    );
                    return Ok(2);
                };
                skip_chars = parsed;
            } else if options && let Some(value) = arg.strip_prefix("--skip-fields=") {
                let Some(parsed) = parse_uniq_skip(value) else {
                    ctx.write_stderr(
                        format!("esh: uniq: invalid field count '{}'\n", value).as_bytes(),
                    );
                    return Ok(2);
                };
                skip_fields = parsed;
            } else if options && let Some(value) = arg.strip_prefix("--skip-chars=") {
                let Some(parsed) = parse_uniq_skip(value) else {
                    ctx.write_stderr(
                        format!("esh: uniq: invalid character count '{}'\n", value).as_bytes(),
                    );
                    return Ok(2);
                };
                skip_chars = parsed;
            } else if options && arg.starts_with('-') && arg != "-" {
                let mut supported = true;
                let mut flags = arg[1..].chars().peekable();
                while let Some(flag) = flags.next() {
                    match flag {
                        'c' => count = true,
                        'd' => repeated_only = true,
                        'u' => unique_only = true,
                        'f' | 's' => {
                            let attached: String = flags.collect();
                            let value = if attached.is_empty() {
                                let Some(value) = iter.next() else {
                                    ctx.write_stderr(if flag == 'f' {
                                        b"esh: uniq: missing field count\n"
                                    } else {
                                        b"esh: uniq: missing character count\n"
                                    });
                                    return Ok(2);
                                };
                                value.clone()
                            } else {
                                attached
                            };
                            let Some(parsed) = parse_uniq_skip(&value) else {
                                ctx.write_stderr(
                                    format!(
                                        "esh: uniq: invalid {} count '{}'\n",
                                        if flag == 'f' { "field" } else { "character" },
                                        value
                                    )
                                    .as_bytes(),
                                );
                                return Ok(2);
                            };
                            if flag == 'f' {
                                skip_fields = parsed;
                            } else {
                                skip_chars = parsed;
                            }
                            break;
                        }
                        _ => {
                            supported = false;
                            break;
                        }
                    }
                }
                if !supported {
                    ctx.write_stderr(
                        format!("esh: uniq: unsupported option '{}'\n", arg).as_bytes(),
                    );
                    return Ok(2);
                }
            } else {
                files.push(arg.clone());
            }
        }

        if (count as u8 + repeated_only as u8 + unique_only as u8) > 1 {
            ctx.write_stderr(b"esh: uniq: choose only one of -c, -d, or -u\n");
            return Ok(2);
        }
        if files.len() > 2 {
            ctx.write_stderr(b"esh: uniq: expected at most an input and output file\n");
            return Ok(2);
        }

        let input = match read_text_inputs(&files[..files.len().min(1)], ctx, "uniq") {
            Ok(input) => input,
            Err(code) => return Ok(code),
        };
        let lines: Vec<&str> = input.split_terminator('\n').collect();
        let keys: Vec<String> = lines
            .iter()
            .map(|line| uniq_comparison_key(line, skip_fields, skip_chars))
            .collect();
        let mut output = String::new();
        let mut index = 0;
        while index < lines.len() {
            let line = lines[index];
            let mut occurrences = 1;
            while index + occurrences < lines.len() && keys[index + occurrences] == keys[index] {
                occurrences += 1;
            }
            if (!repeated_only || occurrences > 1) && (!unique_only || occurrences == 1) {
                if count {
                    output.push_str(&format!("{} {}\n", occurrences, line));
                } else {
                    output.push_str(line);
                    output.push('\n');
                }
            }
            index += occurrences;
        }
        if let Some(output_file) = files.get(1).filter(|path| path.as_str() != "-") {
            if let Err(error) = fs::write(resolve_path(ctx, output_file), output.as_bytes()) {
                ctx.write_stderr(
                    format!("esh: uniq: cannot write '{}': {}\n", output_file, error).as_bytes(),
                );
                return Ok(1);
            }
        } else {
            ctx.write_stdout(output.as_bytes());
        }
        Ok(0)
    })
}

fn parse_uniq_skip(value: &str) -> Option<usize> {
    let count = value.parse::<usize>().ok()?;
    (count > 0).then_some(count)
}

fn uniq_comparison_key(line: &str, skip_fields: usize, skip_chars: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0;
    for _ in 0..skip_fields {
        if index >= chars.len() {
            break;
        }
        while index < chars.len() && matches!(chars[index], ' ' | '\t') {
            index += 1;
        }
        while index < chars.len() && !matches!(chars[index], ' ' | '\t') {
            index += 1;
        }
    }
    let skip_to = index.saturating_add(skip_chars).min(chars.len());
    chars.into_iter().skip(skip_to).collect()
}

fn grep_text_has_match(text: &str, regex: &regex::Regex, invert: bool) -> bool {
    text.split_terminator('\n')
        .any(|line| regex.is_match(line) != invert)
}

fn grep_reader_has_match<R: BufRead>(
    reader: &mut R,
    regex: &regex::Regex,
    invert: bool,
) -> io::Result<bool> {
    let mut line = Vec::new();
    loop {
        let bytes_read = reader.read_until(b'\n', &mut line)?;
        if bytes_read == 0 {
            return Ok(false);
        }
        let content_len = if line.last() == Some(&b'\n') {
            line.len() - 1
        } else {
            line.len()
        };
        let content = std::str::from_utf8(&line[..content_len])
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if regex.is_match(content) != invert {
            return Ok(true);
        }
        line.clear();
    }
}

fn grep_file_has_match(path: &Path, regex: &regex::Regex, invert: bool) -> io::Result<bool> {
    let file = fs::File::open(path)?;
    grep_reader_has_match(&mut BufReader::new(file), regex, invert)
}

pub fn grep<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(
                b"esh: grep: usage: grep [-E|-F] [-i] [-v] [-q] [-s] [-c] [-l] [-n] [-x] [-e pattern]... [-f file]... [pattern] [file...]\n",
            );
            return Ok(2);
        }

        let mut case_insensitive = false;
        let mut invert = false;
        let mut quiet = false;
        let mut silent_errors = false;
        let mut count_only = false;
        let mut list_files = false;
        let mut line_numbers = false;
        let mut whole_line = false;
        let mut fixed_strings = false;
        let mut extended_regex = false;
        let mut patterns = Vec::new();
        let mut pattern_files = Vec::new();
        let mut pattern_source_given = false;
        let mut positional = Vec::new();
        let mut options = true;

        let mut index = 0;
        while index < args.len() {
            let arg = &args[index];
            if options && arg == "--" {
                options = false;
            } else if options && (arg == "-e" || arg == "--regexp") {
                index += 1;
                let Some(pattern) = args.get(index) else {
                    ctx.write_stderr(b"esh: grep: -e requires a pattern\n");
                    return Ok(2);
                };
                pattern_source_given = true;
                append_grep_pattern_list(&mut patterns, pattern);
            } else if options && arg.starts_with("-e") && arg.len() > 2 {
                pattern_source_given = true;
                append_grep_pattern_list(&mut patterns, &arg[2..]);
            } else if options && (arg == "-f" || arg == "--file") {
                index += 1;
                let Some(pattern_file) = args.get(index) else {
                    ctx.write_stderr(b"esh: grep: -f requires a pattern file\n");
                    return Ok(2);
                };
                pattern_source_given = true;
                pattern_files.push(pattern_file.clone());
            } else if options && arg.starts_with("-f") && arg.len() > 2 {
                pattern_source_given = true;
                pattern_files.push(arg[2..].to_string());
            } else if options && (arg == "-i" || arg == "--ignore-case") {
                case_insensitive = true;
            } else if options && (arg == "-v" || arg == "--invert-match") {
                invert = true;
            } else if options && (arg == "-q" || arg == "--quiet" || arg == "--silent") {
                quiet = true;
            } else if options && (arg == "-s" || arg == "--no-messages") {
                silent_errors = true;
            } else if options && (arg == "-c" || arg == "--count") {
                count_only = true;
            } else if options && (arg == "-l" || arg == "--files-with-matches") {
                list_files = true;
            } else if options && (arg == "-n" || arg == "--line-number") {
                line_numbers = true;
            } else if options && (arg == "-x" || arg == "--line-regexp") {
                whole_line = true;
            } else if options && (arg == "-F" || arg == "--fixed-strings") {
                if extended_regex {
                    ctx.write_stderr(b"esh: grep: conflicting matchers specified\n");
                    return Ok(2);
                }
                fixed_strings = true;
            } else if options && (arg == "-E" || arg == "--extended-regexp") {
                if fixed_strings {
                    ctx.write_stderr(b"esh: grep: conflicting matchers specified\n");
                    return Ok(2);
                }
                fixed_strings = false;
                extended_regex = true;
            } else if options
                && arg.starts_with('-')
                && arg != "-"
                && arg.len() > 2
                && !arg.starts_with("--")
            {
                let flags: Vec<char> = arg[1..].chars().collect();
                let mut flag_index = 0;
                while flag_index < flags.len() {
                    let flag = flags[flag_index];
                    match flag {
                        'i' => case_insensitive = true,
                        'v' => invert = true,
                        'q' => quiet = true,
                        's' => silent_errors = true,
                        'c' => count_only = true,
                        'l' => list_files = true,
                        'n' => line_numbers = true,
                        'x' => whole_line = true,
                        'F' => {
                            if extended_regex {
                                ctx.write_stderr(b"esh: grep: conflicting matchers specified\n");
                                return Ok(2);
                            }
                            fixed_strings = true;
                        }
                        'E' => {
                            if fixed_strings {
                                ctx.write_stderr(b"esh: grep: conflicting matchers specified\n");
                                return Ok(2);
                            }
                            fixed_strings = false;
                            extended_regex = true;
                        }
                        'e' => {
                            let attached: String = flags[flag_index + 1..].iter().collect();
                            if !attached.is_empty() {
                                pattern_source_given = true;
                                append_grep_pattern_list(&mut patterns, &attached);
                            } else {
                                index += 1;
                                let Some(pattern) = args.get(index) else {
                                    ctx.write_stderr(b"esh: grep: -e requires a pattern\n");
                                    return Ok(2);
                                };
                                pattern_source_given = true;
                                append_grep_pattern_list(&mut patterns, pattern);
                            }
                            break;
                        }
                        'f' => {
                            let attached: String = flags[flag_index + 1..].iter().collect();
                            let pattern_file = if !attached.is_empty() {
                                attached
                            } else {
                                index += 1;
                                let Some(pattern_file) = args.get(index) else {
                                    ctx.write_stderr(b"esh: grep: -f requires a pattern file\n");
                                    return Ok(2);
                                };
                                pattern_file.clone()
                            };
                            pattern_source_given = true;
                            pattern_files.push(pattern_file);
                            break;
                        }
                        _ => {
                            ctx.write_stderr(
                                format!("esh: grep: unsupported option '-{}'\n", flag).as_bytes(),
                            );
                            return Ok(2);
                        }
                    }
                    flag_index += 1;
                }
            } else if options && arg.starts_with('-') && arg != "-" {
                ctx.write_stderr(format!("esh: grep: unsupported option '{}'\n", arg).as_bytes());
                return Ok(2);
            } else {
                positional.push(arg.clone());
            }
            index += 1;
        }

        for pattern_file in pattern_files {
            if let Err(code) =
                append_grep_pattern_file(&mut patterns, &pattern_file, ctx, silent_errors)
            {
                return Ok(code);
            }
        }

        let files = if patterns.is_empty() && !pattern_source_given {
            if positional.is_empty() {
                ctx.write_stderr(b"esh: grep: missing pattern operand\n");
                return Ok(2);
            }
            patterns.push(positional.remove(0));
            positional
        } else {
            positional
        };

        if patterns.is_empty() {
            patterns.push(r"\b\B".to_string());
        }

        let mut regex_patterns = Vec::with_capacity(patterns.len());
        for pattern in &patterns {
            let pattern = if fixed_strings {
                regex::escape(pattern)
            } else if !extended_regex {
                match grep_basic_pattern_to_regex(pattern) {
                    Ok(pattern) => pattern,
                    Err(error) => {
                        ctx.write_stderr(format!("esh: grep: {error}\n").as_bytes());
                        return Ok(2);
                    }
                }
            } else {
                if let Err(error) = grep_reject_locale_bracket_constructs(pattern) {
                    ctx.write_stderr(format!("esh: grep: {error}\n").as_bytes());
                    return Ok(2);
                }
                pattern.clone()
            };
            regex_patterns.push(format!("(?:{})", pattern));
        }
        let regex_pattern = regex_patterns.join("|");
        let regex_pattern = if whole_line {
            format!("^(?:{})$", regex_pattern)
        } else {
            regex_pattern
        };
        let re = match RegexBuilder::new(&regex_pattern)
            .case_insensitive(case_insensitive)
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                ctx.write_stderr(
                    format!(
                        "esh: grep: invalid regex pattern '{}': {}\n",
                        regex_pattern, e
                    )
                    .as_bytes(),
                );
                return Ok(2);
            }
        };

        if quiet || list_files {
            let mut match_found = false;
            let mut read_error = false;
            if files.is_empty() {
                let input = ctx.take_stdin().unwrap_or_default();
                let text = String::from_utf8_lossy(&input);
                match_found = grep_text_has_match(&text, &re, invert);
                if list_files && match_found && !quiet {
                    ctx.write_stdout(b"(standard input)\n");
                }
            } else {
                for path_str in &files {
                    let (label, result) = if path_str == "-" {
                        let input = ctx.take_stdin().unwrap_or_default();
                        let text = String::from_utf8_lossy(&input);
                        (
                            "(standard input)",
                            Ok(grep_text_has_match(&text, &re, invert)),
                        )
                    } else {
                        let path = resolve_path(ctx, path_str);
                        (path_str.as_str(), grep_file_has_match(&path, &re, invert))
                    };

                    match result {
                        Ok(selected) => {
                            match_found |= selected;
                            if list_files && selected && !quiet {
                                ctx.write_stdout(format!("{}\n", label).as_bytes());
                            }
                        }
                        Err(error) => {
                            read_error = true;
                            if !silent_errors {
                                ctx.write_stderr(
                                    format!("esh: grep: cannot read '{}': {}\n", path_str, error)
                                        .as_bytes(),
                                );
                            }
                        }
                    }
                    if quiet && match_found {
                        break;
                    }
                }
            }

            return Ok(if quiet && match_found {
                0
            } else if read_error {
                2
            } else if match_found {
                0
            } else {
                1
            });
        }

        let mut match_found = false;
        let mut read_error = false;

        let process_lines =
            |text: &str, file_name: &str, file_prefix: Option<&str>, ctx: &mut ExecutionContext| {
                let mut selected_count = 0usize;
                let mut file_match_found = false;
                for (idx, line) in text.split_terminator('\n').enumerate() {
                    let is_match = re.is_match(line);
                    let selected = if invert { !is_match } else { is_match };

                    if selected {
                        file_match_found = true;
                        selected_count += 1;
                        if !quiet && !count_only && !list_files {
                            let mut output = String::new();
                            if let Some(prefix) = file_prefix {
                                output.push_str(prefix);
                                output.push(':');
                            }
                            if line_numbers {
                                output.push_str(&(idx + 1).to_string());
                                output.push(':');
                            }
                            output.push_str(line);
                            output.push('\n');
                            ctx.write_stdout(output.as_bytes());
                        }
                        if quiet {
                            break;
                        }
                    }
                }
                if list_files && file_match_found && !quiet {
                    ctx.write_stdout(format!("{}\n", file_name).as_bytes());
                } else if count_only && !quiet && !list_files {
                    let mut output = String::new();
                    if let Some(prefix) = file_prefix {
                        output.push_str(prefix);
                        output.push(':');
                    }
                    output.push_str(&selected_count.to_string());
                    output.push('\n');
                    ctx.write_stdout(output.as_bytes());
                }
                file_match_found
            };

        if files.is_empty() {
            let buf = ctx.take_stdin().unwrap_or_default();
            let s = String::from_utf8_lossy(&buf);
            match_found |= process_lines(&s, "(standard input)", None, ctx);
        } else {
            let show_file_prefix = files.len() > 1;
            for path_str in &files {
                if *path_str == "-" {
                    let buf = ctx.take_stdin().unwrap_or_default();
                    let text = String::from_utf8_lossy(&buf);
                    let prefix = if show_file_prefix {
                        Some(path_str.as_str())
                    } else {
                        None
                    };
                    match_found |= process_lines(&text, "(standard input)", prefix, ctx);
                    if quiet && match_found {
                        break;
                    }
                    continue;
                }
                let path = resolve_path(ctx, path_str);
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        let prefix = if show_file_prefix {
                            Some(path_str.as_str())
                        } else {
                            None
                        };
                        match_found |= process_lines(&content, path_str, prefix, ctx);
                    }
                    Err(e) => {
                        read_error = true;
                        if !silent_errors {
                            ctx.write_stderr(
                                format!("esh: grep: cannot read '{}': {}\n", path_str, e)
                                    .as_bytes(),
                            );
                        }
                    }
                }
                if quiet && match_found {
                    break;
                }
            }
        }

        if quiet && match_found {
            Ok(0)
        } else if read_error {
            Ok(2)
        } else if match_found {
            Ok(0)
        } else {
            Ok(1)
        }
    })
}

type CutSelection = (char, Vec<(usize, Option<usize>)>);

pub fn cut<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut delim = "\t".to_string();
        let mut selection: Option<CutSelection> = None;
        let mut suppress_undelimited = false;
        let mut no_split_chars = false;
        let mut files = Vec::new();
        let mut options = true;

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if options && arg == "--" {
                options = false;
            } else if options && (arg == "-d" || arg == "--delimiter") {
                let Some(d) = iter.next() else {
                    ctx.write_stderr(b"esh: cut: missing delimiter\n");
                    return Ok(2);
                };
                delim = d.clone();
            } else if options
                && (arg == "-f"
                    || arg == "--fields"
                    || arg == "-c"
                    || arg == "--characters"
                    || arg == "-b"
                    || arg == "--bytes")
            {
                let mode = if arg == "-c" || arg == "--characters" {
                    'c'
                } else if arg == "-b" || arg == "--bytes" {
                    'b'
                } else {
                    'f'
                };
                let Some(list) = iter.next() else {
                    ctx.write_stderr(b"esh: cut: missing selection list\n");
                    return Ok(2);
                };
                match parse_cut_list(list) {
                    Some(ranges) => {
                        if selection.replace((mode, ranges)).is_some() {
                            ctx.write_stderr(
                                b"esh: cut: choose exactly one of bytes, characters, or fields\n",
                            );
                            return Ok(2);
                        }
                    }
                    None => {
                        ctx.write_stderr(
                            format!("esh: cut: invalid selection list '{}'\n", list).as_bytes(),
                        );
                        return Ok(2);
                    }
                }
            } else if options && (arg == "-s" || arg == "--only-delimited") {
                suppress_undelimited = true;
            } else if options && arg == "-n" {
                no_split_chars = true;
            } else if options
                && arg.starts_with('-')
                && arg != "-"
                && !arg.starts_with("--")
                && arg.len() > 2
            {
                let flags = arg[1..].char_indices().peekable();
                for (offset, flag) in flags {
                    match flag {
                        's' => suppress_undelimited = true,
                        'n' => no_split_chars = true,
                        'd' | 'f' | 'c' | 'b' => {
                            let attached = &arg[1 + offset + flag.len_utf8()..];
                            let value = if attached.is_empty() {
                                let Some(value) = iter.next() else {
                                    ctx.write_stderr(if flag == 'd' {
                                        b"esh: cut: missing delimiter\n"
                                    } else {
                                        b"esh: cut: missing selection list\n"
                                    });
                                    return Ok(2);
                                };
                                value.as_str()
                            } else {
                                attached
                            };
                            if flag == 'd' {
                                delim = value.to_string();
                            } else {
                                let mode = flag;
                                let Some(ranges) = parse_cut_list(value) else {
                                    ctx.write_stderr(
                                        format!("esh: cut: invalid selection list '{}'\n", value)
                                            .as_bytes(),
                                    );
                                    return Ok(2);
                                };
                                if selection.replace((mode, ranges)).is_some() {
                                    ctx.write_stderr(
                                        b"esh: cut: choose exactly one of bytes, characters, or fields\n",
                                    );
                                    return Ok(2);
                                }
                            }
                            break;
                        }
                        _ => {
                            ctx.write_stderr(
                                format!("esh: cut: unsupported option '-{}'\n", flag).as_bytes(),
                            );
                            return Ok(2);
                        }
                    }
                }
            } else if options && let Some(d) = arg.strip_prefix("-d") {
                if d.is_empty() {
                    ctx.write_stderr(b"esh: cut: missing delimiter\n");
                    return Ok(2);
                }
                delim = d.to_string();
            } else if options && let Some(list) = arg.strip_prefix("-f") {
                match parse_cut_list(list) {
                    Some(ranges) => {
                        if selection.replace(('f', ranges)).is_some() {
                            ctx.write_stderr(
                                b"esh: cut: choose exactly one of bytes, characters, or fields\n",
                            );
                            return Ok(2);
                        }
                    }
                    None => {
                        ctx.write_stderr(
                            format!("esh: cut: invalid selection list '{}'\n", list).as_bytes(),
                        );
                        return Ok(2);
                    }
                }
            } else if options && let Some(list) = arg.strip_prefix("-c") {
                match parse_cut_list(list) {
                    Some(ranges) => {
                        if selection.replace(('c', ranges)).is_some() {
                            ctx.write_stderr(
                                b"esh: cut: choose exactly one of bytes, characters, or fields\n",
                            );
                            return Ok(2);
                        }
                    }
                    None => {
                        ctx.write_stderr(
                            format!("esh: cut: invalid selection list '{}'\n", list).as_bytes(),
                        );
                        return Ok(2);
                    }
                }
            } else if options && let Some(list) = arg.strip_prefix("-b") {
                match parse_cut_list(list) {
                    Some(ranges) => {
                        if selection.replace(('b', ranges)).is_some() {
                            ctx.write_stderr(
                                b"esh: cut: choose exactly one of bytes, characters, or fields\n",
                            );
                            return Ok(2);
                        }
                    }
                    None => {
                        ctx.write_stderr(
                            format!("esh: cut: invalid selection list '{}'\n", list).as_bytes(),
                        );
                        return Ok(2);
                    }
                }
            } else if options && arg.starts_with('-') && arg != "-" {
                ctx.write_stderr(format!("esh: cut: unsupported option '{}'\n", arg).as_bytes());
                return Ok(2);
            } else {
                files.push(arg.clone());
            }
        }

        let Some((mode, ranges)) = selection else {
            ctx.write_stderr(b"esh: cut: missing byte, character, or field selection\n");
            return Ok(2);
        };
        if delim.chars().count() != 1 {
            ctx.write_stderr(b"esh: cut: delimiter must be one character\n");
            return Ok(2);
        }
        if mode != 'f' && suppress_undelimited {
            ctx.write_stderr(b"esh: cut: -s requires field selection\n");
            return Ok(2);
        }
        if no_split_chars && mode != 'b' {
            ctx.write_stderr(b"esh: cut: -n requires byte selection\n");
            return Ok(2);
        }

        let selected = |index: usize| {
            ranges
                .iter()
                .any(|(start, end)| index >= *start && end.is_none_or(|end| index <= end))
        };
        let process_input = |input: &[u8], ctx: &mut ExecutionContext| {
            if input.is_empty() {
                return;
            }
            let mut records = input.split(|byte| *byte == b'\n').peekable();
            while let Some(record) = records.next() {
                if records.peek().is_none() && record.is_empty() {
                    break;
                }
                match mode {
                    'b' => {
                        let output: Vec<u8> = if no_split_chars
                            && std::str::from_utf8(record).is_ok()
                        {
                            let mut selected_bytes = vec![false; record.len()];
                            for (low, high) in &ranges {
                                let mut start = low.saturating_sub(1);
                                if start >= record.len() {
                                    continue;
                                }
                                while start > 0 && is_utf8_continuation_byte(record[start]) {
                                    start -= 1;
                                }

                                let mut end = high.unwrap_or(record.len()).min(record.len());
                                if end > 0
                                    && end < record.len()
                                    && is_utf8_continuation_byte(record[end])
                                {
                                    end -= 1;
                                    while end > 0 && is_utf8_continuation_byte(record[end]) {
                                        end -= 1;
                                    }
                                }

                                if start < end {
                                    selected_bytes[start..end].fill(true);
                                }
                            }
                            record
                                .iter()
                                .enumerate()
                                .filter_map(|(index, byte)| selected_bytes[index].then_some(*byte))
                                .collect()
                        } else {
                            record
                                .iter()
                                .enumerate()
                                .filter_map(|(index, byte)| selected(index + 1).then_some(*byte))
                                .collect()
                        };
                        let mut output = output;
                        output.push(b'\n');
                        ctx.write_stdout(&output);
                    }
                    'c' => {
                        let line = String::from_utf8_lossy(record);
                        let output: String = line
                            .chars()
                            .enumerate()
                            .filter_map(|(index, ch)| selected(index + 1).then_some(ch))
                            .collect();
                        ctx.write_stdout(format!("{}\n", output).as_bytes());
                    }
                    _ => {
                        let line = String::from_utf8_lossy(record);
                        if !line.contains(&delim) {
                            if !suppress_undelimited {
                                ctx.write_stdout(format!("{}\n", line).as_bytes());
                            }
                            continue;
                        }
                        let output = line
                            .split(&delim)
                            .enumerate()
                            .filter_map(|(index, field)| selected(index + 1).then_some(field))
                            .collect::<Vec<_>>()
                            .join(&delim);
                        ctx.write_stdout(format!("{}\n", output).as_bytes());
                    }
                }
            }
        };

        if files.is_empty() {
            process_input(&ctx.take_stdin().unwrap_or_default(), ctx);
            return Ok(0);
        }
        let mut status = 0;
        for path_str in files {
            let input = if path_str == "-" {
                ctx.take_stdin().unwrap_or_default()
            } else {
                let path = resolve_path(ctx, &path_str);
                match fs::read(&path) {
                    Ok(input) => input,
                    Err(error) => {
                        ctx.write_stderr(
                            format!("esh: cut: cannot read '{}': {}\n", path_str, error).as_bytes(),
                        );
                        status = 1;
                        continue;
                    }
                }
            };
            process_input(&input, ctx);
        }
        Ok(status)
    })
}

fn parse_cut_list(list: &str) -> Option<Vec<(usize, Option<usize>)>> {
    if list.is_empty() {
        return None;
    }
    let mut ranges = Vec::new();
    for comma_group in list.split(',') {
        let comma_group = comma_group.trim();
        if comma_group.is_empty() {
            return None;
        }
        for part in comma_group.split_ascii_whitespace() {
            let range = if let Some((start, end)) = part.split_once('-') {
                let start = if start.is_empty() {
                    1
                } else {
                    start.parse().ok()?
                };
                let end = if end.is_empty() {
                    None
                } else {
                    Some(end.parse().ok()?)
                };
                if start == 0 || end.is_some_and(|end| end < start) {
                    return None;
                }
                (start, end)
            } else {
                let index = part.parse().ok()?;
                if index == 0 {
                    return None;
                }
                (index, Some(index))
            };
            ranges.push(range);
        }
    }
    Some(ranges)
}

fn is_utf8_continuation_byte(byte: u8) -> bool {
    byte & 0b1100_0000 == 0b1000_0000
}

pub fn tr<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: tr: usage: tr [-d] [-s] <set1> [set2]\n");
            return Ok(2);
        }

        let mut delete_mode = false;
        let mut squeeze_mode = false;
        let mut complement = false;
        let mut complement_option = None;
        let mut options = true;
        let mut operands = Vec::new();

        for arg in args {
            if options && arg == "--" {
                options = false;
            } else if options && (arg == "-d" || arg == "--delete") {
                delete_mode = true;
            } else if options && (arg == "-s" || arg == "--squeeze-repeats") {
                squeeze_mode = true;
            } else if options && arg.starts_with('-') && arg != "-" {
                let mut supported = true;
                for flag in arg[1..].chars() {
                    match flag {
                        'd' => delete_mode = true,
                        's' => squeeze_mode = true,
                        'c' | 'C' => {
                            if complement_option.is_some_and(|previous| previous != flag) {
                                ctx.write_stderr(b"esh: tr: -c and -C are mutually exclusive\n");
                                return Ok(2);
                            }
                            complement_option = Some(flag);
                            complement = true;
                        }
                        _ => {
                            supported = false;
                            break;
                        }
                    }
                }
                if !supported {
                    ctx.write_stderr(format!("esh: tr: unsupported option '{}'\n", arg).as_bytes());
                    return Ok(2);
                }
            } else {
                operands.push(arg.as_str());
            }
        }

        let invalid_operand_count = if delete_mode && squeeze_mode {
            operands.len() != 2
        } else if delete_mode {
            operands.len() != 1
        } else if squeeze_mode {
            operands.is_empty() || operands.len() > 2
        } else {
            operands.len() != 2
        };
        if invalid_operand_count {
            ctx.write_stderr(b"esh: tr: invalid number of operands\n");
            return Ok(2);
        }

        let input = match ctx.take_stdin() {
            Some(buf) => String::from_utf8_lossy(&buf).to_string(),
            None => String::new(),
        };

        let set1_str = operands[0];
        let set2_str = if operands.len() > 1 { operands[1] } else { "" };
        let set1 = match expand_char_set(set1_str, None) {
            Some(set) => set,
            None => {
                ctx.write_stderr(
                    format!("esh: tr: invalid character set '{}'\n", set1_str).as_bytes(),
                );
                return Ok(2);
            }
        };
        let set2 = match expand_char_set(set2_str, Some(set1.len())) {
            Some(set) => set,
            None => {
                ctx.write_stderr(
                    format!("esh: tr: invalid character set '{}'\n", set2_str).as_bytes(),
                );
                return Ok(2);
            }
        };
        if !delete_mode && !squeeze_mode && operands.len() != 2 {
            ctx.write_stderr(b"esh: tr: translation requires two character sets\n");
            return Ok(2);
        }

        let mut complement_exclusions = set1.clone();
        complement_exclusions.sort_unstable();
        complement_exclusions.dedup();
        let squeeze_set = if operands.len() == 1 { &set1 } else { &set2 };
        let mut output = String::with_capacity(input.len());
        let mut previous = None;
        for ch in input.chars() {
            let in_set1 = set1.contains(&ch);
            let selected_index = if complement {
                (!in_set1).then(|| unicode_complement_index(ch, &complement_exclusions))
            } else {
                set1.iter().position(|candidate| *candidate == ch)
            };
            if delete_mode && selected_index.is_some() {
                continue;
            }
            let translated = if !delete_mode {
                if let Some(index) = selected_index {
                    if let Some(replacement) = set2.get(index).or_else(|| set2.last()) {
                        *replacement
                    } else {
                        ch
                    }
                } else {
                    ch
                }
            } else {
                ch
            };
            let squeeze_selected = if complement && operands.len() == 1 {
                !set1.contains(&translated)
            } else {
                squeeze_set.contains(&translated)
            };
            if squeeze_mode && squeeze_selected && previous == Some(translated) {
                continue;
            }
            output.push(translated);
            previous = Some(translated);
        }

        ctx.write_stdout(output.as_bytes());
        Ok(0)
    })
}

fn unicode_complement_index(ch: char, excluded: &[char]) -> usize {
    let codepoint = ch as usize;
    let scalar_index = if codepoint <= 0xd800 {
        codepoint
    } else {
        codepoint - 0x800
    };
    let excluded_before = excluded.iter().filter(|excluded| **excluded < ch).count();
    scalar_index - excluded_before
}

fn expand_char_set(s: &str, repeat_to: Option<usize>) -> Option<Vec<char>> {
    enum SetToken {
        Character(char),
        Escaped(char),
        Class(Vec<char>),
        Repeat(char, Option<usize>),
    }

    impl SetToken {
        fn range_endpoint(&self) -> Option<char> {
            match self {
                Self::Character(ch) | Self::Escaped(ch) => Some(*ch),
                Self::Class(_) | Self::Repeat(_, _) => None,
            }
        }
    }

    let mut tokens = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        if let Some(class) = rest.strip_prefix("[:") {
            let end = class.find(":]")?;
            let name = &class[..end];
            tokens.push(SetToken::Class(posix_char_class(name)?));
            rest = &class[end + 2..];
            continue;
        }
        if let Some((character, count, consumed)) = parse_tr_repeat(rest) {
            repeat_to?;
            tokens.push(SetToken::Repeat(character, count));
            rest = &rest[consumed..];
            continue;
        }
        let ch = rest.chars().next()?;
        if ch == '\\' {
            rest = &rest[ch.len_utf8()..];
            if rest
                .chars()
                .next()
                .is_some_and(|ch| ('0'..='7').contains(&ch))
            {
                let octal_len = rest
                    .chars()
                    .take_while(|ch| ('0'..='7').contains(ch))
                    .take(3)
                    .map(char::len_utf8)
                    .sum::<usize>();
                let value = u32::from_str_radix(&rest[..octal_len], 8).ok()?;
                if value > u8::MAX as u32 {
                    return None;
                }
                tokens.push(SetToken::Escaped(char::from_u32(value)?));
                rest = &rest[octal_len..];
                continue;
            }
            let escaped = rest.chars().next()?;
            let value = match escaped {
                'a' => '\u{7}',
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                'b' => '\u{8}',
                'f' => '\u{c}',
                'v' => '\u{b}',
                other => other,
            };
            tokens.push(SetToken::Escaped(value));
            rest = &rest[escaped.len_utf8()..];
        } else {
            tokens.push(SetToken::Character(ch));
            rest = &rest[ch.len_utf8()..];
        }
    }

    let mut expanded = Vec::new();
    let suffix_length = |start: usize| {
        let mut length = 0usize;
        let mut cursor = start;
        while cursor < tokens.len() {
            if cursor + 2 < tokens.len()
                && matches!(tokens[cursor + 1], SetToken::Character('-'))
                && let Some((range_start, range_end)) = tokens[cursor]
                    .range_endpoint()
                    .zip(tokens[cursor + 2].range_endpoint())
            {
                let first = range_start as u32;
                let last = range_end as u32;
                if last >= first {
                    length = length.saturating_add(
                        (first..=last)
                            .filter(|value| char::from_u32(*value).is_some())
                            .count(),
                    );
                    cursor += 3;
                    continue;
                }
            }
            length = length.saturating_add(match &tokens[cursor] {
                SetToken::Character(_) | SetToken::Escaped(_) => 1,
                SetToken::Class(characters) => characters.len(),
                SetToken::Repeat(_, Some(count)) => *count,
                SetToken::Repeat(_, None) => 0,
            });
            cursor += 1;
        }
        length
    };
    let mut index = 0;
    while index < tokens.len() {
        if let SetToken::Repeat(character, count) = tokens[index] {
            let count = match count {
                Some(count) => count,
                None => {
                    let target = repeat_to?;
                    target.saturating_sub(expanded.len().saturating_add(suffix_length(index + 1)))
                }
            };
            expanded.extend(std::iter::repeat_n(character, count));
            index += 1;
            continue;
        }
        let range =
            if index + 2 < tokens.len() && matches!(tokens[index + 1], SetToken::Character('-')) {
                tokens[index]
                    .range_endpoint()
                    .zip(tokens[index + 2].range_endpoint())
            } else {
                None
            };
        if let Some((start, end)) = range {
            let start = start as u32;
            let end = end as u32;
            if end < start {
                return None;
            }
            for codepoint in start..=end {
                if let Some(ch) = char::from_u32(codepoint) {
                    expanded.push(ch);
                }
            }
            index += 3;
        } else {
            match &tokens[index] {
                SetToken::Character(ch) | SetToken::Escaped(ch) => expanded.push(*ch),
                SetToken::Class(chars) => expanded.extend(chars.iter().copied()),
                SetToken::Repeat(_, _) => unreachable!(),
            }
            index += 1;
        }
    }
    Some(expanded)
}

fn parse_tr_repeat(value: &str) -> Option<(char, Option<usize>, usize)> {
    let rest = value.strip_prefix('[')?;
    let (character, character_len) = parse_tr_set_character(rest)?;
    let rest = rest.get(character_len..)?.strip_prefix('*')?;
    let digit_count = rest.bytes().take_while(u8::is_ascii_digit).count();
    let count = if digit_count == 0 {
        None
    } else {
        let digits = &rest[..digit_count];
        let count = if digits.len() > 1 && digits.starts_with('0') {
            usize::from_str_radix(digits, 8).ok()?
        } else {
            digits.parse::<usize>().ok()?
        };
        (count != 0).then_some(count)
    };
    let suffix = rest.get(digit_count..)?.strip_prefix(']')?;
    Some((character, count, value.len() - suffix.len()))
}

fn parse_tr_set_character(value: &str) -> Option<(char, usize)> {
    if let Some(rest) = value.strip_prefix('\\') {
        let first = rest.chars().next()?;
        if ('0'..='7').contains(&first) {
            let digit_count = rest
                .chars()
                .take_while(|ch| ('0'..='7').contains(ch))
                .take(3)
                .map(char::len_utf8)
                .sum::<usize>();
            let codepoint = u32::from_str_radix(&rest[..digit_count], 8).ok()?;
            return Some((char::from_u32(codepoint)?, digit_count + 1));
        }
        let character = match first {
            'a' => '\u{7}',
            'b' => '\u{8}',
            'f' => '\u{c}',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'v' => '\u{b}',
            other => other,
        };
        return Some((character, first.len_utf8() + 1));
    }
    let character = value.chars().next()?;
    Some((character, character.len_utf8()))
}

fn posix_char_class(name: &str) -> Option<Vec<char>> {
    let matches: fn(char) -> bool = match name {
        "alnum" => |ch| ch.is_ascii_alphanumeric(),
        "alpha" => |ch| ch.is_ascii_alphabetic(),
        "blank" => |ch| ch == ' ' || ch == '\t',
        "cntrl" => |ch| ch.is_ascii_control(),
        "digit" => |ch| ch.is_ascii_digit(),
        "graph" => |ch| ch.is_ascii_graphic(),
        "lower" => |ch| ch.is_ascii_lowercase(),
        "print" => |ch| ch.is_ascii_graphic() || ch == ' ',
        "punct" => |ch| ch.is_ascii_punctuation(),
        "space" => is_posix_space,
        "upper" => |ch| ch.is_ascii_uppercase(),
        "xdigit" => |ch| ch.is_ascii_hexdigit(),
        _ => return None,
    };
    Some(
        (0u8..=127)
            .map(char::from)
            .filter(|ch| matches(*ch))
            .collect(),
    )
}

pub fn wc<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut count_lines = false;
        let mut count_words = false;
        let mut count_bytes = false;
        let mut count_chars = false;
        let mut files = Vec::new();
        let mut options = true;

        for arg in args {
            if options && arg == "--" {
                options = false;
            } else if options && (arg == "-l" || arg == "--lines") {
                count_lines = true;
            } else if options && (arg == "-w" || arg == "--words") {
                count_words = true;
            } else if options && (arg == "-c" || arg == "--bytes") {
                count_bytes = true;
            } else if options && (arg == "-m" || arg == "--chars") {
                count_chars = true;
            } else if options && arg.starts_with('-') && arg != "-" {
                let mut supported = true;
                for flag in arg[1..].chars() {
                    match flag {
                        'l' => count_lines = true,
                        'w' => count_words = true,
                        'c' => count_bytes = true,
                        'm' => count_chars = true,
                        _ => {
                            supported = false;
                            break;
                        }
                    }
                }
                if !supported {
                    ctx.write_stderr(format!("esh: wc: unsupported option '{}'\n", arg).as_bytes());
                    return Ok(2);
                }
            } else {
                files.push(arg.as_str());
            }
        }

        // Default: -l -w -c
        if !count_lines && !count_words && !count_bytes && !count_chars {
            count_lines = true;
            count_words = true;
            count_bytes = true;
        }

        let analyze = |content: &[u8]| -> (usize, usize, usize, usize) {
            let text = String::from_utf8_lossy(content);
            let lines = content.iter().filter(|byte| **byte == b'\n').count();
            let words = text.split_whitespace().count();
            let bytes = content.len();
            let chars = text.chars().count();
            (lines, words, bytes, chars)
        };

        let format_counts =
            |lines: usize, words: usize, bytes: usize, chars: usize, label: Option<&str>| {
                let mut parts = Vec::new();
                if count_lines {
                    parts.push(lines.to_string());
                }
                if count_words {
                    parts.push(words.to_string());
                }
                if count_chars {
                    parts.push(chars.to_string());
                }
                if count_bytes {
                    parts.push(bytes.to_string());
                }
                if let Some(lbl) = label {
                    parts.push(lbl.to_string());
                }
                parts.join(" ")
            };

        let stdin_only = files.is_empty();
        let input_count = if stdin_only { 1 } else { files.len() };
        let mut total = (0usize, 0usize, 0usize, 0usize);
        let mut status = 0;

        for path_str in if stdin_only {
            vec![None]
        } else {
            files.into_iter().map(Some).collect()
        } {
            let content = match path_str {
                None | Some("-") => ctx.take_stdin().unwrap_or_default(),
                Some(path_str) => {
                    let path = resolve_path(ctx, path_str);
                    match fs::read(&path) {
                        Ok(content) => content,
                        Err(error) => {
                            ctx.write_stderr(
                                format!("esh: wc: cannot read '{}': {}\n", path_str, error)
                                    .as_bytes(),
                            );
                            status = 1;
                            continue;
                        }
                    }
                }
            };
            let counts = analyze(&content);
            total.0 += counts.0;
            total.1 += counts.1;
            total.2 += counts.2;
            total.3 += counts.3;
            let label = path_str.filter(|path| *path != "-");
            let out = format_counts(counts.0, counts.1, counts.2, counts.3, label);
            ctx.write_stdout(format!("{}\n", out).as_bytes());
        }

        if input_count > 1 {
            let out = format_counts(total.0, total.1, total.2, total.3, Some("total"));
            ctx.write_stdout(format!("{}\n", out).as_bytes());
        }
        Ok(status)
    })
}

#[cfg(test)]
mod grep_stream_tests {
    use super::*;
    use std::io::Read;

    struct MatchThenError {
        reads: usize,
    }

    impl Read for MatchThenError {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            if self.reads == 1 {
                let line = b"hit\n";
                buffer[..line.len()].copy_from_slice(line);
                Ok(line.len())
            } else {
                Err(io::Error::other(
                    "reader was consumed after a matching line",
                ))
            }
        }
    }

    #[test]
    fn quiet_grep_stops_reading_when_it_finds_a_match() {
        let regex = RegexBuilder::new("hit").build().unwrap();
        let source = MatchThenError { reads: 0 };
        let mut reader = BufReader::new(source);

        assert!(grep_reader_has_match(&mut reader, &regex, false).unwrap());
        assert_eq!(reader.get_ref().reads, 1);
    }
}
