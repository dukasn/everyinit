# SPDX-FileCopyrightText: 2026 Eduardo Marques Santos
# SPDX-License-Identifier: GPL-3.0-only
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPORTER = Path(__file__).with_name("dependency-audit-report.py")


class ReportFixtures(unittest.TestCase):
    def run_report(self, cargo, root_npm, vscode_npm, statuses=None, tools=None):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        base = Path(temporary.name)
        source = base / "input"
        output = base / "reports"
        source.mkdir()
        for name, value in (("cargo-audit.json", cargo), ("npm-root-audit.json", root_npm), ("npm-vscode-audit.json", vscode_npm)):
            (source / name).write_text(value if isinstance(value, str) else json.dumps(value), encoding="utf-8")
        metadata = base / "metadata.json"
        metadata.write_text(json.dumps({
            "commit": "abc123", "pull_request": 42, "tools": tools or {"cargo-audit": "cargo-audit-audit 0.22.2", "npm": "10.9.9"},
            "scope": ["Cargo.lock", "package-lock.json", "editors/vscode/package-lock.json"],
            "scanner_status": statuses if statuses is not None else {"cargo-audit": {"exit_code": 0}, "npm-root": {"exit_code": 0}, "npm-vscode": {"exit_code": 0}},
        }), encoding="utf-8")
        result = subprocess.run([sys.executable, str(REPORTER), "--input-dir", str(source), "--output-dir", str(output), "--metadata", str(metadata)], check=False)
        return result.returncode, output, source

    @staticmethod
    def cargo(warnings=None, vulnerabilities=None):
        return {"vulnerabilities": {"found": bool(vulnerabilities), "list": vulnerabilities or []}, "warnings": warnings or {}}

    def test_clean_report(self):
        code, output, _ = self.run_report(self.cargo(), {"vulnerabilities": {}}, {"vulnerabilities": {}})
        self.assertEqual(code, 0)
        self.assertIn("**Status:** PASSED", (output / "summary.md").read_text())
        self.assertEqual(json.loads((output / "findings.json").read_text()), [])

    def test_unmaintained_notices_are_informational(self):
        notice = {"package": {"name": "old", "version": "1.0"}, "advisory": {"id": "RUSTSEC-1", "title": "Unmaintained"}}
        code, output, _ = self.run_report(self.cargo({"unmaintained": [notice]}), {"vulnerabilities": {}}, {"vulnerabilities": {}})
        self.assertEqual(code, 0)
        findings = json.loads((output / "findings.json").read_text())
        self.assertEqual(findings[0]["kind"], "maintenance")

    def test_vulnerabilities_fail(self):
        advisory = {"package": {"name": "bad", "version": "1.0"}, "advisory": {"id": "RUSTSEC-2", "title": "Bad", "url": "https://example.test"}}
        code, output, _ = self.run_report(self.cargo(vulnerabilities=[advisory]), {"vulnerabilities": {}}, {"vulnerabilities": {}})
        self.assertEqual(code, 1)
        self.assertIn("**Status:** FAILED", (output / "summary.md").read_text())

    def test_npm_advisory_is_normalized(self):
        npm = {"vulnerabilities": {"bad-package": {
            "severity": "high", "isDirect": True, "range": "<2.0.0",
            "nodes": ["node_modules/bad-package"], "fixAvailable": False,
            "via": [{"source": 123, "name": "bad-package", "title": "Example flaw", "url": "https://example.test/advisory", "severity": "high", "range": "<2.0.0"}],
        }}}
        code, output, _ = self.run_report(self.cargo(), npm, {"vulnerabilities": {}})
        self.assertEqual(code, 1)
        finding = json.loads((output / "findings.json").read_text())[0]
        self.assertEqual(finding["kind"], "vulnerability")
        self.assertEqual(finding["severity"], "high")
        self.assertEqual(finding["range"], "<2.0.0")
        self.assertEqual(finding["direct"], True)
        self.assertEqual(finding["locations"], ["node_modules/bad-package"])

    def test_malformed_output_fails_closed(self):
        code, output, _ = self.run_report("not json", {"vulnerabilities": {}}, {"vulnerabilities": {}})
        self.assertEqual(code, 1)
        self.assertIn("could not read valid JSON", (output / "summary.md").read_text())

    def test_scanner_error_fails_closed(self):
        statuses = {"cargo-audit": {"exit_code": 1}, "npm-root": {"exit_code": 0}, "npm-vscode": {"exit_code": 0}}
        code, output, _ = self.run_report(self.cargo(), {"vulnerabilities": {}}, {"vulnerabilities": {}}, statuses)
        self.assertEqual(code, 1)
        self.assertIn("scanner execution failed", (output / "summary.md").read_text())

    def test_unpinned_tool_fails_closed(self):
        code, output, _ = self.run_report(self.cargo(), {"vulnerabilities": {}}, {"vulnerabilities": {}}, tools={"cargo-audit": "0.22.1", "npm": "11.16.0"})
        self.assertEqual(code, 1)
        self.assertIn("expected pinned version", (output / "summary.md").read_text())


if __name__ == "__main__":
    unittest.main()
