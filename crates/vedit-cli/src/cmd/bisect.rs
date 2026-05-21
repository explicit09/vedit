use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use std::process::Command;
use vedit_core::bisect::{BisectSession, BisectVerdict};
use vedit_core::object;
use vedit_core::repo::Repo;

pub fn start(good: &str, bad: &str) -> Result<()> {
    let repo = discover_repo()?;
    let session = BisectSession::start(&repo, good, bad)?;
    write_session(&repo, &session)?;
    print_session(&session);
    Ok(())
}

pub fn mark(verdict: BisectVerdict) -> Result<()> {
    let repo = discover_repo()?;
    let session = read_session(&repo)?;
    let session = session.record(&repo, verdict)?;
    if session.first_bad.is_some() {
        remove_session(&repo)?;
    } else {
        write_session(&repo, &session)?;
    }
    print_session(&session);
    Ok(())
}

pub fn reset() -> Result<()> {
    let repo = discover_repo()?;
    remove_session(&repo)?;
    println!("Cleared bisect state.");
    Ok(())
}

pub fn run(good: &str, bad: &str, predicate: &[String]) -> Result<()> {
    if predicate.is_empty() {
        bail!("predicate command is required");
    }
    let repo = discover_repo()?;
    let mut session = BisectSession::start(&repo, good, bad)?;

    while let Some(candidate) = session.current.clone() {
        let verdict = run_predicate(predicate, &candidate)?;
        println!("{} is {:?}", short(&candidate), verdict);
        session = session.record(&repo, verdict)?;
    }

    print_session(&session);
    Ok(())
}

fn discover_repo() -> Result<Repo> {
    let cwd = std::env::current_dir()?;
    Repo::discover(&cwd)
}

fn run_predicate(predicate: &[String], candidate: &str) -> Result<BisectVerdict> {
    let mut command = Command::new(&predicate[0]);
    command.args(&predicate[1..]);
    command.env("VEDIT_BISECT_COMMIT", candidate);
    let status = command
        .status()
        .with_context(|| format!("running predicate command {:?}", predicate))?;
    if status.success() {
        Ok(BisectVerdict::Good)
    } else {
        Ok(BisectVerdict::Bad)
    }
}

fn read_session(repo: &Repo) -> Result<BisectSession> {
    let path = session_path(repo);
    let bytes = std::fs::read(&path).with_context(|| {
        format!(
            "reading {}; run `vedit bisect start --good <ref> --bad <ref>` first",
            path.display()
        )
    })?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

fn write_session(repo: &Repo, session: &BisectSession) -> Result<()> {
    let path = session_path(repo);
    let bytes = serde_json::to_vec_pretty(session)?;
    std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))
}

fn remove_session(repo: &Repo) -> Result<()> {
    let path = session_path(repo);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("removing {}", path.display())),
    }
}

fn session_path(repo: &Repo) -> PathBuf {
    repo.root.join("BISECT")
}

fn print_session(session: &BisectSession) {
    if let Some(first_bad) = &session.first_bad {
        println!("First bad commit: {}", short(first_bad));
        return;
    }
    if let Some(current) = &session.current {
        println!("Bisecting: test {}", short(current));
        println!("Then run `vedit bisect good` or `vedit bisect bad`.");
        println!(
            "Remaining candidate commits after this: {}",
            session.remaining
        );
    }
}

fn short(hash: &str) -> String {
    let body = hash.strip_prefix(object::HASH_PREFIX).unwrap_or(hash);
    body.chars().take(7).collect()
}
