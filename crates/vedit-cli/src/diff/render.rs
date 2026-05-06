use vedit_core::diff::{Change, ClipRef};
use vedit_core::model::{RationalTime, TimeRange};

pub fn render(changes: &[Change]) -> Vec<String> {
    changes.iter().map(render_one).collect()
}

fn render_one(change: &Change) -> String {
    match change {
        Change::TrackAdded { name, kind } => {
            format!("  Added {} track \"{}\"", track_kind_word(kind), name)
        }
        Change::TrackRemoved { name, kind } => {
            format!("  Removed {} track \"{}\"", track_kind_word(kind), name)
        }
        Change::Trimmed {
            clip,
            track: _,
            before,
            after,
        } => render_trim(clip, before, after),
        Change::Moved {
            clip,
            from_track: _,
            from_index,
            to_track: _,
            to_index,
            after_neighbor,
            before_neighbor,
        } => render_move(clip, *from_index, *to_index, after_neighbor, before_neighbor),
        Change::Added {
            clip,
            track,
            index,
        } => format!(
            "  Added \"{}\" to {} at position {}",
            clip_label(clip),
            track,
            index
        ),
        Change::Removed {
            clip,
            track,
            index,
        } => format!(
            "  Removed \"{}\" from {} at position {}",
            clip_label(clip),
            track,
            index
        ),
        Change::EffectsChanged {
            clip,
            track: _,
            before,
            after,
        } => format!(
            "  Effects on \"{}\" changed ({} → {})",
            clip_label(clip),
            before,
            after
        ),
        Change::Replaced {
            clip,
            track: _,
            before_media,
            after_media,
        } => render_replaced(clip, before_media, after_media),
        Change::TransitionAdded {
            track: _,
            between_before,
            between_after,
            name,
            duration,
        } => render_transition_added(between_before, between_after, name, duration),
        Change::TransitionRemoved {
            track: _,
            between_before,
            between_after,
            name,
        } => render_transition_removed(between_before, between_after, name),
    }
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
        // Pure shift in the source.
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
