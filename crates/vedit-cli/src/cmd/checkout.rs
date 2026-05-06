use anyhow::Result;
use std::path::{Path, PathBuf};
use vedit_core::object;
use vedit_core::repo::Repo;

pub fn run(refstr: &str, output: Option<&Path>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let repo = Repo::discover(&cwd)?;

    let commit_hash = repo.resolve(refstr)?;
    let commit = repo.read_commit(&commit_hash)?;
    let timeline_value = repo.read_timeline(&commit.timeline)?;

    let out_path: PathBuf = match output {
        Some(p) => p.to_path_buf(),
        None => {
            let body = commit_hash
                .strip_prefix(object::HASH_PREFIX)
                .unwrap_or(&commit_hash);
            let stem: String = body.chars().take(7).collect();
            cwd.join(format!("{stem}.otio"))
        }
    };

    let pretty = serde_json::to_string_pretty(&timeline_value)?;
    std::fs::write(&out_path, pretty)?;
    println!(
        "Wrote timeline at {} to {}",
        body_short(&commit_hash),
        out_path.display()
    );
    Ok(())
}

fn body_short(hash: &str) -> String {
    let body = hash.strip_prefix(object::HASH_PREFIX).unwrap_or(hash);
    body.chars().take(7).collect()
}
