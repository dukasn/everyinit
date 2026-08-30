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
use std::time::Duration;

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
        return Ok(Duration::from_secs_f64(secs));
    }
    if let Some(num_str) = s.strip_suffix('m') {
        let mins: f64 = num_str
            .parse()
            .map_err(|_| ExecError::Custom(format!("Invalid minutes duration: '{}'", s)))?;
        return Ok(Duration::from_secs_f64(mins * 60.0));
    }
    if let Some(num_str) = s.strip_suffix('h') {
        let hours: f64 = num_str
            .parse()
            .map_err(|_| ExecError::Custom(format!("Invalid hours duration: '{}'", s)))?;
        return Ok(Duration::from_secs_f64(hours * 3600.0));
    }
    if let Ok(secs) = s.parse::<f64>() {
        return Ok(Duration::from_secs_f64(secs));
    }
    Err(ExecError::Custom(format!(
        "Unrecognized duration format: '{}'",
        s
    )))
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
    let unescaped_fmt = unescape_string(fmt);
    let mut out = String::with_capacity(unescaped_fmt.len());
    let mut chars = unescaped_fmt.chars().peekable();
    let mut arg_idx = 0;

    while let Some(c) = chars.next() {
        if c == '%' {
            if chars.peek() == Some(&'%') {
                chars.next();
                out.push('%');
                continue;
            }

            let mut spec = String::new();
            while let Some(&sc) = chars.peek() {
                chars.next();
                spec.push(sc);
                if sc == 's'
                    || sc == 'd'
                    || sc == 'i'
                    || sc == 'f'
                    || sc == 'x'
                    || sc == 'X'
                    || sc == 'c'
                    || sc == 'b'
                {
                    break;
                }
            }

            let type_char = spec.chars().last().unwrap_or('s');
            let arg_val = args.get(arg_idx).map(|s| s.as_str()).unwrap_or("");
            arg_idx += 1;

            match type_char {
                's' => {
                    out.push_str(arg_val);
                }
                'b' => {
                    out.push_str(&unescape_string(arg_val));
                }
                'd' | 'i' => {
                    let num: i64 = arg_val.trim().parse().unwrap_or(0);
                    out.push_str(&num.to_string());
                }
                'f' => {
                    let num: f64 = arg_val.trim().parse().unwrap_or(0.0);
                    out.push_str(&format!("{:.2}", num));
                }
                'x' => {
                    let num: i64 = arg_val.trim().parse().unwrap_or(0);
                    out.push_str(&format!("{:x}", num));
                }
                'X' => {
                    let num: i64 = arg_val.trim().parse().unwrap_or(0);
                    out.push_str(&format!("{:X}", num));
                }
                'c' => {
                    if let Some(ch) = arg_val.chars().next() {
                        out.push(ch);
                    }
                }
                _ => {
                    out.push_str(arg_val);
                }
            }
        } else {
            out.push(c);
        }
    }
    out
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
            if *arg == "-n" {
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
            if output.ends_with('\n') {
                ctx.write_stdout(output.as_bytes());
            } else {
                ctx.write_stdout(format!("{}\n", output).as_bytes());
            }
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
            return Ok(0);
        }
        let fmt = &args[0];
        let rest = &args[1..];
        let output = format_c_style(fmt, rest);
        ctx.write_stdout(output.as_bytes());
        Ok(0)
    })
}

pub fn exit<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let code = args
            .first()
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(0);
        ctx.request_exit(code);
        Ok(code)
    })
}

pub fn r#return<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let code = args
            .first()
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(0);
        ctx.request_return(code);
        Ok(code)
    })
}

pub fn r#break<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let levels = args
            .first()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1);
        if !ctx.in_loop() {
            return Err(ExecError::Custom(
                "break: only meaningful in a `for', `while', or `until' loop".to_string(),
            ));
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
        let levels = args
            .first()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1);
        if !ctx.in_loop() {
            return Err(ExecError::Custom(
                "continue: only meaningful in a `for', `while', or `until' loop".to_string(),
            ));
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
        for arg in args {
            if let Some((key, val)) = arg.split_once('=') {
                ctx.set_env(key, val);
            } else if let Some(val) = ctx.get_var(arg) {
                ctx.set_env(arg, val);
            }
        }
        Ok(0)
    })
}

pub fn shift<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let count = args
            .first()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1);
        ctx.shift_positional_args(count);
        Ok(0)
    })
}

pub fn set<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut iter = args.iter().peekable();
        let mut positional = Vec::new();
        let mut parsing_flags = true;

        while let Some(arg) = iter.next() {
            if parsing_flags && arg == "--" {
                parsing_flags = false;
                continue;
            }

            if parsing_flags
                && (arg.starts_with('-') || arg.starts_with('+'))
                && arg.len() > 1
                && !arg.chars().nth(1).unwrap().is_ascii_digit()
            {
                let enable = arg.starts_with('-');
                let flag_chars: &str = &arg[1..];

                if flag_chars == "o" {
                    if let Some(opt_name) = iter.next() {
                        match opt_name.as_str() {
                            "errexit" | "e" => ctx.options_mut().errexit = enable,
                            "nounset" | "u" => ctx.options_mut().nounset = enable,
                            "xtrace" | "x" => ctx.options_mut().xtrace = enable,
                            "pipefail" => ctx.options_mut().pipefail = enable,
                            "noglob" | "f" => ctx.options_mut().noglob = enable,
                            "noclobber" | "C" => ctx.options_mut().noclobber = enable,
                            _ => {}
                        }
                    }
                    continue;
                }

                for ch in flag_chars.chars() {
                    match ch {
                        'e' => ctx.options_mut().errexit = enable,
                        'u' => ctx.options_mut().nounset = enable,
                        'x' => ctx.options_mut().xtrace = enable,
                        'f' => ctx.options_mut().noglob = enable,
                        'C' => ctx.options_mut().noclobber = enable,
                        _ => {}
                    }
                }
                continue;
            }

            positional.push(arg.clone());
        }

        if !positional.is_empty() || args.iter().any(|a| a == "--") {
            ctx.set_positional_params(&positional);
        }

        Ok(0)
    })
}

pub fn sleep<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
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
            ctx.write_stderr(b"esh: retry: usage: retry [-n attempts] [-d delay] <command...>\n");
            return Ok(1);
        }

        let mut attempts = 3usize;
        let mut delay = Duration::from_millis(500);
        let mut factor = 1.0f64;
        let mut cmd_start_idx = 0usize;

        let mut iter = args.iter().enumerate().peekable();
        while let Some((idx, arg)) = iter.next() {
            if arg == "-n" || arg == "--attempts" {
                if let Some((_, val)) = iter.next()
                    && let Ok(n) = val.parse::<usize>()
                {
                    attempts = n;
                }
            } else if arg == "-d" || arg == "--delay" {
                if let Some((_, val)) = iter.next()
                    && let Ok(d) = parse_duration(val)
                {
                    delay = d;
                }
            } else if arg == "-b" || arg == "--backoff" {
                if let Some((_, val)) = iter.next()
                    && let Ok(f) = val.parse::<f64>()
                {
                    factor = f;
                }
            } else {
                cmd_start_idx = idx;
                break;
            }
        }

        if cmd_start_idx >= args.len() {
            ctx.write_stderr(b"esh: retry: missing command to execute\n");
            return Ok(1);
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
                current_delay = Duration::from_secs_f64(current_delay.as_secs_f64() * factor);
            }
        }

        Ok(last_code)
    })
}

pub fn wait<'a>(
    _args: &'a [String],
    _ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        // Yield to allow background tokio tasks to make progress
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
            ctx.write_stderr(b"esh: trap: usage: trap <handler_or_block> <SIGNAL>\n");
            return Ok(1);
        }

        if args.len() == 1 {
            if let Some(sig) = TrapSignal::parse(&args[0]) {
                ctx.clear_trap(sig);
                return Ok(0);
            }
            ctx.write_stderr(format!("esh: trap: unknown signal '{}'\n", args[0]).as_bytes());
            return Ok(1);
        }

        let handler_str = &args[0];
        let sig_str = &args[1];

        let sig = match TrapSignal::parse(sig_str) {
            Some(s) => s,
            None => {
                ctx.write_stderr(format!("esh: trap: unknown signal '{}'\n", sig_str).as_bytes());
                return Ok(1);
            }
        };

        if handler_str == "-" || handler_str.is_empty() {
            ctx.clear_trap(sig);
            return Ok(0);
        }

        let plan = compile_script(handler_str)
            .map_err(|e| ExecError::Custom(format!("Invalid trap handler script: {}", e)))?;

        ctx.set_trap(sig, plan);
        Ok(0)
    })
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
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "-r" => raw = true,
                "-s" | "--silent" => is_secret = true,
                "-c" | "--confirm" => is_confirm = true,
                "--menu" => is_menu = true,
                "-p" => {
                    if let Some(p) = iter.next() {
                        prompt = Some(p.as_str());
                    }
                }
                "-d" => {
                    if let Some(d) = iter.next() {
                        delimiter = d.bytes().next().unwrap_or(0);
                    }
                }
                "-t" => {
                    if let Some(t) = iter.next() {
                        if let Ok(d) = parse_duration(t) {
                            timeout_dur = Some(d);
                        } else {
                            ctx.write_stderr(b"esh: read: invalid timeout duration\n");
                            return Ok(2);
                        }
                    }
                }
                "-m" | "--mask" => {
                    if let Some(m) = iter.next() {
                        mask_char = m.chars().next();
                    }
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
                    if let Some(d) = iter.next() {
                        default_val = Some(d.clone());
                    }
                }
                "--defaults" => {
                    if let Some(d) = iter.next() {
                        defaults_var = Some(d.clone());
                    }
                }
                other if other.starts_with("-p") => {
                    prompt = Some(&other[2..]);
                }
                other if other.starts_with("-d") => {
                    delimiter = other[2..].bytes().next().unwrap_or(0);
                }
                other if other.starts_with("-t") => {
                    if let Ok(d) = parse_duration(&other[2..]) {
                        timeout_dur = Some(d);
                    } else {
                        ctx.write_stderr(b"esh: read: invalid timeout duration\n");
                        return Ok(2);
                    }
                }
                other if other.starts_with("-m") => {
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

        // ==========================================
        // 1. MENU MODE
        // ==========================================
        if is_menu {
            let mut menu_items: Vec<(String, String)> = Vec::new();
            let target_var: String;

            // Check if stdin has piped data for menu options
            let piped_stdin = ctx.take_stdin();
            if let Some(stdin_bytes) = piped_stdin {
                let text = String::from_utf8_lossy(&stdin_bytes);
                for line in text.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        menu_items.push((trimmed.to_string(), trimmed.to_string()));
                    }
                }

                target_var = if let Some(ref arr_t) = array_target {
                    arr_t.clone()
                } else if let Some(first_pos) = positional_args.first() {
                    first_pos.clone()
                } else {
                    "REPLY".to_string()
                };
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

                    if !rest_bytes.is_empty() {
                        ctx.set_stdin(rest_bytes);
                    }

                    let line_str = String::from_utf8_lossy(&line_bytes)
                        .trim_end_matches('\r')
                        .to_string();
                    Ok(line_str)
                }
                None => {
                    use tokio::io::AsyncBufReadExt;
                    let stdin = tokio::io::stdin();
                    let mut reader = tokio::io::BufReader::new(stdin);
                    let mut line_bytes = Vec::new();
                    let n = reader
                        .read_until(delimiter, &mut line_bytes)
                        .await
                        .map_err(|e| ExecError::IoError(e.to_string()))?;
                    if n == 0 {
                        return Err(ExecError::Custom("EOF".to_string()));
                    }
                    if line_bytes.ends_with(&[delimiter]) {
                        line_bytes.pop();
                    }
                    let line_str = String::from_utf8_lossy(&line_bytes)
                        .trim_end_matches(&['\r', '\n'][..])
                        .to_string();
                    Ok(line_str)
                }
            }
        };

        let line_res = if let Some(dur) = timeout_dur {
            match tokio::time::timeout(dur, read_future).await {
                Ok(res) => res,
                Err(_) => return Ok(1), // Timeout
            }
        } else {
            read_future.await
        };

        let line = match (line_res, default_val) {
            (Ok(line), Some(default)) if line.is_empty() => default,
            (Ok(line), _) => line,
            (Err(_), Some(default)) => default,
            (Err(_), None) => return Ok(1), // EOF or Error
        };

        let ifs = ctx.get_var("IFS").unwrap_or_else(|| " \t\n".to_string());
        let ifs_chars: Vec<char> = ifs.chars().collect();

        if var_names.len() == 1 {
            ctx.set_scoped_var(&var_names[0], line);
        } else {
            let mut words = Vec::new();
            let mut current = String::new();
            for c in line.chars() {
                if ifs_chars.contains(&c) {
                    if !current.is_empty() {
                        words.push(std::mem::take(&mut current));
                    }
                } else {
                    current.push(c);
                }
            }
            if !current.is_empty() {
                words.push(current);
            }

            for (idx, var) in var_names.iter().enumerate() {
                if idx == var_names.len() - 1 {
                    let remaining = if idx < words.len() {
                        words[idx..].join(" ")
                    } else {
                        String::new()
                    };
                    ctx.set_scoped_var(var, remaining);
                } else {
                    let word = words.get(idx).cloned().unwrap_or_default();
                    ctx.set_scoped_var(var, word);
                }
            }
        }

        Ok(0)
    })
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

        let mut names_and_values = Vec::new();

        for arg in args {
            if arg == "-A" {
                is_assoc_map = true;
            } else if arg == "-a" {
                is_indexed_array = true;
            } else if arg == "-x" {
                is_export = true;
            } else if arg == "-r" {
                // Readonly marker
            } else if !arg.starts_with('-') {
                names_and_values.push(arg.as_str());
            }
        }

        for item in names_and_values {
            if let Some((k, v)) = item.split_once('=') {
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
                    ctx.set_scoped_var(k, v.to_string());
                }
                if is_export {
                    ctx.set_env(k, v);
                }
            } else {
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
                    ctx.set_scoped_var(item, String::new());
                }
            }
        }

        Ok(0)
    })
}
