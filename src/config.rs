use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub workspace: Workspace,
    #[serde(default)]
    pub ai: Ai,
    #[serde(default)]
    pub project: Vec<Project>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    #[serde(default)]
    pub branch: String,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            branch: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ai {
    #[serde(default = "default_backend")]
    pub backend: String,
}
fn default_backend() -> String {
    "none".to_string()
}
impl Default for Ai {
    fn default() -> Self {
        Self {
            backend: default_backend(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub source: String,
    #[serde(rename = "type")]
    pub kind: String, // cloned | worktree | local
    pub path: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl Config {
    pub fn load(root: &Path) -> Result<Self> {
        let s = std::fs::read_to_string(root.join("gitm.toml"))?;
        let cfg: Config = toml::from_str(&s)?;
        Ok(cfg)
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let s = toml::to_string_pretty(self)?;
        std::fs::write(root.join("gitm.toml"), s)?;
        Ok(())
    }

    pub fn upsert(&mut self, p: Project) {
        self.project.retain(|x| x.name != p.name);
        self.project.push(p);
    }

    pub fn find(&self, name: &str) -> Option<&Project> {
        self.project.iter().find(|p| p.name == name)
    }
}

/// Walk up from cwd to find a directory containing `gitm.toml`.
pub fn find_root() -> Result<PathBuf> {
    let mut cur = std::env::current_dir()?;
    loop {
        if cur.join("gitm.toml").exists() {
            return Ok(cur);
        }
        if !cur.pop() {
            break;
        }
    }
    Err(anyhow!(
        "not in a gitm workspace (no gitm.toml found); run `gitm init` first"
    ))
}

pub fn basename(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string_lossy().to_string())
}
