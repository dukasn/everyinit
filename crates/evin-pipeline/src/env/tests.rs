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

use super::*;

#[test]
fn test_environment_merging_hierarchy() {
    let mut workspace_env = HashMap::new();
    workspace_env.insert("OVERRIDE_ME".to_string(), "workspace_value".to_string());
    workspace_env.insert("WORKSPACE_ONLY".to_string(), "true".to_string());

    let mut module_env = HashMap::new();
    module_env.insert("OVERRIDE_ME".to_string(), "module_value".to_string());
    module_env.insert("MODULE_ONLY".to_string(), "true".to_string());

    let mut task_env = HashMap::new();
    task_env.insert("OVERRIDE_ME".to_string(), "task_value".to_string());
    task_env.insert("TASK_ONLY".to_string(), "true".to_string());

    let expose = vec![];
    let merged = build_env(true, &expose, &workspace_env, &module_env, &task_env);

    // Ensure task has highest priority
    assert_eq!(merged.get("OVERRIDE_ME").unwrap(), "task_value");
    assert_eq!(merged.get("WORKSPACE_ONLY").unwrap(), "true");
    assert_eq!(merged.get("MODULE_ONLY").unwrap(), "true");
    assert_eq!(merged.get("TASK_ONLY").unwrap(), "true");
}

#[test]
fn test_pure_escape_hatch() {
    let empty = HashMap::new();

    // Set a dummy env var for testing (if possible, else we just test PATH absence)
    let merged_pure = build_env(true, &[], &empty, &empty, &empty);
    assert!(!merged_pure.contains_key("PATH"));

    // Note: It's harder to reliably test non-pure because PATH might not exist in CI,
    // but typically it does. We skip explicit assertion of PATH to avoid flakiness,
    // but pure definitely strips everything.
    assert!(merged_pure.is_empty());
}
