//! Repository operations: init, ref resolution, commit, log walking.

use crate::commit::{Author, Commit};
use crate::object;
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const VEDIT_DIR: &str = ".vedit";
pub const DEFAULT_BRANCH: &str = "main";
const HEAD_FILE: &str = "HEAD";

/// A vedit repository rooted at some directory.
#[derive(Debug, Clone)]
pub struct Repo {
    pub root: PathBuf,
}

impl Repo {
    /// Create a new empty repo at `<workdir>/.vedit/`.
    ///
    /// Returns an error if the directory already exists. Idempotency is
    /// the caller's call to make — vedit doesn't auto-reinitialize.
    pub fn init(workdir: &Path) -> Result<Self> {
        let root = workdir.join(VEDIT_DIR);
        if root.exists() {
            bail!("vedit repository already exists at {}", root.display());
        }
        std::fs::create_dir_all(root.join("objects"))?;
        std::fs::create_dir_all(root.join("refs").join("heads"))?;

        // HEAD points at refs/heads/main symbolically. The branch ref does
        // not exist yet — it will be written by the first commit.
        let head_contents = format!("ref: refs/heads/{DEFAULT_BRANCH}\n");
        std::fs::write(root.join(HEAD_FILE), head_contents)?;

        Ok(Self { root })
    }

    /// Open an existing repo. Walks up from `start` looking for a `.vedit`
    /// directory.
    pub fn discover(start: &Path) -> Result<Self> {
        let mut cur = if start.is_absolute() {
            start.to_path_buf()
        } else {
            std::env::current_dir()?.join(start)
        };
        loop {
            let candidate = cur.join(VEDIT_DIR);
            if candidate.is_dir() {
                return Ok(Self { root: candidate });
            }
            if !cur.pop() {
                bail!("not in a vedit repository (run `vedit init` first)");
            }
        }
    }

    fn objects_dir(&self) -> PathBuf {
        self.root.join("objects")
    }

    fn head_path(&self) -> PathBuf {
        self.root.join(HEAD_FILE)
    }

    fn branch_path(&self, name: &str) -> PathBuf {
        self.root.join("refs").join("heads").join(name)
    }

    /// Read HEAD. Returns either a branch name (symbolic) or a commit hash
    /// (detached).
    pub fn head(&self) -> Result<HeadState> {
        let raw = std::fs::read_to_string(self.head_path())
            .with_context(|| format!("reading {}", self.head_path().display()))?;
        let trimmed = raw.trim();
        if let Some(branch_path) = trimmed.strip_prefix("ref: refs/heads/") {
            Ok(HeadState::Branch(branch_path.to_string()))
        } else if trimmed.starts_with(object::HASH_PREFIX) {
            Ok(HeadState::Detached(trimmed.to_string()))
        } else {
            bail!("HEAD has unexpected contents: {trimmed:?}")
        }
    }

    /// Read the commit hash that a branch currently points to. Returns None
    /// if the branch ref does not exist (e.g. `main` before the first
    /// commit).
    pub fn branch_target(&self, name: &str) -> Result<Option<String>> {
        let path = self.branch_path(name);
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let trimmed = raw.trim();
        if !trimmed.starts_with(object::HASH_PREFIX) {
            bail!("branch {name} has invalid contents: {trimmed:?}");
        }
        Ok(Some(trimmed.to_string()))
    }

    /// List existing branches (sorted alphabetically). Each entry has the
    /// branch name and the commit it points at.
    pub fn list_branches(&self) -> Result<Vec<(String, String)>> {
        let dir = self.root.join("refs").join("heads");
        let mut out: Vec<(String, String)> = Vec::new();
        if !dir.exists() {
            return Ok(out);
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            // We only support flat names in v0.3 — no nested
            // refs/heads/<a>/<b>. Skip subdirs for now.
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(target) = self.branch_target(&name)? {
                out.push((name, target));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    /// Return the current branch name if HEAD is symbolic, else None.
    pub fn current_branch(&self) -> Result<Option<String>> {
        match self.head()? {
            HeadState::Branch(name) => Ok(Some(name)),
            HeadState::Detached(_) => Ok(None),
        }
    }

    /// Create a new branch pointing at the resolved `start_ref`. Errors if
    /// the branch already exists or the name is invalid.
    pub fn create_branch(&self, name: &str, start_ref: &str) -> Result<String> {
        validate_branch_name(name)?;
        let path = self.branch_path(name);
        if path.exists() {
            bail!("branch {name} already exists");
        }
        let target = self.resolve(start_ref)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, format!("{target}\n"))?;
        Ok(target)
    }

    /// Delete a branch. Refuses to delete the branch HEAD currently points
    /// at, since there'd be no way to keep working.
    pub fn delete_branch(&self, name: &str) -> Result<()> {
        if self.current_branch()?.as_deref() == Some(name) {
            bail!("cannot delete the current branch ({name}); switch first");
        }
        let path = self.branch_path(name);
        if !path.exists() {
            bail!("branch {name} does not exist");
        }
        std::fs::remove_file(&path)
            .with_context(|| format!("removing {}", path.display()))?;
        Ok(())
    }

    /// Repoint HEAD at an existing branch. Use this for `vedit checkout
    /// <branch>`. Errors if the branch does not exist.
    pub fn switch_branch(&self, name: &str) -> Result<()> {
        let path = self.branch_path(name);
        if !path.exists() {
            bail!("branch {name} does not exist");
        }
        std::fs::write(self.head_path(), format!("ref: refs/heads/{name}\n"))?;
        Ok(())
    }

    /// Resolve a user-supplied ref string to a full commit hash.
    /// Accepts: "HEAD", branch name, full hash (with or without prefix),
    /// short hash (>=4 hex chars).
    pub fn resolve(&self, refstr: &str) -> Result<String> {
        if refstr == "HEAD" {
            return self.resolve_head();
        }

        // Try branch name first.
        if let Some(hash) = self.branch_target(refstr)? {
            return Ok(hash);
        }

        // Full or partial hash.
        let candidate = if refstr.starts_with(object::HASH_PREFIX) {
            refstr.to_string()
        } else if refstr.chars().all(|c| c.is_ascii_hexdigit()) && refstr.len() >= 4 {
            // Short hash; resolve by scanning objects.
            return self.resolve_short_hash(refstr);
        } else {
            bail!("could not resolve ref: {refstr}");
        };

        // Verify the object actually exists.
        if !object::object_path(&self.objects_dir(), &candidate)?.exists() {
            bail!("ref {refstr} resolves to nonexistent object");
        }
        Ok(candidate)
    }

    fn resolve_head(&self) -> Result<String> {
        match self.head()? {
            HeadState::Branch(name) => self
                .branch_target(&name)?
                .ok_or_else(|| anyhow!("HEAD points at {name} but branch has no commits yet")),
            HeadState::Detached(h) => Ok(h),
        }
    }

    fn resolve_short_hash(&self, prefix: &str) -> Result<String> {
        let objects = self.objects_dir();
        if prefix.len() < 2 {
            bail!("short hash too short: {prefix}");
        }
        let (head, tail) = prefix.split_at(2);
        let dir = objects.join(head);
        if !dir.exists() {
            bail!("could not resolve short hash {prefix}");
        }
        let mut matches: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let fname = entry.file_name();
            let s = fname.to_string_lossy();
            if s.starts_with(tail) {
                matches.push(format!("{}{head}{s}", object::HASH_PREFIX));
            }
        }
        match matches.len() {
            0 => bail!("could not resolve short hash {prefix}"),
            1 => Ok(matches.into_iter().next().unwrap()),
            n => bail!("short hash {prefix} is ambiguous ({n} matches)"),
        }
    }

    /// Write a timeline JSON value as an object and return its hash.
    pub fn write_timeline(&self, timeline_value: &Value) -> Result<String> {
        object::write(&self.objects_dir(), timeline_value)
    }

    /// Read a timeline by hash.
    pub fn read_timeline(&self, hash: &str) -> Result<Value> {
        object::read(&self.objects_dir(), hash)
    }

    /// Read a commit by hash.
    pub fn read_commit(&self, hash: &str) -> Result<Commit> {
        let v = object::read(&self.objects_dir(), hash)?;
        let commit: Commit = serde_json::from_value(v)
            .with_context(|| format!("parsing commit {hash}"))?;
        if commit.schema != Commit::SCHEMA {
            bail!(
                "commit {hash} has unsupported schema {:?} (expected {})",
                commit.schema,
                Commit::SCHEMA
            );
        }
        Ok(commit)
    }

    /// Create a commit object pointing at `timeline_hash`, parented at
    /// the current HEAD (if any), and update HEAD's branch to point at
    /// it. Returns the new commit's hash.
    pub fn commit(
        &self,
        timeline_hash: &str,
        author: Author,
        message: &str,
    ) -> Result<String> {
        let parent = match self.head()? {
            HeadState::Branch(name) => self.branch_target(&name)?,
            HeadState::Detached(_) => {
                bail!("cannot commit while HEAD is detached")
            }
        };
        let parents = parent.into_iter().collect();

        let commit = Commit::new(
            timeline_hash.to_string(),
            parents,
            author,
            Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            message.to_string(),
        );
        let commit_value = serde_json::to_value(&commit)?;
        let commit_hash = self.write_timeline(&commit_value)?;
        // ^ write_timeline writes any JSON object, not just timelines.

        // Advance the current branch.
        if let HeadState::Branch(name) = self.head()? {
            let path = self.branch_path(&name);
            if let Some(parent_dir) = path.parent() {
                std::fs::create_dir_all(parent_dir)?;
            }
            std::fs::write(&path, format!("{commit_hash}\n"))?;
        }

        Ok(commit_hash)
    }

    /// Walk commits starting at HEAD (or the given ref), following the first
    /// parent each step, until a commit with no parents is reached. Returns
    /// the list newest-first.
    pub fn log(&self, start: Option<&str>) -> Result<Vec<(String, Commit)>> {
        let mut out: Vec<(String, Commit)> = Vec::new();
        let head_hash = match start {
            Some(r) => self.resolve(r)?,
            None => match self.resolve_head() {
                Ok(h) => h,
                Err(_) => return Ok(out), // fresh repo
            },
        };
        let mut cursor = Some(head_hash);
        while let Some(h) = cursor {
            let commit = self.read_commit(&h)?;
            let next = commit.parents.first().cloned();
            out.push((h, commit));
            cursor = next;
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HeadState {
    /// HEAD points at refs/heads/<name>.
    Branch(String),
    /// HEAD is detached at a specific commit.
    Detached(String),
}

/// Branch names are restricted to ASCII alphanumerics, dash, underscore,
/// and slash. Empty names, names with whitespace, leading dot, or `..`
/// segments are rejected. Slashes are allowed but the v0.3 listing only
/// covers flat names.
fn validate_branch_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("branch name is empty");
    }
    if name.starts_with('.') || name.starts_with('/') || name.ends_with('/') {
        bail!("invalid branch name: {name}");
    }
    if name.contains("..") || name.contains("//") {
        bail!("invalid branch name: {name}");
    }
    for ch in name.chars() {
        let ok = ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '/' | '.');
        if !ok || ch.is_whitespace() {
            bail!("invalid branch name: {name}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn fake_author() -> Author {
        Author {
            name: "tester".to_string(),
            email: "test@example.com".to_string(),
        }
    }

    #[test]
    fn init_creates_layout() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path()).unwrap();
        assert!(repo.root.join("HEAD").exists());
        assert!(repo.root.join("objects").is_dir());
        assert!(repo.root.join("refs/heads").is_dir());
    }

    #[test]
    fn init_twice_errors() {
        let dir = tempdir().unwrap();
        Repo::init(dir.path()).unwrap();
        assert!(Repo::init(dir.path()).is_err());
    }

    #[test]
    fn discover_walks_up() {
        let dir = tempdir().unwrap();
        Repo::init(dir.path()).unwrap();
        let nested = dir.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        let repo = Repo::discover(&nested).unwrap();
        assert_eq!(repo.root, dir.path().join(".vedit"));
    }

    #[test]
    fn commit_advances_main() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path()).unwrap();
        let timeline = json!({ "OTIO_SCHEMA": "Timeline.1", "name": "t" });
        let th = repo.write_timeline(&timeline).unwrap();
        let ch = repo.commit(&th, fake_author(), "first").unwrap();

        assert_eq!(
            repo.branch_target("main").unwrap().as_deref(),
            Some(ch.as_str())
        );

        let commit = repo.read_commit(&ch).unwrap();
        assert_eq!(commit.timeline, th);
        assert!(commit.parents.is_empty());
        assert_eq!(commit.message, "first");
    }

    #[test]
    fn second_commit_has_first_as_parent() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path()).unwrap();
        let t1 = repo
            .write_timeline(&json!({ "OTIO_SCHEMA": "Timeline.1", "name": "v1" }))
            .unwrap();
        let c1 = repo.commit(&t1, fake_author(), "v1").unwrap();
        let t2 = repo
            .write_timeline(&json!({ "OTIO_SCHEMA": "Timeline.1", "name": "v2" }))
            .unwrap();
        let c2 = repo.commit(&t2, fake_author(), "v2").unwrap();
        let commit2 = repo.read_commit(&c2).unwrap();
        assert_eq!(commit2.parents, vec![c1]);
    }

    #[test]
    fn log_walks_history() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path()).unwrap();
        let mut hashes = Vec::new();
        for i in 0..3 {
            let t = repo
                .write_timeline(&json!({
                    "OTIO_SCHEMA": "Timeline.1",
                    "name": format!("v{i}")
                }))
                .unwrap();
            hashes.push(repo.commit(&t, fake_author(), &format!("v{i}")).unwrap());
        }
        let log = repo.log(None).unwrap();
        assert_eq!(log.len(), 3);
        // Newest first.
        assert_eq!(log[0].0, hashes[2]);
        assert_eq!(log[2].0, hashes[0]);
    }

    #[test]
    fn resolve_full_short_branch_and_head() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path()).unwrap();
        let t = repo
            .write_timeline(&json!({ "OTIO_SCHEMA": "Timeline.1", "name": "v" }))
            .unwrap();
        let c = repo.commit(&t, fake_author(), "v").unwrap();

        // Full hash.
        assert_eq!(repo.resolve(&c).unwrap(), c);
        // HEAD.
        assert_eq!(repo.resolve("HEAD").unwrap(), c);
        // Branch.
        assert_eq!(repo.resolve("main").unwrap(), c);
        // Short hash (10 chars after prefix).
        let body = c.strip_prefix(object::HASH_PREFIX).unwrap();
        let short = &body[..10];
        assert_eq!(repo.resolve(short).unwrap(), c);
    }

    #[test]
    fn create_branch_at_head_then_switch() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path()).unwrap();
        let t = repo
            .write_timeline(&json!({ "OTIO_SCHEMA": "Timeline.1", "name": "v" }))
            .unwrap();
        let c = repo.commit(&t, fake_author(), "v").unwrap();

        repo.create_branch("alt", "HEAD").unwrap();
        assert_eq!(repo.branch_target("alt").unwrap().as_deref(), Some(c.as_str()));

        // List shows both.
        let list = repo.list_branches().unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|(n, _)| n == "main"));
        assert!(list.iter().any(|(n, _)| n == "alt"));

        // Switch and verify.
        assert_eq!(repo.current_branch().unwrap().as_deref(), Some("main"));
        repo.switch_branch("alt").unwrap();
        assert_eq!(repo.current_branch().unwrap().as_deref(), Some("alt"));
    }

    #[test]
    fn cannot_create_existing_branch() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path()).unwrap();
        let t = repo
            .write_timeline(&json!({ "OTIO_SCHEMA": "Timeline.1", "name": "v" }))
            .unwrap();
        repo.commit(&t, fake_author(), "v").unwrap();
        repo.create_branch("alt", "HEAD").unwrap();
        assert!(repo.create_branch("alt", "HEAD").is_err());
    }

    #[test]
    fn cannot_delete_current_branch() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path()).unwrap();
        let t = repo
            .write_timeline(&json!({ "OTIO_SCHEMA": "Timeline.1", "name": "v" }))
            .unwrap();
        repo.commit(&t, fake_author(), "v").unwrap();
        // main is current; deleting it must error.
        assert!(repo.delete_branch("main").is_err());

        // alt is not current; deleting it works.
        repo.create_branch("alt", "HEAD").unwrap();
        repo.delete_branch("alt").unwrap();
        assert!(repo.branch_target("alt").unwrap().is_none());
    }

    #[test]
    fn switch_then_commit_advances_only_that_branch() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path()).unwrap();
        let t1 = repo
            .write_timeline(&json!({ "OTIO_SCHEMA": "Timeline.1", "name": "v1" }))
            .unwrap();
        let main_c1 = repo.commit(&t1, fake_author(), "v1 on main").unwrap();

        repo.create_branch("alt", "HEAD").unwrap();
        repo.switch_branch("alt").unwrap();

        let t2 = repo
            .write_timeline(&json!({ "OTIO_SCHEMA": "Timeline.1", "name": "v2" }))
            .unwrap();
        let alt_c2 = repo.commit(&t2, fake_author(), "v2 on alt").unwrap();

        // main still at v1; alt now at v2.
        assert_eq!(repo.branch_target("main").unwrap().as_deref(), Some(main_c1.as_str()));
        assert_eq!(repo.branch_target("alt").unwrap().as_deref(), Some(alt_c2.as_str()));

        // Logs differ.
        let main_log = repo.log(Some("main")).unwrap();
        let alt_log = repo.log(Some("alt")).unwrap();
        assert_eq!(main_log.len(), 1);
        assert_eq!(alt_log.len(), 2);
    }

    #[test]
    fn invalid_branch_names_rejected() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path()).unwrap();
        let t = repo
            .write_timeline(&json!({ "OTIO_SCHEMA": "Timeline.1", "name": "v" }))
            .unwrap();
        repo.commit(&t, fake_author(), "v").unwrap();
        for bad in ["", ".hidden", "with space", "../escape", "trailing/", "//double"] {
            assert!(
                repo.create_branch(bad, "HEAD").is_err(),
                "should reject {bad:?}"
            );
        }
    }
}
