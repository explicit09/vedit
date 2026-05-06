use anyhow::Result;
use vedit_core::object;
use vedit_core::repo::Repo;

pub fn run() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let repo = Repo::discover(&cwd)?;
    let current = repo.current_branch()?;
    let list = repo.list_branches()?;
    if list.is_empty() {
        println!("No branches yet. (Make a commit on `main` first.)");
        return Ok(());
    }
    for (name, target) in list {
        let mark = if Some(&name) == current.as_ref() {
            "*"
        } else {
            " "
        };
        println!("{mark} {name}  {}", short(&target));
    }
    Ok(())
}

fn short(hash: &str) -> String {
    let body = hash.strip_prefix(object::HASH_PREFIX).unwrap_or(hash);
    body.chars().take(7).collect()
}
