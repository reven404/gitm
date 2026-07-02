use crate::config::{basename, Project};
use crate::gitops;
use std::path::Path;

/// Scan `dir`'s direct subdirectories that are git repos, return Projects.
pub fn scan_dir(dir: &Path) -> Vec<Project> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let gitdir = path.join(".git");
        if !gitdir.exists() {
            continue;
        }
        // skip the root's own .git
        let name = match path.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };
        if name == ".git" {
            continue;
        }
        let (kind, source) = if gitdir.is_file() {
            let content = std::fs::read_to_string(&gitdir).unwrap_or_default();
            let src = extract_worktree_source(&content).unwrap_or_else(|| "(local)".to_string());
            ("worktree", src)
        } else {
            let src = gitops::remote_origin(&path).unwrap_or_else(|| "(local)".to_string());
            ("local", src)
        };
        let branch = gitops::current_branch(&path);
        out.push(Project {
            name,
            source,
            kind: kind.to_string(),
            path: basename(&path),
            branch: Some(branch),
            tags: Vec::new(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Parse `gitdir: /abs/path/.git/worktrees/<name>` → `/abs/path`.
fn extract_worktree_source(content: &str) -> Option<String> {
    let line = content.lines().next()?;
    let p = line.strip_prefix("gitdir: ")?.trim();
    let idx = p.find("/.git/worktrees")?;
    Some(p[..idx].to_string())
}
