#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Eduardo Marques Santos
# SPDX-License-Identifier: GPL-3.0-only
"""Normalize dependency scanner output into a downloadable audit bundle."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


def read_json(path: Path) -> tuple[Any | None, str | None]:
    try:
        return json.loads(path.read_text(encoding="utf-8")), None
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        return None, f"{type(error).__name__}: {error}"


def advisory_finding(advisory: dict[str, Any], package: str, version: str,
                     scanner: str, severity: str = "unknown") -> dict[str, Any]:
    return {
        "kind": "vulnerability",
        "scanner": scanner,
        "package": package,
        "version": version,
        "id": advisory.get("id") or advisory.get("ghsa") or advisory.get("url") or "unknown",
        "title": advisory.get("title") or advisory.get("name") or "Known vulnerability",
        "severity": advisory.get("severity") or severity,
        "url": advisory.get("url"),
        "direct": advisory.get("direct"),
        "range": advisory.get("range") or advisory.get("vulnerable_versions"),
        "patched": advisory.get("patched") or advisory.get("patched_versions"),
        "via": advisory.get("via"),
        "effects": advisory.get("effects"),
    }


def parse_cargo(data: Any, findings: list[dict[str, Any]]) -> str | None:
    if not isinstance(data, dict):
        return "cargo-audit JSON root must be an object"
    vulnerabilities = data.get("vulnerabilities")
    warnings = data.get("warnings", {})
    if not isinstance(vulnerabilities, dict) or not isinstance(warnings, dict):
        return "cargo-audit JSON is missing expected vulnerabilities or warnings objects"
    for item in vulnerabilities.get("list", []):
        if not isinstance(item, dict):
            continue
        package = item.get("package", {})
        advisory = item.get("advisory", {})
        if isinstance(package, dict) and isinstance(advisory, dict):
            finding = advisory_finding(advisory, package.get("name", "unknown"),
                                       package.get("version", "unknown"), "cargo-audit")
            finding.update({
                "affected_versions": item.get("versions") or item.get("affected"),
                "dependency_path": item.get("dependency"),
            })
            findings.append(finding)
    for category, entries in warnings.items():
        if not isinstance(entries, list):
            continue
        for item in entries:
            if not isinstance(item, dict):
                continue
            package = item.get("package", {})
            advisory = item.get("advisory", {})
            if not isinstance(package, dict) or not isinstance(advisory, dict):
                continue
            findings.append({
                "kind": "maintenance" if category == "unmaintained" else "notice",
                "scanner": "cargo-audit",
                "category": category,
                "package": package.get("name", "unknown"),
                "version": package.get("version", "unknown"),
                "id": advisory.get("id", "unknown"),
                "title": advisory.get("title", category),
                "url": advisory.get("url"),
                "description": advisory.get("description"),
            })
    return None


def parse_npm(data: Any, scanner: str, lockfile: Path, findings: list[dict[str, Any]]) -> str | None:
    if not isinstance(data, dict) or not isinstance(data.get("vulnerabilities", {}), dict):
        return f"{scanner} JSON root must contain a vulnerabilities object"
    if data.get("error"):
        return f"{scanner} returned an error: {data['error']}"
    lock_data, lock_error = read_json(lockfile)
    if lock_error or not isinstance(lock_data, dict):
        return f"{scanner} could not read its lockfile ({lock_error or 'invalid lockfile'})"
    locked_packages = lock_data.get("packages", {})
    for name, item in data.get("vulnerabilities", {}).items():
        if not isinstance(item, dict):
            continue
        locations = item.get("nodes", [])
        versions = sorted({
            str(locked_packages.get(location, {}).get("version"))
            for location in locations
            if isinstance(locked_packages, dict) and isinstance(locked_packages.get(location), dict)
            and locked_packages[location].get("version") is not None
        })
        if not versions and isinstance(locked_packages, dict):
            versions = sorted({
                str(value.get("version")) for location, value in locked_packages.items()
                if isinstance(value, dict) and location.rsplit("node_modules/", 1)[-1] == name and value.get("version") is not None
            })
        via = item.get("via", [])
        advisories = [entry for entry in via if isinstance(entry, dict)]
        if not advisories and item.get("isDirect") is False:
            continue
        if not advisories:
            advisories = [{"title": "npm audit vulnerability", "severity": item.get("severity")}]
        for advisory in advisories:
            finding = advisory_finding(advisory, name, ", ".join(versions) or "unknown", scanner,
                                       severity=item.get("severity", "unknown"))
            finding.update({
                "range": item.get("range"),
                "locations": locations,
                "direct": item.get("isDirect"),
                "fix_available": item.get("fixAvailable"),
            })
            findings.append(finding)
    return None


def sha256(path: Path) -> str | None:
    try:
        digest = hashlib.sha256()
        with path.open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(chunk)
        return digest.hexdigest()
    except OSError:
        return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--metadata", type=Path, required=True)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    metadata, metadata_error = read_json(args.metadata)
    if metadata_error or not isinstance(metadata, dict):
        metadata = {"metadata_error": metadata_error or "metadata must be a JSON object"}
    findings: list[dict[str, Any]] = []
    errors: list[str] = []
    tools = metadata.get("tools", {})
    if not isinstance(tools, dict):
        errors.append("tool version metadata must be an object")
    else:
        if "0.22.2" not in str(tools.get("cargo-audit", "")):
            errors.append("cargo-audit: expected pinned version 0.22.2")
        if not str(tools.get("npm", "")).startswith("10.9.9"):
            errors.append("npm: expected pinned version 10.9.9")
    specs = [
        ("cargo-audit.json", "cargo-audit", parse_cargo),
        ("npm-root-audit.json", "npm audit (root)", parse_npm, Path("package-lock.json")),
        ("npm-vscode-audit.json", "npm audit (VS Code)", parse_npm, Path("editors/vscode/package-lock.json")),
    ]
    for spec in specs:
        filename, scanner, normalize = spec[:3]
        data, error = read_json(args.input_dir / filename)
        if error:
            errors.append(f"{scanner}: could not read valid JSON ({error})")
            continue
        if normalize is parse_npm:
            error = normalize(data, scanner, spec[3], findings)
        else:
            error = normalize(data, findings)
        if error:
            errors.append(f"{scanner}: {error}")
    statuses = metadata.get("scanner_status", {})
    if not isinstance(statuses, dict):
        errors.append("scanner_status metadata must be an object")
        statuses = {}
    for scanner, result in statuses.items():
        if not isinstance(result, dict):
            errors.append(f"{scanner}: scanner status is malformed")
            continue
        scanner_aliases = {
            "cargo-audit": {"cargo-audit"},
            "npm-root": {"npm audit (root)"},
            "npm-vscode": {"npm audit (VS Code)"},
        }
        scanner_findings = [item for item in findings if item.get("kind") == "vulnerability" and item.get("scanner") in scanner_aliases.get(scanner, {scanner})]
        if result.get("exit_code") != 0 and not scanner_findings:
            errors.append(f"{scanner}: scanner execution failed (exit {result.get('exit_code', 'unknown')})")
    vulnerabilities = [item for item in findings if item.get("kind") == "vulnerability"]
    maintenance = [item for item in findings if item.get("kind") == "maintenance"]
    metadata.update({
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "finding_counts": {"vulnerabilities": len(vulnerabilities), "maintenance": len(maintenance), "other_notices": len(findings) - len(vulnerabilities) - len(maintenance)},
        "audit_errors": errors,
        "lockfile_sha256": {
            name: sha256(Path(name)) for name in ("Cargo.lock", "package-lock.json", "editors/vscode/package-lock.json")
        },
        "limitations": [
            "Audits the committed dependency lockfiles against scanner advisory databases.",
            "Does not analyze application source code, license policy, build artifacts, or runtime behavior.",
            "Unmaintained notices are informational and do not fail the audit by themselves.",
        ],
    })
    args.output_dir.joinpath("metadata.json").write_text(json.dumps(metadata, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    args.output_dir.joinpath("findings.json").write_text(json.dumps(findings, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    status = "FAILED" if vulnerabilities or errors else "PASSED"
    lines = [
        "# Dependency audit report", "", f"**Status:** {status}",
        f"**Audited commit:** `{metadata.get('commit', 'unknown')}`",
        f"**Pull request:** #{metadata.get('pull_request', 'unknown')}",
        f"**Generated:** {metadata['generated_at']}", "",
        "## Results", "",
        f"| Known vulnerabilities | Maintenance notices | Other notices |", "| ---: | ---: | ---: |",
        f"| {len(vulnerabilities)} | {len(maintenance)} | {len(findings) - len(vulnerabilities) - len(maintenance)} |", "",
    ]
    for heading, entries in (("Known vulnerabilities", vulnerabilities), ("Maintenance notices", maintenance)):
        lines += [f"## {heading}", ""]
        if not entries:
            lines += ["None reported.", ""]
        for item in entries:
            ref = f" [{item['id']}]({item['url']})" if item.get("url") else f" ({item['id']})"
            lines.append(f"- **{item['package']} {item['version']}** — {item['title']}{ref}; {item['scanner']}.")
        lines.append("")
    lines += ["## Scanner and scope details", "", "```json", json.dumps({key: metadata.get(key) for key in ("tools", "scope", "lockfile_sha256", "scanner_status")}, indent=2, sort_keys=True), "```", "", "## Limitations", ""]
    lines.extend(f"- {item}" for item in metadata["limitations"])
    if errors:
        lines += ["", "## Audit execution errors", ""]
        lines.extend(f"- {item}" for item in errors)
    args.output_dir.joinpath("summary.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    if output_path := os.environ.get("GITHUB_OUTPUT"):
        with Path(output_path).open("a", encoding="utf-8") as output:
            output.write(f"vulnerabilities={len(vulnerabilities)}\n")
            output.write(f"maintenance={len(maintenance)}\n")
            output.write(f"audit_status={status}\n")
    return 1 if vulnerabilities or errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
