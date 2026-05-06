use anyhow::Result;
use vedit_core::object;
use vedit_core::repo::Repo;

pub fn run(name: &str, delete: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let repo = Repo::discover(&cwd)?;
    if delete {
        repo.delete_branch(name)?;
        println!("Deleted branch {name}");
    } else {
        let target = repo.create_branch(name, "HEAD")?;
        println!("Created branch {name} at {}", short(&target));
    }
    Ok(())
}

fn short(hash: &str) -> String {
    let body = hash.strip_prefix(object::HASH_PREFIX).unwrap_or(hash);
    body.chars().take(7).collect()
}
