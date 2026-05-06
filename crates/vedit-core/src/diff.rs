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

    // Strong-then-weak two-pass match.
    let mut before_used = vec![false; before_clips.len()];
    let mut after_used = vec![false; after_clips.len()];
    let mut matches: Vec<(usize, usize)> = Vec::new(); // (before_pos_in_list, after_pos_in_list)

    // Pass 1: strong fingerprint (media + source_range exact match).
    for (a_pos, (_, a_clip)) in after_clips.iter().enumerate() {
        for (b_pos, (_, b_clip)) in before_clips.iter().enumerate() {
            if before_used[b_pos] {
                continue;
            }
            if strong_match(b_clip, a_clip) {
                before_used[b_pos] = true;
                after_used[a_pos] = true;
                matches.push((b_pos, a_pos));
                break;
            }
        }
    }

    // Pass 2: weak fingerprint (media + name match) for leftovers.
    for (a_pos, (_, a_clip)) in after_clips.iter().enumerate() {
        if after_used[a_pos] {
            continue;
        }
        for (b_pos, (_, b_clip)) in before_clips.iter().enumerate() {
            if before_used[b_pos] {
                continue;
            }
            if weak_match(b_clip, a_clip) {
                before_used[b_pos] = true;
                after_used[a_pos] = true;
                matches.push((b_pos, a_pos));
                break;
            }
        }
    }

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

        // Move detection uses clip-list positions, not full-children positions.
        // That way an inserted transition doesn't register as a move.
        if b_pos != a_pos {
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

fn strong_match(a: &Clip, b: &Clip) -> bool {
    if a.media_reference.is_none() || b.media_reference.is_none() {
        return false;
    }
    if a.media_reference != b.media_reference {
        return false;
    }
    match (a.source_range, b.source_range) {
        (Some(x), Some(y)) => time_ranges_equal(&x, &y),
        _ => false,
    }
}

fn weak_match(a: &Clip, b: &Clip) -> bool {
    if a.media_reference.is_some() && a.media_reference == b.media_reference {
        return true;
    }
    if !a.name.is_empty() && a.name == b.name {
        return true;
    }
    false
}

fn time_ranges_equal(a: &TimeRange, b: &TimeRange) -> bool {
    rational_eq(&a.start_time, &b.start_time) && rational_eq(&a.duration, &b.duration)
}

fn rational_eq(a: &RationalTime, b: &RationalTime) -> bool {
    if (a.rate - b.rate).abs() < f64::EPSILON {
        (a.value - b.value).abs() < 1e-6
    } else {
        // Compare in seconds when rates differ.
        (a.seconds() - b.seconds()).abs() < 1e-6
    }
}
