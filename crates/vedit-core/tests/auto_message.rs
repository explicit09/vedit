//! Integration tests for auto_message — the function that produces a
//! commit message from a list of Changes when the user doesn't provide
//! one.

use serde_json::json;
use vedit_core::diff::{auto_message, diff};
use vedit_core::otio;

fn timeline_with_clip_count(n: usize) -> serde_json::Value {
    let clips: Vec<serde_json::Value> = (0..n)
        .map(|i| {
            json!({
                "OTIO_SCHEMA": "Clip.2",
                "name": format!("clip_{i}"),
                "source_range": {
                    "OTIO_SCHEMA": "TimeRange.1",
                    "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0.0, "rate": 24.0 },
                    "duration":   { "OTIO_SCHEMA": "RationalTime.1", "value": 24.0, "rate": 24.0 }
                },
                "media_reference": {
                    "OTIO_SCHEMA": "ExternalReference.1",
                    "target_url": format!("media://clip_{i}.mov")
                },
                "effects": [],
                "metadata": {}
            })
        })
        .collect();
    json!({
        "OTIO_SCHEMA": "Timeline.1",
        "name": "doc",
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "name": "tracks",
            "children": [
                {
                    "OTIO_SCHEMA": "Track.1",
                    "name": "V1",
                    "kind": "Video",
                    "children": clips
                }
            ]
        }
    })
}

fn diff_value(before: &serde_json::Value, after: &serde_json::Value) -> Vec<vedit_core::diff::Change> {
    let b = otio::parse_timeline(before).unwrap();
    let a = otio::parse_timeline(after).unwrap();
    diff(&b, &a)
}

#[test]
fn no_changes_message() {
    let same = timeline_with_clip_count(3);
    let changes = diff_value(&same, &same);
    assert_eq!(auto_message(&changes), "No semantic changes");
}

#[test]
fn one_added_clip_uses_verb_phrase() {
    let before = timeline_with_clip_count(3);
    let after = timeline_with_clip_count(4);
    let changes = diff_value(&before, &after);
    let m = auto_message(&changes);
    assert!(m.starts_with("added \""), "unexpected message: {m}");
    assert!(m.contains("clip_3"), "should name the new clip: {m}");
}

#[test]
fn two_changes_joined_with_comma() {
    let before = timeline_with_clip_count(3);
    let after = timeline_with_clip_count(5);
    let changes = diff_value(&before, &after);
    assert_eq!(changes.len(), 2);
    let m = auto_message(&changes);
    assert!(m.contains(", "), "two-change message should be comma-joined: {m}");
    assert!(m.contains("clip_3"));
    assert!(m.contains("clip_4"));
}

#[test]
fn many_changes_use_summary_with_counts() {
    let before = timeline_with_clip_count(2);
    let after = timeline_with_clip_count(7);
    let changes = diff_value(&before, &after);
    assert_eq!(changes.len(), 5);
    let m = auto_message(&changes);
    assert!(m.starts_with("5 edits"), "summary should lead with the count: {m}");
    assert!(m.contains("addition"), "should mention additions: {m}");
}
