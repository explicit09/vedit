//! Semantic diff between two timelines.
//!
//! The diff produces a list of structured entries describing what changed
//! at the level of edit decisions: clips added, removed, moved, trimmed;
//! transitions added or removed; tracks added or removed. The matcher
//! pairs clips by content fingerprint, not by position or any metadata
//! ID. That choice is what makes vedit work on OTIO from any source,
//! including editors that strip third-party metadata.

use crate::model::{
    Clip, RationalTime, TimeRange, Timeline, Track, TrackChild, TrackKind,
};
use serde::{Deserialize, Serialize};

/// One unit of change between two timelines. The shape is designed to be
/// equally legible to humans (rendered as prose) and to AI agents (consumed
/// as JSON).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Change {
    /// Track added in `after`.
    TrackAdded { name: String, kind: TrackKind },
    /// Track removed from `before`.
    TrackRemoved { name: String, kind: TrackKind },
    /// Clip present in both, but its source range narrowed or shifted.
    Trimmed {
        clip: ClipRef,
        track: String,
        before: TimeRange,
        after: TimeRange,
    },
    /// Clip's index within its track changed (or jumped tracks).
    /// Indices are clip-list-relative — they ignore transitions and gaps,
    /// so an inserted transition does not register as a move.
    Moved {
        clip: ClipRef,
        from_track: String,
        from_index: usize,
        to_track: String,
        to_index: usize,
        /// The clip that follows this one in the after-track, if any. Used
        /// for renderings like "Moved X before Y."
        after_neighbor: Option<ClipRef>,
        /// The clip that precedes this one in the after-track, if any.
        before_neighbor: Option<ClipRef>,
    },
    /// Clip exists in `after` with no match in `before`.
    Added {
        clip: ClipRef,
        track: String,
        index: usize,
    },
    /// Clip in `before` had no match in `after`.
    Removed {
        clip: ClipRef,
        track: String,
        index: usize,
    },
    /// Effect count on a matched clip changed.
    EffectsChanged {
        clip: ClipRef,
        track: String,
        before: usize,
        after: usize,
    },
    /// A clip kept its name and position but its media reference changed.
    /// This is the "I dropped a different take onto the same clip slot"
    /// case. Treated as one logical change, not as remove + add.
    Replaced {
        clip: ClipRef,
        track: String,
        before_media: Option<String>,
        after_media: Option<String>,
    },
    /// Transition appeared between two adjacent matched clips.
    TransitionAdded {
        track: String,
        between_before: Option<ClipRef>,
        between_after: Option<ClipRef>,
        name: String,
        duration: Option<RationalTime>,
    },
    /// Transition disappeared between two adjacent matched clips.
    TransitionRemoved {
        track: String,
        between_before: Option<ClipRef>,
        between_after: Option<ClipRef>,
        name: String,
    },
}

/// Reference to a clip suitable for human display: name + media url.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipRef {
    pub name: String,
    pub media_reference: Option<String>,
}

impl From<&Clip> for ClipRef {
    fn from(c: &Clip) -> Self {
        ClipRef {
            name: c.name.clone(),
            media_reference: c.media_reference.clone(),
        }
    }
}

/// Compute the diff. The two timelines are assumed to have been parsed via
/// [`crate::otio::load`]; matching is content-based and order-stable.
pub fn diff(before: &Timeline, after: &Timeline) -> Vec<Change> {
    let mut changes = Vec::new();

    // Track-level pairing by name, then by ordered position. We keep it
    // simple for v0.1: tracks of the same kind whose names match are
    // paired. Tracks that fail to pair are reported as added/removed.
    let mut before_tracks_used = vec![false; before.tracks.len()];
    let mut after_tracks_used = vec![false; after.tracks.len()];
    let mut paired_tracks: Vec<(usize, usize)> = Vec::new();

    for (a_idx, a_track) in after.tracks.iter().enumerate() {
        if let Some(b_idx) = find_matching_track(a_track, &before.tracks, &before_tracks_used)
        {
            before_tracks_used[b_idx] = true;
            after_tracks_used[a_idx] = true;
            paired_tracks.push((b_idx, a_idx));
        }
    }

    // Diff inside paired tracks.
    for (b_idx, a_idx) in &paired_tracks {
        let before_track = &before.tracks[*b_idx];
        let after_track = &after.tracks[*a_idx];
        diff_track(before_track, after_track, &mut changes);
    }

    // Unpaired tracks become added/removed.
    for (idx, used) in before_tracks_used.iter().enumerate() {
        if !used {
            let t = &before.tracks[idx];
            changes.push(Change::TrackRemoved {
                name: t.name.clone(),
                kind: t.kind,
            });
        }
    }
    for (idx, used) in after_tracks_used.iter().enumerate() {
        if !used {
            let t = &after.tracks[idx];
            changes.push(Change::TrackAdded {
                name: t.name.clone(),
                kind: t.kind,
            });
        }
    }

    changes
}

fn find_matching_track(
    needle: &Track,
    haystack: &[Track],
    used: &[bool],
) -> Option<usize> {
    // Prefer same kind + same name.
    for (i, t) in haystack.iter().enumerate() {
        if used[i] {
            continue;
        }
        if t.kind == needle.kind && t.name == needle.name && !t.name.is_empty() {
            return Some(i);
        }
    }
    // Fall back to same kind, first available.
    for (i, t) in haystack.iter().enumerate() {
        if used[i] {
            continue;
        }
        if t.kind == needle.kind {
            return Some(i);
        }
    }
    None
}

fn diff_track(before: &Track, after: &Track, out: &mut Vec<Change>) {
    let track_name = if !after.name.is_empty() {
        after.name.clone()
    } else {
        before.name.clone()
    };

    let before_clips: Vec<(usize, &Clip)> = before
        .children
        .iter()
        .enumerate()
        .filter_map(|(i, child)| match child {
            TrackChild::Clip(c) => Some((i, c)),
            _ => None,
        })
        .collect();
    let after_clips: Vec<(usize, &Clip)> = after
        .children
        .iter()
        .enumerate()
        .filter_map(|(i, child)| match child {
            TrackChild::Clip(c) => Some((i, c)),
            _ => None,
        })
        .collect();

    // Match clips in two passes:
    //   1. Strong fingerprint = (media_reference, source_range). When this
    //      collides (multiple clips with same media + range), pair them
    //      by closest position so re-using the same clip across the
    //      timeline doesn't produce "self-moved" entries.
    //   2. Weak fingerprint = name. Same position-aware tie-breaking.
    //
    // The position-aware step is what makes vedit usable on real Resolve
    // exports, where the same media file is dropped onto the timeline
    // many times.
    let mut before_used = vec![false; before_clips.len()];
    let mut after_used = vec![false; after_clips.len()];
    let mut matches: Vec<(usize, usize)> = Vec::new();

    let strong_pairs = pair_by_fingerprint(
        &before_clips,
        &after_clips,
        &mut before_used,
        &mut after_used,
        strong_fingerprint,
    );
    matches.extend(strong_pairs);

    let weak_pairs = pair_by_fingerprint(
        &before_clips,
        &after_clips,
        &mut before_used,
        &mut after_used,
        weak_fingerprint,
    );
    matches.extend(weak_pairs);

    // A clip is "moved" only if its position breaks the relative order of
    // other matched clips. Compute the longest order-preserving subset of
    // matches (LCS-style); pairs in that subset stayed in place, the
    // others moved.
    matches.sort_by_key(|(b, _)| *b);
    let stable_after: std::collections::HashSet<usize> =
        longest_increasing_after_indices(&matches)
            .into_iter()
            .collect();

    // Emit changes for matched clips.
    for (b_pos, a_pos) in &matches {
        let (_, b_clip) = before_clips[*b_pos];
        let (_, a_clip) = after_clips[*a_pos];

        // Trim detection.
        if let (Some(br), Some(ar)) = (b_clip.source_range, a_clip.source_range)
            && !time_ranges_equal(&br, &ar) {
                out.push(Change::Trimmed {
                    clip: a_clip.into(),
                    track: track_name.clone(),
                    before: br,
                    after: ar,
                });
            }

        // Move detection: only flag pairs that aren't part of the stable
        // order. If an inserted/removed clip merely shifted absolute
        // indices, the pair stays in `stable_after` and we don't report
        // a move.
        if !stable_after.contains(a_pos) {
            let after_neighbor = after_clips.get(*a_pos + 1).map(|(_, c)| (*c).into());
            let before_neighbor = if *a_pos > 0 {
                after_clips.get(*a_pos - 1).map(|(_, c)| (*c).into())
            } else {
                None
            };
            out.push(Change::Moved {
                clip: a_clip.into(),
                from_track: track_name.clone(),
                from_index: *b_pos,
                to_track: track_name.clone(),
                to_index: *a_pos,
                after_neighbor,
                before_neighbor,
            });
        }

        // Effect count delta.
        if b_clip.effect_count != a_clip.effect_count {
            out.push(Change::EffectsChanged {
                clip: a_clip.into(),
                track: track_name.clone(),
                before: b_clip.effect_count,
                after: a_clip.effect_count,
            });
        }

        // Media replacement: same name (weak match), different URL.
        if b_clip.media_reference != a_clip.media_reference {
            out.push(Change::Replaced {
                clip: a_clip.into(),
                track: track_name.clone(),
                before_media: b_clip.media_reference.clone(),
                after_media: a_clip.media_reference.clone(),
            });
        }
    }

    // Unmatched clips: removed (in before) and added (in after).
    for (b_pos, used) in before_used.iter().enumerate() {
        if !*used {
            let (_, clip) = before_clips[b_pos];
            out.push(Change::Removed {
                clip: clip.into(),
                track: track_name.clone(),
                index: b_pos,
            });
        }
    }
    for (a_pos, used) in after_used.iter().enumerate() {
        if !*used {
            let (_, clip) = after_clips[a_pos];
            out.push(Change::Added {
                clip: clip.into(),
                track: track_name.clone(),
                index: a_pos,
            });
        }
    }

    // Transition-level diff: walk the children of both tracks and emit
    // transition_added / transition_removed when the set of transitions
    // between matched neighbors differs.
    diff_transitions(before, after, &track_name, &matches, &before_clips, &after_clips, out);
}

fn diff_transitions(
    before: &Track,
    after: &Track,
    track_name: &str,
    matches: &[(usize, usize)],
    before_clips: &[(usize, &Clip)],
    after_clips: &[(usize, &Clip)],
    out: &mut Vec<Change>,
) {
    // Build maps from clip-list-position to whether-it-has-a-following-transition.
    let before_transitions = transitions_after_each_clip(before, before_clips);
    let after_transitions = transitions_after_each_clip(after, after_clips);

    for (b_pos, a_pos) in matches {
        let b_t = &before_transitions[*b_pos];
        let a_t = &after_transitions[*a_pos];

        let before_neighbor: Option<ClipRef> = before_clips
            .get(*b_pos + 1)
            .map(|(_, c)| (*c).into());
        let after_neighbor: Option<ClipRef> = after_clips
            .get(*a_pos + 1)
            .map(|(_, c)| (*c).into());
        let this_clip: ClipRef = before_clips[*b_pos].1.into();
        let _ = this_clip; // not used currently; kept for symmetry

        match (b_t, a_t) {
            (None, Some(t)) => out.push(Change::TransitionAdded {
                track: track_name.to_string(),
                between_before: Some(after_clips[*a_pos].1.into()),
                between_after: after_neighbor.clone(),
                name: t.name.clone(),
                duration: t.duration,
            }),
            (Some(t), None) => out.push(Change::TransitionRemoved {
                track: track_name.to_string(),
                between_before: Some(before_clips[*b_pos].1.into()),
                between_after: before_neighbor.clone(),
                name: t.name.clone(),
            }),
            _ => {}
        }
    }
}

/// For each clip in `clip_list` (in list order), return the transition that
/// immediately follows it in the track's children, if any.
fn transitions_after_each_clip(
    track: &Track,
    clip_list: &[(usize, &Clip)],
) -> Vec<Option<crate::model::Transition>> {
    let mut out = Vec::with_capacity(clip_list.len());
    for (clip_child_idx, _) in clip_list {
        let next = track.children.get(*clip_child_idx + 1);
        match next {
            Some(TrackChild::Transition(t)) => out.push(Some(t.clone())),
            _ => out.push(None),
        }
    }
    out
}

/// A serializable identity key for grouping clips. `None` means "this
/// clip cannot be identified at this fingerprint level" — skip it.
type Fingerprint = Option<String>;

fn strong_fingerprint(c: &Clip) -> Fingerprint {
    let media = c.media_reference.as_deref()?;
    let sr = c.source_range.as_ref()?;
    Some(format!(
        "{media}|{:.6}|{:.6}|{:.6}|{:.6}",
        sr.start_time.value, sr.start_time.rate, sr.duration.value, sr.duration.rate
    ))
}

fn weak_fingerprint(c: &Clip) -> Fingerprint {
    if !c.name.is_empty() {
        Some(format!("name:{}", c.name))
    } else {
        c.media_reference.as_ref().map(|m| format!("media:{m}"))
    }
}

/// Pair clips on each side that share a fingerprint, by closest position.
///
/// For each fingerprint that appears on both sides:
/// - List the unmatched before-positions sorted ascending
/// - List the unmatched after-positions sorted ascending
/// - Pair them in order. This way, the i-th occurrence of a clip in the
///   before-track maps to the i-th occurrence in the after-track,
///   producing the most natural mapping when a media file is reused
///   multiple times.
fn pair_by_fingerprint(
    before_clips: &[(usize, &Clip)],
    after_clips: &[(usize, &Clip)],
    before_used: &mut [bool],
    after_used: &mut [bool],
    fingerprint: impl Fn(&Clip) -> Fingerprint,
) -> Vec<(usize, usize)> {
    let mut groups: std::collections::BTreeMap<String, (Vec<usize>, Vec<usize>)> =
        Default::default();

    for (b_pos, (_, b_clip)) in before_clips.iter().enumerate() {
        if before_used[b_pos] {
            continue;
        }
        if let Some(fp) = fingerprint(b_clip) {
            groups.entry(fp).or_default().0.push(b_pos);
        }
    }
    for (a_pos, (_, a_clip)) in after_clips.iter().enumerate() {
        if after_used[a_pos] {
            continue;
        }
        if let Some(fp) = fingerprint(a_clip) {
            groups.entry(fp).or_default().1.push(a_pos);
        }
    }

    let mut out = Vec::new();
    for (_, (befores, afters)) in groups {
        for (b_pos, a_pos) in befores.into_iter().zip(afters) {
            before_used[b_pos] = true;
            after_used[a_pos] = true;
            out.push((b_pos, a_pos));
        }
    }
    out
}

/// Given matches sorted ascending by before-position, return the
/// after-positions that form a longest strictly-increasing subsequence.
/// Those are the pairs whose relative order was preserved, i.e., the
/// clips that didn't actually move — their absolute indices may have
/// shifted only because of insertions/removals around them.
fn longest_increasing_after_indices(matches: &[(usize, usize)]) -> Vec<usize> {
    let n = matches.len();
    if n == 0 {
        return Vec::new();
    }
    let after: Vec<usize> = matches.iter().map(|(_, a)| *a).collect();
    // Patience sorting / LIS with predecessors so we can reconstruct.
    let mut tails: Vec<usize> = Vec::new(); // indices into `after`
    let mut prev: Vec<Option<usize>> = vec![None; n];
    for i in 0..n {
        let v = after[i];
        // Binary search for the first tail with after[tail] >= v.
        let mut lo = 0usize;
        let mut hi = tails.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            if after[tails[mid]] < v {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo > 0 {
            prev[i] = Some(tails[lo - 1]);
        }
        if lo == tails.len() {
            tails.push(i);
        } else {
            tails[lo] = i;
        }
    }
    // Reconstruct.
    let mut result = Vec::with_capacity(tails.len());
    let mut cursor = tails.last().copied();
    while let Some(i) = cursor {
        result.push(after[i]);
        cursor = prev[i];
    }
    result.reverse();
    result
}

fn time_ranges_equal(a: &TimeRange, b: &TimeRange) -> bool {
    rational_eq(&a.start_time, &b.start_time) && rational_eq(&a.duration, &b.duration)
}

/// Render a list of changes as a one-line commit message. The shape:
///
/// - 0 changes  → `"No semantic changes"`
/// - 1 change   → that change's verb phrase, e.g. `Trimmed "drone_shot_04" by 1.80s (in)`
/// - 2 changes  → `"<verb 1>, <verb 2>"`
/// - 3+ changes → `"5 edits: 2 trims, 1 move, 1 transition added, 1 effect change"`
///
/// `verb` here means a short label without quoted clip names, so the
/// summary stays readable at high counts. The 1-change and 2-change
/// branches keep the clip names because they're short enough to scan.
pub fn auto_message(changes: &[Change]) -> String {
    match changes.len() {
        0 => "No semantic changes".to_string(),
        1 => verb_phrase(&changes[0]),
        2 => format!("{}, {}", verb_phrase(&changes[0]), verb_phrase(&changes[1])),
        _ => summary_phrase(changes),
    }
}

fn verb_phrase(change: &Change) -> String {
    match change {
        Change::TrackAdded { name, .. } => format!("added track \"{name}\""),
        Change::TrackRemoved { name, .. } => format!("removed track \"{name}\""),
        Change::Trimmed { clip, before, after, .. } => {
            let in_delta = after.start_time.seconds() - before.start_time.seconds();
            let dur_delta = after.duration.seconds() - before.duration.seconds();
            let amount = if in_delta.abs() > 1e-6 {
                in_delta.abs()
            } else {
                dur_delta.abs()
            };
            let dir = if in_delta > 1e-6 || dur_delta < -1e-6 {
                "in"
            } else {
                "out"
            };
            format!("trimmed \"{}\" by {:.2}s ({dir})", clip.name, amount)
        }
        Change::Moved { clip, after_neighbor, before_neighbor, .. } => {
            if let Some(n) = after_neighbor {
                format!("moved \"{}\" before \"{}\"", clip.name, n.name)
            } else if let Some(n) = before_neighbor {
                format!("moved \"{}\" after \"{}\"", clip.name, n.name)
            } else {
                format!("moved \"{}\"", clip.name)
            }
        }
        Change::Added { clip, .. } => format!("added \"{}\"", clip.name),
        Change::Removed { clip, .. } => format!("removed \"{}\"", clip.name),
        Change::EffectsChanged { clip, before, after, .. } => {
            format!("effects on \"{}\" {}→{}", clip.name, before, after)
        }
        Change::Replaced { clip, .. } => format!("replaced media on \"{}\"", clip.name),
        Change::TransitionAdded { name, .. } => {
            if name.is_empty() {
                "added transition".to_string()
            } else {
                format!("added {name}")
            }
        }
        Change::TransitionRemoved { name, .. } => {
            if name.is_empty() {
                "removed transition".to_string()
            } else {
                format!("removed {name}")
            }
        }
    }
}

fn summary_phrase(changes: &[Change]) -> String {
    let mut trims = 0u32;
    let mut moves = 0u32;
    let mut adds = 0u32;
    let mut removes = 0u32;
    let mut replaces = 0u32;
    let mut effects = 0u32;
    let mut transitions_added = 0u32;
    let mut transitions_removed = 0u32;
    let mut tracks_added = 0u32;
    let mut tracks_removed = 0u32;

    for c in changes {
        match c {
            Change::Trimmed { .. } => trims += 1,
            Change::Moved { .. } => moves += 1,
            Change::Added { .. } => adds += 1,
            Change::Removed { .. } => removes += 1,
            Change::Replaced { .. } => replaces += 1,
            Change::EffectsChanged { .. } => effects += 1,
            Change::TransitionAdded { .. } => transitions_added += 1,
            Change::TransitionRemoved { .. } => transitions_removed += 1,
            Change::TrackAdded { .. } => tracks_added += 1,
            Change::TrackRemoved { .. } => tracks_removed += 1,
        }
    }

    let mut parts: Vec<String> = Vec::new();
    let push = |parts: &mut Vec<String>, n: u32, singular: &str, plural: &str| {
        if n > 0 {
            let label = if n == 1 { singular } else { plural };
            parts.push(format!("{n} {label}"));
        }
    };
    push(&mut parts, trims, "trim", "trims");
    push(&mut parts, moves, "move", "moves");
    push(&mut parts, adds, "addition", "additions");
    push(&mut parts, removes, "removal", "removals");
    push(&mut parts, replaces, "replacement", "replacements");
    push(&mut parts, effects, "effect change", "effect changes");
    push(&mut parts, transitions_added, "transition added", "transitions added");
    push(&mut parts, transitions_removed, "transition removed", "transitions removed");
    push(&mut parts, tracks_added, "track added", "tracks added");
    push(&mut parts, tracks_removed, "track removed", "tracks removed");

    let total = changes.len();
    let edits_word = if total == 1 { "edit" } else { "edits" };
    format!("{total} {edits_word}: {}", parts.join(", "))
}

fn rational_eq(a: &RationalTime, b: &RationalTime) -> bool {
    if (a.rate - b.rate).abs() < f64::EPSILON {
        (a.value - b.value).abs() < 1e-6
    } else {
        // Compare in seconds when rates differ.
        (a.seconds() - b.seconds()).abs() < 1e-6
    }
}

#[cfg(test)]
mod auto_message_tests {
    use super::*;
    use crate::model::TrackKind;

    fn tr_added(name: &str) -> Change {
        Change::TrackAdded {
            name: name.to_string(),
            kind: TrackKind::Audio,
        }
    }

    #[test]
    fn no_changes() {
        assert_eq!(auto_message(&[]), "No semantic changes");
    }

    #[test]
    fn one_change_uses_verb_phrase() {
        let m = auto_message(&[tr_added("A1")]);
        assert_eq!(m, "added track \"A1\"");
    }

    #[test]
    fn two_changes_joined_with_comma() {
        let m = auto_message(&[tr_added("V2"), tr_added("A1")]);
        assert_eq!(m, "added track \"V2\", added track \"A1\"");
    }

    #[test]
    fn three_changes_summarized() {
        let m = auto_message(&[tr_added("V2"), tr_added("A1"), tr_added("A2")]);
        assert_eq!(m, "3 edits: 3 tracks added");
    }
}
