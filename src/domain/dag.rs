//! Dependency-free DAG validation and readiness.

use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSpec {
    pub id: String,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    DuplicateTask(String),
    UnknownDependency { task: String, dependency: String },
    SelfDependency(String),
    DuplicateDependency { task: String, dependency: String },
    Cycle,
}

#[derive(Debug, Clone)]
pub struct TaskGraph {
    tasks: Vec<TaskSpec>,
    depth: usize,
}

impl TaskGraph {
    pub fn new(tasks: Vec<TaskSpec>) -> Result<Self, GraphError> {
        let mut ids = HashSet::new();
        for task in &tasks {
            if !ids.insert(task.id.as_str()) {
                return Err(GraphError::DuplicateTask(task.id.clone()));
            }
        }
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut remaining = HashMap::new();
        let mut roots = VecDeque::new();
        for task in &tasks {
            let id = task.id.as_str();
            let mut seen = HashSet::new();
            for dependency in &task.depends_on {
                if id == dependency {
                    return Err(GraphError::SelfDependency(task.id.clone()));
                }
                if !ids.contains(dependency.as_str()) {
                    return Err(GraphError::UnknownDependency {
                        task: task.id.clone(),
                        dependency: dependency.clone(),
                    });
                }
                if !seen.insert(dependency) {
                    return Err(GraphError::DuplicateDependency {
                        task: task.id.clone(),
                        dependency: dependency.clone(),
                    });
                }
                dependents.entry(dependency).or_default().push(id);
            }
            if seen.is_empty() {
                roots.push_back((id, 1));
            }
            remaining.insert(id, seen.len());
        }

        // FIFO processes each depth before the next, so the last prerequisite
        // removed for a task is always at its greatest prerequisite depth.
        let mut depth = 0;
        let mut visited = 0;
        while let Some((id, level)) = roots.pop_front() {
            depth = level;
            visited += 1;
            for &dependent in dependents.get(id).into_iter().flatten() {
                let count = remaining.get_mut(dependent).expect("validated task");
                *count -= 1;
                if *count == 0 {
                    roots.push_back((dependent, level + 1));
                }
            }
        }
        if visited != tasks.len() {
            return Err(GraphError::Cycle);
        }
        Ok(Self { tasks, depth })
    }

    /// Number of tasks on the longest prerequisite chain.
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Returns IDs whose dependencies have completed, excluding completed tasks.
    /// The caller supplies the set of verified completions.
    pub fn ready<'a>(&'a self, completed: &HashSet<&str>) -> Vec<&'a str> {
        self.tasks
            .iter()
            .filter(|task| {
                !completed.contains(task.id.as_str())
                    && task
                        .depends_on
                        .iter()
                        .all(|dependency| completed.contains(dependency.as_str()))
            })
            .map(|task| task.id.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, deps: &[&str]) -> TaskSpec {
        TaskSpec {
            id: id.to_owned(),
            depends_on: deps.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn independent_tasks_are_ready() {
        let graph = TaskGraph::new(vec![task("a", &[]), task("b", &[])]).unwrap();
        assert_eq!(graph.ready(&HashSet::new()), vec!["a", "b"]);
    }

    #[test]
    fn dependent_waits_for_all_predecessors() {
        let graph =
            TaskGraph::new(vec![task("a", &[]), task("b", &[]), task("c", &["a", "b"])]).unwrap();
        let completed = HashSet::from(["a"]);
        assert_eq!(graph.ready(&completed), vec!["b"]);
        let completed = HashSet::from(["a", "b"]);
        assert_eq!(graph.ready(&completed), vec!["c"]);
    }

    #[test]
    fn rejects_unknown_dependency() {
        let err = TaskGraph::new(vec![task("a", &["missing"])]).unwrap_err();
        assert!(matches!(err, GraphError::UnknownDependency { .. }));
    }

    #[test]
    fn rejects_duplicate_task_ids() {
        let err = TaskGraph::new(vec![task("a", &[]), task("a", &[])]).unwrap_err();
        assert_eq!(err, GraphError::DuplicateTask("a".to_owned()));
    }

    #[test]
    fn rejects_duplicate_edges() {
        let error = TaskGraph::new(vec![task("a", &[]), task("b", &["a", "a"])]).unwrap_err();
        assert_eq!(
            error,
            GraphError::DuplicateDependency {
                task: "b".into(),
                dependency: "a".into(),
            }
        );
    }

    #[test]
    fn rejects_cycle() {
        let err = TaskGraph::new(vec![task("a", &["b"]), task("b", &["a"])]).unwrap_err();
        assert_eq!(err, GraphError::Cycle);
    }

    #[test]
    fn rejects_self_dependency() {
        let err = TaskGraph::new(vec![task("a", &["a"])]).unwrap_err();
        assert_eq!(err, GraphError::SelfDependency("a".to_owned()));
    }
}
