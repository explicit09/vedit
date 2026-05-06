use crate::author;
use anyhow::{Context, Result};
use std::path::Path;
use vedit_core::object;
use vedit_core::repo::Repo;

pub fn run(timeline_path: &Path, message: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let repo = Repo::discover(&cwd)?;

    let bytes = std::fs::read(timeline_path)
        .with_context(|| format!("reading {}", timeline_path.display()))?;
    let timeline_value: serde_json::Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing {} as JSON", timeline_path.display()))?;

    let timeline_hash = repo.write_timeline(&timeline_value)?;
    let author = author::resolve()?;
    let commit_hash = repo.commit(&timeline_hash, author, message)?;

    println!(
        "[{} {}] {}",
        current_branch_label(&repo),
        short(&commit_hash),
        first_line(message)
    );
    Ok(())
}

fn current_branch_label(repo: &Repo) -> String {
    match repo.head() {
        Ok(vedit_core::repo::HeadState::Branch(name)) => {
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
