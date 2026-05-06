//! `vedit watch` — auto-commit on file change.
//!
//! Polls the timeline file's mtime + size every `interval` ms. When the
//! fingerprint changes, waits for it to settle (no changes for `settle`
//! ms) so we don't commit a half-written file. After settling, we parse
//! the OTIO and diff it against HEAD; if there are no semantic changes
//! we skip silently. Otherwise we commit with an auto-generated message.
//!
//! This is the editor-agnostic "set it and forget it" mode: configure
//! Resolve / Premiere / FCP to export OTIO to one path on a hotkey, run
//! `vedit watch` once, and you get a commit per export with a useful
//! message.

use crate::author;
use crate::cmd::commit::derive_message;
use anyhow::{Context, Result};
use std::path::Path;
use std::time::{Duration, SystemTime};
use vedit_core::object;
use vedit_core::otio;
use vedit_core::repo::{HeadState, Repo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    mtime_ms: u128,
    size: u64,
}

impl Fingerprint {
    fn read(path: &Path) -> Result<Option<Self>> {
        match std::fs::metadata(path) {
            Ok(md) => {
                let size = md.len();
                let mtime_ms = md
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                Ok(Some(Self { mtime_ms, size }))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WatchOptions {
    pub interval: Duration,
    pub settle: Duration,
    pub message_prefix: Option<String>,
    pub once: bool,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            interval: Duration::from_millis(500),
            settle: Duration::from_millis(200),
            message_prefix: None,
            once: false,
        }
    }
}

pub fn run(timeline_path: &Path, options: WatchOptions) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let repo = Repo::discover(&cwd)?;

    println!(
        "Watching {} (interval {}ms, settle {}ms){}",
        timeline_path.display(),
        options.interval.as_millis(),
        options.settle.as_millis(),
        if options.once { ", one-shot" } else { "" }
    );

    // Track the last fingerprint we have already committed (or that was
    // present at startup). On every poll, if the file changed and then
    // settled, we attempt a commit.
    let mut last_committed = Fingerprint::read(timeline_path)?;

    loop {
        std::thread::sleep(options.interval);

        let cur = match Fingerprint::read(timeline_path)? {
            Some(fp) => fp,
            None => continue, // file disappeared briefly
        };

        if Some(cur) == last_committed {
            continue; // no change since last commit
        }

        // Wait for the fingerprint to stop changing for `settle` ms.
        if !wait_for_settle(timeline_path, cur, options.settle, options.interval)? {
            continue;
        }

        match attempt_commit(&repo, timeline_path, options.message_prefix.as_deref()) {
            Ok(Some(record)) => {
                println!(
                    "[{} {}] {}",
                    record.branch_label,
                    short(&record.commit_hash),
                    record.message
                );
                last_committed = Fingerprint::read(timeline_path)?;
            }
            Ok(None) => {
                // File was unparseable or empty; don't update fingerprint
                // so we'll retry on the next change.
            }
            Err(e) => {
                eprintln!("watch: commit failed: {e:#}");
            }
        }

        if options.once {
            break;
        }
    }

    Ok(())
}

struct CommitRecord {
    commit_hash: String,
    branch_label: String,
    message: String,
}

fn attempt_commit(
    repo: &Repo,
    timeline_path: &Path,
    message_prefix: Option<&str>,
) -> Result<Option<CommitRecord>> {
    let bytes = std::fs::read(timeline_path)
        .with_context(|| format!("reading {}", timeline_path.display()))?;
    if bytes.is_empty() {
        return Ok(None);
    }
    let timeline_value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return Ok(None), // mid-write, try again next time
    };

    // Sanity-check parse so we don't commit garbage.
    if otio::parse_timeline(&timeline_value).is_err() {
        return Ok(None);
    }

    let auto = derive_message(repo, &timeline_value)?;
    let message = match message_prefix {
        Some(p) if !p.is_empty() => format!("{p} {auto}"),
        _ => auto,
    };

    let timeline_hash = repo.write_timeline(&timeline_value)?;
    let author = author::resolve()?;
    let commit_hash = repo.commit(&timeline_hash, author, &message)?;

    Ok(Some(CommitRecord {
        commit_hash,
        branch_label: current_branch_label(repo),
        message,
    }))
}

/// Wait for the file's fingerprint to stop changing for at least `settle`.
/// Returns `Ok(true)` once it has settled, `Ok(false)` if the file goes
/// missing during the wait.
fn wait_for_settle(
    path: &Path,
    initial: Fingerprint,
    settle: Duration,
    interval: Duration,
) -> Result<bool> {
    let mut last = initial;
    let mut stable_for = Duration::ZERO;
    loop {
        std::thread::sleep(interval);
        let cur = match Fingerprint::read(path)? {
            Some(fp) => fp,
            None => return Ok(false),
        };
        if cur == last {
            stable_for += interval;
            if stable_for >= settle {
                return Ok(true);
            }
        } else {
            last = cur;
            stable_for = Duration::ZERO;
        }
    }
}

fn current_branch_label(repo: &Repo) -> String {
    match repo.head() {
        Ok(HeadState::Branch(name)) => match repo.branch_target(&name) {
            Ok(Some(_)) => name,
            _ => format!("{name} (root)"),
        },
        _ => "detached".to_string(),
    }
}

fn short(hash: &str) -> String {
    let body = hash.strip_prefix(object::HASH_PREFIX).unwrap_or(hash);
    body.chars().take(7).collect()
}
