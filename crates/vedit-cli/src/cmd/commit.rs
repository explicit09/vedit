use crate::author;
use anyhow::{Context, Result};
use std::path::Path;
use vedit_core::diff::{auto_message, diff};
use vedit_core::model::TrackChild;
use vedit_core::object;
use vedit_core::otio;
use vedit_core::repo::{HeadState, Repo};

/// Snapshot the OTIO file at `timeline_path` as a new commit on the
/// current branch. If `message` is None, generate one from the diff
/// against HEAD's timeline (or a "Initial commit" summary if HEAD has no
/// commit yet).
pub fn run(timeline_path: &Path, message: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let repo = Repo::discover(&cwd)?;

    let bytes = std::fs::read(timeline_path)
        .with_context(|| format!("reading {}", timeline_path.display()))?;
    let timeline_value: serde_json::Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing {} as JSON", timeline_path.display()))?;

    let resolved_message = match message {
        Some(m) => m.to_string(),
        None => derive_message(&repo, &timeline_value)?,
    };

    let timeline_hash = repo.write_timeline(&timeline_value)?;
    let author = author::resolve()?;
    let commit_hash = repo.commit(&timeline_hash, author, &resolved_message)?;

    println!(
        "[{} {}] {}",
        current_branch_label(&repo),
        short(&commit_hash),
        first_line(&resolved_message)
    );
    Ok(())
}

/// Auto-generate a commit message by diffing the new timeline against
/// HEAD's timeline. For the very first commit we summarize tracks/clips
/// instead, since there's no parent to diff against.
pub fn derive_message(repo: &Repo, new_timeline: &serde_json::Value) -> Result<String> {
    let head_hash = match repo.head()? {
        HeadState::Branch(name) => repo.branch_target(&name)?,
        HeadState::Detached(_) => {
            anyhow::bail!("cannot auto-message while HEAD is detached")
        }
    };

    let new_tl = otio::parse_timeline(new_timeline)?;

    let Some(parent_hash) = head_hash else {
        // First commit on a fresh branch — describe what we're storing.
        let mut clip_count = 0usize;
        for track in &new_tl.tracks {
            for child in &track.children {
                if matches!(child, TrackChild::Clip(_)) {
                    clip_count += 1;
                }
            }
        }
        return Ok(format!(
            "Initial commit: {} track(s), {} clip(s)",
            new_tl.tracks.len(),
            clip_count
        ));
    };

    let parent_commit = repo.read_commit(&parent_hash)?;
    let parent_timeline_value = repo.read_timeline(&parent_commit.timeline)?;
    let parent_tl = otio::parse_timeline(&parent_timeline_value)?;
    let changes = diff(&parent_tl, &new_tl);
    Ok(auto_message(&changes))
}

fn current_branch_label(repo: &Repo) -> String {
    match repo.head() {
        Ok(HeadState::Branch(name)) => {
            // Mark "(root)" before the first commit lands.
            match repo.branch_target(&name) {
                Ok(Some(_)) => name,
                _ => format!("{name} (root)"),
            }
        }
        _ => "detached".to_string(),
    }
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or(s)
}

fn short(hash: &str) -> String {
    let body = hash.strip_prefix(object::HASH_PREFIX).unwrap_or(hash);
    body.chars().take(7).collect()
}
