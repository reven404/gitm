use crate::config::Config;
use crate::exec;
use crate::gitops;
use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;

enum Host {
    Github,
    Gitlab,
    Other,
}

fn parse_host(url: &str) -> Host {
    if url.contains("github.com") {
        Host::Github
    } else if url.contains("gitlab") {
        Host::Gitlab
    } else {
        Host::Other
    }
}

fn which(exe: &str) -> bool {
    Command::new(exe)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn run(
    root: &Path,
    cfg: &Config,
    action: &str,
    project: &Option<String>,
    tag: &Option<String>,
) -> Result<i32> {
    let targets = exec::filter(cfg, project, tag)?;
    let mut rc = 0;
    for p in &targets {
        let path = root.join(&p.path);
        let url = gitops::remote_origin(&path).unwrap_or_default();
        let host = parse_host(&url);
        let cli = match &host {
            Host::Github => "gh",
            Host::Gitlab => "glab",
            Host::Other => {
                eprintln!("[{}] unsupported remote: {}", p.name, url);
                rc = 1;
                continue;
            }
        };
        if !which(cli) {
            eprintln!("[{}] {} not installed; install {} to use `gitm pr`", p.name, cli, cli);
            rc = 1;
            continue;
        }
        // For create we use gh/gl specific args; for list/view the noun differs.
        let args: Vec<&str> = match action {
            "create" => {
                if matches!(host, Host::Github) {
                    vec!["pr", "create", "--fill"]
                } else {
                    vec!["mr", "create"]
                }
            }
            "list" | "view" => {
                let noun = if matches!(host, Host::Github) { "pr" } else { "mr" };
                vec![noun, action]
            }
            other => return Err(anyhow!("unknown pr action: {}", other)),
        };
        print!("[{}] $ {} ", p.name, cli);
        for a in &args {
            print!("{} ", a);
        }
        println!("@ {}", path.display());
        let status = Command::new(cli).args(&args).current_dir(&path).status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                eprintln!("[{}] {} exited {}", p.name, cli, s);
                rc = 1;
            }
            Err(e) => {
                eprintln!("[{}] {} failed: {}", p.name, cli, e);
                rc = 1;
            }
        }
    }
    Ok(rc)
}
