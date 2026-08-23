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

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::hash::Hash;

#[derive(Debug, Default)]
pub struct DirectedGraph<T: Hash + Eq + Clone + Ord + std::fmt::Debug> {
    pub adjacency_list: HashMap<T, Vec<T>>,
    pub in_degree: HashMap<T, usize>,
    nodes: HashSet<T>,
}

impl<T: Hash + Eq + Clone + Ord + std::fmt::Debug> DirectedGraph<T> {
    pub fn new() -> Self {
        Self {
            adjacency_list: HashMap::new(),
            in_degree: HashMap::new(),
            nodes: HashSet::new(),
        }
    }

    pub fn add_node(&mut self, node: T) {
        self.nodes.insert(node.clone());
        self.in_degree.entry(node).or_insert(0);
    }

    pub fn add_edge(&mut self, from: T, to: T) {
        self.add_node(from.clone());
        self.add_node(to.clone());
        self.adjacency_list
            .entry(from)
            .or_default()
            .push(to.clone());
        *self.in_degree.entry(to).or_insert(0) += 1;
    }

    pub fn topological_sort(&self) -> Result<Vec<T>, Vec<T>> {
        let mut in_degree = self.in_degree.clone();
        let mut queue: BinaryHeap<Reverse<T>> = BinaryHeap::new();
        let mut sorted: Vec<T> = Vec::new();

        for (node, &degree) in &in_degree {
            if degree == 0 {
                queue.push(Reverse(node.clone()));
            }
        }

        while let Some(Reverse(node)) = queue.pop() {
            sorted.push(node.clone());

            if let Some(neighbors) = self.adjacency_list.get(&node) {
                for neighbor in neighbors {
                    if let Some(degree) = in_degree.get_mut(neighbor) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push(Reverse(neighbor.clone()));
                        }
                    }
                }
            }
        }

        if sorted.len() != self.nodes.len() {
            let mut cycle_nodes = Vec::new();
            for (node, &degree) in &in_degree {
                if degree > 0 {
                    cycle_nodes.push(node.clone());
                }
            }
            cycle_nodes.sort(); // Deterministic start
            return Err(cycle_nodes);
        }

        Ok(sorted)
    }

    pub fn find_cycle_path(&self, cycle_nodes: &[T]) -> Vec<T> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();

        for node in cycle_nodes {
            if !visited.contains(node) && self.dfs(node, &mut visited, &mut rec_stack, &mut path) {
                let last = path.last().unwrap().clone();
                let start_idx = path.iter().position(|x| x == &last).unwrap();
                return path[start_idx..].to_vec();
            }
        }
        cycle_nodes.to_vec()
    }

    fn dfs(
        &self,
        node: &T,
        visited: &mut HashSet<T>,
        rec_stack: &mut HashSet<T>,
        path: &mut Vec<T>,
    ) -> bool {
        visited.insert(node.clone());
        rec_stack.insert(node.clone());
        path.push(node.clone());

        if let Some(neighbors) = self.adjacency_list.get(node) {
            let mut sorted_neighbors = neighbors.clone();
            sorted_neighbors.sort();
            for neighbor in sorted_neighbors {
                if !visited.contains(&neighbor) {
                    if self.dfs(&neighbor, visited, rec_stack, path) {
                        return true;
                    }
                } else if rec_stack.contains(&neighbor) {
                    path.push(neighbor.clone());
                    return true;
                }
            }
        }

        rec_stack.remove(node);
        path.pop();
        false
    }
}
