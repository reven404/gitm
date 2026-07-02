//! Self-update via GitHub releases.
//!
//! - `check_and_notify()`: throttled (24h) background-style check; prints a
//!   one-line stderr notice if a newer release exists. Called from main on
//!   every run unless `--no-check` or the subcommand is `update`.
//! - `run_check()`: `gitm update --check` — explicit check + print.
//! - `run_update()`: `gitm update` — download the matching release asset,
//!   extract, atomically replace the running binary.
//!
//! No HTTP crate: shells out to `curl` (consistent with the rest of gitm,
//! which already shells out to git/gh/claude). Version compare via `semver`.
//! Repo owner/name overridable at build or runtime (`GITM_REPO_OWNER/NAME`).

use anyhow::{anyhow, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

const DEFAULT_OWNER: &str = match option_env!("GITM_REPO_OWNER") {
    Some(s) => s,
    None => "reven404",
};
const DEFAULT_NAME: &str = match option_env!("GITM_REPO_NAME") {
    Some(s) => s,
    None => "gitm",
};

pub fn repo_owner() -> String {
    std::env::var("GITM_REPO_OWNER").unwrap_or_else(|_| DEFAULT_OWNER.to_string())
}
pub fn repo_name() -> String {
    std::env::var("GITM_REPO_NAME").unwrap_or_else(|_| DEFAULT_NAME.to_string())
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Debug, Clone)]
pub struct Release {
    pub tag: String,
    pub assets: Vec<(String, String)>, // (name, browser_download_url)
}

pub fn latest_release() -> Result<Release> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        repo_owner(),
        repo_name()
    );
    let out = Command::new("curl")
        .args(["-fsSL", "-H", "User-Agent: gitm", &url])
        .output()?;
    if !out.status.success() {
        return Err(anyhow!(
            "github api request failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let v: Value = serde_json::from_slice(&out.stdout)?;
    let tag = v["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow!("release has no tag_name"))?
        .to_string();
    let mut assets = Vec::new();
    if let Some(arr) = v["assets"].as_array() {
        for a in arr {
            let name = a["name"].as_str().unwrap_or("").to_string();
            let url = a["browser_download_url"].as_str().unwrap_or("").to_string();
            if !name.is_empty() && !url.is_empty() {
                assets.push((name, url));
            }
        }
    }
    Ok(Release { tag, assets })
}

fn strip_v(s: &str) -> &str {
    s.trim().trim_start_matches('v')
}

pub fn is_newer(remote: &str, local: &str) -> bool {
    match (
        semver::Version::parse(strip_v(remote)),
        semver::Version::parse(strip_v(local)),
    ) {
        (Ok(r), Ok(l)) => r > l,
        _ => strip_v(remote) != strip_v(local),
    }
}

fn cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".cache").join("gitm")
}

fn last_check_path() -> PathBuf {
    cache_dir().join("last_check")
}

/// Throttled check (≤ once / 24h). Prints a stderr notice on newer release.
/// Silent on any network error so it never blocks normal usage.
pub fn check_and_notify() {
    let p = last_check_path();
    if let Ok(meta) = std::fs::metadata(&p) {
        if let Ok(mtime) = meta.modified() {
            if let Ok(age) = SystemTime::now().duration_since(mtime) {
                if age < Duration::from_secs(24 * 3600) {
                    return;
                }
            }
        }
    }
    let _ = std::fs::create_dir_all(cache_dir());
    let _ = std::fs::write(&p, b"");
    let cur = current_version();
    match latest_release() {
        Ok(r) if is_newer(&r.tag, cur) => {
            eprintln!(
                "⚡ gitm: new version {} available (current v{}). Run `gitm update`.",
                r.tag, cur
            );
        }
        _ => {}
    }
}

fn target_markers() -> (&'static str, &'static str) {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    let os_mark = match os {
        "macos" => "darwin",
        "linux" => "linux",
        "windows" => "windows",
        other => other,
    };
    (arch, os_mark)
}

pub fn run_check() -> Result<()> {
    let cur = current_version();
    println!("current : v{}", cur);
    let r = latest_release()?;
    println!("latest  : {}", r.tag);
    if is_newer(&r.tag, cur) {
        println!("status  : update available — run `gitm update`");
    } else {
        println!("status  : up to date");
    }
    Ok(())
}

pub fn run_update() -> Result<()> {
    let cur = current_version();
    let r = latest_release()?;
    if !is_newer(&r.tag, cur) {
        println!("already up to date (v{})", cur);
        return Ok(());
    }
    let (arch, os_mark) = target_markers();
    let asset = r
        .assets
        .iter()
        .find(|(n, _)| n.contains(arch) && n.contains(os_mark))
        .or_else(|| r.assets.iter().find(|(n, _)| n.contains(arch)));
    let Some((asset_name, url)) = asset else {
        let names: Vec<_> = r.assets.iter().map(|(n, _)| n.as_str()).collect();
        return Err(anyhow!(
            "no release asset matching {} {} (available: {:?})",
            arch,
            os_mark,
            names
        ));
    };
    println!("downloading {} ...", asset_name);
    let cache = cache_dir();
    let _ = std::fs::create_dir_all(&cache);
    let archive = cache.join("download.tar.gz");
    let s = Command::new("curl")
        .args([
            "-fsSL",
            "-H",
            "User-Agent: gitm",
            "-o",
            &archive.to_string_lossy(),
            url,
        ])
        .status()?;
    if !s.success() {
        return Err(anyhow!("download failed"));
    }
    let extract_dir = cache.join("extract");
    let _ = std::fs::remove_dir_all(&extract_dir);
    std::fs::create_dir_all(&extract_dir)?;
    let s = Command::new("tar")
        .args([
            "xzf",
            &archive.to_string_lossy(),
            "-C",
            &extract_dir.to_string_lossy(),
        ])
        .status()?;
    if !s.success() {
        return Err(anyhow!("extract failed"));
    }
    let new_bin = find_bin(&extract_dir)
        .ok_or_else(|| anyhow!("gitm binary not found in archive"))?;
    let exe = std::env::current_exe()?;
    let staging = exe.with_extension("new");
    if std::fs::rename(&new_bin, &staging).is_err() {
        std::fs::copy(&new_bin, &staging)?;
        let _ = std::fs::remove_file(&new_bin);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&staging)?.permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&staging, perm)?;
    }
    // On Unix, renaming over a running binary is fine (old inode stays alive).
    std::fs::rename(&staging, &exe)?;
    let _ = std::fs::remove_file(&archive);
    println!("updated gitm to {}", r.tag);
    Ok(())
}

fn find_bin(dir: &Path) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(found) = find_bin(&p) {
                return Some(found);
            }
        } else if p.file_name().map(|f| f == "gitm").unwrap_or(false) {
            return Some(p);
        }
    }
    None
}
