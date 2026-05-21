//! Track-level three-way merge for OTIO timelines.
//!
//! Given a base timeline and two diverging timelines (`ours` and
//! `theirs`), produce either a merged timeline (if no track was touched
//! by both sides) or a list of conflicts (if any track was touched by
//! both). v0.6 is deliberately conservative: a track is the unit of
//! conflict, even when the two sides changed different clips inside the
//! same track. v0.6.1 will refine this to clip-level merging.
//!
//! The algorithm is the simplest correct version:
//!
//! 1. Pair up tracks across base/ours/theirs by (name, kind). Tracks
//!    missing from `base` are treated as added; tracks missing from
//!    `ours` or `theirs` are treated as removed by that side.
//! 2. For each track triple, ask: did `ours` touch it (diff vs base
//!    non-empty)? Did `theirs` touch it?
//!    - Neither: keep base track.
//!    - Only ours: keep ours's track.
//!    - Only theirs: keep theirs's track.
//!    - Both: conflict.
//! 3. If both sides added a new track with the same (name, kind), and
//!    its content differs, conflict. If identical, take it once.
//! 4. If both sides removed the same track, take the removal.

use crate::diff::diff;
use crate::model::{Clip, Timeline, Track, TrackChild, TrackKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Result of a three-way merge attempt.
#[derive(Debug, Clone, PartialEq)]
pub enum MergeOutcome {
    /// Merge succeeded. The merged timeline is ready to be committed.
    Clean(Timeline),
    /// One or more conflicts were detected. Nothing has been written.
    Conflicts(Vec<Conflict>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChangedClipIdMergeOutcome {
    Clean(ChangedClipIdMergeClean),
    ClipIdConflicts(ChangedClipIdMergeConflict),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangedClipIdMergeClean {
    pub source_ref: String,
    pub target_ref: String,
    pub commit_hash: String,
    pub parents: Vec<String>,
    pub source_changed_clip_ids: Vec<String>,
    pub target_changed_clip_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangedClipIdMergeConflict {
    pub source_ref: String,
    pub target_ref: String,
    pub source_changed_clip_ids: Vec<String>,
    pub target_changed_clip_ids: Vec<String>,
    pub overlapping_clip_ids: Vec<String>,
}

/// A single merge conflict at the track level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conflict {
    pub track_name: String,
    pub track_kind: TrackKind,
    pub kind: ConflictKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ConflictKind {
    /// Both branches changed clips, transitions, or effects on the same
    /// track. v0.6 reports this at track granularity; v0.6.1 will
    /// refine to clip granularity.
    BothModified,
    /// Both branches added a new track with the same name and kind, but
    /// with different content.
    BothAdded,
    /// One branch deleted the track while the other modified it.
    DeleteVsModify { deleter: Side },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Ours,
    Theirs,
}

pub fn changed_clip_ids(base: Option<&Timeline>, after: &Timeline) -> Vec<String> {
    let after_clips = clips_by_id(after);
    let mut changed = BTreeSet::new();

    let Some(base) = base else {
        return after_clips.keys().cloned().collect();
    };

    let base_clips = clips_by_id(base);
    for (id, after_clip) in &after_clips {
        match base_clips.get(id) {
            Some(base_clip) if *base_clip == *after_clip => {}
            _ => {
                changed.insert(id.clone());
            }
        }
    }
    for id in base_clips.keys() {
        if !after_clips.contains_key(id) {
            changed.insert(id.clone());
        }
    }

    changed.into_iter().collect()
}

pub fn merge_non_overlapping_changed_clip_ids(
    base: &Timeline,
    target: &Timeline,
    source: &Timeline,
) -> Result<Timeline, ChangedClipIdMergeConflict> {
    let source_changed = changed_clip_ids(Some(base), source);
    let target_changed = changed_clip_ids(Some(base), target);
    let overlap: Vec<String> = source_changed
        .iter()
        .filter(|id| target_changed.binary_search(id).is_ok())
        .cloned()
        .collect();

    if !overlap.is_empty() {
        return Err(ChangedClipIdMergeConflict {
            source_ref: String::new(),
            target_ref: String::new(),
            source_changed_clip_ids: source_changed,
            target_changed_clip_ids: target_changed,
            overlapping_clip_ids: overlap,
        });
    }

    Ok(overlay_clip_id_changes(target, source, &source_changed))
}

/// Run a three-way merge on three already-parsed timelines.
pub fn merge(base: &Timeline, ours: &Timeline, theirs: &Timeline) -> MergeOutcome {
    let mut merged_tracks: Vec<Track> = Vec::new();
    let mut conflicts: Vec<Conflict> = Vec::new();

    // Index every track by (name, kind) for each side. We use ordered
    // collection of keys so the merged output's track order is stable
    // and predictable.
    let base_index = index_tracks(base);
    let ours_index = index_tracks(ours);
    let theirs_index = index_tracks(theirs);

    // Walk the union of keys in a stable order: ours's order first
    // (preserving any reordering ours did), then any new keys theirs
    // introduced that ours didn't have.
    let mut seen: std::collections::HashSet<(String, TrackKind)> = std::collections::HashSet::new();
    let mut merge_order: Vec<&(String, TrackKind)> = Vec::new();
    for t in &ours.tracks {
        let key = (t.name.clone(), t.kind);
        if seen.insert(key.clone()) {
            // Push a borrow to the in-place key copy from ours_index
            // so we don't end up cloning unnecessarily later.
            if let Some(stored) = ours_index.get_key_value(&key) {
                merge_order.push(stored.0);
            }
        }
    }
    for t in &theirs.tracks {
        let key = (t.name.clone(), t.kind);
        if seen.insert(key.clone())
            && let Some(stored) = theirs_index.get_key_value(&key)
        {
            merge_order.push(stored.0);
        }
    }
    // Tracks that exist only in base (deleted by both) are handled below
    // implicitly: they contribute nothing to merge_order, so they stay
    // out of the merged output, which is the correct behavior.
    for key in base_index.keys() {
        // If a base track was deleted by both ours and theirs, do nothing
        // (correctly absent from output). If only one side deleted it,
        // we'll handle that as a conflict or adoption below.
        let _ = key;
    }

    for key in &merge_order {
        let base_track = base_index.get(*key).copied();
        let ours_track = ours_index.get(*key).copied();
        let theirs_track = theirs_index.get(*key).copied();

        match resolve_track(base_track, ours_track, theirs_track) {
            TrackResolution::Take(t) => merged_tracks.push(t.clone()),
            TrackResolution::Conflict(kind) => conflicts.push(Conflict {
                track_name: key.0.clone(),
                track_kind: key.1,
                kind,
            }),
            TrackResolution::Drop => {}
        }
    }

    // Now check for tracks that exist in base but in neither side's
    // merge_order — handled implicitly. But we also need to surface
    // the case where one side deleted a track that the other modified.
    for (key, base_track) in &base_index {
        if seen.contains(key) {
            continue; // already considered above
        }
        // base has this track; neither ours nor theirs has it. Both
        // deleted — that's clean (track stays absent).
        let _ = base_track;
    }

    if conflicts.is_empty() {
        MergeOutcome::Clean(Timeline {
            name: ours.name.clone(),
            tracks: merged_tracks,
        })
    } else {
        MergeOutcome::Conflicts(conflicts)
    }
}

enum TrackResolution<'a> {
    Take(&'a Track),
    Conflict(ConflictKind),
    Drop,
}

fn resolve_track<'a>(
    base: Option<&'a Track>,
    ours: Option<&'a Track>,
    theirs: Option<&'a Track>,
) -> TrackResolution<'a> {
    match (base, ours, theirs) {
        // Track present everywhere: standard three-way case.
        (Some(b), Some(o), Some(t)) => {
            let ours_changed = !diff_track(b, o).is_empty();
            let theirs_changed = !diff_track(b, t).is_empty();
            match (ours_changed, theirs_changed) {
                (false, false) => TrackResolution::Take(b),
                (true, false) => TrackResolution::Take(o),
                (false, true) => TrackResolution::Take(t),
                (true, true) => {
                    if tracks_equal(o, t) {
                        TrackResolution::Take(o)
                    } else {
                        TrackResolution::Conflict(ConflictKind::BothModified)
                    }
                }
            }
        }
        // Track added by both sides.
        (None, Some(o), Some(t)) => {
            if tracks_equal(o, t) {
                TrackResolution::Take(o)
            } else {
                TrackResolution::Conflict(ConflictKind::BothAdded)
            }
        }
        // Track added by one side only.
        (None, Some(o), None) => TrackResolution::Take(o),
        (None, None, Some(t)) => TrackResolution::Take(t),
        // Track deleted by ours, modified by theirs (or vice versa).
        (Some(b), None, Some(t)) => {
            if tracks_equal(b, t) {
                TrackResolution::Drop // theirs didn't actually change it; ours deleted; respect deletion.
            } else {
                TrackResolution::Conflict(ConflictKind::DeleteVsModify {
                    deleter: Side::Ours,
                })
            }
        }
        (Some(b), Some(o), None) => {
            if tracks_equal(b, o) {
                TrackResolution::Drop // ours didn't actually change it; theirs deleted.
            } else {
                TrackResolution::Conflict(ConflictKind::DeleteVsModify {
                    deleter: Side::Theirs,
                })
            }
        }
        // Deleted by both — drop.
        (Some(_), None, None) => TrackResolution::Drop,
        // Doesn't exist in any side. Shouldn't happen given how we walk
        // the union, but be safe.
        (None, None, None) => TrackResolution::Drop,
    }
}

fn index_tracks(tl: &Timeline) -> std::collections::BTreeMap<(String, TrackKind), &Track> {
    let mut out = std::collections::BTreeMap::new();
    for t in &tl.tracks {
        out.insert((t.name.clone(), t.kind), t);
    }
    out
}

/// Compare two tracks for byte-level equality of their content. Used to
/// detect "both sides changed it the same way" and treat that as
/// non-conflicting.
fn tracks_equal(a: &Track, b: &Track) -> bool {
    a == b
}

/// Wrap a single track in a one-track Timeline so we can use the
/// existing diff engine to detect "did this side touch this track."
fn diff_track(before: &Track, after: &Track) -> Vec<crate::diff::Change> {
    let before_tl = Timeline {
        name: String::new(),
        tracks: vec![before.clone()],
    };
    let after_tl = Timeline {
        name: String::new(),
        tracks: vec![after.clone()],
    };
    diff(&before_tl, &after_tl)
}

fn clips_by_id(timeline: &Timeline) -> BTreeMap<String, &Clip> {
    let mut out = BTreeMap::new();
    for track in &timeline.tracks {
        for child in &track.children {
            if let TrackChild::Clip(clip) = child
                && let Some(id) = &clip.clip_id
            {
                out.insert(id.clone(), clip);
            }
        }
    }
    out
}

fn overlay_clip_id_changes(
    target: &Timeline,
    source: &Timeline,
    source_changed: &[String],
) -> Timeline {
    let source_changed: BTreeSet<String> = source_changed.iter().cloned().collect();
    let source_clips = clips_by_id(source);
    let mut merged = target.clone();
    let mut applied = BTreeSet::new();

    for track in &mut merged.tracks {
        for child in &mut track.children {
            if let TrackChild::Clip(target_clip) = child
                && let Some(id) = target_clip.clip_id.clone()
                && source_changed.contains(&id)
                && let Some(source_clip) = source_clips.get(&id)
            {
                *target_clip = (*source_clip).clone();
                applied.insert(id);
            }
        }
    }

    for source_track in &source.tracks {
        let source_additions: Vec<TrackChild> = source_track
            .children
            .iter()
            .filter_map(|child| match child {
                TrackChild::Clip(clip)
                    if clip
                        .clip_id
                        .as_ref()
                        .is_some_and(|id| source_changed.contains(id) && !applied.contains(id)) =>
                {
                    Some(TrackChild::Clip(clip.clone()))
                }
                _ => None,
            })
            .collect();
        if source_additions.is_empty() {
            continue;
        }

        if let Some(target_track) = merged
            .tracks
            .iter_mut()
            .find(|track| track.name == source_track.name && track.kind == source_track.kind)
        {
            for child in source_additions {
                if let TrackChild::Clip(clip) = &child
                    && let Some(id) = &clip.clip_id
                {
                    applied.insert(id.clone());
                }
                target_track.children.push(child);
            }
        } else {
            let mut track = source_track.clone();
            track.children = source_additions;
            for child in &track.children {
                if let TrackChild::Clip(clip) = child
                    && let Some(id) = &clip.clip_id
                {
                    applied.insert(id.clone());
                }
            }
            merged.tracks.push(track);
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Clip, RationalTime, TimeRange, TrackChild};

    fn rt(value: f64) -> RationalTime {
        RationalTime { value, rate: 24.0 }
    }

    fn clip(name: &str, media: &str) -> TrackChild {
        TrackChild::Clip(Clip {
            clip_id: None,
            name: name.to_string(),
            media_reference: Some(media.to_string()),
            source_range: Some(TimeRange {
                start_time: rt(0.0),
                duration: rt(24.0),
            }),
            effects: Vec::new(),
        })
    }

    fn track(name: &str, kind: TrackKind, children: Vec<TrackChild>) -> Track {
        Track {
            name: name.to_string(),
            kind,
            children,
        }
    }

    fn timeline(tracks: Vec<Track>) -> Timeline {
        Timeline {
            name: "doc".to_string(),
            tracks,
        }
    }

    #[test]
    fn no_changes_means_base_is_returned() {
        let base = timeline(vec![track(
            "V1",
            TrackKind::Video,
            vec![clip("a", "media://a.mov")],
        )]);
        let outcome = merge(&base, &base, &base);
        match outcome {
            MergeOutcome::Clean(tl) => assert_eq!(tl.tracks, base.tracks),
            other => panic!("expected Clean, got {:?}", other),
        }
    }

    #[test]
    fn ours_changed_theirs_didnt_takes_ours() {
        let base = timeline(vec![track(
            "V1",
            TrackKind::Video,
            vec![clip("a", "media://a.mov")],
        )]);
        let ours = timeline(vec![track(
            "V1",
            TrackKind::Video,
            vec![clip("a", "media://a.mov"), clip("b", "media://b.mov")],
        )]);
        let theirs = base.clone();
        match merge(&base, &ours, &theirs) {
            MergeOutcome::Clean(tl) => assert_eq!(tl.tracks, ours.tracks),
            other => panic!("expected Clean, got {:?}", other),
        }
    }

    #[test]
    fn each_side_changes_a_different_track_clean_merge() {
        let base = timeline(vec![
            track("V1", TrackKind::Video, vec![clip("a", "media://a.mov")]),
            track("A1", TrackKind::Audio, vec![clip("vo", "media://vo.wav")]),
        ]);
        let ours = timeline(vec![
            track(
                "V1",
                TrackKind::Video,
                vec![clip("a", "media://a.mov"), clip("b", "media://b.mov")],
            ),
            track("A1", TrackKind::Audio, vec![clip("vo", "media://vo.wav")]),
        ]);
        let theirs = timeline(vec![
            track("V1", TrackKind::Video, vec![clip("a", "media://a.mov")]),
            track(
                "A1",
                TrackKind::Audio,
                vec![clip("vo", "media://vo.wav"), clip("sfx", "media://sfx.wav")],
            ),
        ]);
        match merge(&base, &ours, &theirs) {
            MergeOutcome::Clean(tl) => {
                assert_eq!(tl.tracks.len(), 2);
                // V1 should match ours (with b added)
                assert_eq!(tl.tracks[0].children.len(), 2);
                // A1 should match theirs (with sfx added)
                assert_eq!(tl.tracks[1].children.len(), 2);
            }
            other => panic!("expected Clean, got {:?}", other),
        }
    }

    #[test]
    fn both_sides_change_same_track_conflicts() {
        let base = timeline(vec![track(
            "V1",
            TrackKind::Video,
            vec![clip("a", "media://a.mov")],
        )]);
        let ours = timeline(vec![track(
            "V1",
            TrackKind::Video,
            vec![clip("a", "media://a.mov"), clip("b", "media://b.mov")],
        )]);
        let theirs = timeline(vec![track(
            "V1",
            TrackKind::Video,
            vec![clip("a", "media://a.mov"), clip("c", "media://c.mov")],
        )]);
        match merge(&base, &ours, &theirs) {
            MergeOutcome::Conflicts(cs) => {
                assert_eq!(cs.len(), 1);
                assert_eq!(cs[0].track_name, "V1");
                assert!(matches!(cs[0].kind, ConflictKind::BothModified));
            }
            other => panic!("expected Conflicts, got {:?}", other),
        }
    }

    #[test]
    fn both_sides_make_identical_change_clean() {
        let base = timeline(vec![track(
            "V1",
            TrackKind::Video,
            vec![clip("a", "media://a.mov")],
        )]);
        let after = timeline(vec![track(
            "V1",
            TrackKind::Video,
            vec![clip("a", "media://a.mov"), clip("b", "media://b.mov")],
        )]);
        // Both sides made the exact same edit.
        match merge(&base, &after, &after) {
            MergeOutcome::Clean(tl) => assert_eq!(tl.tracks, after.tracks),
            other => panic!("expected Clean, got {:?}", other),
        }
    }

    #[test]
    fn both_sides_add_same_track_identically_clean() {
        let base = timeline(vec![track(
            "V1",
            TrackKind::Video,
            vec![clip("a", "media://a.mov")],
        )]);
        let added_track = track("A1", TrackKind::Audio, vec![clip("vo", "media://vo.wav")]);
        let ours = timeline(vec![base.tracks[0].clone(), added_track.clone()]);
        let theirs = ours.clone();
        match merge(&base, &ours, &theirs) {
            MergeOutcome::Clean(tl) => assert_eq!(tl.tracks.len(), 2),
            other => panic!("expected Clean, got {:?}", other),
        }
    }

    #[test]
    fn both_sides_add_same_track_differently_conflicts() {
        let base = timeline(vec![track(
            "V1",
            TrackKind::Video,
            vec![clip("a", "media://a.mov")],
        )]);
        let ours = timeline(vec![
            base.tracks[0].clone(),
            track("A1", TrackKind::Audio, vec![clip("vo", "media://vo1.wav")]),
        ]);
        let theirs = timeline(vec![
            base.tracks[0].clone(),
            track("A1", TrackKind::Audio, vec![clip("vo", "media://vo2.wav")]),
        ]);
        match merge(&base, &ours, &theirs) {
            MergeOutcome::Conflicts(cs) => {
                assert_eq!(cs.len(), 1);
                assert!(matches!(cs[0].kind, ConflictKind::BothAdded));
            }
            other => panic!("expected Conflicts, got {:?}", other),
        }
    }

    #[test]
    fn delete_vs_modify_conflicts() {
        let base = timeline(vec![
            track("V1", TrackKind::Video, vec![clip("a", "media://a.mov")]),
            track("A1", TrackKind::Audio, vec![clip("vo", "media://vo.wav")]),
        ]);
        // Ours deletes A1, theirs modifies it.
        let ours = timeline(vec![base.tracks[0].clone()]);
        let theirs = timeline(vec![
            base.tracks[0].clone(),
            track(
                "A1",
                TrackKind::Audio,
                vec![clip("vo", "media://vo.wav"), clip("sfx", "media://sfx.wav")],
            ),
        ]);
        match merge(&base, &ours, &theirs) {
            MergeOutcome::Conflicts(cs) => {
                assert_eq!(cs.len(), 1);
                assert_eq!(cs[0].track_name, "A1");
                assert!(matches!(
                    cs[0].kind,
                    ConflictKind::DeleteVsModify {
                        deleter: Side::Ours
                    }
                ));
            }
            other => panic!("expected Conflicts, got {:?}", other),
        }
    }
}
