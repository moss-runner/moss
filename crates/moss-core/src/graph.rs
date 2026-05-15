//! Dependency graph for Moss tasks.
//!
//! Builds a directed acyclic graph (DAG) from the `deps=[…]` declarations
//! in a [`Mossfile`] and returns an execution order via topological sort.
//!
//! The parser already guarantees there are no cycles or unknown deps, so
//! this module focuses purely on ordering.
use std::collections::{HashMap, HashSet, VecDeque};

use moss_parser::Mossfile;

use crate::error::RunError;

// ── Graph ─────────────────────────────────────────────────────────────────────

// Adjacency-list representation of the task dependency graph.
//
// An edge `A → B` means "task A depends on task B", i.e. B must run first.
pub struct DependencyGraph<'mf> {
    // Maps task name → list of dependency names.
    edges: HashMap<&'mf str, Vec<&'mf str>>,
}

impl<'mf> DependencyGraph<'mf> {
    // Build a graph from all tasks declared in `mossfile`.
    pub fn build(mossfile: &'mf Mossfile) -> Self {
        let mut edges: HashMap<&'mf str, Vec<&'mf str>> = HashMap::new();

        for task in &mossfile.tasks {
            let deps: Vec<&str> = task.flags.deps.iter().map(|s| s.as_str()).collect();
            edges.insert(task.name.as_str(), deps);
        }

        Self { edges }
    }

    // Return the names of all tasks that must run before `target`, in the
    // correct execution order (dependencies first).
    //
    // The returned list always ends with `target` itself.
    //
    // # Errors
    //
    // Returns [`RunError::TaskNotFound`] if `target` is not in the graph.
    pub fn execution_order(&self, target: &'mf str) -> Result<Vec<&'mf str>, RunError> {
        if !self.edges.contains_key(target) {
            return Err(RunError::TaskNotFound(target.to_string()));
        }

        // Kahn's algorithm (BFS-based topological sort) over the subgraph
        // reachable from `target`.
        let reachable = self.reachable_from(target);

        // Build in-degree map restricted to the reachable subgraph.
        let mut in_degree: HashMap<&str, usize> = reachable.iter().map(|&name| (name, 0)).collect();

        // Count in-degrees: for each edge dep → task, increment task's degree.
        for &name in &reachable {
            for &dep in self.edges.get(name).unwrap_or(&vec![]) {
                if reachable.contains(dep) {
                    *in_degree.entry(name).or_insert(0) += 1;
                }
            }
        }

        // Seed the queue with nodes that have no dependencies.
        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&name, _)| name)
            .collect();

        let mut order: Vec<&str> = Vec::with_capacity(reachable.len());

        while let Some(name) = queue.pop_front() {
            order.push(name);

            // Reduce in-degree of tasks that depend on `name`.
            for &dependent in &reachable {
                if self
                    .edges
                    .get(dependent)
                    .is_some_and(|deps| deps.contains(&name))
                {
                    let deg = in_degree.entry(dependent).or_insert(0);
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push_back(dependent);
                    }
                }
            }
        }

        // The parser already verified no cycles, so `order` should contain
        // every reachable node.  Ensure `target` is last.
        if order.last() != Some(&target) {
            // Move target to the end if Kahn placed it elsewhere.
            order.retain(|&n| n != target);
            order.push(target);
        }

        Ok(order)
    }

    // Collect all task names reachable from `start` via dependency edges
    // (including `start` itself).
    fn reachable_from(&self, start: &'mf str) -> HashSet<&'mf str> {
        let mut visited: HashSet<&'mf str> = HashSet::new();
        let mut stack: Vec<&str> = vec![start];

        while let Some(name) = stack.pop() {
            if visited.insert(name) {
                if let Some(deps) = self.edges.get(name) {
                    for &dep in deps {
                        if !visited.contains(dep) {
                            stack.push(dep);
                        }
                    }
                }
            }
        }

        visited
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use moss_parser::parse;

    #[test]
    fn test_single_task_no_deps() {
        let src = "task build:\n  cargo build\n";
        let mf = parse(src).unwrap();
        let graph = DependencyGraph::build(&mf);
        let order = graph.execution_order("build").unwrap();
        assert_eq!(order, vec!["build"]);
    }

    #[test]
    fn test_linear_deps() {
        let src = concat!(
            "task lint:\n  cargo clippy\n",
            "task build deps=[lint]:\n  cargo build\n",
            "task test deps=[build]:\n  cargo test\n",
        );
        let mf = parse(src).unwrap();
        let graph = DependencyGraph::build(&mf);
        let order = graph.execution_order("test").unwrap();

        // lint must come before build, build before test.
        let pos = |n: &str| order.iter().position(|&x| x == n).unwrap();
        assert!(pos("lint") < pos("build"));
        assert!(pos("build") < pos("test"));
        assert_eq!(order.last(), Some(&"test"));
    }

    #[test]
    fn test_unknown_task_error() {
        let src = "task build:\n  cargo build\n";
        let mf = parse(src).unwrap();
        let graph = DependencyGraph::build(&mf);
        assert!(matches!(
            graph.execution_order("nonexistent"),
            Err(RunError::TaskNotFound(_))
        ));
    }

    #[test]
    fn test_only_reachable_tasks_included() {
        let src = concat!(
            "task lint:\n  cargo clippy\n",
            "task build:\n  cargo build\n", // no dep on lint
        );
        let mf = parse(src).unwrap();
        let graph = DependencyGraph::build(&mf);
        let order = graph.execution_order("build").unwrap();
        // `lint` is not reachable from `build`.
        assert!(!order.contains(&"lint"));
        assert!(order.contains(&"build"));
    }
}
