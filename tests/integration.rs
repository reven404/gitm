use std::fs;
use std::process::Command;

fn bin() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("target");
    p.push("debug");
    p.push("gitm");
    p
}

fn git(cwd: &std::path::Path, args: &[&str]) {
    let s = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .expect("git");
    assert!(s.success(), "git {:?} in {:?}", args, cwd);
}

/// Build a tiny local git repo to use as a clone/worktree source.
fn make_repo(dir: &std::path::Path) {
    fs::create_dir_all(dir).unwrap();
    git(dir, &["init"]);
    git(dir, &["config", "user.email", "t@t.t"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
    fs::write(dir.join("README.md"), "# demo\n").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-m", "init"]);
}

fn run(args: &[&str], cwd: &std::path::Path) -> String {
    let out = Command::new(bin())
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("gitm");
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let e = String::from_utf8_lossy(&out.stderr).to_string();
    if !out.status.success() {
        panic!("gitm {:?} failed: {}\n{}", args, out.status, e);
    }
    s
}

#[test]
fn init_add_ls_x_rm_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = tmp.path().join("ws");
    let src = tmp.path().join("src-repo");
    make_repo(&src);

    // init (no AI, no scan)
    let out = run(&["--no-ai", "init", &ws.to_string_lossy()], tmp.path());
    assert!(out.contains("Initialized gitm workspace"));

    // the workspace root must NOT be a git repo — subprojects are independent
    // clones/worktrees; a root .git would pollute status and nest inside any
    // parent repo.
    assert!(!ws.join(".git").exists(), "init must not git-init the workspace root");

    // add as worktree (local path)
    let out = run(
        &["--no-ai", "add", &src.to_string_lossy(), "--tag", "demo"],
        &ws,
    );
    assert!(out.contains("added project"));

    // the project name should equal the source basename
    let name = "src-repo";

    // ls (table)
    let out = run(&["ls"], &ws);
    assert!(out.contains(name));

    // ls --format json
    let out = run(&["ls", "--format", "json"], &ws);
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(v[0]["name"], serde_json::json!(name));
    assert_eq!(v[0]["type"], serde_json::json!("worktree"));

    // branch should equal workspace basename (ws)
    assert_eq!(v[0]["branch"], serde_json::json!("ws"));

    // x --dry-run
    let out = run(&["x", "--dry-run", "--", "echo", "hi"], &ws);
    assert!(out.contains("$ echo hi"));

    // x -- echo
    let out = run(&["x", "-j", "2", "--", "echo", "hello"], &ws);
    assert!(out.contains("hello"));

    // x -t demo
    let out = run(&["x", "-t", "demo", "--", "echo", "tagged"], &ws);
    assert!(out.contains("tagged"));

    // CLAUDE.md got a pending row (since --no-ai during add)
    let claude = fs::read_to_string(ws.join("CLAUDE.md")).unwrap();
    assert!(claude.contains(&format!("| {} |", name)));

    // rm (worktree: unregister + worktree remove)
    let out = run(&["rm", name], &ws);
    assert!(out.contains("removed project"));
    assert!(!ws.join(name).exists());

    // CLAUDE.md row removed
    let claude = fs::read_to_string(ws.join("CLAUDE.md")).unwrap();
    assert!(!claude.contains(&format!("| {} |", name)) || !claude.contains("(pending)"));
}

#[test]
fn init_scans_existing_repos() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = tmp.path().join("ws2");
    fs::create_dir_all(&ws).unwrap();
    // two pre-existing repos
    make_repo(&ws.join("alpha"));
    make_repo(&ws.join("beta"));

    let out = run(&["--no-ai", "init", &ws.to_string_lossy()], tmp.path());
    assert!(out.contains("projects=2"));

    let ls = run(&["ls"], &ws);
    assert!(ls.contains("alpha"));
    assert!(ls.contains("beta"));
}
