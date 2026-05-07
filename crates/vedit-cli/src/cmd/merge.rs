//! `vedit merge <branch>` — three-way merge of another branch into the
//! current branch.
//!
//! Cases handled:
//!   - Already up-to-date (target is an ancestor of HEAD): no-op
//!   - Fast-forward (HEAD is an ancestor of target): advance the branch
//!     pointer to target, no merge commit
//!   - True merge: run a three-way merge via vedit-core::merge. On
//!     `Clean`, create a two-parent merge commit. On `Conflicts`, print
//!     and exit non-zero — v0.6 has no in-place resolution UX.

use crate::author;
use anyhow::{anyhow, bail, Result};
use vedit_core::merge::{merge as run_merge, MergeOutcome};
use vedit_core::object;
use vedit_core::otio;
use vedit_core::repo::{HeadState, Repo};

pub struct MergeOptions {
    pub message: Option<String>,
    pub dry_run: bool,
}

pub fn run(target: &str, options: MergeOptions) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let repo = Repo::discover(&cwd)?;

    let current_branch = match repo.head()? {
        HeadState::Branch(name) => name,
        HeadState::Detached(_) => {
            bail!("cannot merge while HEAD is detached; switch to a branch first")
        }
    };
    let head_hash = repo
        .branch_target(&current_branch)?
        .ok_or_else(|| anyhow!("current branch {current_branch} has no commits"))?;
    let target_hash = repo.resolve(target)?;

    if head_hash == target_hash {
        println!("Already up to date.");
        return Ok(());
    }

    // Already-merged case: target is an ancestor of HEAD.
    if is_ancestor(&repo, &target_hash, &head_hash)? {
        println!("Already up to date.");
        return Ok(());
    }

    // Fast-forward case: HEAD is an ancestor of target.
    if is_ancestor(&repo, &head_hash, &target_hash)? {
        if options.dry_run {
            println!(
                "Would fast-forward {current_branch} from {} to {}",
                short(&head_hash),
                short(&target_hash)
            );
            return Ok(());
        }
        std::fs::write(
            repo.root.join("refs/heads").join(&current_branch),
            format!("{target_hash}\n"),
        )?;
        println!(
            "Fast-forwarded {current_branch} from {} to {}",
            short(&head_hash),
            short(&target_hash)
        );
        return Ok(());
    }

    // True three-way merge.
    let base_hash = repo
        .merge_base(&head_hash, &target_hash)?
        .ok_or_else(|| anyhow!("no common ancestor between {current_branch} and {target}"))?;

    let base = parse_commit_timeline(&repo, &base_hash)?;
    let ours = parse_commit_timeline(&repo, &head_hash)?;
    let theirs = parse_commit_timeline(&repo, &target_hash)?;

    let outcome = run_merge(&base, &ours, &theirs);

    match outcome {
        MergeOutcome::Conflicts(conflicts) => {
            eprintln!(
                "Merge of {target} into {current_branch} hit {} conflict(s):",
                conflicts.len()
            );
            for c in &conflicts {
                eprintln!(
                    "  - {:?} track \"{}\" — {}",
                    c.track_kind,
                    c.track_name,
                    describe_conflict(&c.kind)
                );
            }
            eprintln!();
            eprintln!("Nothing was committed. v0.6 has no in-place conflict resolution.");
            eprintln!("To unblock: edit one branch to incorporate the other side's changes,");
            eprintln!("commit, then re-run vedit merge.");
            bail!("merge aborted due to conflicts");
        }
        MergeOutcome::Clean(merged_timeline) => {
            if options.dry_run {
                println!(
                    "Would merge {target} into {current_branch} cleanly ({} track(s))",
                    merged_timeline.tracks.len()
                );
                return Ok(());
            }
            // Re-emit the merged timeline as OTIO JSON so the snapshot
            // we store has the same shape as anything else in the
            // object store. We round-trip via the union of `ours` (for
            // any object-level metadata our model dropped) where
            // possible, but for v0.6 the simple path is to use a
            // synthesized OTIO from the merged Timeline plus the
            // unchanged sides.
            let merged_value = synthesize_otio(&repo, &head_hash, &target_hash, &merged_timeline)?;
            let timeline_hash = repo.write_timeline(&merged_value)?;

            let message = options.message.unwrap_or_else(|| {
                format!("Merge branch '{target}' into {current_branch}")
            });

            let commit_hash = repo.commit_with_parents(
                &timeline_hash,
                vec![head_hash.clone(), target_hash.clone()],
                author::resolve()?,
                &message,
            )?;

            println!(
                "[{} {}] {}",
                current_branch,
                short(&commit_hash),
                message.lines().next().unwrap_or("")
            );
            println!(
                "  parents: {} (ours), {} (theirs)",
                short(&head_hash),
                short(&target_hash)
            );
        }
    }

    Ok(())
}

fn is_ancestor(repo: &Repo, ancestor: &str, descendant: &str) -> Result<bool> {
    if ancestor == descendant {
        return Ok(true);
    }
    // BFS from descendant through parents; if we hit ancestor we're done.
    let mut frontier: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    frontier.push_back(descendant.to_string());
    while let Some(h) = frontier.pop_front() {
        if !seen.insert(h.clone()) {
            continue;
        }
        if h == ancestor {
            return Ok(true);
        }
        let commit = repo.read_commit(&h)?;
        for p in commit.parents {
            frontier.push_back(p);
        }
    }
    Ok(false)
}

fn parse_commit_timeline(repo: &Repo, commit_hash: &str) -> Result<vedit_core::model::Timeline> {
    let commit = repo.read_commit(commit_hash)?;
    let value = repo.read_timeline(&commit.timeline)?;
    otio::parse_timeline(&value)
}

/// For v0.6, picking the merged OTIO bytes is the lossy step we accept.
/// We use ours's OTIO as the base, then for each track that ended up in
/// the merged timeline:
///   - if the track is structurally equal to ours's same-named track,
///     keep ours's bytes (preserves any metadata our model dropped)
///   - if the track came from theirs, splice in theirs's bytes
///   - otherwise (added/removed track), reflect that
///
/// This is good enough for round-tripping the merged state while
/// preserving as much editor-written metadata as possible.
fn synthesize_otio(
    repo: &Repo,
    ours_hash: &str,
    theirs_hash: &str,
    merged: &vedit_core::model::Timeline,
) -> Result<serde_json::Value> {
    use serde_json::Value;
    let ours_commit = repo.read_commit(ours_hash)?;
    let theirs_commit = repo.read_commit(theirs_hash)?;
    let ours_value = repo.read_timeline(&ours_commit.timeline)?;
    let theirs_value = repo.read_timeline(&theirs_commit.timeline)?;

    // Index ours and theirs's track JSON objects by (name, kind).
    let mut ours_tracks_json = index_track_objects(&ours_value);
    let mut theirs_tracks_json = index_track_objects(&theirs_value);

    // Parse them back through the model so we can compare structurally.
    let ours_parsed = otio::parse_timeline(&ours_value)?;
    let theirs_parsed = otio::parse_timeline(&theirs_value)?;
    let ours_index: std::collections::BTreeMap<_, _> = ours_parsed
        .tracks
        .iter()
        .map(|t| ((t.name.clone(), t.kind), t))
        .collect();
    let theirs_index: std::collections::BTreeMap<_, _> = theirs_parsed
        .tracks
        .iter()
        .map(|t| ((t.name.clone(), t.kind), t))
        .collect();

    let mut new_track_children: Vec<Value> = Vec::with_capacity(merged.tracks.len());
    for merged_track in &merged.tracks {
        let key = (merged_track.name.clone(), merged_track.kind);
        // Prefer ours's JSON if structurally equal.
        if let Some(t) = ours_index.get(&key)
            && *t == merged_track
                && let Some(json) = ours_tracks_json.remove(&key) {
                    new_track_children.push(json);
                    continue;
                }
        if let Some(t) = theirs_index.get(&key)
            && *t == merged_track
                && let Some(json) = theirs_tracks_json.remove(&key) {
                    new_track_children.push(json);
                    continue;
                }
        // Fallback: emit a minimal-but-valid Track JSON from our model.
        new_track_children.push(synth_track_json(merged_track));
    }

    // Take ours's top-level Timeline JSON and replace its tracks.children.
    let mut out = ours_value.clone();
    if let Some(stack) = out.get_mut("tracks").and_then(|s| s.as_object_mut()) {
        stack.insert("children".to_string(), Value::Array(new_track_children));
    }
    Ok(out)
}

fn index_track_objects(
    timeline: &serde_json::Value,
) -> std::collections::BTreeMap<(String, vedit_core::model::TrackKind), serde_json::Value> {
    let mut out = std::collections::BTreeMap::new();
    let Some(stack) = timeline.get("tracks").and_then(|s| s.as_object()) else {
        return out;
    };
    let Some(children) = stack.get("children").and_then(|c| c.as_array()) else {
        return out;
    };
    for child in children {
        let schema = child
            .get("OTIO_SCHEMA")
            .and_then(|s| s.as_str())
            .unwrap_or("");
        if !schema.starts_with("Track.") {
            continue;
        }
        let name = child
            .get("name")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let kind = child
            .get("kind")
            .and_then(|s| s.as_str())
            .map(|s| match s.to_ascii_lowercase().as_str() {
                "video" => vedit_core::model::TrackKind::Video,
                "audio" => vedit_core::model::TrackKind::Audio,
                _ => vedit_core::model::TrackKind::Other,
            })
            .unwrap_or(vedit_core::model::TrackKind::Other);
        out.insert((name, kind), child.clone());
    }
    out
}

fn synth_track_json(track: &vedit_core::model::Track) -> serde_json::Value {
    serde_json::json!({
        "OTIO_SCHEMA": "Track.1",
        "name": track.name,
        "kind": match track.kind {
            vedit_core::model::TrackKind::Video => "Video",
            vedit_core::model::TrackKind::Audio => "Audio",
            vedit_core::model::TrackKind::Other => "Other",
        },
        "children": [],
        "metadata": {}
    })
}

fn describe_conflict(kind: &vedit_core::merge::ConflictKind) -> String {
    use vedit_core::merge::ConflictKind::*;
    use vedit_core::merge::Side;
    match kind {
        BothModified => "both branches modified this track".to_string(),
        BothAdded => "both branches added this track with different content".to_string(),
        DeleteVsModify { deleter: Side::Ours } => {
            "ours deleted this track but theirs modified it".to_string()
        }
        DeleteVsModify { deleter: Side::Theirs } => {
            "theirs deleted this track but ours modified it".to_string()
        }
    }
}

fn short(hash: &str) -> String {
    let body = hash.strip_prefix(object::HASH_PREFIX).unwrap_or(hash);
    body.chars().take(7).collect()
}
