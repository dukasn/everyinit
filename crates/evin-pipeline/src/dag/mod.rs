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

use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub enum DagError {
    MissingDependency { task: String, dep: String },
    CycleDetected { path: String },
}

impl From<DagError> for evin_diagnostic::IssueCode {
    fn from(error: DagError) -> Self {
        match error {
            DagError::MissingDependency { task, dep } => {
                evin_diagnostic::IssueCode::new("issue-missing-dependency")
                    .arg("task", task)
                    .arg("dep", dep)
            }
            DagError::CycleDetected { path } => {
                evin_diagnostic::IssueCode::new("issue-cycle-detected").arg("path", path)
            }
        }
    }
}

use evin_config::models::TaskObject;

pub mod graph;
use graph::DirectedGraph;

pub type PipelineState = (
    HashMap<String, TaskObject>,
    HashMap<String, Vec<String>>,
    HashMap<String, usize>,
);

#[derive(Debug, Default)]
pub struct DagResolver {
    registry: HashMap<String, TaskObject>,
    graph: DirectedGraph<String>,
}

impl DagResolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_task(&mut self, task: TaskObject) {
        self.registry.insert(task.name.clone(), task);
    }

    pub fn registry(&self) -> &HashMap<String, TaskObject> {
        &self.registry
    }

    fn build_graph(&mut self) -> Result<(), DagError> {
        self.graph = DirectedGraph::new();

        // Validate dependencies and initialize in_degree
        for (task_name, task) in &self.registry {
            self.graph.add_node(task_name.clone());

            if let Some(needs) = &task.needs {
                for dep in needs {
                    if !self.registry.contains_key(dep) {
                        return Err(DagError::MissingDependency {
                            task: task_name.clone(),
                            dep: dep.clone(),
                        });
                    }
                    self.graph.add_edge(dep.clone(), task_name.clone());
                }
            }
        }

        Ok(())
    }

    pub fn build_graph_public(&mut self) -> Result<(), DagError> {
        self.build_graph()
    }

    pub fn into_pipeline_state(self) -> PipelineState {
        (
            self.registry,
            self.graph.adjacency_list,
            self.graph.in_degree,
        )
    }

    pub fn resolve(&mut self) -> Result<Vec<String>, DagError> {
        self.build_graph()?;

        match self.graph.topological_sort() {
            Ok(sorted) => Ok(sorted),
            Err(cycle_nodes) => {
                let path = self.graph.find_cycle_path(&cycle_nodes);
                Err(DagError::CycleDetected {
                    path: path.join(" -> "),
                })
            }
        }
    }

    pub fn resolve_targets(&mut self, targets: &[String]) -> Result<Vec<String>, DagError> {
        self.build_graph()?;

        let mut target_set = HashSet::new();
        let mut queue: Vec<String> = targets.to_vec();

        // 1. Subgraph Discovery
        while let Some(node) = queue.pop() {
            if !self.registry.contains_key(&node) {
                return Err(DagError::MissingDependency {
                    task: node.clone(),
                    dep: node.clone(),
                });
            }
            if target_set.insert(node.clone())
                && let Some(task) = self.registry.get(&node)
                && let Some(needs) = &task.needs
            {
                for dep in needs {
                    queue.push(dep.clone());
                }
            }
        }

        // Prune the registry so only the subgraph is executed by the PipelineEngine
        self.registry.retain(|k, _| target_set.contains(k));

        // 2. Topological Sort on Subgraph
        let mut subgraph = DirectedGraph::new();
        for node in &target_set {
            subgraph.add_node(node.clone());
        }

        for node in &target_set {
            if let Some(task) = self.registry.get(node)
                && let Some(needs) = &task.needs
            {
                for dep in needs {
                    if target_set.contains(dep) {
                        subgraph.add_edge(dep.clone(), node.clone());
                    }
                }
            }
        }

        match subgraph.topological_sort() {
            Ok(sorted) => Ok(sorted),
            Err(cycle_nodes) => {
                let path = subgraph.find_cycle_path(&cycle_nodes);
                Err(DagError::CycleDetected {
                    path: path.join(" -> "),
                })
            }
        }
    }
}
