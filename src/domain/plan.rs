use super::dag::{GraphError, TaskGraph, TaskSpec};

pub const PLAN_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Lead,
    Developer,
    Verifier,
    Reviewer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    pub id: String,
    pub role: Role,
}

#[derive(Debug, Clone)]
pub struct PlannedTask {
    pub graph: TaskSpec,
    pub role: Role,
    pub priority: i32,
    pub allowed_paths: Vec<String>,
    pub acceptance: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_tasks: usize,
    pub max_depth: usize,
    pub max_parallel: usize,
    /// Total attempts per task, including the first.
    pub max_attempts: u32,
}

pub struct Plan {
    pub version: u32,
    pub tasks: Vec<PlannedTask>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PlanError {
    Version(u32),
    InvalidLimits,
    TaskCount(usize),
    Depth(usize),
    EmptyTaskId,
    MissingAcceptance(String),
    InvalidScope(String),
    UnsafePath { task: String, path: String },
    Graph(GraphError),
}

#[derive(Debug)]
pub struct ValidatedPlan {
    pub(super) tasks: Vec<PlannedTask>,
    pub(super) graph: TaskGraph,
    pub(super) limits: Limits,
}

impl Plan {
    /// Checks proposed paths against trusted workspace-relative roots.
    /// Empty task path lists represent tasks with no requested write scope.
    pub fn validate(self, limits: Limits, scope: &[&str]) -> Result<ValidatedPlan, PlanError> {
        if self.version != PLAN_VERSION {
            return Err(PlanError::Version(self.version));
        }
        if [limits.max_tasks, limits.max_depth, limits.max_parallel].contains(&0)
            || limits.max_attempts == 0
        {
            return Err(PlanError::InvalidLimits);
        }
        if self.tasks.is_empty() || self.tasks.len() > limits.max_tasks {
            return Err(PlanError::TaskCount(self.tasks.len()));
        }
        for &root in scope {
            if !relative_path(root) {
                return Err(PlanError::InvalidScope(root.into()));
            }
        }
        for task in &self.tasks {
            if task.graph.id.trim().is_empty() {
                return Err(PlanError::EmptyTaskId);
            }
            if task.acceptance.is_empty()
                || task.acceptance.iter().any(|item| item.trim().is_empty())
            {
                return Err(PlanError::MissingAcceptance(task.graph.id.clone()));
            }
            for path in &task.allowed_paths {
                if !relative_path(path)
                    || !scope.iter().any(|root| {
                        path == root
                            || path
                                .strip_prefix(root)
                                .is_some_and(|rest| rest.starts_with('/'))
                    })
                {
                    return Err(PlanError::UnsafePath {
                        task: task.graph.id.clone(),
                        path: path.clone(),
                    });
                }
            }
        }
        let graph = TaskGraph::new(self.tasks.iter().map(|task| task.graph.clone()).collect())
            .map_err(PlanError::Graph)?;
        if graph.depth() > limits.max_depth {
            return Err(PlanError::Depth(graph.depth()));
        }
        Ok(ValidatedPlan {
            tasks: self.tasks,
            graph,
            limits,
        })
    }
}

fn relative_path(path: &str) -> bool {
    !path.contains(['\\', ':'])
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !matches!(part, "" | "." | "..") && !part.eq_ignore_ascii_case(".git"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> Plan {
        Plan {
            version: PLAN_VERSION,
            tasks: vec![PlannedTask {
                graph: TaskSpec {
                    id: "a".into(),
                    depends_on: vec![],
                },
                role: Role::Developer,
                priority: 0,
                allowed_paths: vec!["src/lib.rs".into()],
                acceptance: vec!["Tests pass".into()],
            }],
        }
    }

    fn limits() -> Limits {
        Limits {
            max_tasks: 1,
            max_depth: 1,
            max_parallel: 1,
            max_attempts: 1,
        }
    }

    #[test]
    fn only_allows_clean_paths_under_trusted_roots() {
        plan().validate(limits(), &["src"]).unwrap();
        for path in [
            "/src/lib.rs",
            "src/../README.md",
            "src-old/file",
            "src/.git/config",
            "src//file",
            "src/./file",
            "src/a\\b",
            "C:/src/file",
            "",
            "src/\0file",
        ] {
            let mut plan = plan();
            plan.tasks[0].allowed_paths = vec![path.into()];
            assert!(
                matches!(
                    plan.validate(limits(), &["src"]),
                    Err(PlanError::UnsafePath { .. })
                ),
                "{path:?}"
            );
        }
        assert_eq!(
            plan().validate(limits(), &["../src"]).unwrap_err(),
            PlanError::InvalidScope("../src".into())
        );
    }

    #[test]
    fn enforces_version_acceptance_and_positive_limits() {
        let mut proposal = plan();
        proposal.version += 1;
        assert_eq!(
            proposal.validate(limits(), &["src"]).unwrap_err(),
            PlanError::Version(2)
        );

        let mut proposal = plan();
        proposal.tasks[0].acceptance = vec![" ".into()];
        assert_eq!(
            proposal.validate(limits(), &["src"]).unwrap_err(),
            PlanError::MissingAcceptance("a".into())
        );

        let mut limits = limits();
        limits.max_attempts = 0;
        assert_eq!(
            plan().validate(limits, &["src"]).unwrap_err(),
            PlanError::InvalidLimits
        );
    }

    #[test]
    fn enforces_graph_size_and_longest_chain() {
        let make_plan = || {
            let mut proposal = plan();
            let mut dependent = proposal.tasks[0].clone();
            dependent.graph.id = "b".into();
            dependent.graph.depends_on = vec!["a".into()];
            proposal.tasks.insert(0, dependent);
            proposal
        };
        let mut limits = limits();
        assert_eq!(
            make_plan().validate(limits, &["src"]).unwrap_err(),
            PlanError::TaskCount(2)
        );
        limits.max_tasks = 2;
        assert_eq!(
            make_plan().validate(limits, &["src"]).unwrap_err(),
            PlanError::Depth(2)
        );
        limits.max_depth = 2;
        make_plan().validate(limits, &["src"]).unwrap();
    }
}
