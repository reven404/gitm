use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;

/// True if `s` looks like a remote git URL.
pub fn is_git_url(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("git@")
        || s.starts_with("ssh://")
        || s.starts_with("git://")
        || s.ends_with(".git")
}

/// Derive a project name from a source (URL or path).
pub fn derive_name(source: &str) -> String {
    let trimmed = source.trim_end_matches('/').trim_end_matches(".git");
    let last = trimmed.rsplit(['/', ':']).next().unwrap_or(trimmed);
    last.to_string()
}

/// Run git, return stdout as a trimmed String. Errors on non-zero exit.
pub fn git_out(cwd: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()?;
    if !out.status.success() {
        return Err(anyhow!(
            "git {:?} failed ({}): {}",
            args,
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Run git ignoring failure; returns stdout Option.
pub fn git_out_opt(cwd: &Path, args: &[&str]) -> Option<String> {
    git_out(cwd, args).ok()
}

/// `git -C <cwd> <args>` with inherited stdio, returning status.
pub fn git_inherit(cwd: &Path, args: &[&str]) -> Result<std::process::ExitStatus> {
    let status = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()?;
    Ok(status)
}

pub fn remote_origin(cwd: &Path) -> Option<String> {
    git_out_opt(cwd, &["remote", "get-url", "origin"])
}

pub fn current_branch(cwd: &Path) -> String {
    git_out_opt(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "detached".into())
}

/// Clone a git URL into `dest` (relative to root), then checkout/force-create `branch`.
pub fn clone_into(root: &Path, url: &str, dest_rel: &str, branch: &str) -> Result<()> {
    git_inherit(root, &["clone", url, dest_rel])?;
    let dest = root.join(dest_rel);
    git_out(&dest, &["checkout", "-B", branch])?;
    Ok(())
}

/// Create a worktree of `src` at `dest_abs` (detached), then create `branch`.
pub fn worktree_add(src: &Path, dest_abs: &Path, branch: &str) -> Result<()> {
    let dest_str = dest_abs.to_string_lossy().to_string();
    git_out(src, &["worktree", "add", "--detach", &dest_str])?;
    git_out(dest_abs, &["checkout", "-B", branch])?;
    Ok(())
}

/// Remove a worktree from its main repo.
pub fn worktree_remove(main_repo: &Path, dest_abs: &Path) -> Result<()> {
    let dest_str = dest_abs.to_string_lossy().to_string();
    git_out(main_repo, &["worktree", "remove", &dest_str])
        .or_else(|_| git_out(main_repo, &["worktree", "remove", "--force", &dest_str]))
        .map(|_| ())
}

/// sync: fetch --prune + pull --ff-only; skips dirty repos.
pub fn sync_one(path: &Path) -> Result<()> {
    let dirty = !git_out_opt(path, &["status", "--porcelain"])
        .unwrap_or_default()
        .is_empty();
    if dirty {
        return Err(anyhow!("dirty working tree, skipping"));
    }
    let _ = git_out(path, &["fetch", "--prune", "origin"]);
    git_out(path, &["pull", "--ff-only"])?;
    Ok(())
}
