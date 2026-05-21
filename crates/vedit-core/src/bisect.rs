use crate::repo::Repo;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BisectVerdict {
    Good,
    Bad,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BisectSession {
    pub good: String,
    pub bad: String,
    pub current: Option<String>,
    pub first_bad: Option<String>,
    pub remaining: usize,
}

impl BisectSession {
    pub fn start(repo: &Repo, good: &str, bad: &str) -> Result<Self> {
        let good = repo.resolve(good)?;
        let bad = repo.resolve(bad)?;
        session_for_bounds(repo, good, bad)
    }

    pub fn record(self, repo: &Repo, verdict: BisectVerdict) -> Result<Self> {
        let Some(current) = self.current else {
            bail!("bisect is already complete");
        };

        let (good, bad) = match verdict {
            BisectVerdict::Good => (current, self.bad),
            BisectVerdict::Bad => (self.good, current),
        };
        session_for_bounds(repo, good, bad)
    }
}

fn session_for_bounds(repo: &Repo, good: String, bad: String) -> Result<BisectSession> {
    if good == bad {
        bail!("good and bad refs resolve to the same commit");
    }

    let path = first_parent_path(repo, &bad, &good)?;
    if path.len() < 2 {
        bail!("{good} is not an ancestor of {bad}");
    }
    if path.last() != Some(&good) {
        bail!("{good} is not an ancestor of {bad}");
    }

    if path.len() == 2 {
        return Ok(BisectSession {
            good,
            bad: bad.clone(),
            current: None,
            first_bad: Some(bad),
            remaining: 0,
        });
    }

    let candidate_index = path.len() / 2;
    Ok(BisectSession {
        good,
        bad,
        current: Some(path[candidate_index].clone()),
        first_bad: None,
        remaining: path.len().saturating_sub(3),
    })
}

fn first_parent_path(repo: &Repo, bad: &str, good: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut cursor = bad.to_string();
    loop {
        out.push(cursor.clone());
        if cursor == good {
            return Ok(out);
        }
        let commit = repo.read_commit(&cursor)?;
        let Some(parent) = commit.parents.first() else {
            return Ok(out);
        };
        cursor = parent.clone();
    }
}
