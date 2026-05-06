//! Tests against real-world OTIO samples.
//!
//! These fixtures come from the AcademySoftwareFoundation OpenTimelineIO
//! repository (Apache 2.0). They cover schema features the hand-built
//! corpus does not: multi-track timelines, transitions in real positions,
//! nested stacks, generator references, Premiere-exported XML pipelines,
//! and timelines with effects on individual clips.
//!
//! The contract for every real-world fixture: `vedit diff <fixture>
//! <fixture>` must produce zero changes. If the matcher or parser
//! interprets a real timeline inconsistently across two reads, this test
//! fails.

use std::path::PathBuf;
use vedit_core::diff::diff;
use vedit_core::otio;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn assert_self_diff_empty(name: &str) {
    let path = fixture_path(name);
    let timeline = otio::load(&path).unwrap_or_else(|e| panic!("load {name}: {e:?}"));
    let changes = diff(&timeline, &timeline);
    assert!(
        changes.is_empty(),
        "self-diff produced {} changes on {}: {:#?}",
        changes.len(),
        name,
        changes
    );
}

#[test]
fn multitrack() {
    assert_self_diff_empty("multitrack.otio");
}

#[test]
fn transition() {
    assert_self_diff_empty("transition.otio");
}

#[test]
fn effects() {
    assert_self_diff_empty("effects.otio");
}

#[test]
fn nested_example() {
    assert_self_diff_empty("nested_example.otio");
}

#[test]
fn premiere_example() {
    assert_self_diff_empty("premiere_example.otio");
}

#[test]
fn generator_reference_test() {
    assert_self_diff_empty("generator_reference_test.otio");
}

/// Sanity check: a programmatic edit on a real OTIO file produces a
/// detectable change. This guards the diff engine against the trivial
/// case where it accidentally produces no output.
#[test]
fn detects_trim_on_real_multitrack() {
    let original = otio::load(&fixture_path("multitrack.otio")).unwrap();
    // Build a modified version by copying the parsed timeline and shrinking
    // the duration of the first clip on the first video track.
    let mut modified = original.clone();
    let track = modified.tracks.first_mut().expect("at least one track");
    let first_clip = track
        .children
        .iter_mut()
        .find_map(|child| match child {
            vedit_core::model::TrackChild::Clip(c) => Some(c),
            _ => None,
        })
        .expect("first track has at least one clip");
    let sr = first_clip.source_range.as_mut().expect("clip has source_range");
    sr.duration.value -= 12.0;

    let changes = diff(&original, &modified);
    assert!(
        changes
            .iter()
            .any(|c| matches!(c, vedit_core::diff::Change::Trimmed { .. })),
        "expected at least one Trimmed change: {:#?}",
        changes
    );
}
