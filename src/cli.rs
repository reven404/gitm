use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// gitm — AI-aware multi-repo workspace orchestrator.
#[derive(Parser, Debug)]
#[command(
    name = "gitm",
    version,
    about = "AI-aware multi-repo workspace orchestrator",
    long_about = "AI-aware multi-repo workspace orchestrator.\n\n\
        Group git repos under one workspace, run commands in parallel, sync, PR via gh/glab, \
        and auto-maintain a CLAUDE.md subproject catalog with an AI backend \
        (claude/opencode/codex).",
    after_help = "EXAMPLES:\n  \
        gitm init myws && cd myws\n  \
        gitm add git@github.com:org/svc.git --tag backend\n  \
        gitm add ../local-repo --tag frontend\n  \
        gitm ls                 # live status (branch/dirty/ahead/behind)\n  \
        gitm ls --format json\n  \
        gitm x -j 4 -- 'make test'\n  \
        gitm x -t backend -- 'go build ./...'\n  \
        gitm x --dry-run -- 'make test'\n  \
        gitm sync && gitm pr create\n\n\
        AI reference:  gitm docs   (also llms.txt at the repo root)",
    arg_required_else_help = true
)]
pub struct Cli {
    /// Skip any AI step (offline / CI).
    #[arg(long, global = true)]
    pub no_ai: bool,

    /// Override AI backend (claude | opencode | codex | none).
    #[arg(long, global = true, value_name = "BACKEND")]
    pub ai: Option<String>,

    /// Verbose.
    #[arg(short = 'v', long, global = true)]
    pub verbose: bool,

    /// Skip the startup version-update check.
    #[arg(long, global = true)]
    pub no_check: bool,

    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Initialize a gitm workspace.
    Init {
        /// Target directory (default: current).
        dir: Option<PathBuf>,
        /// Explicitly set the AI backend, skip detection.
        #[arg(long, value_name = "BACKEND")]
        ai: Option<String>,
        /// Do not scan DIR for existing git sub-repos.
        #[arg(long)]
        no_scan: bool,
    },

    /// Add a project (git URL clone or local-path worktree).
    Add {
        source: String,
        name: Option<String>,
        /// Tag(s) to attach to the project (repeatable).
        #[arg(short = 'T', long = "tag", value_name = "TAG")]
        tags: Vec<String>,
    },

    /// List projects with live status.
    Ls {
        #[arg(long, value_name = "FORMAT", default_value = "table")]
        format: String,
        /// Filter by tag.
        #[arg(short = 't', long, value_name = "TAG")]
        tag: Option<String>,
    },

    /// Run a command across projects (parallel).
    X {
        /// Single project.
        #[arg(short = 'p', long, value_name = "NAME")]
        project: Option<String>,
        /// Filter by tag.
        #[arg(short = 't', long, value_name = "TAG")]
        tag: Option<String>,
        /// Max parallel jobs (0 = num cpus).
        #[arg(short = 'j', long, default_value = "0")]
        jobs: usize,
        /// Stop on first failure.
        #[arg(long)]
        fail_fast: bool,
        /// Print what would run, do not execute.
        #[arg(long)]
        dry_run: bool,
        /// Command (after --).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        cmd: Vec<String>,
    },

    /// Fetch + fast-forward pull across projects.
    Sync {
        #[arg(short = 'j', long, default_value = "0")]
        jobs: usize,
    },

    /// Remove a project.
    Rm {
        name: String,
        /// Also delete on disk for cloned repos (worktree/local are unregistered only / worktree-removed).
        #[arg(long)]
        force: bool,
    },

    /// (Re)run AI analysis and rewrite CLAUDE.md rows.
    Ai {
        name: Option<String>,
        #[arg(long)]
        refresh: bool,
    },

    /// Check / fetch a newer gitm release from GitHub.
    Update {
        /// Only check, do not download.
        #[arg(long)]
        check: bool,
    },

    /// Print version and build info.
    Version,

    /// Print the AI-friendly reference (same as llms.txt).
    Docs,
}
