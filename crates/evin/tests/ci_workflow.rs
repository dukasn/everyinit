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

use serde_yaml::Value;
use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn ci_runs_full_validation_and_gate_on_pushes_and_pull_requests() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let workflow: Value =
        serde_yaml::from_str(&fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap())
            .unwrap();
    for event in ["push", "pull_request"] {
        assert!(
            workflow["on"][event]["branches"]
                .as_sequence()
                .unwrap()
                .iter()
                .any(|branch| branch.as_str() == Some("main"))
        );
    }

    let jobs = &workflow["jobs"];
    assert!(jobs["classify"].is_null());
    assert_eq!(
        jobs["validation"]["if"].as_str(),
        Some("github.event_name != 'schedule'")
    );
    let steps = jobs["validation"]["steps"].as_sequence().unwrap();
    let commands = steps
        .iter()
        .filter_map(|step| step["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for command in [
        "cargo evin-dev run quality",
        "cargo evin-dev run test",
        "cargo evin-dev run e2e",
        "cargo evin-dev run --verbose release:distribution-rehearsal",
        "cargo evin-dev run --verbose release:release-archives-verify",
    ] {
        assert!(commands.contains(command), "missing command: {command}");
    }

    let gate = &jobs["ci-gate"];
    assert_eq!(gate["needs"].as_str(), Some("validation"));
    assert_eq!(
        gate["if"].as_str(),
        Some("always() && github.event_name != 'schedule'")
    );
    let command = gate["steps"][0]["run"].as_str().unwrap();
    for (result, expected) in [
        ("success", true),
        ("failure", false),
        ("skipped", false),
        ("cancelled", false),
    ] {
        let status = Command::new("sh")
            .args(["-c", command])
            .env("VALIDATION_RESULT", result)
            .status()
            .unwrap();
        assert_eq!(status.success(), expected, "validation result: {result}");
    }
}

#[test]
fn flow_and_cargo_workspace_versions_match() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let flow: Value =
        serde_yaml::from_str(&fs::read_to_string(root.join(".evin/workspace.flow")).unwrap())
            .unwrap();
    assert_eq!(
        flow["workspace"]["version"].as_str(),
        Some(env!("CARGO_PKG_VERSION"))
    );
}
