use anyhow::Result;
use std::path::Path;
use vedit_core::object;
use vedit_core::repo::Repo;

/// `checkout` is dual-purpose:
///   - With `-o <path>`: write the timeline at the given ref to that path.
///     This is the "extract a snapshot" mode; the ref can be a branch name,
///     short hash, full hash, or HEAD.
///   - Without `-o`: switch HEAD to the named branch. The ref must be an
///     existing branch. There is no working copy, so nothing is written
///     to disk; the next `vedit show` or `vedit log` reflects the new
///     branch.
pub fn run(refstr: &str, output: Option<&Path>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let repo = Repo::discover(&cwd)?;

    if let Some(out_path) = output {
        let commit_hash = repo.resolve(refstr)?;
        let commit = repo.read_commit(&commit_hash)?;
        let timeline_value = repo.read_timeline(&commit.timeline)?;
        let pretty = serde_json::to_string_pretty(&timeline_value)?;
        std::fs::write(out_path, pretty)?;
        println!(
            "Wrote timeline at {} to {}",
            short(&commit_hash),
            out_path.display()
        );
        return Ok(());
    }

    // Branch-switch mode. The ref must be a branch.
    if repo.branch_target(refstr)?.is_none() {
        anyhow::bail!("{refstr} is not a branch. Use `-o <path>` to write a timeline by hash.");
    }
    repo.switch_branch(refstr)?;
    println!("Switched to branch {refstr}");
    Ok(())
}

fn short(hash: &str) -> String {
    let body = hash.strip_prefix(object::HASH_PREFIX).unwrap_or(hash);
    body.chars().take(7).collect()
}
