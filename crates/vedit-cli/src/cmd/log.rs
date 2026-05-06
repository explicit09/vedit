use anyhow::Result;
use vedit_core::object;
use vedit_core::repo::{HeadState, Repo};

pub fn run(refstr: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let repo = Repo::discover(&cwd)?;
    let start = if refstr == "HEAD" { None } else { Some(refstr) };
    let entries = repo.log(start)?;

    if entries.is_empty() {
        println!("No commits yet. Run `vedit commit <timeline.otio> -m \"...\"` to start.");
        return Ok(());
    }

    let head_target = match repo.head()? {
        HeadState::Branch(name) => Some(("HEAD".to_string(), name)),
        HeadState::Detached(_) => None,
    };

    for (hash, commit) in entries.iter() {
        let mut tags: Vec<String> = Vec::new();
        if let Some((head_label, branch_name)) = &head_target
            && let Ok(Some(target)) = repo.branch_target(branch_name)
                && &target == hash {
                    tags.push(format!("{head_label} -> {branch_name}"));
                }
        let tag_str = if tags.is_empty() {
            String::new()
        } else {
            format!("  ({})", tags.join(", "))
        };
        println!(
            "{}  {}{}",
            short(hash),
            commit.message.lines().next().unwrap_or(""),
            tag_str
        );
    }
    Ok(())
}

fn short(hash: &str) -> String {
    let body = hash.strip_prefix(object::HASH_PREFIX).unwrap_or(hash);
    body.chars().take(7).collect()
}
