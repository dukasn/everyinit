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
use crate::runtime::context::ExecutionContext;
use crate::runtime::traits::BoxFuture;
use regex::RegexBuilder;
use std::fs;
use std::path::{Path, PathBuf};

fn resolve_path(ctx: &ExecutionContext, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        ctx.cwd().join(p)
    }
}

pub fn grep<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(
                b"esh: grep: usage: grep [-F] [-i] [-v] [-q] [-n] <pattern> [file...]\n",
            );
            return Ok(2);
        }

        let mut case_insensitive = false;
        let mut invert = false;
        let mut quiet = false;
        let mut line_numbers = false;
        let mut fixed_strings = false;
        let mut pattern: Option<&str> = None;
        let mut files = Vec::new();
        let mut options = true;

        for arg in args {
            if options && arg == "--" {
                options = false;
            } else if options && (arg == "-i" || arg == "--ignore-case") {
                case_insensitive = true;
            } else if options && (arg == "-v" || arg == "--invert-match") {
                invert = true;
            } else if options && (arg == "-q" || arg == "--quiet" || arg == "--silent") {
                quiet = true;
            } else if options && (arg == "-n" || arg == "--line-number") {
                line_numbers = true;
            } else if options && (arg == "-F" || arg == "--fixed-strings") {
                fixed_strings = true;
            } else if options && (arg == "-E" || arg == "--extended-regexp") {
                fixed_strings = false;
            } else if options
                && pattern.is_none()
                && arg.starts_with('-')
                && arg.len() > 2
                && !arg.starts_with("--")
            {
                for flag in arg[1..].chars() {
                    match flag {
                        'i' => case_insensitive = true,
                        'v' => invert = true,
                        'q' => quiet = true,
                        'n' => line_numbers = true,
                        'F' => fixed_strings = true,
                        'E' => fixed_strings = false,
                        _ => {
                            ctx.write_stderr(
                                format!("esh: grep: unsupported option '-{}'\n", flag).as_bytes(),
                            );
                            return Ok(2);
                        }
                    }
                }
            } else if options && pattern.is_none() && arg.starts_with('-') {
                ctx.write_stderr(format!("esh: grep: unsupported option '{}'\n", arg).as_bytes());
                return Ok(2);
            } else if pattern.is_none() {
                pattern = Some(arg.as_str());
            } else {
                files.push(arg.as_str());
            }
        }

        let pat_str = match pattern {
            Some(p) => p,
            None => {
                ctx.write_stderr(b"esh: grep: missing pattern operand\n");
                return Ok(2);
            }
        };

        let regex_pattern = if fixed_strings {
            regex::escape(pat_str)
        } else {
            pat_str.to_string()
        };
        let re = match RegexBuilder::new(&regex_pattern)
            .case_insensitive(case_insensitive)
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                ctx.write_stderr(
                    format!("esh: grep: invalid regex pattern '{}': {}\n", pat_str, e).as_bytes(),
                );
                return Ok(2);
            }
        };

        let mut match_found = false;

        let mut process_lines =
            |text: &str, file_prefix: Option<&str>, ctx: &mut ExecutionContext| {
                for (idx, line) in text.lines().enumerate() {
                    let is_match = re.is_match(line);
                    let selected = if invert { !is_match } else { is_match };

                    if selected {
                        match_found = true;
                        if !quiet {
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
                    }
                }
            };

        if files.is_empty() {
            if let Some(buf) = ctx.take_stdin() {
                let s = String::from_utf8_lossy(&buf);
                process_lines(&s, None, ctx);
            }
        } else {
            let show_file_prefix = files.len() > 1;
            for path_str in &files {
                let path = resolve_path(ctx, path_str);
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        let prefix = if show_file_prefix {
                            Some(*path_str)
                        } else {
                            None
                        };
                        process_lines(&content, prefix, ctx);
                    }
                    Err(e) => {
                        ctx.write_stderr(
                            format!("esh: grep: cannot read '{}': {}\n", path_str, e).as_bytes(),
                        );
                    }
                }
            }
        }

        if match_found { Ok(0) } else { Ok(1) }
    })
}

pub fn cut<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut delim = "\t".to_string();
        let mut fields: Vec<usize> = Vec::new();
        let mut files = Vec::new();

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "-d" || arg == "--delimiter" {
                if let Some(d) = iter.next() {
                    delim = d.clone();
                }
            } else if let Some(d) = arg.strip_prefix("-d") {
                delim = d.to_string();
            } else if arg == "-f" || arg == "--fields" {
                if let Some(f) = iter.next() {
                    for part in f.split(',') {
                        if let Ok(idx) = part.parse::<usize>()
                            && idx > 0
                        {
                            fields.push(idx - 1);
                        }
                    }
                }
            } else if let Some(f) = arg.strip_prefix("-f") {
                for part in f.split(',') {
                    if let Ok(idx) = part.parse::<usize>()
                        && idx > 0
                    {
                        fields.push(idx - 1);
                    }
                }
            } else if !arg.starts_with('-') {
                files.push(arg.as_str());
            }
        }

        let process_text = |text: &str, ctx: &mut ExecutionContext| {
            for line in text.lines() {
                let parts: Vec<&str> = line.split(&delim).collect();
                if fields.is_empty() {
                    ctx.write_stdout(format!("{}\n", line).as_bytes());
                } else {
                    let mut selected = Vec::new();
                    for &f_idx in &fields {
                        if f_idx < parts.len() {
                            selected.push(parts[f_idx]);
                        }
                    }
                    ctx.write_stdout(format!("{}\n", selected.join(&delim)).as_bytes());
                }
            }
        };

        if files.is_empty() {
            if let Some(buf) = ctx.take_stdin() {
                let s = String::from_utf8_lossy(&buf);
                process_text(&s, ctx);
            }
            return Ok(0);
        }

        for path_str in files {
            let path = resolve_path(ctx, path_str);
            match fs::read_to_string(&path) {
                Ok(content) => process_text(&content, ctx),
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: cut: cannot read '{}': {}\n", path_str, e).as_bytes(),
                    );
                    return Ok(1);
                }
            }
        }

        Ok(0)
    })
}

pub fn tr<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: tr: usage: tr [-d] <set1> [set2]\n");
            return Ok(1);
        }

        let mut delete_mode = false;
        let mut operands = Vec::new();

        for arg in args {
            if arg == "-d" || arg == "--delete" {
                delete_mode = true;
            } else if !arg.starts_with('-') || operands.is_empty() {
                operands.push(arg.as_str());
            }
        }

        let input = match ctx.take_stdin() {
            Some(buf) => String::from_utf8_lossy(&buf).to_string(),
            None => String::new(),
        };

        if delete_mode {
            if operands.is_empty() {
                ctx.write_stderr(b"esh: tr: missing operand after -d\n");
                return Ok(1);
            }
            let del_chars = expand_char_set(operands[0]);
            let output: String = input.chars().filter(|c| !del_chars.contains(c)).collect();
            ctx.write_stdout(output.as_bytes());
            return Ok(0);
        }

        if operands.is_empty() {
            ctx.write_stderr(b"esh: tr: missing operand\n");
            return Ok(1);
        }

        let set1_str = operands[0];
        let set2_str = if operands.len() > 1 { operands[1] } else { "" };

        // Special case: [:lower:] -> [:upper:]
        if (set1_str == "[:lower:]" || set1_str == "a-z")
            && (set2_str == "[:upper:]" || set2_str == "A-Z")
        {
            ctx.write_stdout(input.to_uppercase().as_bytes());
            return Ok(0);
        }
        if (set1_str == "[:upper:]" || set1_str == "A-Z")
            && (set2_str == "[:lower:]" || set2_str == "a-z")
        {
            ctx.write_stdout(input.to_lowercase().as_bytes());
            return Ok(0);
        }

        let set1 = expand_char_set(set1_str);
        let set2 = expand_char_set(set2_str);

        let mut output = String::with_capacity(input.len());
        for c in input.chars() {
            if let Some(pos) = set1.iter().position(|&x| x == c) {
                if pos < set2.len() {
                    output.push(set2[pos]);
                } else if let Some(&last) = set2.last() {
                    output.push(last);
                } else {
                    output.push(c);
                }
            } else {
                output.push(c);
            }
        }

        ctx.write_stdout(output.as_bytes());
        Ok(0)
    })
}

fn expand_char_set(s: &str) -> Vec<char> {
    let mut chars = Vec::new();
    let raw = s.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if i + 2 < raw.len() && raw[i + 1] == b'-' {
            let start = raw[i] as char;
            let end = raw[i + 2] as char;
            for c in start..=end {
                chars.push(c);
            }
            i += 3;
        } else {
            chars.push(raw[i] as char);
            i += 1;
        }
    }
    chars
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

        for arg in args {
            if arg == "-l" || arg == "--lines" {
                count_lines = true;
            } else if arg == "-w" || arg == "--words" {
                count_words = true;
            } else if arg == "-c" || arg == "--bytes" {
                count_bytes = true;
            } else if arg == "-m" || arg == "--chars" {
                count_chars = true;
            } else if !arg.starts_with('-') {
                files.push(arg.as_str());
            }
        }

        // Default: -l -w -c
        if !count_lines && !count_words && !count_bytes && !count_chars {
            count_lines = true;
            count_words = true;
            count_bytes = true;
        }

        let analyze = |content: &str| -> (usize, usize, usize, usize) {
            let lines = content.lines().count();
            let words = content.split_whitespace().count();
            let bytes = content.len();
            let chars = content.chars().count();
            (lines, words, bytes, chars)
        };

        let format_counts = |lines: usize,
                             words: usize,
                             bytes: usize,
                             chars: usize,
                             label: Option<&str>|
         -> String {
            let mut parts = Vec::new();
            if count_lines {
                parts.push(lines.to_string());
            }
            if count_words {
                parts.push(words.to_string());
            }
            if count_bytes {
                parts.push(bytes.to_string());
            }
            if count_chars && !count_bytes {
                parts.push(chars.to_string());
            }
            if let Some(lbl) = label {
                parts.push(lbl.to_string());
            }
            parts.join(" ")
        };

        if files.is_empty() {
            if let Some(buf) = ctx.take_stdin() {
                let s = String::from_utf8_lossy(&buf);
                let (l, w, b, c) = analyze(&s);
                let out = format_counts(l, w, b, c, None);
                ctx.write_stdout(format!("{}\n", out).as_bytes());
            }
            return Ok(0);
        }

        for path_str in files {
            let path = resolve_path(ctx, path_str);
            match fs::read_to_string(&path) {
                Ok(content) => {
                    let (l, w, b, c) = analyze(&content);
                    let out = format_counts(l, w, b, c, Some(path_str));
                    ctx.write_stdout(format!("{}\n", out).as_bytes());
                }
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: wc: cannot read '{}': {}\n", path_str, e).as_bytes(),
                    );
                    return Ok(1);
                }
            }
        }

        Ok(0)
    })
}
