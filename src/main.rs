mod ai;
mod cli;
mod config;
mod exec;
mod gitops;
mod pr;
mod scan;
mod status;
mod update;

use anyhow::{anyhow, Result};
use clap::Parser;
use cli::{Cli, Cmd};
use config::{basename, find_root, Config, Project};
use rayon::prelude::*;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

const TEMPLATE_CLAUDE_MD: &str = include_str!("../templates/CLAUDE.md");

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    // Auto version check (throttled 24h, silent on failure), unless disabled
    // or the user is already running `update`.
    let is_meta = matches!(
        cli.cmd,
        Cmd::Update { .. } | Cmd::Version | Cmd::Docs
    );
    if !cli.no_check && !is_meta {
        update::check_and_notify();
    }
    match &cli.cmd {
        Cmd::Init { dir, ai, no_scan } => cmd_init(dir.clone(), ai.clone(), *no_scan, &cli),
        Cmd::Add { source, name, tags } => cmd_add(source, name.clone(), tags.clone(), &cli),
        Cmd::Ls { format, tag, watch } => cmd_ls(format, tag, watch, &cli),
        Cmd::X {
            project,
            tag,
            jobs,
            fail_fast,
            dry_run,
            cmd,
        } => cmd_x(project, tag, *jobs, *fail_fast, *dry_run, cmd, &cli),
        Cmd::Sync { jobs } => cmd_sync(*jobs, &cli),
        Cmd::Pr {
            action,
            project,
            tag,
        } => cmd_pr(action, project, tag, &cli),
        Cmd::Rm { name, force } => cmd_rm(name, *force, &cli),
        Cmd::Ai { name, refresh } => cmd_ai(name, *refresh, &cli),
        Cmd::Update { check } => cmd_update(*check),
        Cmd::Version => cmd_version(),
        Cmd::Docs => cmd_docs(),
    }
}

const DOCS: &str = include_str!("../llms.txt");

fn cmd_docs() -> Result<()> {
    print!("{}", DOCS);
    Ok(())
}

fn cmd_version() -> Result<()> {
    println!("gitm {}", env!("CARGO_PKG_VERSION"));
    println!("target  {}/{}", std::env::consts::OS, std::env::consts::ARCH);
    println!("repo    {}/{}", update::repo_owner(), update::repo_name());
    let avail = ai::detect();
    println!(
        "ai      {}",
        if avail.is_empty() {
            "(none detected)".to_string()
        } else {
            avail.join(", ")
        }
    );
    Ok(())
}

fn cmd_update(check: bool) -> Result<()> {
    if check {
        update::run_check()
    } else {
        update::run_update()
    }
}

/// Resolve effective AI backend from CLI override or config.
fn backend_of(cli: &Cli, cfg: &Config) -> String {
    if cli.no_ai {
        return "none".to_string();
    }
    cli.ai.clone().unwrap_or_else(|| cfg.ai.backend.clone())
}

/// Build a config view with the resolved backend (does not persist).
fn cfg_with_backend(cli: &Cli, mut cfg: Config) -> Config {
    cfg.ai.backend = backend_of(cli, &cfg);
    cfg
}

fn cmd_init(dir: Option<PathBuf>, ai: Option<String>, no_scan: bool, cli: &Cli) -> Result<()> {
    let dir = dir.unwrap_or_else(|| PathBuf::from("."));
    let abs = dir
        .canonicalize()
        .unwrap_or_else(|_| {
            let mut p = std::env::current_dir().unwrap();
            p.push(&dir);
            p
        });
    if !abs.exists() {
        std::fs::create_dir_all(&abs)?;
    }
    if abs.join("gitm.toml").exists() {
        return Err(anyhow!(
            "already a gitm workspace: {} (remove gitm.toml to re-init)",
            abs.display()
        ));
    }
    gitops::git_inherit(&abs, &["init"])?;
    let claude = abs.join("CLAUDE.md");
    if !claude.exists() {
        std::fs::write(&claude, TEMPLATE_CLAUDE_MD)?;
    }
    let backend = if cli.no_ai {
        "none".to_string()
    } else {
        ai::pick_backend(ai)
    };
    let mut projects = Vec::new();
    if !no_scan {
        projects = scan::scan_dir(&abs);
    }
    let branch = basename(&abs);
    let cfg = Config {
        workspace: config::Workspace {
            branch: branch.clone(),
        },
        ai: config::Ai {
            backend: backend.clone(),
        },
        project: projects,
    };
    cfg.save(&abs)?;
    let view = cfg_with_backend(cli, cfg.clone());
    for p in &view.project {
        let _ = ai::analyze(&abs, &view, p, false);
    }
    println!(
        "Initialized gitm workspace at {} (branch={}, ai={}, projects={})",
        abs.display(),
        branch,
        backend,
        view.project.len()
    );
    Ok(())
}

fn cmd_add(source: &str, name: Option<String>, tags: Vec<String>, cli: &Cli) -> Result<()> {
    let root = find_root()?;
    let mut cfg = Config::load(&root)?;
    let name = name.unwrap_or_else(|| gitops::derive_name(source));
    let path = name.clone();
    let dest = root.join(&path);
    if dest.exists() {
        return Err(anyhow!("project path already exists: {}", dest.display()));
    }
    let branch = cfg.workspace.branch.clone();
    let kind;
    if gitops::is_git_url(source) {
        gitops::clone_into(&root, source, &path, &branch)?;
        kind = "cloned".to_string();
    } else {
        let src = Path::new(source)
            .canonicalize()
            .map_err(|e| anyhow!("source path not found: {}: {}", source, e))?;
        gitops::git_out(&src, &["rev-parse", "--git-dir"])?;
        gitops::worktree_add(&src, &dest, &branch)?;
        kind = "worktree".to_string();
    }
    let p = Project {
        name: name.clone(),
        source: source.to_string(),
        kind,
        path,
        branch: Some(branch),
        tags,
    };
    cfg.upsert(p.clone());
    cfg.save(&root)?;
    let view = cfg_with_backend(cli, cfg.clone());
    let _ = ai::analyze(&root, &view, &p, false);
    println!("added project {}", name);
    Ok(())
}

fn cmd_ls(format: &str, tag: &Option<String>, watch: &Option<u64>, cli: &Cli) -> Result<()> {
    let _ = cli;
    let root = find_root()?;
    let cfg = Config::load(&root)?;
    let projs: Vec<&Project> = match tag {
        Some(t) => cfg.project.iter().filter(|p| p.tags.contains(t)).collect(),
        None => cfg.project.iter().collect(),
    };
    if format == "json" {
        if watch.is_some() {
            return Err(anyhow!("--watch is incompatible with --format json"));
        }
        let rows: Vec<_> = projs
            .iter()
            .map(|p| {
                let s = status::compute(&root, p);
                serde_json::json!({
                    "name": p.name,
                    "type": p.kind,
                    "source": p.source,
                    "path": p.path,
                    "branch": s.branch,
                    "tags": p.tags,
                    "status": {
                        "dirty": s.dirty,
                        "ahead": s.ahead,
                        "behind": s.behind,
                        "has_upstream": s.has_upstream,
                    },
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Array(rows))?
        );
        return Ok(());
    } else if format != "table" {
        return Err(anyhow!("unknown format: {} (table|json)", format));
    }

    let secs = watch.unwrap_or(0);
    loop {
        let statuses = status::compute_all(&root, &projs);
        if secs > 0 {
            print!("\x1b[2J\x1b[H");
        }
        println!(
            "{:<24} {:<10} {:<20} {:<16} {}",
            "NAME", "TYPE", "BRANCH", "STATUS", "PATH"
        );
        println!("{}", "-".repeat(90));
        for (p, s) in projs.iter().zip(statuses.iter()) {
            println!(
                "{:<24} {:<10} {:<20} {:<16} {}",
                p.name, p.kind, s.branch, s.cell(), p.path
            );
        }
        let _ = std::io::stdout().flush();
        if secs == 0 {
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(secs));
    }
}

fn cmd_x(
    project: &Option<String>,
    tag: &Option<String>,
    jobs: usize,
    fail_fast: bool,
    dry_run: bool,
    cmd: &[String],
    cli: &Cli,
) -> Result<()> {
    let _ = cli;
    let root = find_root()?;
    let cfg = Config::load(&root)?;
    let rc = exec::run(&root, &cfg, project, tag, jobs, fail_fast, dry_run, cmd)?;
    if rc != 0 {
        std::process::exit(rc);
    }
    Ok(())
}

fn cmd_sync(jobs: usize, cli: &Cli) -> Result<()> {
    let _ = cli;
    let root = find_root()?;
    let cfg = Config::load(&root)?;
    let pool = exec::pool_jobs(jobs)?;
    let fail = std::sync::atomic::AtomicBool::new(false);
    pool.install(|| {
        cfg.project.par_iter().for_each(|p| {
            let path = root.join(&p.path);
            match gitops::sync_one(&path) {
                Ok(()) => println!("[{}] synced", p.name),
                Err(e) => {
                    eprintln!("[{}] sync: {}", p.name, e);
                    fail.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
        });
    });
    if fail.load(std::sync::atomic::Ordering::SeqCst) {
        std::process::exit(1);
    }
    Ok(())
}

fn cmd_pr(action: &str, project: &Option<String>, tag: &Option<String>, cli: &Cli) -> Result<()> {
    let _ = cli;
    let root = find_root()?;
    let cfg = Config::load(&root)?;
    let rc = pr::run(&root, &cfg, action, project, tag)?;
    if rc != 0 {
        std::process::exit(rc);
    }
    Ok(())
}

fn cmd_rm(name: &str, force: bool, cli: &Cli) -> Result<()> {
    let _ = cli;
    let root = find_root()?;
    let mut cfg = Config::load(&root)?;
    let p = cfg
        .find(name)
        .cloned()
        .ok_or_else(|| anyhow!("project not found: {}", name))?;
    let dest = root.join(&p.path);
    match p.kind.as_str() {
        "worktree" => {
            let main = Path::new(&p.source);
            if main.is_dir() {
                gitops::worktree_remove(main, &dest)?;
            } else {
                let _ = std::fs::remove_dir_all(&dest);
            }
        }
        "cloned" => {
            if force {
                std::fs::remove_dir_all(&dest)?;
            } else {
                println!(
                    "cloned project {} left on disk at {} (use --force to delete)",
                    p.name,
                    dest.display()
                );
            }
        }
        "local" => {
            println!("local project {} unregistered (directory kept)", p.name);
        }
        _ => {}
    }
    cfg.project.retain(|x| x.name != name);
    cfg.save(&root)?;
    let claude = root.join("CLAUDE.md");
    if claude.exists() {
        let content = std::fs::read_to_string(&claude)?;
        let updated = ai::remove_catalog_row(&content, name);
        std::fs::write(&claude, updated)?;
    }
    println!("removed project {}", name);
    Ok(())
}

fn cmd_ai(name: &Option<String>, refresh: bool, cli: &Cli) -> Result<()> {
    let root = find_root()?;
    let cfg = Config::load(&root)?;
    let view = cfg_with_backend(cli, cfg.clone());
    if view.ai.backend == "none" {
        eprintln!(
            "AI backend is 'none'; set [ai].backend in gitm.toml or install claude/opencode/codex"
        );
        return Ok(());
    }
    let targets: Vec<&Project> = match name {
        Some(n) => {
            let p = view
                .find(n)
                .ok_or_else(|| anyhow!("project not found: {}", n))?;
            vec![p]
        }
        None => view.project.iter().collect(),
    };
    for p in targets {
        ai::analyze(&root, &view, p, refresh)?;
        println!("analyzed {}", p.name);
    }
    Ok(())
}
