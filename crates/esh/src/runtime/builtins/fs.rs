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
use crate::runtime::path_presentation::present;
use crate::runtime::traits::{BoxFuture, ScopeManager};
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

#[cfg(unix)]
fn set_touch_times(path: &Path, access: bool, modification: bool) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains a NUL byte")
    })?;
    let times = [
        libc::timespec {
            tv_sec: 0,
            tv_nsec: if access {
                libc::UTIME_NOW
            } else {
                libc::UTIME_OMIT
            },
        },
        libc::timespec {
            tv_sec: 0,
            tv_nsec: if modification {
                libc::UTIME_NOW
            } else {
                libc::UTIME_OMIT
            },
        },
    ];

    // SAFETY: `path` is a valid NUL-terminated path and `times` points to two
    // initialized timespec values for the duration of the call.
    let result = unsafe { libc::utimensat(libc::AT_FDCWD, path.as_ptr(), times.as_ptr(), 0) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
fn set_touch_times(path: &Path, access: bool, modification: bool) -> std::io::Result<()> {
    let file = fs::OpenOptions::new().write(true).open(path)?;
    let now = std::time::SystemTime::now();
    let mut times = fs::FileTimes::new();
    if access {
        times = times.set_accessed(now);
    }
    if modification {
        times = times.set_modified(now);
    }
    file.set_times(times)
}

fn set_touch_times_from(
    path: &Path,
    accessed: Option<std::time::SystemTime>,
    modified: Option<std::time::SystemTime>,
) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains a NUL byte")
        })?;
        let time = |value| match value {
            Some(value) => system_time_to_timespec(value),
            None => libc::timespec {
                tv_sec: 0,
                tv_nsec: libc::UTIME_OMIT,
            },
        };
        let times = [time(accessed), time(modified)];
        // SAFETY: path and times remain valid for the duration of the call.
        let result = unsafe { libc::utimensat(libc::AT_FDCWD, path.as_ptr(), times.as_ptr(), 0) };
        if result == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    #[cfg(not(unix))]
    {
        let file = fs::OpenOptions::new().write(true).open(path)?;
        let mut times = fs::FileTimes::new();
        if let Some(accessed) = accessed {
            times = times.set_accessed(accessed);
        }
        if let Some(modified) = modified {
            times = times.set_modified(modified);
        }
        file.set_times(times)
    }
}

fn parse_touch_timestamp(value: &str) -> Result<std::time::SystemTime, String> {
    use chrono::{Datelike, Local, LocalResult, TimeZone};

    let (digits, seconds) = match value.split_once('.') {
        Some((digits, seconds))
            if seconds.len() == 2 && seconds.bytes().all(|b| b.is_ascii_digit()) =>
        {
            (
                digits,
                seconds.parse::<u32>().map_err(|_| "invalid seconds")?,
            )
        }
        Some(_) => return Err("expected exactly two digits after '.'".to_owned()),
        None => (value, 0),
    };
    if !matches!(digits.len(), 8 | 10 | 12) || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err("expected [[CC]YY]MMDDhhmm[.SS]".to_owned());
    }

    let (year, fields) = match digits.len() {
        8 => (Local::now().year(), digits),
        10 => {
            let short_year = digits[..2].parse::<i32>().map_err(|_| "invalid year")?;
            (
                if short_year >= 69 {
                    1900 + short_year
                } else {
                    2000 + short_year
                },
                &digits[2..],
            )
        }
        12 => (
            digits[..4].parse::<i32>().map_err(|_| "invalid year")?,
            &digits[4..],
        ),
        _ => unreachable!(),
    };
    let parse_field = |range: std::ops::Range<usize>| {
        fields[range]
            .parse::<u32>()
            .map_err(|_| "invalid date or time field")
    };
    let month = parse_field(0..2)?;
    let day = parse_field(2..4)?;
    let hour = parse_field(4..6)?;
    let minute = parse_field(6..8)?;
    if seconds > 60 {
        return Err("seconds must be between 00 and 60".to_owned());
    }

    let base_second = seconds.min(59);
    let local = match Local.with_ymd_and_hms(year, month, day, hour, minute, base_second) {
        LocalResult::Single(value) => value,
        LocalResult::Ambiguous(_, _) => {
            return Err("timestamp is ambiguous in the local timezone".to_owned());
        }
        LocalResult::None => return Err("invalid or nonexistent local date and time".to_owned()),
    };
    let local = if seconds == 60 {
        local
            .checked_add_signed(chrono::Duration::seconds(1))
            .ok_or_else(|| "timestamp is out of range".to_owned())?
    } else {
        local
    };
    system_time_from_unix_parts(local.timestamp(), local.timestamp_subsec_nanos())
}

fn parse_touch_date_time(value: &str) -> Result<std::time::SystemTime, String> {
    use chrono::{Local, LocalResult, NaiveDate, TimeZone};

    let (value, utc) = value
        .strip_suffix('Z')
        .map_or((value, false), |value| (value, true));
    let (date, time) = value
        .split_once('T')
        .or_else(|| value.split_once(' '))
        .ok_or_else(|| "expected YYYY-MM-DDThh:mm:SS[.frac][Z]".to_owned())?;
    let mut date_fields = date.split('-');
    let year_text = date_fields.next().unwrap_or_default();
    let month_text = date_fields.next().unwrap_or_default();
    let day_text = date_fields.next().unwrap_or_default();
    if date_fields.next().is_some()
        || year_text.len() < 4
        || month_text.len() != 2
        || day_text.len() != 2
        || !year_text.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("invalid calendar date".to_owned());
    }
    let year = year_text
        .parse::<i32>()
        .map_err(|_| "year is out of range".to_owned())?;
    let month = month_text
        .parse::<u32>()
        .map_err(|_| "invalid month".to_owned())?;
    let day = day_text
        .parse::<u32>()
        .map_err(|_| "invalid day".to_owned())?;

    let mut time_fields = time.split(':');
    let hour_text = time_fields.next().unwrap_or_default();
    let minute_text = time_fields.next().unwrap_or_default();
    if hour_text.len() != 2 || minute_text.len() != 2 {
        return Err("hour and minute fields must have two digits".to_owned());
    }
    let hour = hour_text
        .parse::<u32>()
        .map_err(|_| "invalid hour".to_owned())?;
    let minute = minute_text
        .parse::<u32>()
        .map_err(|_| "invalid minute".to_owned())?;
    let second_field = time_fields.next().unwrap_or_default();
    if time_fields.next().is_some() {
        return Err("invalid time".to_owned());
    }
    let (second_text, fraction) = second_field
        .find(['.', ','])
        .map(|index| (&second_field[..index], Some(&second_field[index + 1..])))
        .unwrap_or((second_field, None));
    let second = second_text
        .parse::<u32>()
        .map_err(|_| "invalid second".to_owned())?;
    if second_text.len() != 2 || second > 60 {
        return Err("seconds must be two digits in the range 00 through 60".to_owned());
    }
    let nanos = if let Some(fraction) = fraction {
        if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("fraction must contain decimal digits".to_owned());
        }
        let nanos_text = &fraction[..fraction.len().min(9)];
        let parsed = nanos_text
            .parse::<u32>()
            .map_err(|_| "invalid fractional second".to_owned())?;
        parsed * 10_u32.pow((9 - nanos_text.len()) as u32)
    } else {
        0
    };

    let date = NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| "invalid calendar date".to_owned())?;
    let datetime = date
        .and_hms_nano_opt(hour, minute, second.min(59), nanos)
        .ok_or_else(|| "invalid time".to_owned())?;
    let datetime = if second == 60 {
        datetime
            .checked_add_signed(chrono::Duration::seconds(1))
            .ok_or_else(|| "timestamp is out of range".to_owned())?
    } else {
        datetime
    };
    if utc {
        let datetime = datetime.and_utc();
        system_time_from_unix_parts(datetime.timestamp(), datetime.timestamp_subsec_nanos())
    } else {
        match Local.from_local_datetime(&datetime) {
            LocalResult::Single(datetime) => {
                system_time_from_unix_parts(datetime.timestamp(), datetime.timestamp_subsec_nanos())
            }
            LocalResult::Ambiguous(_, _) => {
                Err("timestamp is ambiguous in the local timezone".to_owned())
            }
            LocalResult::None => Err("invalid or nonexistent local date and time".to_owned()),
        }
    }
}

fn system_time_from_unix_parts(
    timestamp: i64,
    nanos: u32,
) -> Result<std::time::SystemTime, String> {
    use std::time::{Duration, UNIX_EPOCH};
    if timestamp >= 0 {
        UNIX_EPOCH
            .checked_add(Duration::new(timestamp as u64, nanos))
            .ok_or_else(|| "timestamp is out of range".to_owned())
    } else {
        let magnitude = timestamp.unsigned_abs();
        let duration = if nanos == 0 {
            Duration::new(magnitude, 0)
        } else {
            Duration::new(magnitude - 1, 1_000_000_000 - nanos)
        };
        UNIX_EPOCH
            .checked_sub(duration)
            .ok_or_else(|| "timestamp is out of range".to_owned())
    }
}

fn write_multi_file_header(
    path: &str,
    enabled: bool,
    first: &mut bool,
    ctx: &mut ExecutionContext,
) {
    if !enabled {
        return;
    }
    if !*first {
        ctx.write_stdout(b"\n");
    }
    ctx.write_stdout(format!("==> {} <==\n", path).as_bytes());
    *first = false;
}

pub fn pwd<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut logical = true;
        let mut end_of_options = false;
        for arg in args {
            if end_of_options {
                ctx.write_stderr(format!("esh: pwd: unsupported operand '{arg}'\n").as_bytes());
                return Ok(2);
            }
            if arg == "--" {
                end_of_options = true;
                continue;
            }
            if let Some(options) = arg.strip_prefix('-') {
                if options.is_empty() {
                    ctx.write_stderr(format!("esh: pwd: unsupported operand '{arg}'\n").as_bytes());
                    return Ok(2);
                }
                for option in options.chars() {
                    match option {
                        'L' => logical = true,
                        'P' => logical = false,
                        _ => {
                            ctx.write_stderr(
                                format!("esh: pwd: unsupported operand '{arg}'\n").as_bytes(),
                            );
                            return Ok(2);
                        }
                    }
                }
                continue;
            }
            ctx.write_stderr(format!("esh: pwd: unsupported operand '{arg}'\n").as_bytes());
            return Ok(2);
        }

        let physical = present(ctx.cwd());
        let cwd_str = if logical {
            ctx.get_var("PWD")
                .filter(|pwd| {
                    Path::new(pwd).is_absolute()
                        && !pwd
                            .split(|ch| {
                                ch == std::path::MAIN_SEPARATOR || (cfg!(windows) && ch == '/')
                            })
                            .any(|component| component == "." || component == "..")
                        && fs::canonicalize(pwd).is_ok_and(|resolved| resolved == ctx.cwd())
                })
                .unwrap_or(physical)
        } else {
            physical
        };
        ctx.write_stdout(format!("{}\n", cwd_str).as_bytes());
        Ok(0)
    })
}

pub fn cd<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut logical = true;
        let mut operand = None;
        let mut parsing_options = true;
        let mut end_of_options = false;
        for arg in args {
            if parsing_options && arg == "--" {
                parsing_options = false;
                end_of_options = true;
            } else if parsing_options && arg.starts_with('-') && arg != "-" {
                for option in arg[1..].chars() {
                    match option {
                        'L' => logical = true,
                        'P' => logical = false,
                        _ => {
                            ctx.write_stderr(
                                format!("esh: cd: unsupported option '-{}'\n", option).as_bytes(),
                            );
                            return Ok(2);
                        }
                    }
                }
            } else if operand.replace(arg.as_str()).is_some() {
                ctx.write_stderr(b"esh: cd: too many operands\n");
                return Ok(2);
            } else {
                parsing_options = false;
            }
        }

        let old_pwd = logical_pwd(ctx);
        if operand == Some("") {
            ctx.write_stderr(b"esh: cd: empty directory operand\n");
            return Ok(1);
        }
        let oldpwd_shortcut = operand == Some("-") && !end_of_options;
        let target = match (operand, oldpwd_shortcut) {
            (Some("-"), true) => match ctx.get_var("OLDPWD") {
                Some(path) if !path.is_empty() => path,
                _ => {
                    ctx.write_stderr(b"esh: cd: OLDPWD is not set\n");
                    return Ok(1);
                }
            },
            (Some(path), false) => path.to_string(),
            (None, _) => ctx
                .get_var("HOME")
                .filter(|path| !path.is_empty())
                .or_else(|| ctx.get_var("USERPROFILE").filter(|path| !path.is_empty()))
                .unwrap_or_else(|| ".".to_string()),
            _ => unreachable!("the OLDPWD shortcut has a matching operand"),
        };

        let mut candidates = Vec::new();
        let search_cdpath = operand.is_some_and(|arg| {
            (arg != "-" || end_of_options)
                && arg != "."
                && arg != ".."
                && !Path::new(arg).is_absolute()
                && Path::new(arg).components().count() == 1
        });
        if search_cdpath && let Some(cdpath) = ctx.get_var("CDPATH") {
            for entry in cdpath.split(':') {
                let candidate = if entry.is_empty() {
                    target.clone()
                } else {
                    Path::new(entry)
                        .join(&target)
                        .to_string_lossy()
                        .into_owned()
                };
                candidates.push((candidate, !entry.is_empty()));
            }
        }
        candidates.push((target, false));

        let mut last_error = None;
        for (candidate, print_path) in candidates {
            let next_logical_pwd = if logical {
                logical_candidate_path(&old_pwd, &candidate)
            } else {
                None
            };
            let directory_to_enter = next_logical_pwd.as_deref().unwrap_or(candidate.as_str());
            match ctx.set_cwd(directory_to_enter) {
                Ok(_) => {
                    let physical_pwd = crate::runtime::path_presentation::present(ctx.cwd());
                    let new_pwd = next_logical_pwd
                        .filter(|path| {
                            fs::canonicalize(path).is_ok_and(|canonical| canonical == ctx.cwd())
                        })
                        .unwrap_or(physical_pwd);
                    ctx.set_exported_var("OLDPWD", old_pwd.clone());
                    ctx.set_exported_var("PWD", new_pwd.clone());
                    if oldpwd_shortcut || print_path {
                        ctx.write_stdout(format!("{new_pwd}\n").as_bytes());
                    }
                    return Ok(0);
                }
                Err(error) => last_error = Some(error),
            }
        }
        let error = last_error.expect("the direct target is always attempted");
        ctx.write_stderr(format!("esh: cd: {}\n", error).as_bytes());
        Ok(1)
    })
}

fn logical_pwd(ctx: &ExecutionContext) -> String {
    if let Some(pwd) = ctx.get_var("PWD")
        && Path::new(&pwd).is_absolute()
        && fs::canonicalize(&pwd).is_ok_and(|canonical| canonical == ctx.cwd())
    {
        pwd
    } else {
        crate::runtime::path_presentation::present(ctx.cwd())
    }
}

fn logical_candidate_path(base: &str, candidate: &str) -> Option<String> {
    let candidate_path = Path::new(candidate);
    let joined = if candidate_path.is_absolute() {
        candidate.to_owned()
    } else {
        format!("{}/{}", base.trim_end_matches('/'), candidate)
    };
    normalize_absolute_path(&joined)
}

fn normalize_absolute_path(path: &str) -> Option<String> {
    if !path.starts_with('/') {
        return None;
    }
    let mut components: Vec<&str> = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            component => components.push(component),
        }
    }
    if components.is_empty() {
        Some("/".to_owned())
    } else {
        Some(format!("/{}", components.join("/")))
    }
}

pub fn mkdir<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut parents = false;
        let mut requested_mode = None;
        let mut paths = Vec::new();
        let mut options = true;
        let mut index = 0;

        while index < args.len() {
            let arg = &args[index];
            if options && arg == "--" {
                options = false;
            } else if options && arg == "--parents" {
                parents = true;
            } else if options && arg.starts_with('-') && arg != "-" {
                let option_chars: Vec<char> = arg[1..].chars().collect();
                let mut option_index = 0;
                while option_index < option_chars.len() {
                    match option_chars[option_index] {
                        'p' => parents = true,
                        'm' => {
                            let mode_string = if option_index + 1 < option_chars.len() {
                                option_chars[option_index + 1..].iter().collect::<String>()
                            } else {
                                index += 1;
                                match args.get(index) {
                                    Some(value) => value.clone(),
                                    None => {
                                        ctx.write_stderr(b"esh: mkdir: -m requires a mode\n");
                                        return Ok(2);
                                    }
                                }
                            };
                            match parse_directory_mode(&mode_string) {
                                Some(mode) => requested_mode = Some(mode),
                                None => {
                                    ctx.write_stderr(
                                        format!(
                                            "esh: mkdir: invalid or unsupported mode '{}'\n",
                                            mode_string
                                        )
                                        .as_bytes(),
                                    );
                                    return Ok(2);
                                }
                            }
                            break;
                        }
                        option => {
                            ctx.write_stderr(
                                format!("esh: mkdir: unsupported option '-{}'\n", option)
                                    .as_bytes(),
                            );
                            return Ok(2);
                        }
                    }
                    option_index += 1;
                }
            } else {
                options = false;
                paths.push(arg.as_str());
            }
            index += 1;
        }

        if paths.is_empty() {
            ctx.write_stderr(b"esh: mkdir: missing operand\n");
            return Ok(1);
        }

        if requested_mode.is_some() && !cfg!(unix) {
            ctx.write_stderr(b"esh: mkdir: -m is unsupported on this platform\n");
            return Ok(2);
        }

        let mut status = 0;
        for path_str in paths {
            let path = resolve_path(ctx, path_str);
            let already_directory = path.is_dir();
            let res = if parents {
                create_dir_with_posix_parents(&path)
            } else {
                fs::create_dir(&path)
            };

            if let Err(error) = res {
                ctx.write_stderr(
                    format!(
                        "esh: mkdir: cannot create directory '{}': {}\n",
                        path_str, error
                    )
                    .as_bytes(),
                );
                status = 1;
                continue;
            }

            if let Some(mode) = requested_mode
                && !already_directory
                && let Err(error) = set_directory_mode(&path, mode)
            {
                ctx.write_stderr(
                    format!("esh: mkdir: cannot set mode on '{}': {}\n", path_str, error)
                        .as_bytes(),
                );
                status = 1;
            }
        }

        Ok(status)
    })
}

#[cfg(unix)]
fn create_dir_with_posix_parents(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        return Ok(());
    }

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        create_posix_intermediate_directories(parent)?;
    }

    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(not(unix))]
fn create_dir_with_posix_parents(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)
}

#[cfg(unix)]
fn create_posix_intermediate_directories(path: &Path) -> std::io::Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        if current.is_dir() {
            continue;
        }
        match fs::create_dir(&current) {
            Ok(()) => add_owner_write_search(&current)?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && current.is_dir() => {
                // Another process created this component; do not change its mode.
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn add_owner_write_search(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path)?;
    let mode = metadata.permissions().mode();
    let required_mode = intermediate_directory_mode(mode);
    if mode != required_mode {
        fs::set_permissions(path, fs::Permissions::from_mode(required_mode))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn add_owner_write_search(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn intermediate_directory_mode(mode: u32) -> u32 {
    (mode & 0o0777) | 0o300
}

fn parse_directory_mode(value: &str) -> Option<u32> {
    if !value.is_empty() && value.bytes().all(|byte| (b'0'..=b'7').contains(&byte)) {
        let mode = u32::from_str_radix(value, 8).ok()?;
        return (mode <= 0o7777).then_some(mode);
    }

    let mut mode = 0o777;
    for clause in value.split(',') {
        let chars: Vec<char> = clause.chars().collect();
        let mut index = 0;
        let mut who = 0;
        while index < chars.len() {
            match chars[index] {
                'u' => who |= 0o4700,
                'g' => who |= 0o2070,
                'o' => who |= 0o1007,
                'a' => who |= 0o7777,
                _ => break,
            }
            index += 1;
        }
        if who == 0 {
            return None;
        }
        if index == chars.len() {
            return None;
        }

        while index < chars.len() {
            let operation = chars[index];
            if !matches!(operation, '+' | '-' | '=') {
                return None;
            }
            index += 1;
            let mut permissions = 0;
            while index < chars.len() && !matches!(chars[index], '+' | '-' | '=') {
                let permission = match chars[index] {
                    'r' => who & 0o444,
                    'w' => who & 0o222,
                    'x' | 'X' => who & 0o111,
                    's' => who & 0o6000,
                    't' if who == 0 || who & 0o1000 != 0 => 0o1000,
                    't' => 0,
                    'u' => copy_permission_bits(mode, 0o700, who),
                    'g' => copy_permission_bits(mode, 0o070, who),
                    'o' => copy_permission_bits(mode, 0o007, who),
                    _ => return None,
                };
                permissions |= permission;
                index += 1;
            }

            match operation {
                '+' => mode |= permissions,
                '-' => mode &= !permissions,
                '=' => mode = (mode & !who) | permissions,
                _ => unreachable!(),
            }
        }
    }
    Some(mode)
}

fn copy_permission_bits(mode: u32, source_mask: u32, target_mask: u32) -> u32 {
    let source = (mode & source_mask) >> source_mask.trailing_zeros();
    let mut copied = 0;
    for (shift, target_class_mask) in [(6, 0o700), (3, 0o070), (0, 0o007)] {
        if target_mask & target_class_mask != 0 {
            copied |= source << shift;
        }
    }
    copied
}

#[cfg(unix)]
fn set_directory_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_directory_mode(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "directory modes are unsupported on this platform",
    ))
}

pub fn rm<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut recursive = false;
        let mut force = false;
        let mut paths = Vec::new();
        let mut options = true;

        for arg in args {
            if options && arg == "--" {
                options = false;
            } else if options && (arg == "--recursive" || arg == "--force") {
                if arg == "--recursive" {
                    recursive = true;
                } else {
                    force = true;
                }
            } else if options && arg.starts_with('-') && arg != "-" {
                let mut valid = true;
                for option in arg[1..].chars() {
                    match option {
                        'r' | 'R' => recursive = true,
                        'f' => force = true,
                        _ => {
                            valid = false;
                            break;
                        }
                    }
                }
                if !valid {
                    ctx.write_stderr(format!("esh: rm: unsupported option '{}'\n", arg).as_bytes());
                    return Ok(2);
                }
            } else {
                options = false;
                paths.push(arg.as_str());
            }
        }

        if paths.is_empty() {
            if force {
                return Ok(0);
            }
            ctx.write_stderr(b"esh: rm: missing operand\n");
            return Ok(1);
        }

        let mut status = 0;
        for path_str in paths {
            let path = resolve_path(ctx, path_str);
            let trimmed_path = path_str.trim_end_matches(std::path::MAIN_SEPARATOR);
            let symlink_with_trailing_separator = path_str.ends_with(std::path::MAIN_SEPARATOR)
                && !trimmed_path.is_empty()
                && fs::symlink_metadata(resolve_path(ctx, trimmed_path))
                    .is_ok_and(|metadata| metadata.file_type().is_symlink());
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && force => continue,
                Err(error) => {
                    ctx.write_stderr(
                        format!("esh: rm: cannot remove '{}': {}\n", path_str, error).as_bytes(),
                    );
                    status = 1;
                    continue;
                }
            };
            let final_component = trimmed_path
                .rsplit(std::path::MAIN_SEPARATOR)
                .next()
                .unwrap_or(trimmed_path);
            // `rm link-to-root` removes the link entry; only reject paths that traverse it.
            let resolves_to_root = !metadata.file_type().is_symlink()
                && path.canonicalize().is_ok_and(|canonical| {
                    canonical.parent().is_none()
                        || canonical == Path::new(std::path::MAIN_SEPARATOR_STR)
                });
            if matches!(final_component, "." | "..") || resolves_to_root {
                ctx.write_stderr(
                    format!("esh: rm: refusing to remove '{}': unsafe path\n", path_str).as_bytes(),
                );
                status = 1;
                continue;
            }

            let is_directory = metadata.file_type().is_dir();
            let res = if is_directory {
                if recursive {
                    fs::remove_dir_all(&path)
                } else {
                    Err(std::io::Error::other("Is a directory"))
                }
            } else {
                fs::remove_file(&path)
            };

            if let Err(error) = res {
                if error.kind() == std::io::ErrorKind::NotFound && force {
                    continue;
                }
                ctx.write_stderr(
                    format!("esh: rm: cannot remove '{}': {}\n", path_str, error).as_bytes(),
                );
                status = 1;
            } else if symlink_with_trailing_separator {
                ctx.write_stderr(
                    format!("esh: rm: cannot remove '{}': Not a directory\n", path_str).as_bytes(),
                );
                status = 1;
            }
        }

        Ok(status)
    })
}

pub fn cp<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut recursive = false;
        let mut preserve = false;
        let mut force = false;
        let mut link_mode = CopyLinkMode::Default;
        let mut paths = Vec::new();
        let mut options = true;

        for arg in args {
            if options && arg == "--" {
                options = false;
            } else if options
                && (matches!(
                    arg.as_str(),
                    "--recursive"
                        | "--preserve"
                        | "--force"
                        | "--dereference"
                        | "--dereference-command-line"
                        | "--no-dereference"
                ) || arg.starts_with('-') && arg != "-")
            {
                let letters = match arg.as_str() {
                    "--recursive" => "r",
                    "--preserve" => "p",
                    "--force" => "f",
                    "--dereference" => "L",
                    "--dereference-command-line" => "H",
                    "--no-dereference" => "P",
                    _ => &arg[1..],
                };
                for option in letters.chars() {
                    match option {
                        'r' | 'R' => recursive = true,
                        'p' => preserve = true,
                        'f' => force = true,
                        'H' => link_mode = CopyLinkMode::CommandLine,
                        'L' => link_mode = CopyLinkMode::Logical,
                        'P' => link_mode = CopyLinkMode::Physical,
                        _ => {
                            ctx.write_stderr(
                                format!("esh: cp: unsupported option '-{}'\n", option).as_bytes(),
                            );
                            return Ok(2);
                        }
                    }
                }
            } else {
                options = false;
                paths.push(arg.as_str());
            }
        }

        if paths.len() < 2 {
            ctx.write_stderr(b"esh: cp: missing file operand\n");
            return Ok(1);
        }

        let source_operands = &paths[..paths.len() - 1];
        let destination_operand = paths[paths.len() - 1];
        let destination = resolve_path(ctx, destination_operand);
        let destination_is_directory = destination.is_dir();
        if source_operands.len() > 1 && !destination_is_directory {
            ctx.write_stderr(
                format!(
                    "esh: cp: target '{}' is not a directory\n",
                    destination_operand
                )
                .as_bytes(),
            );
            return Ok(1);
        }

        let mut status = 0;
        for source_operand in source_operands {
            let source = resolve_path(ctx, source_operand);
            let final_destination = if destination_is_directory {
                match source.file_name() {
                    Some(name) => destination.join(name),
                    None => {
                        ctx.write_stderr(
                            format!("esh: cp: cannot determine name for '{}'\n", source_operand)
                                .as_bytes(),
                        );
                        status = 1;
                        continue;
                    }
                }
            } else {
                destination.clone()
            };

            let source_is_symlink = fs::symlink_metadata(&source)
                .is_ok_and(|metadata| metadata.file_type().is_symlink());
            let follow_source_link = cp_follows_link(link_mode, recursive, true);
            let source_is_recursive_directory =
                source.is_dir() && !(source_is_symlink && !follow_source_link);
            let mut ancestry = Vec::new();
            let result = if source_is_recursive_directory {
                if !recursive {
                    Err(std::io::Error::other(
                        "-r not specified; omitting directory",
                    ))
                } else if copy_would_recurse_into_source(&source, &final_destination) {
                    Err(std::io::Error::other("cannot copy a directory into itself"))
                } else {
                    copy_path_with_links(
                        &source,
                        &final_destination,
                        recursive,
                        preserve,
                        force,
                        link_mode,
                        true,
                        &mut ancestry,
                    )
                }
            } else {
                copy_path_with_links(
                    &source,
                    &final_destination,
                    recursive,
                    preserve,
                    force,
                    link_mode,
                    true,
                    &mut ancestry,
                )
            };

            if let Err(error) = result {
                ctx.write_stderr(
                    format!(
                        "esh: cp: cannot copy '{}' to '{}': {}\n",
                        source_operand, destination_operand, error
                    )
                    .as_bytes(),
                );
                status = 1;
            }
        }

        Ok(status)
    })
}

fn copy_would_recurse_into_source(source: &Path, destination: &Path) -> bool {
    let Ok(source) = fs::canonicalize(source) else {
        return false;
    };
    canonicalize_with_missing_suffix(destination)
        .is_some_and(|destination| destination.starts_with(source))
}

fn canonicalize_with_missing_suffix(path: &Path) -> Option<PathBuf> {
    let mut ancestor = path;
    let mut suffix = Vec::new();
    loop {
        if let Ok(mut canonical) = fs::canonicalize(ancestor) {
            for component in suffix.iter().rev() {
                canonical.push(component);
            }
            return Some(canonical);
        }
        suffix.push(ancestor.file_name()?.to_os_string());
        ancestor = ancestor.parent()?;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CopyLinkMode {
    Default,
    CommandLine,
    Logical,
    Physical,
}

fn cp_follows_link(mode: CopyLinkMode, recursive: bool, source_operand: bool) -> bool {
    match mode {
        CopyLinkMode::Default => !recursive,
        CopyLinkMode::CommandLine => source_operand,
        CopyLinkMode::Logical => true,
        CopyLinkMode::Physical => false,
    }
}

fn copy_path(
    source: &Path,
    destination: &Path,
    recursive: bool,
    preserve: bool,
    force: bool,
) -> std::io::Result<()> {
    copy_path_with_links(
        source,
        destination,
        recursive,
        preserve,
        force,
        CopyLinkMode::Default,
        true,
        &mut Vec::new(),
    )
}

#[allow(clippy::too_many_arguments)]
fn copy_path_with_links(
    source: &Path,
    destination: &Path,
    recursive: bool,
    preserve: bool,
    force: bool,
    link_mode: CopyLinkMode,
    source_operand: bool,
    ancestry: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    let link_metadata = fs::symlink_metadata(source)?;
    if link_metadata.file_type().is_symlink()
        && !cp_follows_link(link_mode, recursive, source_operand)
    {
        copy_symlink(source, destination)?;
        if preserve {
            copy_metadata(destination, &link_metadata, true)?;
        }
        return Ok(());
    }
    let metadata = fs::metadata(source)?;
    if metadata.is_dir() {
        if !recursive {
            return Err(std::io::Error::other(
                "-r not specified; omitting directory",
            ));
        }
        let canonical = fs::canonicalize(source)?;
        if ancestry.contains(&canonical) {
            return Err(std::io::Error::other(
                "symbolic link loop while copying directory",
            ));
        }
        ancestry.push(canonical);
        let result = copy_dir_all(source, destination, preserve, force, link_mode, ancestry);
        ancestry.pop();
        let created = result?;
        if preserve && created {
            copy_metadata(destination, &metadata, false)?;
        }
    } else {
        copy_regular_file(source, destination, &metadata, force)?;
        if preserve {
            copy_metadata(destination, &metadata, false)?;
        }
    }
    Ok(())
}

fn copy_metadata(
    destination: &Path,
    metadata: &fs::Metadata,
    symlink: bool,
) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let destination_c = CString::new(destination.as_os_str().as_bytes()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains a NUL byte")
        })?;
        let flags = if symlink {
            libc::AT_SYMLINK_NOFOLLOW
        } else {
            0
        };
        // Ownership is best effort under the POSIX `cp -p` contract. If it
        // cannot be copied, the set-ID bits must not survive on the target.
        // SAFETY: destination_c is NUL-terminated and valid for the call.
        let ownership_preserved = unsafe {
            libc::fchownat(
                libc::AT_FDCWD,
                destination_c.as_ptr(),
                metadata.uid(),
                metadata.gid(),
                flags,
            ) == 0
        };

        if !symlink {
            let mut mode = metadata.permissions().mode() & 0o7777;
            if !ownership_preserved {
                // libc gives these constants different integer types across
                // Unix platforms; PermissionsExt::mode() always returns u32.
                #[allow(clippy::unnecessary_cast)]
                let set_id_bits = libc::S_ISUID as u32 | libc::S_ISGID as u32;
                mode &= !set_id_bits;
            }
            fs::set_permissions(destination, fs::Permissions::from_mode(mode))?;
        }

        let accessed = metadata.accessed()?;
        let modified = metadata.modified()?;
        let times = [
            system_time_to_timespec(accessed),
            system_time_to_timespec(modified),
        ];
        // SAFETY: destination_c and times are valid for the duration of the call.
        let result = unsafe {
            libc::utimensat(
                libc::AT_FDCWD,
                destination_c.as_ptr(),
                times.as_ptr(),
                flags,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }

    #[cfg(not(unix))]
    {
        if !symlink {
            let accessed = metadata.accessed()?;
            let modified = metadata.modified()?;
            #[cfg(windows)]
            set_windows_file_times(destination, accessed, modified)?;
            #[cfg(not(windows))]
            {
                let file = fs::OpenOptions::new().write(true).open(destination)?;
                file.set_times(
                    fs::FileTimes::new()
                        .set_accessed(accessed)
                        .set_modified(modified),
                )?;
            }
            fs::set_permissions(destination, metadata.permissions())?;
        }
    }

    Ok(())
}

#[cfg(windows)]
fn set_windows_file_times(
    path: &Path,
    accessed: std::time::SystemTime,
    modified: std::time::SystemTime,
) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ,
            FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, OPEN_EXISTING, SetFileTime,
        },
    };

    fn filetime(time: std::time::SystemTime) -> std::io::Result<FILETIME> {
        const WINDOWS_TO_UNIX_EPOCH_SECONDS: i128 = 11_644_473_600;
        const TICKS_PER_SECOND: i128 = 10_000_000;

        let ticks = match time.duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => {
                WINDOWS_TO_UNIX_EPOCH_SECONDS * TICKS_PER_SECOND
                    + i128::from(duration.as_secs()) * TICKS_PER_SECOND
                    + i128::from(duration.subsec_nanos() / 100)
            }
            Err(error) => {
                WINDOWS_TO_UNIX_EPOCH_SECONDS * TICKS_PER_SECOND
                    - i128::from(error.duration().as_secs()) * TICKS_PER_SECOND
                    - i128::from(error.duration().subsec_nanos() / 100)
            }
        };
        let ticks = u64::try_from(ticks).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "time is outside FILETIME range",
            )
        })?;
        Ok(FILETIME {
            dwLowDateTime: ticks as u32,
            dwHighDateTime: (ticks >> 32) as u32,
        })
    }

    let accessed = filetime(accessed)?;
    let modified = filetime(modified)?;
    let mut wide_path: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide_path.push(0);

    // Opening with backup semantics allows timestamp preservation on directories.
    // SAFETY: wide_path is NUL-terminated and all pointers are valid for the call.
    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            FILE_WRITE_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }

    // SAFETY: handle is valid and both FILETIME values live through the call.
    let success = unsafe { SetFileTime(handle, std::ptr::null(), &accessed, &modified) };
    let result = if success == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    };
    // SAFETY: handle was returned by CreateFileW and is closed exactly once.
    unsafe { CloseHandle(handle) };
    result
}

#[cfg(unix)]
fn system_time_to_timespec(time: std::time::SystemTime) -> libc::timespec {
    use std::time::UNIX_EPOCH;

    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => libc::timespec {
            tv_sec: duration.as_secs() as libc::time_t,
            tv_nsec: duration.subsec_nanos() as libc::c_long,
        },
        Err(error) => {
            let duration = error.duration();
            if duration.subsec_nanos() == 0 {
                libc::timespec {
                    tv_sec: -(duration.as_secs() as libc::time_t),
                    tv_nsec: 0,
                }
            } else {
                libc::timespec {
                    tv_sec: -(duration.as_secs() as libc::time_t) - 1,
                    tv_nsec: (1_000_000_000 - duration.subsec_nanos()) as libc::c_long,
                }
            }
        }
    }
}

fn copy_symlink(source: &Path, destination: &Path) -> std::io::Result<()> {
    let target = fs::read_link(source)?;
    if let Ok(metadata) = fs::symlink_metadata(destination) {
        if metadata.file_type().is_dir() {
            return Err(std::io::Error::other(
                "cannot replace a directory with a symbolic link",
            ));
        }
        fs::remove_file(destination)?;
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, destination)
    }
    #[cfg(windows)]
    {
        if fs::metadata(source).is_ok_and(|metadata| metadata.is_dir()) {
            std::os::windows::fs::symlink_dir(target, destination)
        } else {
            std::os::windows::fs::symlink_file(target, destination)
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, destination);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "symbolic-link copying is unsupported on this platform",
        ))
    }
}

fn copy_regular_file(
    src: &Path,
    dst: &Path,
    metadata: &fs::Metadata,
    force: bool,
) -> std::io::Result<()> {
    use std::io;
    if let Ok(destination_metadata) = fs::metadata(dst) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.dev() == destination_metadata.dev()
                && metadata.ino() == destination_metadata.ino()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "source and destination are the same file",
                ));
            }
        }
        #[cfg(not(unix))]
        if let (Ok(source_path), Ok(destination_path)) =
            (fs::canonicalize(src), fs::canonicalize(dst))
        {
            if source_path == destination_path {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "source and destination are the same file",
                ));
            }
        }
    }
    let mut input = fs::File::open(src)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        options.mode(metadata.mode() & 0o7777);
    }
    let mut output = match options.open(dst) {
        Ok(output) => output,
        Err(_) if force => {
            fs::remove_file(dst)?;
            options.open(dst)?
        }
        Err(error) => return Err(error),
    };
    io::copy(&mut input, &mut output)?;
    Ok(())
}

fn copy_dir_all(
    src: &Path,
    dst: &Path,
    preserve: bool,
    force: bool,
    link_mode: CopyLinkMode,
    ancestry: &mut Vec<PathBuf>,
) -> std::io::Result<bool> {
    let created = match fs::create_dir(dst) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && dst.is_dir() => false,
        Err(error) => return Err(error),
    };
    #[cfg(unix)]
    let allowed_mode = {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(dst)?.permissions().mode() & 0o777;
        if created {
            fs::set_permissions(dst, fs::Permissions::from_mode(mode | 0o700))?;
        }
        mode
    };
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        copy_path_with_links(
            &entry.path(),
            &dst_path,
            true,
            preserve,
            force,
            link_mode,
            false,
            ancestry,
        )?;
    }
    #[cfg(unix)]
    if created && !preserve {
        use std::os::unix::fs::PermissionsExt;
        let source_mode = fs::metadata(src)?.permissions().mode() & 0o777;
        fs::set_permissions(dst, fs::Permissions::from_mode(source_mode & allowed_mode))?;
    }
    Ok(created)
}

pub fn mv<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut paths = Vec::new();
        let mut options = true;
        for arg in args {
            if options && arg == "--" {
                options = false;
            } else if options && arg.starts_with('-') && arg != "-" {
                for option in arg[1..].chars() {
                    if option != 'f' {
                        ctx.write_stderr(
                            format!("esh: mv: unsupported option '-{}'\n", option).as_bytes(),
                        );
                        return Ok(2);
                    }
                }
            } else {
                options = false;
                paths.push(arg.as_str());
            }
        }

        if paths.len() < 2 {
            ctx.write_stderr(b"esh: mv: missing file operand\n");
            return Ok(1);
        }

        let source_operands = &paths[..paths.len() - 1];
        let destination_operand = paths[paths.len() - 1];
        let destination = resolve_path(ctx, destination_operand);
        let destination_is_directory = destination.is_dir();
        if source_operands.len() > 1 && !destination_is_directory {
            ctx.write_stderr(
                format!(
                    "esh: mv: target '{}' is not a directory\n",
                    destination_operand
                )
                .as_bytes(),
            );
            return Ok(1);
        }

        let mut status = 0;
        for source_operand in source_operands {
            let source = resolve_path(ctx, source_operand);
            let final_destination = if destination_is_directory {
                match source.file_name() {
                    Some(name) => destination.join(name),
                    None => {
                        ctx.write_stderr(
                            format!("esh: mv: cannot determine name for '{}'\n", source_operand)
                                .as_bytes(),
                        );
                        status = 1;
                        continue;
                    }
                }
            } else {
                destination.clone()
            };

            if let Err(error) = move_with_cross_device_fallback(&source, &final_destination) {
                ctx.write_stderr(
                    format!(
                        "esh: mv: cannot move '{}' to '{}': {}\n",
                        source_operand, destination_operand, error
                    )
                    .as_bytes(),
                );
                status = 1;
            }
        }

        Ok(status)
    })
}

fn move_with_cross_device_fallback(source: &Path, destination: &Path) -> std::io::Result<()> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => {
            move_by_copy_and_remove(source, destination)
        }
        Err(error) => Err(error),
    }
}

fn move_by_copy_and_remove(source: &Path, destination: &Path) -> std::io::Result<()> {
    copy_path(source, destination, true, true, false)?;
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_dir() {
        fs::remove_dir_all(source)
    } else {
        fs::remove_file(source)
    }
}

pub fn touch<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut set_access = false;
        let mut set_modification = false;
        let mut no_create = false;
        let mut reference_operand = None;
        let mut timestamp_operand = None;
        let mut date_time_operand = None;
        let mut paths = Vec::new();
        let mut options = true;

        let mut index = 0;
        while index < args.len() {
            let arg = &args[index];
            if options && arg == "--" {
                options = false;
            } else if options && arg.starts_with('-') && arg != "-" {
                for (offset, option) in arg[1..].char_indices() {
                    match option {
                        'a' => set_access = true,
                        'm' => set_modification = true,
                        'c' => no_create = true,
                        'r' => {
                            if timestamp_operand.is_some() || date_time_operand.is_some() {
                                ctx.write_stderr(
                                    b"esh: touch: '-r', '-t', and '-d' are mutually exclusive\n",
                                );
                                return Ok(2);
                            }
                            let attached = &arg[offset + 2..];
                            if attached.is_empty() {
                                index += 1;
                                let Some(reference) = args.get(index) else {
                                    ctx.write_stderr(
                                        b"esh: touch: option '-r' requires a reference file\n",
                                    );
                                    return Ok(2);
                                };
                                reference_operand = Some(reference.as_str());
                            } else {
                                reference_operand = Some(attached);
                            }
                            break;
                        }
                        't' => {
                            if reference_operand.is_some() || date_time_operand.is_some() {
                                ctx.write_stderr(
                                    b"esh: touch: '-r', '-t', and '-d' are mutually exclusive\n",
                                );
                                return Ok(2);
                            }
                            let attached = &arg[offset + 2..];
                            if attached.is_empty() {
                                index += 1;
                                let Some(timestamp) = args.get(index) else {
                                    ctx.write_stderr(
                                        b"esh: touch: option '-t' requires a timestamp\n",
                                    );
                                    return Ok(2);
                                };
                                timestamp_operand = Some(timestamp.as_str());
                            } else {
                                timestamp_operand = Some(attached);
                            }
                            break;
                        }
                        'd' => {
                            if reference_operand.is_some() || timestamp_operand.is_some() {
                                ctx.write_stderr(
                                    b"esh: touch: '-r', '-t', and '-d' are mutually exclusive\n",
                                );
                                return Ok(2);
                            }
                            let attached = &arg[offset + 2..];
                            if attached.is_empty() {
                                index += 1;
                                let Some(date_time) = args.get(index) else {
                                    ctx.write_stderr(
                                        b"esh: touch: option '-d' requires a date and time\n",
                                    );
                                    return Ok(2);
                                };
                                date_time_operand = Some(date_time.as_str());
                            } else {
                                date_time_operand = Some(attached);
                            }
                            break;
                        }
                        _ => {
                            ctx.write_stderr(
                                format!("esh: touch: unsupported option '-{}'\n", option)
                                    .as_bytes(),
                            );
                            return Ok(2);
                        }
                    }
                }
            } else {
                options = false;
                paths.push(arg.as_str());
            }
            index += 1;
        }

        if paths.is_empty() {
            ctx.write_stderr(b"esh: touch: missing file operand\n");
            return Ok(1);
        }

        let set_both = !set_access && !set_modification;
        let reference_times = if let Some(reference) = reference_operand {
            match fs::metadata(resolve_path(ctx, reference)) {
                Ok(metadata) => match (metadata.accessed(), metadata.modified()) {
                    (Ok(accessed), Ok(modified)) => Some((accessed, modified)),
                    (Err(error), _) | (_, Err(error)) => {
                        ctx.write_stderr(
                            format!(
                                "esh: touch: cannot read timestamps from '{}': {}\n",
                                reference, error
                            )
                            .as_bytes(),
                        );
                        return Ok(1);
                    }
                },
                Err(error) => {
                    ctx.write_stderr(
                        format!(
                            "esh: touch: cannot access reference file '{}': {}\n",
                            reference, error
                        )
                        .as_bytes(),
                    );
                    return Ok(1);
                }
            }
        } else {
            None
        };
        let timestamp_times = if let Some(timestamp) = timestamp_operand {
            match parse_touch_timestamp(timestamp) {
                Ok(timestamp) => Some((timestamp, timestamp)),
                Err(error) => {
                    ctx.write_stderr(
                        format!("esh: touch: invalid timestamp '{}': {}\n", timestamp, error)
                            .as_bytes(),
                    );
                    return Ok(2);
                }
            }
        } else {
            None
        };
        let date_time_times = if let Some(date_time) = date_time_operand {
            match parse_touch_date_time(date_time) {
                Ok(date_time) => Some((date_time, date_time)),
                Err(error) => {
                    ctx.write_stderr(
                        format!(
                            "esh: touch: invalid date and time '{}': {}\n",
                            date_time, error
                        )
                        .as_bytes(),
                    );
                    return Ok(2);
                }
            }
        } else {
            None
        };
        let selected_times = reference_times.or(timestamp_times).or(date_time_times);
        let mut status = 0;
        for path_str in paths {
            let path = resolve_path(ctx, path_str);
            let access = set_both || set_access;
            let modification = set_both || set_modification;
            let update_times = || {
                if let Some((reference_accessed, reference_modified)) = selected_times {
                    set_touch_times_from(
                        &path,
                        access.then_some(reference_accessed),
                        modification.then_some(reference_modified),
                    )
                } else {
                    set_touch_times(&path, access, modification)
                }
            };
            let result = match update_times() {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && no_create => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    match fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)
                    {
                        Ok(_) => update_times(),
                        Err(create_error)
                            if create_error.kind() == std::io::ErrorKind::AlreadyExists =>
                        {
                            update_times()
                        }
                        Err(create_error) => Err(create_error),
                    }
                }
                Err(error) => Err(error),
            };

            if let Err(error) = result {
                ctx.write_stderr(
                    format!("esh: touch: cannot touch '{}': {}\n", path_str, error).as_bytes(),
                );
                status = 1;
            }
        }

        Ok(status)
    })
}

pub fn cat<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut operands = Vec::new();
        let mut parsing_options = true;
        for arg in args {
            if parsing_options && arg == "--" {
                parsing_options = false;
            } else if parsing_options && arg.starts_with('-') && arg != "-" {
                if arg[1..].chars().all(|option| option == 'u') {
                    continue;
                }
                ctx.write_stderr(format!("esh: cat: unsupported option '{}'\n", arg).as_bytes());
                return Ok(2);
            } else {
                parsing_options = false;
                operands.push(arg.as_str());
            }
        }

        if operands.is_empty() {
            if let Some(buf) = ctx.take_stdin() {
                ctx.write_stdout(&buf);
            }
            return Ok(0);
        }

        let mut read_error = false;
        for path_str in operands {
            if path_str == "-" {
                if let Some(data) = ctx.take_stdin() {
                    ctx.write_stdout(&data);
                }
                continue;
            }
            let path = resolve_path(ctx, path_str);
            match fs::read(&path) {
                Ok(data) => ctx.write_stdout(&data),
                Err(e) => {
                    read_error = true;
                    ctx.write_stderr(
                        format!("esh: cat: cannot open '{}': {}\n", path_str, e).as_bytes(),
                    );
                }
            }
        }

        Ok(if read_error { 1 } else { 0 })
    })
}

pub fn head<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut line_count = 10usize;
        let mut byte_count = None;
        let mut count_mode = None;
        let mut files = Vec::new();
        let mut parsing_options = true;

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if parsing_options && arg == "--" {
                parsing_options = false;
            } else if parsing_options && (arg == "-n" || arg == "--lines") {
                if count_mode.is_some_and(|mode| mode != 'n') {
                    ctx.write_stderr(b"esh: head: -n and -c are mutually exclusive\n");
                    return Ok(2);
                }
                count_mode = Some('n');
                let Some(value) = iter.next() else {
                    ctx.write_stderr(b"esh: head: missing line count\n");
                    return Ok(2);
                };
                match value.parse::<usize>() {
                    Ok(value) => line_count = value,
                    Err(_) => {
                        ctx.write_stderr(
                            format!("esh: head: invalid line count '{}'\n", value).as_bytes(),
                        );
                        return Ok(2);
                    }
                }
            } else if parsing_options && (arg == "-c" || arg == "--bytes") {
                if count_mode.is_some_and(|mode| mode != 'c') {
                    ctx.write_stderr(b"esh: head: -n and -c are mutually exclusive\n");
                    return Ok(2);
                }
                count_mode = Some('c');
                let Some(value) = iter.next() else {
                    ctx.write_stderr(b"esh: head: missing byte count\n");
                    return Ok(2);
                };
                match value.parse::<usize>() {
                    Ok(value) => byte_count = Some(value),
                    Err(_) => {
                        ctx.write_stderr(
                            format!("esh: head: invalid byte count '{}'\n", value).as_bytes(),
                        );
                        return Ok(2);
                    }
                }
            } else if parsing_options && arg.starts_with('-') && arg != "-" {
                ctx.write_stderr(format!("esh: head: unsupported option '{}'\n", arg).as_bytes());
                return Ok(2);
            } else {
                parsing_options = false;
                files.push(arg.as_str());
            }
        }

        if let Some(count) = byte_count {
            let write_prefix = |data: &[u8], ctx: &mut ExecutionContext| {
                ctx.write_stdout(&data[..data.len().min(count)]);
            };

            if files.is_empty() {
                if let Some(data) = ctx.take_stdin() {
                    write_prefix(&data, ctx);
                }
                return Ok(0);
            }

            let show_headers = files.len() > 1;
            let mut first_header = true;
            let mut read_error = false;
            for path_str in files {
                if path_str == "-" {
                    let data = ctx.take_stdin().unwrap_or_default();
                    write_multi_file_header(path_str, show_headers, &mut first_header, ctx);
                    write_prefix(&data, ctx);
                    continue;
                }
                let path = resolve_path(ctx, path_str);
                match fs::read(&path) {
                    Ok(data) => {
                        write_multi_file_header(path_str, show_headers, &mut first_header, ctx);
                        write_prefix(&data, ctx);
                    }
                    Err(e) => {
                        read_error = true;
                        ctx.write_stderr(
                            format!("esh: head: cannot read '{}': {}\n", path_str, e).as_bytes(),
                        );
                    }
                }
            }
            return Ok(if read_error { 1 } else { 0 });
        }

        let process_lines = |data: &[u8], ctx: &mut ExecutionContext| {
            if line_count == 0 {
                return;
            }
            let mut lines_seen = 0;
            for (index, byte) in data.iter().enumerate() {
                if *byte == b'\n' {
                    lines_seen += 1;
                    if lines_seen == line_count {
                        ctx.write_stdout(&data[..index + 1]);
                        return;
                    }
                }
            }
            ctx.write_stdout(data);
        };

        if files.is_empty() {
            if let Some(buf) = ctx.take_stdin() {
                process_lines(&buf, ctx);
            }
            return Ok(0);
        }

        let show_headers = files.len() > 1;
        let mut first_header = true;
        let mut read_error = false;
        for path_str in files {
            if path_str == "-" {
                let data = ctx.take_stdin().unwrap_or_default();
                write_multi_file_header(path_str, show_headers, &mut first_header, ctx);
                process_lines(&data, ctx);
                continue;
            }
            let path = resolve_path(ctx, path_str);
            match fs::read(&path) {
                Ok(content) => {
                    write_multi_file_header(path_str, show_headers, &mut first_header, ctx);
                    process_lines(&content, ctx);
                }
                Err(e) => {
                    read_error = true;
                    ctx.write_stderr(
                        format!("esh: head: cannot read '{}': {}\n", path_str, e).as_bytes(),
                    );
                }
            }
        }

        Ok(if read_error { 1 } else { 0 })
    })
}

pub fn tail<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        let mut count = 10usize;
        let mut byte_count = None;
        let mut line_from_start = false;
        let mut byte_from_start = false;
        let mut count_mode = None;
        let mut files = Vec::new();
        let mut parsing_options = true;

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if parsing_options && arg == "--" {
                parsing_options = false;
            } else if parsing_options && (arg == "-n" || arg == "--lines") {
                if count_mode.is_some_and(|mode| mode != 'n') {
                    ctx.write_stderr(b"esh: tail: -n and -c are mutually exclusive\n");
                    return Ok(2);
                }
                count_mode = Some('n');
                let Some(value) = iter.next() else {
                    ctx.write_stderr(b"esh: tail: missing line count\n");
                    return Ok(2);
                };
                match parse_tail_count(value) {
                    Ok((value, from_start)) if !from_start || value > 0 => {
                        count = value;
                        line_from_start = from_start;
                    }
                    Err(_) => {
                        ctx.write_stderr(
                            format!("esh: tail: invalid line count '{}'\n", value).as_bytes(),
                        );
                        return Ok(2);
                    }
                    _ => {
                        ctx.write_stderr(
                            format!("esh: tail: invalid line count '{}'\n", value).as_bytes(),
                        );
                        return Ok(2);
                    }
                }
            } else if parsing_options && (arg == "-c" || arg == "--bytes") {
                if count_mode.is_some_and(|mode| mode != 'c') {
                    ctx.write_stderr(b"esh: tail: -n and -c are mutually exclusive\n");
                    return Ok(2);
                }
                count_mode = Some('c');
                let Some(value) = iter.next() else {
                    ctx.write_stderr(b"esh: tail: missing byte count\n");
                    return Ok(2);
                };
                match parse_tail_count(value) {
                    Ok((value, from_start)) if !from_start || value > 0 => {
                        byte_count = Some(value);
                        byte_from_start = from_start;
                    }
                    _ => {
                        ctx.write_stderr(
                            format!("esh: tail: invalid byte count '{}'\n", value).as_bytes(),
                        );
                        return Ok(2);
                    }
                }
            } else if parsing_options && arg.starts_with('-') && arg != "-" {
                ctx.write_stderr(format!("esh: tail: unsupported option '{}'\n", arg).as_bytes());
                return Ok(2);
            } else {
                parsing_options = false;
                files.push(arg.as_str());
            }
        }

        let process_input = |data: &[u8], ctx: &mut ExecutionContext| {
            if let Some(byte_count) = byte_count {
                let offset = if byte_from_start {
                    byte_count.saturating_sub(1).min(data.len())
                } else {
                    data.len().saturating_sub(byte_count)
                };
                ctx.write_stdout(&data[offset..]);
                return;
            }

            if count == 0 {
                return;
            }

            let mut line_starts = vec![0];
            for (index, byte) in data.iter().enumerate() {
                if *byte == b'\n' && index + 1 < data.len() {
                    line_starts.push(index + 1);
                }
            }
            if line_from_start {
                if let Some(offset) = line_starts.get(count - 1) {
                    ctx.write_stdout(&data[*offset..]);
                }
            } else {
                let start_index = line_starts.len().saturating_sub(count);
                ctx.write_stdout(&data[line_starts[start_index]..]);
            }
        };

        if files.is_empty() {
            if let Some(buf) = ctx.take_stdin() {
                process_input(&buf, ctx);
            }
            return Ok(0);
        }

        let show_headers = files.len() > 1;
        let mut first_header = true;
        let mut read_error = false;
        for path_str in files {
            if path_str == "-" {
                let data = ctx.take_stdin().unwrap_or_default();
                write_multi_file_header(path_str, show_headers, &mut first_header, ctx);
                process_input(&data, ctx);
                continue;
            }
            let path = resolve_path(ctx, path_str);
            match fs::read(&path) {
                Ok(data) => {
                    write_multi_file_header(path_str, show_headers, &mut first_header, ctx);
                    process_input(&data, ctx);
                }
                Err(e) => {
                    read_error = true;
                    ctx.write_stderr(
                        format!("esh: tail: cannot read '{}': {}\n", path_str, e).as_bytes(),
                    );
                }
            }
        }

        Ok(if read_error { 1 } else { 0 })
    })
}

fn parse_tail_count(value: &str) -> Result<(usize, bool), ()> {
    if let Some(count) = value.strip_prefix('+') {
        count
            .parse::<usize>()
            .map(|count| (count, true))
            .map_err(|_| ())
    } else if let Some(count) = value.strip_prefix('-') {
        count
            .parse::<usize>()
            .map(|count| (count, false))
            .map_err(|_| ())
    } else {
        value
            .parse::<usize>()
            .map(|count| (count, false))
            .map_err(|_| ())
    }
}

pub fn stat<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: stat: missing operand\n");
            return Ok(1);
        }

        let mut format_spec: Option<&str> = None;
        let mut files = Vec::new();

        let mut iter = args.iter().peekable();
        while let Some(arg) = iter.next() {
            if arg == "-c" || arg == "--format" {
                if let Some(f) = iter.next() {
                    format_spec = Some(f.as_str());
                }
            } else if let Some(f) = arg.strip_prefix("-c") {
                format_spec = Some(f);
            } else if let Some(f) = arg.strip_prefix("--format=") {
                format_spec = Some(f);
            } else if !arg.starts_with('-') {
                files.push(arg.as_str());
            }
        }

        if files.is_empty() {
            ctx.write_stderr(b"esh: stat: missing operand\n");
            return Ok(1);
        }

        let mut all_ok = true;
        for path_str in files {
            let path = resolve_path(ctx, path_str);
            match fs::metadata(&path) {
                Ok(meta) => {
                    if let Some(fmt) = format_spec {
                        let mut out = fmt.to_string();
                        out = out.replace("%s", &meta.len().to_string());
                        out = out.replace("%n", path_str);
                        let file_type = if meta.is_dir() {
                            "directory"
                        } else {
                            "regular file"
                        };
                        out = out.replace("%F", file_type);
                        ctx.write_stdout(format!("{}\n", out).as_bytes());
                    } else {
                        let file_type = if meta.is_dir() {
                            "directory"
                        } else {
                            "regular file"
                        };
                        ctx.write_stdout(
                            format!(
                                "  File: {}\n  Size: {}\tType: {}\n",
                                path_str,
                                meta.len(),
                                file_type
                            )
                            .as_bytes(),
                        );
                    }
                }
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: stat: cannot stat '{}': {}\n", path_str, e).as_bytes(),
                    );
                    all_ok = false;
                }
            }
        }

        if all_ok { Ok(0) } else { Ok(1) }
    })
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::intermediate_directory_mode;
    use super::{move_by_copy_and_remove, parse_touch_date_time, parse_touch_timestamp};
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    #[cfg(unix)]
    #[test]
    fn posix_parent_directory_mode_adds_owner_write_and_search_only() {
        assert_eq!(intermediate_directory_mode(0o000), 0o300);
        assert_eq!(intermediate_directory_mode(0o444), 0o744);
        assert_eq!(intermediate_directory_mode(0o750), 0o750);
        assert_eq!(intermediate_directory_mode(0o2770), 0o770);
        assert_eq!(intermediate_directory_mode(0o1777), 0o777);
    }

    #[test]
    fn touch_timestamp_parser_handles_year_pivot_seconds_and_invalid_fields() {
        use chrono::{DateTime, Datelike, Local};

        let year_68 = DateTime::<Local>::from(parse_touch_timestamp("6801020304").unwrap());
        let year_69 = DateTime::<Local>::from(parse_touch_timestamp("6901020304").unwrap());
        assert_eq!(year_68.year(), 2068);
        assert_eq!(year_69.year(), 1969);

        let minute_before_leap = parse_touch_timestamp("202001020304.59").unwrap();
        let leap_second = parse_touch_timestamp("202001020304.60").unwrap();
        assert_eq!(
            leap_second.duration_since(minute_before_leap).unwrap(),
            std::time::Duration::from_secs(1)
        );

        assert!(parse_touch_timestamp("202013020304").is_err());
        assert!(parse_touch_timestamp("202001020304.6").is_err());
        assert_eq!(
            parse_touch_timestamp("202001020304.61").unwrap_err(),
            "seconds must be between 00 and 60"
        );
    }

    #[test]
    fn touch_date_time_parser_handles_iso_forms_fractional_precision_and_pre_epoch_times() {
        use std::time::{Duration, UNIX_EPOCH};

        let comma_fraction = parse_touch_date_time("2020-01-02 03:04:05,1234567899Z").unwrap();
        let nanosecond_precision = parse_touch_date_time("2020-01-02T03:04:05.123456789Z").unwrap();
        assert_eq!(comma_fraction, nanosecond_precision);

        let before_epoch = parse_touch_date_time("1969-12-31T23:59:59.5Z").unwrap();
        assert_eq!(
            UNIX_EPOCH.duration_since(before_epoch).unwrap(),
            Duration::from_millis(500)
        );

        let second_59 = parse_touch_date_time("2020-01-02T03:04:59Z").unwrap();
        let leap_second = parse_touch_date_time("2020-01-02T03:04:60Z").unwrap();
        assert_eq!(
            leap_second.duration_since(second_59).unwrap(),
            Duration::from_secs(1)
        );
        assert!(parse_touch_date_time("2020-13-02T03:04:05Z").is_err());
        assert!(parse_touch_date_time("2020-01-02T03:04:05+01:00").is_err());
    }

    #[test]
    fn cross_device_move_fallback_copies_a_directory_tree_then_removes_source() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("nested/file"), "payload").unwrap();
        #[cfg(unix)]
        {
            use std::time::{Duration, UNIX_EPOCH};

            let file = source.join("nested/file");
            fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
            fs::File::open(&file)
                .unwrap()
                .set_times(
                    fs::FileTimes::new()
                        .set_accessed(UNIX_EPOCH + Duration::from_secs(1_600_000_000))
                        .set_modified(UNIX_EPOCH + Duration::from_secs(1_650_000_000)),
                )
                .unwrap();
        }
        #[cfg(unix)]
        let source_metadata = fs::metadata(source.join("nested/file")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("nested/file", source.join("file-link")).unwrap();

        move_by_copy_and_remove(&source, &destination).unwrap();

        assert!(!source.exists());
        #[cfg(unix)]
        {
            use std::time::{Duration, UNIX_EPOCH};

            let metadata = fs::metadata(destination.join("nested/file")).unwrap();
            assert_eq!(metadata.permissions().mode() & 0o7777, 0o640);
            assert_eq!(
                metadata.accessed().unwrap(),
                UNIX_EPOCH + Duration::from_secs(1_600_000_000)
            );
            assert_eq!(
                metadata.modified().unwrap(),
                UNIX_EPOCH + Duration::from_secs(1_650_000_000)
            );
            assert_eq!(metadata.uid(), source_metadata.uid());
            assert_eq!(metadata.gid(), source_metadata.gid());
        }
        assert_eq!(
            fs::read(destination.join("nested/file")).unwrap(),
            b"payload"
        );
        #[cfg(unix)]
        assert!(
            fs::symlink_metadata(destination.join("file-link"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}
