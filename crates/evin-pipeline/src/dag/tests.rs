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
use evin_config::models::TaskObject;

fn new_task(name: &str, needs: Option<Vec<&str>>) -> TaskObject {
    TaskObject {
        name: name.to_string(),
        needs: needs.map(|v| v.into_iter().map(|s| s.to_string()).collect()),
        ..Default::default()
    }
}

#[test]
fn test_happy_path_linear() {
    let mut resolver = DagResolver::new();

    // C needs B, B needs A
    resolver.add_task(new_task("A", None));
    resolver.add_task(new_task("B", Some(vec!["A"])));
    resolver.add_task(new_task("C", Some(vec!["B"])));

    let sorted = resolver.resolve().unwrap();
    assert_eq!(sorted, vec!["A", "B", "C"]);
}

#[test]
fn test_diamond_dependency() {
    let mut resolver = DagResolver::new();

    // A needs B and C. Both B and C need D.
    resolver.add_task(new_task("D", None));
    resolver.add_task(new_task("B", Some(vec!["D"])));
    resolver.add_task(new_task("C", Some(vec!["D"])));
    resolver.add_task(new_task("A", Some(vec!["B", "C"])));

    let sorted = resolver.resolve().unwrap();

    // D must be first, A must be last
    assert_eq!(sorted.first().unwrap(), "D");
    assert_eq!(sorted.last().unwrap(), "A");

    // Length must be exactly 4 (no duplicates)
    assert_eq!(sorted.len(), 4);
}

#[test]
fn test_cycle_detection() {
    let mut resolver = DagResolver::new();

    // A needs B, B needs C, C needs A
    resolver.add_task(new_task("A", Some(vec!["B"])));
    resolver.add_task(new_task("B", Some(vec!["C"])));
    resolver.add_task(new_task("C", Some(vec!["A"])));

    let res = resolver.resolve();
    assert!(matches!(res, Err(DagError::CycleDetected { .. })));

    if let Err(DagError::CycleDetected { path }) = res {
        // Since we sort initial nodes for DFS, it starts at A
        // A -> B -> C -> A
        // Wait! A needs B, meaning B is the dependency.
        // Adjacency list is Built: B -> A, C -> B, A -> C
        // So graph edges point from Dep -> Task
        // If DFS explores from A: it sees C (A -> C)
        // Let's assert it contains "A -> C -> B -> A" or something similar.
        assert!(path.contains("->"));
        println!("Cycle path: {}", path);
    }
}

#[test]
fn test_missing_dependencies() {
    let mut resolver = DagResolver::new();

    // A needs Z, Z is not defined
    resolver.add_task(new_task("A", Some(vec!["Z"])));

    let res = resolver.resolve();
    match res {
        Err(DagError::MissingDependency { task, dep }) => {
            assert_eq!(task, "A");
            assert_eq!(dep, "Z");
        }
        _ => panic!("Expected MissingDependency error"),
    }
}

#[test]
fn test_orphan_tasks() {
    let mut resolver = DagResolver::new();

    // 3 orphans: O1, O2, O3
    // 2 dependent tasks: D1 needs O1, D2 needs O2
    resolver.add_task(new_task("D1", Some(vec!["O1"])));
    resolver.add_task(new_task("D2", Some(vec!["O2"])));
    resolver.add_task(new_task("O1", None));
    resolver.add_task(new_task("O3", None));
    resolver.add_task(new_task("O2", None));

    let sorted = resolver.resolve().unwrap();

    // Ensure the resolution is strictly deterministic based on alphabetical priority
    assert_eq!(sorted, vec!["O1", "D1", "O2", "D2", "O3"]);
}

#[test]
fn test_strict_determinism() {
    let mut resolver = DagResolver::new();

    // Mix of orphans
    resolver.add_task(new_task("Zeta", None));
    resolver.add_task(new_task("Alpha", None));
    resolver.add_task(new_task("Gamma", None));
    resolver.add_task(new_task("Beta", None));

    let sorted = resolver.resolve().unwrap();
    assert_eq!(sorted, vec!["Alpha", "Beta", "Gamma", "Zeta"]);
}

#[test]
fn test_yaml_trigger_parsing() {
    let yaml_str = r#"
name: lint
on:
  hooks:
    - pre-commit
run: cargo clippy
"#;

    let ast = evin_yaml::parse_yaml(yaml_str).expect("Failed to parse YAML");
    let task: TaskObject = evin_yaml::de::from_spanned_value(ast).expect("Failed to deserialize");
    assert_eq!(task.name, "lint");
    assert!(task.on.is_some());

    let triggers = task.on.unwrap();
    assert!(triggers.hooks.is_some());
    assert_eq!(triggers.hooks.unwrap(), vec!["pre-commit".to_string()]);
}

#[test]
fn test_resolve_targets_subgraph() {
    let mut resolver = DagResolver::new();

    // A -> B -> C
    // X -> Y
    resolver.add_task(new_task("A", None));
    resolver.add_task(new_task("B", Some(vec!["A"])));
    resolver.add_task(new_task("C", Some(vec!["B"])));

    resolver.add_task(new_task("X", None));
    resolver.add_task(new_task("Y", Some(vec!["X"])));

    // Request to resolve only "B"
    let sorted = resolver.resolve_targets(&["B".to_string()]).unwrap();

    // Should return A, B and ignore C, X, Y
    assert_eq!(sorted, vec!["A", "B"]);
}
