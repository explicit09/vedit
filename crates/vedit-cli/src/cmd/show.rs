use crate::diff::render;
use anyhow::Result;
use vedit_core::diff::diff;
use vedit_core::object;
use vedit_core::otio;
use vedit_core::repo::Repo;

pub fn run(refstr: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let repo = Repo::discover(&cwd)?;

    let hash = repo.resolve(refstr)?;
    let commit = repo.read_commit(&hash)?;

    println!("commit {}", short(&hash));
    println!("Author: {} <{}>", commit.author.name, commit.author.email);
    for co_author in commit.authors.iter().skip(1) {
        println!("Co-author: {} <{}>", co_author.name, co_author.email);
    }
    println!("Date:   {}", commit.timestamp);
    println!();
    for line in commit.message.lines() {
        println!("    {line}");
    }
    println!();

    let after_value = repo.read_timeline(&commit.timeline)?;
    let after = otio::parse_timeline(&after_value)?;

    if let Some(parent_hash) = commit.parents.first() {
        let parent_commit = repo.read_commit(parent_hash)?;
        let before_value = repo.read_timeline(&parent_commit.timeline)?;
        let before = otio::parse_timeline(&before_value)?;
        let changes = diff(&before, &after);
        if changes.is_empty() {
            println!("(no semantic changes from parent)");
        } else {
            for line in render::render(&changes) {
                println!("{line}");
            }
        }
    } else {
        // Initial commit — no parent. Show track + clip count summary.
        let mut clip_count = 0usize;
        for track in &after.tracks {
            for child in &track.children {
                if matches!(child, vedit_core::model::TrackChild::Clip(_)) {
                    clip_count += 1;
                }
            }
        }
        println!(
            "Initial commit. {} track(s), {} clip(s).",
            after.tracks.len(),
            clip_count
        );
    }
    Ok(())
}

fn short(hash: &str) -> String {
    let body = hash.strip_prefix(object::HASH_PREFIX).unwrap_or(hash);
    body.chars().take(7).collect()
}
