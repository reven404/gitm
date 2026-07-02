use crate::config::Project;
use crate::gitops::git_out_opt;
use rayon::prelude::*;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub branch: String,
    pub dirty: bool,
    pub ahead: usize,
    pub behind: usize,
    pub has_upstream: bool,
}

impl Status {
    /// Render a compact status cell, e.g. `✔`, `✱dirty`, `↑2↓1`, `∅no-upstream`.
    pub fn cell(&self) -> String {
        if !self.has_upstream {
            return "∅no-upstream".to_string();
        }
        let mut parts = Vec::new();
        if self.dirty {
            parts.push("✱dirty".to_string());
        }
        if self.ahead > 0 {
            parts.push(format!("↑{}", self.ahead));
        }
        if self.behind > 0 {
            parts.push(format!("↓{}", self.behind));
        }
        if parts.is_empty() {
            "✔".to_string()
        } else {
            parts.join(" ")
        }
    }
}

pub fn compute(root: &Path, p: &Project) -> Status {
    let path = root.join(&p.path);
    let branch = crate::gitops::current_branch(&path);
    let dirty = !git_out_opt(&path, &["status", "--porcelain"])
        .unwrap_or_default()
        .is_empty();
    let (ahead, behind, has_upstream) =
        match git_out_opt(&path, &["rev-list", "--left-right", "--count", "@{u}...HEAD"]) {
            Some(s) => {
                let mut it = s.split_whitespace();
                let behind = it.next().and_then(|x| x.parse::<usize>().ok()).unwrap_or(0);
                let ahead = it.next().and_then(|x| x.parse::<usize>().ok()).unwrap_or(0);
                (ahead, behind, true)
            }
            None => (0, 0, false),
        };
    Status {
        branch,
        dirty,
        ahead,
        behind,
        has_upstream,
    }
}

pub fn compute_all(root: &Path, projects: &[&Project]) -> Vec<Status> {
    projects
        .par_iter()
        .map(|p| compute(root, p))
        .collect()
}
