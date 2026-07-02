use crate::config::{Config, Project};
use anyhow::{anyhow, Result};
use rayon::prelude::*;
use rayon::ThreadPool;
use rayon::ThreadPoolBuilder;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub fn pool_jobs(jobs: usize) -> Result<ThreadPool> {
    let n = if jobs == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    } else {
        jobs
    };
    Ok(ThreadPoolBuilder::new().num_threads(n).build()?)
}

pub fn filter<'a>(
    cfg: &'a Config,
    project: &Option<String>,
    tag: &Option<String>,
) -> Result<Vec<&'a Project>> {
    match (project, tag) {
        (Some(_), Some(_)) => Err(anyhow!("--project and --tag are mutually exclusive")),
        (Some(name), None) => {
            let p = cfg
                .find(name)
                .ok_or_else(|| anyhow!("project not found: {}", name))?;
            Ok(vec![p])
        }
        (None, Some(t)) => Ok(cfg.project.iter().filter(|p| p.tags.contains(t)).collect()),
        (None, None) => Ok(cfg.project.iter().collect()),
    }
}

pub fn run(
    root: &std::path::Path,
    cfg: &Config,
    project: &Option<String>,
    tag: &Option<String>,
    jobs: usize,
    fail_fast: bool,
    dry_run: bool,
    cmd: &[String],
) -> Result<i32> {
    let targets = filter(cfg, project, tag)?;
    let cmd_str = cmd.join(" ");
    if cmd_str.trim().is_empty() {
        return Err(anyhow!("no command given; usage: gitm x -- <CMD...>"));
    }

    if dry_run {
        for p in &targets {
            println!("[{}] $ {} @ {}", p.name, cmd_str, root.join(&p.path).display());
        }
        return Ok(0);
    }

    let pool = pool_jobs(jobs)?;
    let any_fail = Arc::new(AtomicBool::new(false));
    let stdout_lock = Arc::new(Mutex::new(std::io::stdout()));

    let results: Vec<i32> = pool.install(|| {
        targets
            .par_iter()
            .map(|p| run_one(root, p, &cmd_str, fail_fast, &any_fail, &stdout_lock))
            .collect()
    });

    let failed = results.iter().any(|&c| c != 0);
    Ok(if failed { 1 } else { 0 })
}

fn run_one(
    root: &std::path::Path,
    p: &Project,
    cmd: &str,
    fail_fast: bool,
    any_fail: &Arc<AtomicBool>,
    stdout_lock: &Arc<Mutex<std::io::Stdout>>,
) -> i32 {
    if fail_fast && any_fail.load(Ordering::SeqCst) {
        // best-effort skip; rayon can't cancel in-flight tasks
        return -1;
    }
    let path = root.join(&p.path);
    let mut child = match Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(&path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let mut o = stdout_lock.lock().unwrap();
            let _ = writeln!(&mut *o, "[{}] spawn failed: {}", p.name, e);
            any_fail.store(true, Ordering::SeqCst);
            return 127;
        }
    };
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let name = p.name.clone();
    let lock1 = Arc::clone(stdout_lock);
    let lock2 = Arc::clone(stdout_lock);
    let name1 = name.clone();
    let name2 = name.clone();
    let t1 = std::thread::spawn(move || stream_lines(out, &name1, &lock1));
    let t2 = std::thread::spawn(move || stream_lines(err, &name2, &lock2));
    let status = child.wait().ok();
    let _ = t1.join();
    let _ = t2.join();
    let code = status
        .and_then(|s| s.code())
        .unwrap_or(1);
    if code != 0 {
        any_fail.store(true, Ordering::SeqCst);
    }
    let _ = name;
    code
}

fn stream_lines<R: Read>(r: R, name: &str, lock: &Arc<Mutex<std::io::Stdout>>) {
    let reader = BufReader::new(r);
    for line in reader.lines().flatten() {
        let mut o = lock.lock().unwrap();
        let _ = writeln!(&mut *o, "[{}] {}", name, line);
    }
}
