//! The corpus: pairs of OTIO documents with known semantic deltas.
//!
//! Each test builds a `before` and `after` OTIO JSON document, runs the
//! diff, and asserts the expected `Change` variants. These are the
//! regression tests for the engine and the source of truth for what
//! "semantic diff" means in vedit.

use serde_json::{Value, json};
use vedit_core::diff::{Change, diff};
use vedit_core::otio;

// --- helpers -----------------------------------------------------------

fn rt(value: f64, rate: f64) -> Value {
    json!({
        "OTIO_SCHEMA": "RationalTime.1",
        "value": value,
        "rate": rate
    })
}

fn time_range(start: f64, dur: f64, rate: f64) -> Value {
    json!({
        "OTIO_SCHEMA": "TimeRange.1",
        "start_time": rt(start, rate),
        "duration": rt(dur, rate),
    })
}

fn external_ref(url: &str) -> Value {
    json!({
        "OTIO_SCHEMA": "ExternalReference.1",
        "target_url": url,
    })
}

fn clip(name: &str, media: &str, src_start: f64, src_dur: f64) -> Value {
    json!({
        "OTIO_SCHEMA": "Clip.2",
        "name": name,
        "source_range": time_range(src_start, src_dur, 24.0),
        "media_reference": external_ref(media),
        "effects": [],
        "markers": [],
        "metadata": {}
    })
}

fn effect(name: &str, metadata: Value) -> Value {
    json!({
        "OTIO_SCHEMA": "Effect.1",
        "name": name,
        "metadata": metadata,
    })
}

fn effect_with_effect_name(effect_name: &str) -> Value {
    json!({
        "OTIO_SCHEMA": "Effect.1",
        "name": "",
        "effect_name": effect_name,
        "metadata": {},
    })
}

fn clip_with_effects(
    name: &str,
    media: &str,
    src_start: f64,
    src_dur: f64,
    effects: Vec<Value>,
) -> Value {
    json!({
        "OTIO_SCHEMA": "Clip.2",
        "name": name,
        "source_range": time_range(src_start, src_dur, 24.0),
        "media_reference": external_ref(media),
        "effects": effects,
        "markers": [],
        "metadata": {}
    })
}

fn transition(name: &str, in_off: f64, out_off: f64) -> Value {
    json!({
        "OTIO_SCHEMA": "Transition.1",
        "name": name,
        "in_offset": rt(in_off, 24.0),
        "out_offset": rt(out_off, 24.0),
        "metadata": {}
    })
}

fn track(name: &str, kind: &str, children: Vec<Value>) -> Value {
    json!({
        "OTIO_SCHEMA": "Track.1",
        "name": name,
        "kind": kind,
        "children": children,
        "metadata": {}
    })
}

fn timeline(name: &str, tracks: Vec<Value>) -> Value {
    json!({
        "OTIO_SCHEMA": "Timeline.1",
        "name": name,
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "name": "tracks",
            "children": tracks,
            "metadata": {}
        }
    })
}

fn run_diff(before: Value, after: Value) -> Vec<Change> {
    let b = otio::parse_timeline(&before).expect("before parses");
    let a = otio::parse_timeline(&after).expect("after parses");
    diff(&b, &a)
}

// --- corpus tests -------------------------------------------------------

#[test]
fn case_01_clip_trimmed_in() {
    let before = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip("drone_shot_04", "media://drone.mov", 0.0, 48.0)],
        )],
    );
    let after = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip("drone_shot_04", "media://drone.mov", 12.0, 36.0)],
        )],
    );
    let changes = run_diff(before, after);
    assert_eq!(changes.len(), 1, "{:#?}", changes);
    match &changes[0] {
        Change::Trimmed { clip, .. } => assert_eq!(clip.name, "drone_shot_04"),
        other => panic!("expected Trimmed, got {:?}", other),
    }
}

#[test]
fn case_02_clip_trimmed_out() {
    let before = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip("interview", "media://interview.mov", 0.0, 100.0)],
        )],
    );
    let after = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip("interview", "media://interview.mov", 0.0, 80.0)],
        )],
    );
    let changes = run_diff(before, after);
    assert_eq!(changes.len(), 1, "{:#?}", changes);
    matches!(&changes[0], Change::Trimmed { .. });
}

#[test]
fn case_03_clip_moved() {
    let before = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![
                clip("a", "media://a.mov", 0.0, 24.0),
                clip("b", "media://b.mov", 0.0, 24.0),
                clip("c", "media://c.mov", 0.0, 24.0),
            ],
        )],
    );
    // Reorder: c is now first.
    let after = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![
                clip("c", "media://c.mov", 0.0, 24.0),
                clip("a", "media://a.mov", 0.0, 24.0),
                clip("b", "media://b.mov", 0.0, 24.0),
            ],
        )],
    );
    let changes = run_diff(before, after);
    let moved: Vec<_> = changes
        .iter()
        .filter(|c| matches!(c, Change::Moved { .. }))
        .collect();
    assert!(
        !moved.is_empty(),
        "expected at least one Moved: {:#?}",
        changes
    );
    // c moved from index 2 -> 0.
    let c_moved = moved.iter().any(|c| {
        matches!(
            c,
            Change::Moved { clip, from_index, to_index, .. }
                if clip.name == "c" && *from_index == 2 && *to_index == 0
        )
    });
    assert!(c_moved, "expected c moved 2->0: {:#?}", moved);
}

#[test]
fn case_04_clip_added() {
    let before = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip("a", "media://a.mov", 0.0, 24.0)],
        )],
    );
    let after = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![
                clip("a", "media://a.mov", 0.0, 24.0),
                clip("b_new", "media://b.mov", 0.0, 24.0),
            ],
        )],
    );
    let changes = run_diff(before, after);
    assert_eq!(changes.len(), 1, "{:#?}", changes);
    match &changes[0] {
        Change::Added { clip, .. } => assert_eq!(clip.name, "b_new"),
        other => panic!("expected Added, got {:?}", other),
    }
}

#[test]
fn case_05_clip_removed() {
    let before = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![
                clip("a", "media://a.mov", 0.0, 24.0),
                clip("b", "media://b.mov", 0.0, 24.0),
            ],
        )],
    );
    let after = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip("a", "media://a.mov", 0.0, 24.0)],
        )],
    );
    let changes = run_diff(before, after);
    assert_eq!(changes.len(), 1, "{:#?}", changes);
    match &changes[0] {
        Change::Removed { clip, .. } => assert_eq!(clip.name, "b"),
        other => panic!("expected Removed, got {:?}", other),
    }
}

#[test]
fn case_06_clip_replaced() {
    // Same name and position, different media URL. The matcher should
    // weak-match by name and then emit a Replaced change to surface the
    // media swap.
    let before = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip("intro", "media://intro_v1.mov", 0.0, 24.0)],
        )],
    );
    let after = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip("intro", "media://intro_v2.mov", 0.0, 24.0)],
        )],
    );
    let changes = run_diff(before, after);
    let replaced: Vec<_> = changes
        .iter()
        .filter(|c| matches!(c, Change::Replaced { .. }))
        .collect();
    assert_eq!(replaced.len(), 1, "{:#?}", changes);
    match replaced[0] {
        Change::Replaced {
            clip,
            before_media,
            after_media,
            ..
        } => {
            assert_eq!(clip.name, "intro");
            assert_eq!(before_media.as_deref(), Some("media://intro_v1.mov"));
            assert_eq!(after_media.as_deref(), Some("media://intro_v2.mov"));
        }
        _ => unreachable!(),
    }
}

#[test]
fn case_07_transition_added() {
    let before = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![
                clip("a", "media://a.mov", 0.0, 24.0),
                clip("b", "media://b.mov", 0.0, 24.0),
            ],
        )],
    );
    let after = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![
                clip("a", "media://a.mov", 0.0, 24.0),
                transition("crossfade", 6.0, 6.0),
                clip("b", "media://b.mov", 0.0, 24.0),
            ],
        )],
    );
    let changes = run_diff(before, after);
    let added: Vec<_> = changes
        .iter()
        .filter(|c| matches!(c, Change::TransitionAdded { .. }))
        .collect();
    assert_eq!(added.len(), 1, "{:#?}", changes);
}

#[test]
fn case_08_transition_removed() {
    let before = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![
                clip("a", "media://a.mov", 0.0, 24.0),
                transition("crossfade", 6.0, 6.0),
                clip("b", "media://b.mov", 0.0, 24.0),
            ],
        )],
    );
    let after = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![
                clip("a", "media://a.mov", 0.0, 24.0),
                clip("b", "media://b.mov", 0.0, 24.0),
            ],
        )],
    );
    let changes = run_diff(before, after);
    let removed: Vec<_> = changes
        .iter()
        .filter(|c| matches!(c, Change::TransitionRemoved { .. }))
        .collect();
    assert_eq!(removed.len(), 1, "{:#?}", changes);
}

#[test]
fn case_09_effects_changed() {
    let before = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip_with_effects("a", "media://a.mov", 0.0, 24.0, vec![])],
        )],
    );
    let after = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip_with_effects(
                "a",
                "media://a.mov",
                0.0,
                24.0,
                vec![
                    effect("blur", json!({"radius": 4})),
                    effect("color", json!({"saturation": 1.2})),
                ],
            )],
        )],
    );
    let changes = run_diff(before, after);
    assert_eq!(changes.len(), 1, "{:#?}", changes);
    match &changes[0] {
        Change::EffectsChanged { before, after, .. } => {
            assert!(before.is_empty());
            assert_eq!(after.len(), 2);
            assert_eq!(after[0].name, "blur");
            assert_eq!(after[0].metadata["radius"], json!(4));
            assert_eq!(after[1].name, "color");
            assert_eq!(after[1].metadata["saturation"], json!(1.2));
        }
        other => panic!("expected EffectsChanged, got {:?}", other),
    }
}

#[test]
fn case_10_effect_parameters_changed_without_count_change() {
    let before = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip_with_effects(
                "a",
                "media://a.mov",
                0.0,
                24.0,
                vec![effect("blur", json!({"radius": 4}))],
            )],
        )],
    );
    let after = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip_with_effects(
                "a",
                "media://a.mov",
                0.0,
                24.0,
                vec![effect("blur", json!({"radius": 12}))],
            )],
        )],
    );
    let changes = run_diff(before, after);
    assert_eq!(changes.len(), 1, "{:#?}", changes);
    match &changes[0] {
        Change::EffectsChanged { before, after, .. } => {
            assert_eq!(before.len(), 1);
            assert_eq!(after.len(), 1);
            assert_eq!(before[0].metadata["radius"], json!(4));
            assert_eq!(after[0].metadata["radius"], json!(12));
        }
        other => panic!("expected EffectsChanged, got {:?}", other),
    }
}

#[test]
fn case_11_transition_changed() {
    let before = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![
                clip("a", "media://a.mov", 0.0, 24.0),
                transition("crossfade", 6.0, 6.0),
                clip("b", "media://b.mov", 0.0, 24.0),
            ],
        )],
    );
    let after = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![
                clip("a", "media://a.mov", 0.0, 24.0),
                transition("dip to black", 12.0, 12.0),
                clip("b", "media://b.mov", 0.0, 24.0),
            ],
        )],
    );
    let changes = run_diff(before, after);
    assert_eq!(changes.len(), 1, "{:#?}", changes);
    match &changes[0] {
        Change::TransitionChanged {
            before_name,
            after_name,
            before_duration,
            after_duration,
            ..
        } => {
            assert_eq!(before_name, "crossfade");
            assert_eq!(after_name, "dip to black");
            assert_eq!(before_duration.unwrap().value, 12.0);
            assert_eq!(after_duration.unwrap().value, 24.0);
        }
        other => panic!("expected TransitionChanged, got {:?}", other),
    }
}

#[test]
fn case_12_effect_name_changed_without_metadata_change() {
    let before = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip_with_effects(
                "a",
                "media://a.mov",
                0.0,
                24.0,
                vec![effect_with_effect_name("LinearTimeWarp")],
            )],
        )],
    );
    let after = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip_with_effects(
                "a",
                "media://a.mov",
                0.0,
                24.0,
                vec![effect_with_effect_name("FreezeFrame")],
            )],
        )],
    );
    let changes = run_diff(before, after);
    assert_eq!(changes.len(), 1, "{:#?}", changes);
    assert!(
        matches!(changes[0], Change::EffectsChanged { .. }),
        "expected EffectsChanged, got {:?}",
        changes[0]
    );
}

#[test]
fn case_13_transition_retarget_reports_remove_and_add_not_changed() {
    let before = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![
                clip("a", "media://a.mov", 0.0, 24.0),
                transition("crossfade", 6.0, 6.0),
                clip("b", "media://b.mov", 0.0, 24.0),
                clip("c", "media://c.mov", 0.0, 24.0),
            ],
        )],
    );
    let after = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![
                clip("a", "media://a.mov", 0.0, 24.0),
                transition("dip to black", 12.0, 12.0),
                clip("c", "media://c.mov", 0.0, 24.0),
                clip("b", "media://b.mov", 0.0, 24.0),
            ],
        )],
    );
    let changes = run_diff(before, after);
    assert!(
        !changes
            .iter()
            .any(|c| matches!(c, Change::TransitionChanged { .. })),
        "retargeted transition should not be TransitionChanged: {:#?}",
        changes
    );
    assert!(
        changes
            .iter()
            .any(|c| matches!(c, Change::TransitionRemoved { .. })),
        "expected TransitionRemoved: {:#?}",
        changes
    );
    assert!(
        changes
            .iter()
            .any(|c| matches!(c, Change::TransitionAdded { .. })),
        "expected TransitionAdded: {:#?}",
        changes
    );
}

#[test]
fn case_14_track_added_and_multitrack() {
    let before = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip("a", "media://a.mov", 0.0, 24.0)],
        )],
    );
    let after = timeline(
        "doc",
        vec![
            track("V1", "Video", vec![clip("a", "media://a.mov", 0.0, 24.0)]),
            track(
                "A1",
                "Audio",
                vec![clip("voiceover", "media://vo.wav", 0.0, 48.0)],
            ),
        ],
    );
    let changes = run_diff(before, after);
    let track_added: Vec<_> = changes
        .iter()
        .filter(|c| matches!(c, Change::TrackAdded { .. }))
        .collect();
    assert_eq!(track_added.len(), 1, "{:#?}", changes);
}

#[test]
fn case_15_no_changes() {
    let same = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip("a", "media://a.mov", 0.0, 24.0)],
        )],
    );
    let changes = run_diff(same.clone(), same);
    assert!(changes.is_empty(), "{:#?}", changes);
}

#[test]
fn case_16_combined_trim_and_add() {
    let before = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![clip("intro", "media://i.mov", 0.0, 100.0)],
        )],
    );
    let after = timeline(
        "doc",
        vec![track(
            "V1",
            "Video",
            vec![
                clip("intro", "media://i.mov", 10.0, 80.0),
                clip("outro", "media://o.mov", 0.0, 24.0),
            ],
        )],
    );
    let changes = run_diff(before, after);
    let trimmed = changes
        .iter()
        .filter(|c| matches!(c, Change::Trimmed { .. }))
        .count();
    let added = changes
        .iter()
        .filter(|c| matches!(c, Change::Added { .. }))
        .count();
    assert_eq!(trimmed, 1, "{:#?}", changes);
    assert_eq!(added, 1, "{:#?}", changes);
}
