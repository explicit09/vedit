//! Repository operations: init, ref resolution, commit, log walking.

use crate::atomic;
use crate::commit::{Author, Commit};
use crate::merge::{
    ChangedClipIdMergeClean, ChangedClipIdMergeConflict, ChangedClipIdMergeOutcome,
    changed_clip_ids, merge_non_overlapping_changed_clip_ids,
};
use crate::model::{
    Clip, Effect, Gap, RationalTime, TimeRange, Timeline, Track, TrackChild, TrackKind,
};
use crate::object;
use crate::otio;
use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const VEDIT_DIR: &str = ".vedit";
pub const DEFAULT_BRANCH: &str = "main";
const HEAD_FILE: &str = "HEAD";
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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
        write_text_atomic(&root.join(HEAD_FILE), &head_contents)?;

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
            if validate_branch_name(&name).is_err() {
                continue;
            }
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
        write_text_atomic(&path, &format!("{target}\n"))?;
        Ok(target)
    }

    /// Atomically repoint an existing branch at a resolved commit.
    pub fn set_branch_target(&self, name: &str, target: &str) -> Result<()> {
        validate_branch_name(name)?;
        let path = self.branch_path(name);
        if !path.exists() {
            bail!("branch {name} does not exist");
        }
        let target = self.resolve(target)?;
        write_text_atomic(&path, &format!("{target}\n"))
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
        std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        Ok(())
    }

    /// Repoint HEAD at an existing branch. Use this for `vedit checkout
    /// <branch>`. Errors if the branch does not exist.
    pub fn switch_branch(&self, name: &str) -> Result<()> {
        let path = self.branch_path(name);
        if !path.exists() {
            bail!("branch {name} does not exist");
        }
        write_text_atomic(&self.head_path(), &format!("ref: refs/heads/{name}\n"))?;
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
        let commit: Commit =
            serde_json::from_value(v).with_context(|| format!("parsing commit {hash}"))?;
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
    pub fn commit(&self, timeline_hash: &str, author: Author, message: &str) -> Result<String> {
        self.commit_with_authors(timeline_hash, vec![author], message)
    }

    /// Create a commit with one primary author followed by any co-authors.
    pub fn commit_with_authors(
        &self,
        timeline_hash: &str,
        authors: Vec<Author>,
        message: &str,
    ) -> Result<String> {
        let parent = match self.head()? {
            HeadState::Branch(name) => self.branch_target(&name)?,
            HeadState::Detached(_) => {
                bail!("cannot commit while HEAD is detached")
            }
        };
        let parents = parent.into_iter().collect();

        let commit = Commit::new_with_authors(
            timeline_hash.to_string(),
            parents,
            authors,
            Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            message.to_string(),
        )?;
        let commit_value = serde_json::to_value(&commit)?;
        let commit_hash = self.write_timeline(&commit_value)?;
        // ^ write_timeline writes any JSON object, not just timelines.

        // Advance the current branch.
        if let HeadState::Branch(name) = self.head()? {
            let path = self.branch_path(&name);
            if let Some(parent_dir) = path.parent() {
                std::fs::create_dir_all(parent_dir)?;
            }
            write_text_atomic(&path, &format!("{commit_hash}\n"))?;
        }

        Ok(commit_hash)
    }

    /// Commit a timeline with explicit parents, advancing the current
    /// branch. Used by `vedit merge` to write a two-parent merge commit.
    /// The first entry of `parents` should be the current branch's tip
    /// (i.e. "ours"); subsequent entries are merged-in branches.
    pub fn commit_with_parents(
        &self,
        timeline_hash: &str,
        parents: Vec<String>,
        author: Author,
        message: &str,
    ) -> Result<String> {
        self.commit_with_parents_and_authors(timeline_hash, parents, vec![author], message)
    }

    pub fn commit_with_parents_and_authors(
        &self,
        timeline_hash: &str,
        parents: Vec<String>,
        authors: Vec<Author>,
        message: &str,
    ) -> Result<String> {
        let commit = Commit::new_with_authors(
            timeline_hash.to_string(),
            parents,
            authors,
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            message.to_string(),
        )?;
        let commit_value = serde_json::to_value(&commit)?;
        let commit_hash = self.write_timeline(&commit_value)?;

        if let HeadState::Branch(name) = self.head()? {
            let path = self.branch_path(&name);
            if let Some(parent_dir) = path.parent() {
                std::fs::create_dir_all(parent_dir)?;
            }
            write_text_atomic(&path, &format!("{commit_hash}\n"))?;
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

    /// Find the most recent common ancestor of two commits, or None if
    /// they have no common history. Walks ancestors of `a` first into a
    /// set, then BFS from `b` and returns the first hit.
    ///
    /// This is the simple "merge base" algorithm. It does not handle
    /// criss-cross merges (multiple equally-good bases) — for v0.6 we
    /// pick whichever ancestor of `b` is found first in BFS order, which
    /// is the most recent on `b`'s side.
    pub fn merge_base(&self, a: &str, b: &str) -> Result<Option<String>> {
        let a_hash = self.resolve(a)?;
        let b_hash = self.resolve(b)?;
        if a_hash == b_hash {
            return Ok(Some(a_hash));
        }

        let mut a_ancestors: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut frontier: std::collections::VecDeque<String> = std::collections::VecDeque::new();
        frontier.push_back(a_hash);
        while let Some(h) = frontier.pop_front() {
            if !a_ancestors.insert(h.clone()) {
                continue;
            }
            let commit = self.read_commit(&h)?;
            for p in commit.parents {
                frontier.push_back(p);
            }
        }

        let mut b_seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut frontier: std::collections::VecDeque<String> = std::collections::VecDeque::new();
        frontier.push_back(b_hash);
        while let Some(h) = frontier.pop_front() {
            if !b_seen.insert(h.clone()) {
                continue;
            }
            if a_ancestors.contains(&h) {
                return Ok(Some(h));
            }
            let commit = self.read_commit(&h)?;
            for p in commit.parents {
                frontier.push_back(p);
            }
        }

        Ok(None)
    }

    /// Return the stable clip IDs changed by `refstr` relative to its first
    /// parent. For a root commit, every identifiable clip is reported.
    pub fn changed_clip_ids(&self, refstr: &str) -> Result<Vec<String>> {
        let commit_hash = self.resolve(refstr)?;
        let commit = self.read_commit(&commit_hash)?;
        let after_value = self.read_timeline(&commit.timeline)?;
        let after = otio::parse_timeline(&after_value)?;
        let base = if let Some(parent_hash) = commit.parents.first() {
            Some(self.parse_commit_timeline(parent_hash)?)
        } else {
            None
        };

        Ok(changed_clip_ids(base.as_ref(), &after))
    }

    /// Merge `source_ref` into `target_ref` using changed clip IDs as the
    /// conflict key. Non-overlapping changes are overlaid and committed with
    /// parents `[target, source]`; overlapping clip IDs return a typed
    /// conflict without writing a commit.
    pub fn merge_changed_clip_ids(
        &self,
        source_ref: &str,
        target_ref: &str,
        author: Author,
        message: &str,
    ) -> Result<ChangedClipIdMergeOutcome> {
        let source_hash = self.resolve(source_ref)?;
        let target_hash = self.resolve(target_ref)?;
        let base_hash = self
            .merge_base(&target_hash, &source_hash)?
            .ok_or_else(|| anyhow!("no common ancestor between {target_ref} and {source_ref}"))?;

        let base = self.parse_commit_timeline(&base_hash)?;
        let target = self.parse_commit_timeline(&target_hash)?;
        let source = self.parse_commit_timeline(&source_hash)?;
        let source_changed_clip_ids = changed_clip_ids(Some(&base), &source);
        let target_changed_clip_ids = changed_clip_ids(Some(&base), &target);

        let merged = match merge_non_overlapping_changed_clip_ids(&base, &target, &source) {
            Ok(merged) => merged,
            Err(conflict) => {
                return Ok(ChangedClipIdMergeOutcome::ClipIdConflicts(
                    ChangedClipIdMergeConflict {
                        source_ref: source_ref.to_string(),
                        target_ref: target_ref.to_string(),
                        source_changed_clip_ids,
                        target_changed_clip_ids,
                        overlapping_clip_ids: conflict.overlapping_clip_ids,
                    },
                ));
            }
        };

        let timeline_hash = self.write_timeline(&timeline_to_otio_value(&merged))?;
        let parents = vec![target_hash, source_hash];
        let commit = Commit::new(
            timeline_hash,
            parents.clone(),
            author,
            Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            message.to_string(),
        );
        let commit_hash = self.write_timeline(&serde_json::to_value(&commit)?)?;
        let target_branch = if target_ref == "HEAD" {
            self.current_branch()?
        } else if self.branch_target(target_ref)?.is_some() {
            Some(target_ref.to_string())
        } else {
            None
        };
        if let Some(branch) = target_branch {
            self.set_branch_target(&branch, &commit_hash)?;
        }

        Ok(ChangedClipIdMergeOutcome::Clean(ChangedClipIdMergeClean {
            source_ref: source_ref.to_string(),
            target_ref: target_ref.to_string(),
            commit_hash,
            parents,
            source_changed_clip_ids,
            target_changed_clip_ids,
        }))
    }

    fn parse_commit_timeline(&self, commit_hash: &str) -> Result<Timeline> {
        let commit = self.read_commit(commit_hash)?;
        let value = self.read_timeline(&commit.timeline)?;
        otio::parse_timeline(&value)
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

fn write_text_atomic(path: &Path, contents: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;

    let tmp_path = temp_path_for(path);
    let mut tmp = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .with_context(|| format!("creating temp file {}", tmp_path.display()))?;
    tmp.write_all(contents.as_bytes())
        .with_context(|| format!("writing {}", tmp_path.display()))?;
    tmp.sync_all()
        .with_context(|| format!("syncing {}", tmp_path.display()))?;
    drop(tmp);

    if let Err(e) = atomic::replace_file(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }
    sync_parent_dir(parent);
    Ok(())
}

fn temp_path_for(path: &Path) -> PathBuf {
    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("ref");
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{file_name}.tmp.{}.{}", std::process::id(), n))
}

fn sync_parent_dir(path: &Path) {
    if let Ok(dir) = std::fs::File::open(path) {
        let _ = dir.sync_all();
    }
}

fn timeline_to_otio_value(timeline: &Timeline) -> Value {
    serde_json::json!({
        "OTIO_SCHEMA": "Timeline.1",
        "name": timeline.name,
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "name": "tracks",
            "children": timeline.tracks.iter().map(track_to_otio_value).collect::<Vec<_>>(),
        },
        "metadata": {}
    })
}

fn track_to_otio_value(track: &Track) -> Value {
    serde_json::json!({
        "OTIO_SCHEMA": "Track.1",
        "name": track.name,
        "kind": match track.kind {
            TrackKind::Video => "Video",
            TrackKind::Audio => "Audio",
            TrackKind::Other => "Other",
        },
        "children": track.children.iter().map(track_child_to_otio_value).collect::<Vec<_>>(),
        "metadata": {}
    })
}

fn track_child_to_otio_value(child: &TrackChild) -> Value {
    match child {
        TrackChild::Clip(clip) => clip_to_otio_value(clip),
        TrackChild::Transition(transition) => {
            let offset = transition.duration.map(|duration| RationalTime {
                value: duration.value / 2.0,
                rate: duration.rate,
            });
            serde_json::json!({
                "OTIO_SCHEMA": "Transition.1",
                "name": transition.name,
                "in_offset": offset.map(rational_time_to_otio_value),
                "out_offset": offset.map(rational_time_to_otio_value),
                "metadata": {}
            })
        }
        TrackChild::Gap(gap) => gap_to_otio_value(gap),
    }
}

fn clip_to_otio_value(clip: &Clip) -> Value {
    let mut metadata = serde_json::Map::new();
    if let Some(id) = &clip.clip_id {
        metadata.insert("clip_id".to_string(), Value::String(id.clone()));
    }
    serde_json::json!({
        "OTIO_SCHEMA": "Clip.2",
        "name": clip.name,
        "metadata": Value::Object(metadata),
        "media_reference": clip.media_reference.as_ref().map(|target_url| {
            serde_json::json!({
                "OTIO_SCHEMA": "ExternalReference.1",
                "target_url": target_url,
            })
        }),
        "source_range": clip.source_range.map(time_range_to_otio_value),
        "effects": clip.effects.iter().map(effect_to_otio_value).collect::<Vec<_>>()
    })
}

fn effect_to_otio_value(effect: &Effect) -> Value {
    serde_json::json!({
        "OTIO_SCHEMA": "Effect.1",
        "name": effect.name,
        "effect_name": effect.effect_name,
        "metadata": effect.metadata
    })
}

fn gap_to_otio_value(gap: &Gap) -> Value {
    serde_json::json!({
        "OTIO_SCHEMA": "Gap.1",
        "source_range": gap.duration.map(|duration| TimeRange {
            start_time: RationalTime { value: 0.0, rate: duration.rate },
            duration,
        }).map(time_range_to_otio_value),
        "metadata": {}
    })
}

fn time_range_to_otio_value(range: TimeRange) -> Value {
    serde_json::json!({
        "OTIO_SCHEMA": "TimeRange.1",
        "start_time": rational_time_to_otio_value(range.start_time),
        "duration": rational_time_to_otio_value(range.duration),
    })
}

fn rational_time_to_otio_value(time: RationalTime) -> Value {
    serde_json::json!({
        "OTIO_SCHEMA": "RationalTime.1",
        "value": time.value,
        "rate": time.rate,
    })
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
        assert_eq!(
            repo.branch_target("alt").unwrap().as_deref(),
            Some(c.as_str())
        );

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
        assert_eq!(
            repo.branch_target("main").unwrap().as_deref(),
            Some(main_c1.as_str())
        );
        assert_eq!(
            repo.branch_target("alt").unwrap().as_deref(),
            Some(alt_c2.as_str())
        );

        // Logs differ.
        let main_log = repo.log(Some("main")).unwrap();
        let alt_log = repo.log(Some("alt")).unwrap();
        assert_eq!(main_log.len(), 1);
        assert_eq!(alt_log.len(), 2);
    }

    #[test]
    fn set_branch_target_updates_existing_branch() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path()).unwrap();
        let t = repo
            .write_timeline(&json!({ "OTIO_SCHEMA": "Timeline.1", "name": "v" }))
            .unwrap();
        let c1 = repo.commit(&t, fake_author(), "c1").unwrap();
        let c2 = repo.commit(&t, fake_author(), "c2").unwrap();

        repo.set_branch_target("main", &c1).unwrap();
        assert_eq!(
            repo.branch_target("main").unwrap().as_deref(),
            Some(c1.as_str())
        );

        repo.set_branch_target("main", &c2).unwrap();
        assert_eq!(
            repo.branch_target("main").unwrap().as_deref(),
            Some(c2.as_str())
        );
    }

    #[test]
    fn list_branches_ignores_stale_temp_ref_files() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path()).unwrap();
        let t = repo
            .write_timeline(&json!({ "OTIO_SCHEMA": "Timeline.1", "name": "v" }))
            .unwrap();
        let c = repo.commit(&t, fake_author(), "v").unwrap();

        std::fs::write(
            repo.root.join("refs/heads/.main.tmp.123.0"),
            format!("{c}\n"),
        )
        .unwrap();

        let branches = repo.list_branches().unwrap();
        assert_eq!(branches, vec![("main".to_string(), c)]);
    }

    #[test]
    fn invalid_branch_names_rejected() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path()).unwrap();
        let t = repo
            .write_timeline(&json!({ "OTIO_SCHEMA": "Timeline.1", "name": "v" }))
            .unwrap();
        repo.commit(&t, fake_author(), "v").unwrap();
        for bad in [
            "",
            ".hidden",
            "with space",
            "../escape",
            "trailing/",
            "//double",
        ] {
            assert!(
                repo.create_branch(bad, "HEAD").is_err(),
                "should reject {bad:?}"
            );
        }
    }

    #[test]
    fn merge_base_linear_history() {
        // Linear: c1 -> c2 -> c3. merge_base(c3, c1) is c1.
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path()).unwrap();
        let t = repo
            .write_timeline(&json!({ "OTIO_SCHEMA": "Timeline.1", "name": "v" }))
            .unwrap();
        let c1 = repo.commit(&t, fake_author(), "c1").unwrap();
        let c2 = repo.commit(&t, fake_author(), "c2").unwrap();
        let c3 = repo.commit(&t, fake_author(), "c3").unwrap();

        assert_eq!(
            repo.merge_base(&c3, &c1).unwrap().as_deref(),
            Some(c1.as_str())
        );
        assert_eq!(
            repo.merge_base(&c1, &c3).unwrap().as_deref(),
            Some(c1.as_str())
        );
        assert_eq!(
            repo.merge_base(&c2, &c2).unwrap().as_deref(),
            Some(c2.as_str())
        );
    }

    #[test]
    fn merge_base_diverged_branches() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path()).unwrap();
        let t = repo
            .write_timeline(&json!({ "OTIO_SCHEMA": "Timeline.1", "name": "v" }))
            .unwrap();
        let base = repo.commit(&t, fake_author(), "base").unwrap();

        let main_tip = repo.commit(&t, fake_author(), "main extra").unwrap();

        repo.create_branch("alt", "HEAD").unwrap();
        repo.switch_branch("alt").unwrap();
        // alt diverges from `base` (the parent of main_tip), not from main_tip.
        // Reset alt to point at base.
        repo.set_branch_target("alt", &base).unwrap();
        let alt_tip = repo.commit(&t, fake_author(), "alt extra").unwrap();

        let mb = repo.merge_base(&main_tip, &alt_tip).unwrap();
        assert_eq!(mb.as_deref(), Some(base.as_str()));
    }

    #[test]
    fn merge_base_unrelated_returns_none() {
        // Two repos, different histories. We fake "unrelated" by creating
        // a commit then resetting HEAD to detach.
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path()).unwrap();
        let t = repo
            .write_timeline(&json!({ "OTIO_SCHEMA": "Timeline.1", "name": "v" }))
            .unwrap();
        let c1 = repo.commit(&t, fake_author(), "c1").unwrap();

        // Manually craft an unrelated commit object with no parents.
        let unrelated = crate::commit::Commit::new(
            t.clone(),
            vec![], // no parents — root commit
            fake_author(),
            "1970-01-01T00:00:00Z".to_string(),
            "unrelated".to_string(),
        );
        let v = serde_json::to_value(&unrelated).unwrap();
        let unrelated_hash = repo.write_timeline(&v).unwrap();

        let mb = repo.merge_base(&c1, &unrelated_hash).unwrap();
        assert!(mb.is_none(), "unrelated commits should have no merge base");
    }
}
