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
use semver::Version;

pub fn semver<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: semver: usage: semver [-v ver] [-c ver1 op ver2] [-b patch|minor|major ver] [-g part ver]\n");
            return Ok(1);
        }

        let first = args[0].as_str();
        if first == "-v" || first == "--validate" {
            if args.len() < 2 {
                ctx.write_stderr(b"esh: semver: missing version argument\n");
                return Ok(1);
            }
            let v_str = args[1].trim_start_matches('v');
            if Version::parse(v_str).is_ok() {
                Ok(0)
            } else {
                Ok(1)
            }
        } else if first == "-c" || first == "--compare" {
            if args.len() < 4 {
                ctx.write_stderr(b"esh: semver -c: usage: semver -c <ver1> <op> <ver2>\n");
                return Ok(1);
            }
            let v1_str = args[1].trim_start_matches('v');
            let op_str = &args[2];
            let v2_str = args[3].trim_start_matches('v');

            let v1 = match Version::parse(v1_str) {
                Ok(v) => v,
                Err(_) => return Ok(1),
            };
            let v2 = match Version::parse(v2_str) {
                Ok(v) => v,
                Err(_) => return Ok(1),
            };

            let matched = match op_str.as_str() {
                "=" | "==" => v1 == v2,
                "!=" => v1 != v2,
                ">" => v1 > v2,
                ">=" => v1 >= v2,
                "<" => v1 < v2,
                "<=" => v1 <= v2,
                _ => {
                    ctx.write_stderr(
                        format!("esh: semver: unknown operator '{}'\n", op_str).as_bytes(),
                    );
                    return Ok(1);
                }
            };

            if matched { Ok(0) } else { Ok(1) }
        } else if first == "-b" || first == "--bump" {
            if args.len() < 3 {
                ctx.write_stderr(
                    b"esh: semver -b: usage: semver -b <patch|minor|major> <version>\n",
                );
                return Ok(1);
            }
            let bump_type = &args[1];
            let v_str = args[2].trim_start_matches('v');

            let mut v = match Version::parse(v_str) {
                Ok(v) => v,
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: semver: invalid version '{}': {}\n", args[2], e).as_bytes(),
                    );
                    return Ok(1);
                }
            };

            match bump_type.to_lowercase().as_str() {
                "patch" => {
                    v.patch += 1;
                    v.pre = semver::Prerelease::EMPTY;
                    v.build = semver::BuildMetadata::EMPTY;
                }
                "minor" => {
                    v.minor += 1;
                    v.patch = 0;
                    v.pre = semver::Prerelease::EMPTY;
                    v.build = semver::BuildMetadata::EMPTY;
                }
                "major" => {
                    v.major += 1;
                    v.minor = 0;
                    v.patch = 0;
                    v.pre = semver::Prerelease::EMPTY;
                    v.build = semver::BuildMetadata::EMPTY;
                }
                other => {
                    ctx.write_stderr(
                        format!(
                            "esh: semver: unknown bump type '{}' (expected patch, minor, major)\n",
                            other
                        )
                        .as_bytes(),
                    );
                    return Ok(1);
                }
            }

            ctx.write_stdout(format!("{}\n", v).as_bytes());
            Ok(0)
        } else if first == "-g" || first == "--get" {
            if args.len() < 3 {
                ctx.write_stderr(
                    b"esh: semver -g: usage: semver -g <major|minor|patch|pre|build> <version>\n",
                );
                return Ok(1);
            }
            let part = &args[1];
            let v_str = args[2].trim_start_matches('v');
            let v = match Version::parse(v_str) {
                Ok(v) => v,
                Err(e) => {
                    ctx.write_stderr(
                        format!("esh: semver: invalid version '{}': {}\n", args[2], e).as_bytes(),
                    );
                    return Ok(1);
                }
            };

            let out = match part.to_lowercase().as_str() {
                "major" => v.major.to_string(),
                "minor" => v.minor.to_string(),
                "patch" => v.patch.to_string(),
                "pre" | "prerelease" => v.pre.as_str().to_string(),
                "build" => v.build.as_str().to_string(),
                other => {
                    ctx.write_stderr(format!("esh: semver: unknown part '{}'\n", other).as_bytes());
                    return Ok(1);
                }
            };

            ctx.write_stdout(format!("{}\n", out).as_bytes());
            Ok(0)
        } else {
            // Default: try parsing as version to check validity
            let v_str = first.trim_start_matches('v');
            if Version::parse(v_str).is_ok() {
                Ok(0)
            } else {
                ctx.write_stderr(format!("esh: semver: invalid version '{}'\n", first).as_bytes());
                Ok(1)
            }
        }
    })
}
