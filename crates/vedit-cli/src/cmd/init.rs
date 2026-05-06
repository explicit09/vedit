use anyhow::Result;
use vedit_core::repo::Repo;

pub fn run() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let repo = Repo::init(&cwd)?;
    println!(
        "Initialized empty vedit repository in {}",
        repo.root.display()
    );
    Ok(())
}
