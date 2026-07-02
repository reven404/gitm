use crate::config::{Config, Project};
use anyhow::{anyhow, Result};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BACKENDS: &[(&str, &str)] = &[("claude", "claude"), ("opencode", "opencode"), ("codex", "codex")];

/// Return names of available AI backends in priority order.
pub fn detect() -> Vec<&'static str> {
    BACKENDS
        .iter()
        .filter(|(_, exe)| available(exe))
        .map(|(name, _)| *name)
        .collect()
}

fn available(exe: &str) -> bool {
    Command::new(exe)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Decide backend at init time: explicit > interactive pick > auto.
pub fn pick_backend(explicit: Option<String>) -> String {
    if let Some(b) = explicit {
        return b;
    }
    let found = detect();
    match found.len() {
        0 => {
            eprintln!("warn: no AI backend found (claude/opencode/codex); AI features disabled. Set [ai].backend to enable.");
            "none".to_string()
        }
        1 => found[0].to_string(),
        _ => {
            if !std::io::stdin().is_terminal() {
                eprintln!(
                    "warn: multiple AI backends detected {:?}; selected {} (non-interactive)",
                    found, found[0]
                );
                return found[0].to_string();
            }
            println!("Multiple AI backends detected. Select one:");
            for (i, b) in found.iter().enumerate() {
                println!("  {}) {}", i + 1, b);
            }
            print!("> ");
            let _ = std::io::stdout().flush();
            let mut s = String::new();
            let _ = std::io::stdin().read_line(&mut s);
            match s.trim().parse::<usize>() {
                Ok(n) if n >= 1 && n <= found.len() => found[n - 1].to_string(),
                _ => {
                    eprintln!("invalid input; using {}", found[0]);
                    found[0].to_string()
                }
            }
        }
    }
}

trait AiBackend {
    fn name(&self) -> &str;
    fn build(&self, prompt: &str, cwd: &Path) -> Command;
}

struct Claude;
impl AiBackend for Claude {
    fn name(&self) -> &str {
        "claude"
    }
    fn build(&self, prompt: &str, cwd: &Path) -> Command {
        let mut c = Command::new("claude");
        c.arg("-p").arg(prompt).current_dir(cwd);
        c
    }
}

struct Opencode;
impl AiBackend for Opencode {
    fn name(&self) -> &str {
        "opencode"
    }
    fn build(&self, prompt: &str, cwd: &Path) -> Command {
        let mut c = Command::new("opencode");
        c.arg("run").arg(prompt).current_dir(cwd);
        c
    }
}

struct Codex;
impl AiBackend for Codex {
    fn name(&self) -> &str {
        "codex"
    }
    fn build(&self, prompt: &str, cwd: &Path) -> Command {
        let mut c = Command::new("codex");
        c.arg("exec").arg(prompt).current_dir(cwd);
        c
    }
}

fn resolve(backend: &str) -> Result<Box<dyn AiBackend>> {
    match backend {
        "claude" => Ok(Box::new(Claude)),
        "opencode" => Ok(Box::new(Opencode)),
        "codex" => Ok(Box::new(Codex)),
        "none" => Err(anyhow!("AI backend is 'none'; set [ai].backend or install claude/opencode/codex")),
        other => Err(anyhow!("unknown AI backend: {}", other)),
    }
}

fn prompt_for(name: &str) -> String {
    format!(
        "You are analyzing a git project in the current working directory.\n\
         Read its manifest files (e.g. package.json, go.mod, Cargo.toml, pom.xml, build.gradle, \
         pyproject.toml, requirements.txt, composer.json, Gemfile) and identify the primary \
         language, framework, and build tool.\n\
         Output EXACTLY ONE line in this Markdown table-row format and NOTHING ELSE (no prose, no code fence):\n\
         | {name} | <stack> | <role> |\n\
         where <stack> is a concise tech-stack description (e.g. 'Node 18 + Egg.js') and <role> is a \
         4-8 word description of the project's purpose inferred from its files.",
        name = name
    )
}

/// Extract the last `| ... |` line from the model output; fallback None.
fn extract_row(stdout: &str, name: &str) -> Option<String> {
    stdout
        .lines()
        .map(|l| l.trim())
        .filter(|l| l.starts_with('|') && l.ends_with('|') && l.chars().filter(|c| *c == '|').count() >= 3)
        .last()
        .map(|l| {
            // normalize: ensure it starts with `| <name> `
            let first = l.trim_start_matches('|').split('|').next().unwrap_or("").trim();
            if first == name {
                l.to_string()
            } else {
                l.replacen(first, name, 1)
            }
        })
}

/// Rewrite the Subproject Catalog table in CLAUDE.md: replace row by name or append.
pub fn update_catalog(content: &str, name: &str, new_row: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let header_idx = lines
        .iter()
        .position(|l| l.contains("Subproject") && l.contains("Stack"));
    let Some(hi) = header_idx else {
        return content.to_string();
    };
    // separator is hi+1; rows start at hi+2
    if hi + 1 >= lines.len() {
        return content.to_string();
    }
    let mut out: Vec<String> = lines[..=hi + 1].iter().map(|s| s.to_string()).collect();
    let mut replaced = false;
    let mut i = hi + 2;
    while i < lines.len() {
        let l = lines[i];
        if !l.trim_start().starts_with('|') {
            break;
        }
        if l.starts_with("|-") || l.starts_with("| -") || l.starts_with("| --") {
            out.push(l.to_string());
            i += 1;
            continue;
        }
        let first_cell = l
            .trim_start_matches('|')
            .split('|')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if first_cell == name {
            out.push(new_row.to_string());
            replaced = true;
        } else {
            out.push(l.to_string());
        }
        i += 1;
    }
    if !replaced {
        out.push(new_row.to_string());
    }
    out.extend(lines[i..].iter().map(|s| s.to_string()));
    out.join("\n")
}

pub fn remove_catalog_row(content: &str, name: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let header_idx = lines
        .iter()
        .position(|l| l.contains("Subproject") && l.contains("Stack"));
    let Some(hi) = header_idx else {
        return content.to_string();
    };
    if hi + 1 >= lines.len() {
        return content.to_string();
    }
    let mut out: Vec<String> = lines[..=hi + 1].iter().map(|s| s.to_string()).collect();
    let mut i = hi + 2;
    while i < lines.len() {
        let l = lines[i];
        if !l.trim_start().starts_with('|') {
            break;
        }
        if l.starts_with("|-") || l.starts_with("| -") {
            out.push(l.to_string());
            i += 1;
            continue;
        }
        let first_cell = l
            .trim_start_matches('|')
            .split('|')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if first_cell == name {
            // skip (remove)
        } else {
            out.push(l.to_string());
        }
        i += 1;
    }
    out.extend(lines[i..].iter().map(|s| s.to_string()));
    out.join("\n")
}

/// Run AI analysis for a project and rewrite its CLAUDE.md row.
/// Best-effort: on any failure writes a pending row and returns Ok.
pub fn analyze(root: &Path, cfg: &Config, p: &Project, _refresh: bool) -> Result<()> {
    let claude_md: PathBuf = root.join("CLAUDE.md");
    let row = match resolve(&cfg.ai.backend) {
        Ok(backend) => {
            let prompt = prompt_for(&p.name);
            let out = backend
                .build(&prompt, &root.join(&p.path))
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    let s = String::from_utf8_lossy(&o.stdout);
                    extract_row(&s, &p.name)
                        .unwrap_or_else(|| format!("| {} | (pending) | |", p.name))
                }
                Ok(o) => {
                    eprintln!(
                        "warn: AI backend {} exited {} for {}; row set pending",
                        backend.name(),
                        o.status,
                        p.name
                    );
                    format!("| {} | (pending) | |", p.name)
                }
                Err(e) => {
                    eprintln!(
                        "warn: AI backend {} failed to spawn for {}: {}; row set pending",
                        backend.name(),
                        p.name,
                        e
                    );
                    format!("| {} | (pending) | |", p.name)
                }
            }
        }
        Err(_) => {
            // backend is 'none' or unconfigured; write a silent pending placeholder
            format!("| {} | (pending) | |", p.name)
        }
    };
    let content = std::fs::read_to_string(&claude_md).unwrap_or_default();
    let updated = update_catalog(&content, &p.name, &row);
    std::fs::write(&claude_md, updated)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_replace_in_place_and_idempotent() {
        let content = include_str!("../templates/CLAUDE.md");
        let updated = update_catalog(content, "foo", "| foo | Rust | demo |");
        assert!(updated.contains("| foo | Rust | demo |"));
        let again = update_catalog(&updated, "foo", "| foo | Go | demo2 |");
        assert!(again.contains("| foo | Go | demo2 |"));
        assert!(!again.contains("Rust"));
        assert_eq!(again.matches("| foo |").count(), 1);
    }

    #[test]
    fn catalog_remove_row() {
        let content = include_str!("../templates/CLAUDE.md");
        let with_row = update_catalog(content, "bar", "| bar | Python | svc |");
        assert!(with_row.contains("| bar |"));
        let removed = remove_catalog_row(&with_row, "bar");
        assert!(!removed.contains("| bar | Python | svc |"));
    }

    #[test]
    fn extract_row_picks_last_table_line() {
        let out = "thinking...\n| foo | Rust | demo |\nextra\n| foo | Go | demo2 |";
        let row = extract_row(out, "foo").unwrap();
        assert!(row.contains("Go"));
    }
}
