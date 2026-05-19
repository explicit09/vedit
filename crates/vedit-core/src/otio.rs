//! Read OTIO JSON into vedit's internal model.
//!
//! We aim for forward-compatibility: unknown schema versions and unknown
//! fields are tolerated. The parser fails only when the document is
//! structurally not an OTIO timeline.

use crate::model::{
    Clip, Effect, Gap, RationalTime, TimeRange, Timeline, Track, TrackChild, TrackKind,
};
use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::path::Path;

pub fn load(path: &Path) -> Result<Timeline> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing {} as JSON", path.display()))?;
    parse_timeline(&value).with_context(|| format!("interpreting {}", path.display()))
}

pub fn parse_timeline(value: &Value) -> Result<Timeline> {
    let map = value
        .as_object()
        .ok_or_else(|| anyhow!("expected an object at the OTIO root"))?;
    let schema = map
        .get("OTIO_SCHEMA")
        .and_then(|s| s.as_str())
        .unwrap_or("");
    if !schema.starts_with("Timeline.") {
        return Err(anyhow!("expected Timeline root, got {schema}"));
    }
    let name = map
        .get("name")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    let tracks = map
        .get("tracks")
        .ok_or_else(|| anyhow!("Timeline has no `tracks`"))?;
    let parsed_tracks = parse_stack_children(tracks)?;

    Ok(Timeline {
        name,
        tracks: parsed_tracks,
    })
}

fn parse_stack_children(stack: &Value) -> Result<Vec<Track>> {
    let map = stack
        .as_object()
        .ok_or_else(|| anyhow!("expected the top stack to be an object"))?;
    let children = map
        .get("children")
        .and_then(|c| c.as_array())
        .ok_or_else(|| anyhow!("stack has no `children` array"))?;
    let mut tracks = Vec::with_capacity(children.len());
    for child in children {
        if is_schema(child, "Track") {
            tracks.push(parse_track(child)?);
        }
        // Other top-level child types (nested Stacks, etc.) are skipped for
        // v0.1. We extend the model later if real-world inputs need them.
    }
    Ok(tracks)
}

fn parse_track(value: &Value) -> Result<Track> {
    let map = value
        .as_object()
        .ok_or_else(|| anyhow!("track is not an object"))?;
    let name = map
        .get("name")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    let kind = map
        .get("kind")
        .and_then(|s| s.as_str())
        .map(parse_track_kind)
        .unwrap_or(TrackKind::Other);
    let children_array = map
        .get("children")
        .and_then(|c| c.as_array())
        .map(|v| v.as_slice())
        .unwrap_or(&[]);

    let mut children = Vec::with_capacity(children_array.len());
    for child in children_array {
        if let Some(c) = parse_track_child(child) {
            children.push(c);
        }
    }

    Ok(Track {
        name,
        kind,
        children,
    })
}

fn parse_track_kind(s: &str) -> TrackKind {
    match s.to_ascii_lowercase().as_str() {
        "video" => TrackKind::Video,
        "audio" => TrackKind::Audio,
        _ => TrackKind::Other,
    }
}

fn parse_track_child(value: &Value) -> Option<TrackChild> {
    if is_schema(value, "Clip") {
        Some(TrackChild::Clip(parse_clip(value)))
    } else if is_schema(value, "Transition") {
        Some(TrackChild::Transition(parse_transition(value)))
    } else if is_schema(value, "Gap") || is_schema(value, "Filler") {
        Some(TrackChild::Gap(parse_gap(value)))
    } else {
        None
    }
}

fn parse_clip(value: &Value) -> Clip {
    let map = value.as_object().cloned().unwrap_or_default();
    let name = map
        .get("name")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    let source_range = map.get("source_range").and_then(parse_time_range);
    let media_reference = map.get("media_reference").and_then(parse_media_reference);
    let effects = map
        .get("effects")
        .and_then(|e| e.as_array())
        .map(|effects| effects.iter().map(parse_effect).collect())
        .unwrap_or_default();
    Clip {
        name,
        media_reference,
        source_range,
        effects,
    }
}

fn parse_effect(value: &Value) -> Effect {
    let map = value.as_object().cloned().unwrap_or_default();
    let name = map
        .get("name")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    let effect_name = map
        .get("effect_name")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());
    let metadata = map
        .get("metadata")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));
    Effect {
        name,
        effect_name,
        metadata,
    }
}

fn parse_transition(value: &Value) -> crate::model::Transition {
    let map = value.as_object().cloned().unwrap_or_default();
    let name = map
        .get("name")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    // OTIO transitions store duration as in_offset + out_offset, both
    // RationalTime. Sum them to get the total transition duration.
    let in_offset = map.get("in_offset").and_then(parse_rational_time);
    let out_offset = map.get("out_offset").and_then(parse_rational_time);
    let duration = match (in_offset, out_offset) {
        (Some(a), Some(b)) if (a.rate - b.rate).abs() < f64::EPSILON => Some(RationalTime {
            value: a.value + b.value,
            rate: a.rate,
        }),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        _ => None,
    };
    crate::model::Transition { name, duration }
}

fn parse_gap(value: &Value) -> Gap {
    let map = value.as_object().cloned().unwrap_or_default();
    let duration = map
        .get("source_range")
        .and_then(parse_time_range)
        .map(|tr| tr.duration);
    Gap { duration }
}

fn parse_media_reference(value: &Value) -> Option<String> {
    let map = value.as_object()?;
    if let Some(url) = map.get("target_url").and_then(|s| s.as_str())
        && !url.is_empty()
    {
        return Some(url.to_string());
    }
    // Some OTIO files use ExternalReference with `target_url`, others use
    // `MissingReference` with metadata, others use `GeneratorReference`. We
    // fall back to a reproducible identity string so generators still match.
    if let Some(name) = map.get("name").and_then(|s| s.as_str())
        && !name.is_empty()
    {
        return Some(format!("ref-by-name:{name}"));
    }
    None
}

fn parse_time_range(value: &Value) -> Option<TimeRange> {
    let map = value.as_object()?;
    let start_time = map.get("start_time").and_then(parse_rational_time)?;
    let duration = map.get("duration").and_then(parse_rational_time)?;
    Some(TimeRange {
        start_time,
        duration,
    })
}

fn parse_rational_time(value: &Value) -> Option<RationalTime> {
    let map = value.as_object()?;
    let value_n = map.get("value").and_then(|n| n.as_f64())?;
    let rate = map.get("rate").and_then(|n| n.as_f64()).unwrap_or(1.0);
    Some(RationalTime {
        value: value_n,
        rate,
    })
}

fn is_schema(value: &Value, prefix: &str) -> bool {
    value
        .as_object()
        .and_then(|m| m.get("OTIO_SCHEMA"))
        .and_then(|s| s.as_str())
        .map(|s| s.split('.').next().unwrap_or("") == prefix)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rt(value: f64, rate: f64) -> Value {
        json!({
            "OTIO_SCHEMA": "RationalTime.1",
            "value": value,
            "rate": rate
        })
    }

    #[test]
    fn parses_minimal_timeline() {
        let v = json!({
            "OTIO_SCHEMA": "Timeline.1",
            "name": "test",
            "tracks": {
                "OTIO_SCHEMA": "Stack.1",
                "children": [
                    {
                        "OTIO_SCHEMA": "Track.1",
                        "name": "V1",
                        "kind": "Video",
                        "children": [
                            {
                                "OTIO_SCHEMA": "Clip.2",
                                "name": "shot_a",
                                "source_range": {
                                    "OTIO_SCHEMA": "TimeRange.1",
                                    "start_time": rt(10.0, 24.0),
                                    "duration": rt(48.0, 24.0),
                                },
                                "media_reference": {
                                    "OTIO_SCHEMA": "ExternalReference.1",
                                    "target_url": "file:///media/shot_a.mov"
                                }
                            }
                        ]
                    }
                ]
            }
        });
        let tl = parse_timeline(&v).unwrap();
        assert_eq!(tl.name, "test");
        assert_eq!(tl.tracks.len(), 1);
        let track = &tl.tracks[0];
        assert_eq!(track.kind, TrackKind::Video);
        assert_eq!(track.children.len(), 1);
        match &track.children[0] {
            TrackChild::Clip(c) => {
                assert_eq!(c.name, "shot_a");
                assert_eq!(
                    c.media_reference.as_deref(),
                    Some("file:///media/shot_a.mov")
                );
                let sr = c.source_range.as_ref().unwrap();
                assert_eq!(sr.start_time.value, 10.0);
                assert_eq!(sr.duration.value, 48.0);
            }
            _ => panic!("expected clip"),
        }
    }
}
