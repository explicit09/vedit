use vedit_core::diff::{Change, ClipRef};
use vedit_core::model::{RationalTime, TimeRange};

pub fn render(changes: &[Change]) -> Vec<String> {
    let collapsed = collapse_synced_pairs(changes);
    collapsed
        .iter()
        .map(|(c, synced)| render_one(c, *synced))
        .collect()
}

/// Detect video/audio mirror pairs and collapse them. Two changes are
/// mirrors when their non-track content matches structurally (same op,
/// same clip name, same numbers, etc.) and they live on different
/// tracks. We keep the first occurrence and tag it `synced=true`.
///
/// JSON output sees the full uncollapsed list; this collapse only affects
/// human prose.
fn collapse_synced_pairs(changes: &[Change]) -> Vec<(Change, bool)> {
    let mut out: Vec<(Change, bool)> = Vec::with_capacity(changes.len());
    let mut consumed = vec![false; changes.len()];

    for (i, c) in changes.iter().enumerate() {
        if consumed[i] {
            continue;
        }
        let mut synced = false;
        for (j, other) in changes.iter().enumerate().skip(i + 1) {
            if consumed[j] {
                continue;
            }
            if mirrors(c, other) {
                consumed[j] = true;
                synced = true;
                // Don't break — a track-add and another mirror could both
                // collapse onto one entry, though in practice we'll only
                // see one mirror per change.
                break;
            }
        }
        out.push((c.clone(), synced));
    }
    out
}

fn mirrors(a: &Change, b: &Change) -> bool {
    use Change::*;
    match (a, b) {
        (
            Trimmed {
                clip: ca,
                before: ba,
                after: aa,
                track: ta,
            },
            Trimmed {
                clip: cb,
                before: bb,
                after: ab,
                track: tb,
            },
        ) => ta != tb && ca.name == cb.name && ba == bb && aa == ab,
        (
            Moved {
                clip: ca,
                after_neighbor: na,
                before_neighbor: pa,
                ..
            },
            Moved {
                clip: cb,
                after_neighbor: nb,
                before_neighbor: pb,
                ..
            },
        ) => ca.name == cb.name && neighbor_names_match(na, nb) && neighbor_names_match(pa, pb),
        (
            Added {
                clip: ca,
                track: ta,
                ..
            },
            Added {
                clip: cb,
                track: tb,
                ..
            },
        ) => ta != tb && ca.name == cb.name,
        (
            Removed {
                clip: ca,
                track: ta,
                ..
            },
            Removed {
                clip: cb,
                track: tb,
                ..
            },
        ) => ta != tb && ca.name == cb.name,
        (
            EffectsChanged {
                clip: ca,
                before: ba,
                after: aa,
                track: ta,
            },
            EffectsChanged {
                clip: cb,
                before: bb,
                after: ab,
                track: tb,
            },
        ) => ta != tb && ca.name == cb.name && ba == bb && aa == ab,
        (
            TransitionAdded {
                between_before: ba1,
                between_after: ba2,
                duration: da,
                track: ta,
                ..
            },
            TransitionAdded {
                between_before: bb1,
                between_after: bb2,
                duration: db,
                track: tb,
                ..
            },
        ) => {
            ta != tb && neighbor_names_match(ba1, bb1) && neighbor_names_match(ba2, bb2) && da == db
        }
        (
            TransitionRemoved {
                between_before: ba1,
                between_after: ba2,
                track: ta,
                ..
            },
            TransitionRemoved {
                between_before: bb1,
                between_after: bb2,
                track: tb,
                ..
            },
        ) => ta != tb && neighbor_names_match(ba1, bb1) && neighbor_names_match(ba2, bb2),
        (
            TransitionChanged {
                between_before: ba1,
                between_after: ba2,
                before_name: bna,
                after_name: ana,
                before_duration: bda,
                after_duration: ada,
                track: ta,
                ..
            },
            TransitionChanged {
                between_before: bb1,
                between_after: bb2,
                before_name: bnb,
                after_name: anb,
                before_duration: bdb,
                after_duration: adb,
                track: tb,
                ..
            },
        ) => {
            ta != tb
                && neighbor_names_match(ba1, bb1)
                && neighbor_names_match(ba2, bb2)
                && bna == bnb
                && ana == anb
                && bda == bdb
                && ada == adb
        }
        _ => false,
    }
}

fn neighbor_names_match(a: &Option<ClipRef>, b: &Option<ClipRef>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => x.name == y.name,
        (None, None) => true,
        _ => false,
    }
}

fn render_one(change: &Change, synced: bool) -> String {
    let suffix = if synced { " (with synced audio)" } else { "" };
    match change {
        Change::TrackAdded { name, kind } => {
            format!("  Added {} track \"{}\"", track_kind_word(kind), name)
        }
        Change::TrackRemoved { name, kind } => {
            format!("  Removed {} track \"{}\"", track_kind_word(kind), name)
        }
        Change::Trimmed {
            clip,
            before,
            after,
            ..
        } => {
            format!("{}{}", render_trim(clip, before, after), suffix)
        }
        Change::Moved {
            clip,
            from_index,
            to_index,
            after_neighbor,
            before_neighbor,
            ..
        } => format!(
            "{}{}",
            render_move(
                clip,
                *from_index,
                *to_index,
                after_neighbor,
                before_neighbor
            ),
            suffix
        ),
        Change::Added { clip, track, index } => format!(
            "  Added \"{}\" to {} at position {}{}",
            clip_label(clip),
            track,
            index,
            suffix
        ),
        Change::Removed { clip, track, index } => format!(
            "  Removed \"{}\" from {} at position {}{}",
            clip_label(clip),
            track,
            index,
            suffix
        ),
        Change::EffectsChanged {
            clip,
            before,
            after,
            ..
        } => format!(
            "  Effects on \"{}\" changed ({} → {}){}",
            clip_label(clip),
            effect_summary(before),
            effect_summary(after),
            suffix
        ),
        Change::Replaced {
            clip,
            before_media,
            after_media,
            ..
        } => format!(
            "{}{}",
            render_replaced(clip, before_media, after_media),
            suffix
        ),
        Change::TransitionAdded {
            between_before,
            between_after,
            name,
            duration,
            ..
        } => format!(
            "{}{}",
            render_transition_added(between_before, between_after, name, duration),
            suffix
        ),
        Change::TransitionRemoved {
            between_before,
            between_after,
            name,
            ..
        } => format!(
            "{}{}",
            render_transition_removed(between_before, between_after, name),
            suffix
        ),
        Change::TransitionChanged {
            between_before,
            between_after,
            before_name,
            after_name,
            before_duration,
            after_duration,
            ..
        } => format!(
            "{}{}",
            render_transition_changed(
                between_before,
                between_after,
                before_name,
                after_name,
                before_duration,
                after_duration
            ),
            suffix
        ),
    }
}

fn effect_summary(effects: &[vedit_core::model::Effect]) -> String {
    if effects.is_empty() {
        return "none".to_string();
    }
    effects
        .iter()
        .map(|effect| {
            if effect.name.is_empty() {
                effect
                    .effect_name
                    .clone()
                    .unwrap_or_else(|| "unnamed".to_string())
            } else {
                effect.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_replaced(
    clip: &ClipRef,
    before_media: &Option<String>,
    after_media: &Option<String>,
) -> String {
    match (before_media, after_media) {
        (Some(b), Some(a)) => format!(
            "  Replaced media on \"{}\" ({} → {})",
            clip_label(clip),
            short_media(b),
            short_media(a)
        ),
        _ => format!("  Replaced media on \"{}\"", clip_label(clip)),
    }
}

fn short_media(url: &str) -> &str {
    url.rsplit('/').next().unwrap_or(url)
}

fn render_move(
    clip: &ClipRef,
    from_index: usize,
    to_index: usize,
    after_neighbor: &Option<ClipRef>,
    before_neighbor: &Option<ClipRef>,
) -> String {
    if let Some(n) = after_neighbor {
        return format!(
            "  Moved \"{}\" before \"{}\"",
            clip_label(clip),
            clip_label(n)
        );
    }
    if let Some(n) = before_neighbor {
        return format!(
            "  Moved \"{}\" after \"{}\"",
            clip_label(clip),
            clip_label(n)
        );
    }
    format!(
        "  Moved \"{}\" from position {} to position {}",
        clip_label(clip),
        from_index,
        to_index
    )
}

fn render_trim(clip: &ClipRef, before: &TimeRange, after: &TimeRange) -> String {
    let in_delta_sec = after.start_time.seconds() - before.start_time.seconds();
    let dur_delta_sec = after.duration.seconds() - before.duration.seconds();

    if in_delta_sec.abs() > 1e-6 && dur_delta_sec.abs() < 1e-6 {
        return format!(
            "  Shifted \"{}\" source by {}",
            clip_label(clip),
            signed_seconds(in_delta_sec)
        );
    }

    if in_delta_sec.abs() > 1e-6 {
        let direction = if in_delta_sec > 0.0 { "in" } else { "out" };
        return format!(
            "  Trimmed \"{}\" by {} ({})",
            clip_label(clip),
            unsigned_seconds(in_delta_sec.abs()),
            direction
        );
    }

    if dur_delta_sec.abs() > 1e-6 {
        let direction = if dur_delta_sec < 0.0 { "in" } else { "out" };
        return format!(
            "  Trimmed \"{}\" by {} ({})",
            clip_label(clip),
            unsigned_seconds(dur_delta_sec.abs()),
            direction
        );
    }

    format!("  Re-ranged \"{}\"", clip_label(clip))
}

fn render_transition_added(
    between_before: &Option<ClipRef>,
    between_after: &Option<ClipRef>,
    name: &str,
    duration: &Option<RationalTime>,
) -> String {
    let endpoints = endpoints_phrase(between_before, between_after);
    let dur = duration
        .map(|d| format!(" ({:.0} frames)", d.frames()))
        .unwrap_or_default();
    let label = if name.is_empty() {
        "transition".to_string()
    } else {
        name.to_string()
    };
    format!("  Added {} {}{}", label, endpoints, dur)
}

fn render_transition_removed(
    between_before: &Option<ClipRef>,
    between_after: &Option<ClipRef>,
    name: &str,
) -> String {
    let endpoints = endpoints_phrase(between_before, between_after);
    let label = if name.is_empty() {
        "transition".to_string()
    } else {
        name.to_string()
    };
    format!("  Removed {} {}", label, endpoints)
}

fn render_transition_changed(
    before: &Option<ClipRef>,
    after: &Option<ClipRef>,
    before_name: &str,
    after_name: &str,
    before_duration: &Option<RationalTime>,
    after_duration: &Option<RationalTime>,
) -> String {
    let left = before.as_ref().map(clip_label).unwrap_or("start");
    let right = after.as_ref().map(clip_label).unwrap_or("end");
    format!(
        "  Changed transition between \"{}\" and \"{}\" ({} {} → {} {})",
        left,
        right,
        transition_name(before_name),
        fmt_duration(before_duration),
        transition_name(after_name),
        fmt_duration(after_duration)
    )
}

fn transition_name(name: &str) -> &str {
    if name.is_empty() { "transition" } else { name }
}

fn fmt_duration(duration: &Option<RationalTime>) -> String {
    duration
        .map(|d| format!("({:.0} frames)", d.frames()))
        .unwrap_or_else(|| "(unknown duration)".to_string())
}

fn endpoints_phrase(a: &Option<ClipRef>, b: &Option<ClipRef>) -> String {
    match (a, b) {
        (Some(x), Some(y)) => format!("between \"{}\" and \"{}\"", clip_label(x), clip_label(y)),
        (Some(x), None) => format!("after \"{}\"", clip_label(x)),
        (None, Some(y)) => format!("before \"{}\"", clip_label(y)),
        (None, None) => "in track".to_string(),
    }
}

fn clip_label(clip: &ClipRef) -> &str {
    if !clip.name.is_empty() {
        &clip.name
    } else if let Some(m) = &clip.media_reference {
        m
    } else {
        "(unnamed)"
    }
}

fn track_kind_word(kind: &vedit_core::model::TrackKind) -> &'static str {
    match kind {
        vedit_core::model::TrackKind::Video => "video",
        vedit_core::model::TrackKind::Audio => "audio",
        vedit_core::model::TrackKind::Other => "",
    }
}

fn signed_seconds(s: f64) -> String {
    let sign = if s >= 0.0 { "+" } else { "-" };
    format!("{}{:.2}s", sign, s.abs())
}

fn unsigned_seconds(s: f64) -> String {
    format!("{:.2}s", s)
}
