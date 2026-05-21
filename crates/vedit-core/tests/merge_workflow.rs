//! End-to-end test of v0.6 merge surface using the core library.

use serde_json::json;
use tempfile::tempdir;
use vedit_core::commit::Author;
use vedit_core::merge::{ChangedClipIdMergeOutcome, MergeOutcome, merge as run_merge};
use vedit_core::otio;
use vedit_core::repo::Repo;

fn author() -> Author {
    Author {
        name: "tester".to_string(),
        email: "test@example.com".to_string(),
    }
}

fn timeline_with_tracks(tracks: Vec<serde_json::Value>) -> serde_json::Value {
    json!({
        "OTIO_SCHEMA": "Timeline.1",
        "name": "doc",
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "name": "tracks",
            "children": tracks,
        }
    })
}

fn video_track(name: &str, clips: Vec<&str>) -> serde_json::Value {
    let children: Vec<serde_json::Value> = clips
        .into_iter()
        .map(|c| {
            json!({
                "OTIO_SCHEMA": "Clip.2",
                "name": c,
                "metadata": {},
                "media_reference": {
                    "OTIO_SCHEMA": "ExternalReference.1",
                    "target_url": format!("media://{c}.mov")
                },
                "source_range": {
                    "OTIO_SCHEMA": "TimeRange.1",
                    "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0.0, "rate": 24.0 },
                    "duration":   { "OTIO_SCHEMA": "RationalTime.1", "value": 24.0, "rate": 24.0 }
                }
            })
        })
        .collect();
    json!({
        "OTIO_SCHEMA": "Track.1",
        "name": name,
        "kind": "Video",
        "children": children,
    })
}

fn identified_clip(id: &str, duration: f64) -> serde_json::Value {
    json!({
        "OTIO_SCHEMA": "Clip.2",
        "name": id,
        "metadata": { "clip_id": id },
        "media_reference": {
            "OTIO_SCHEMA": "ExternalReference.1",
            "target_url": format!("media://{id}.mov")
        },
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0.0, "rate": 24.0 },
            "duration":   { "OTIO_SCHEMA": "RationalTime.1", "value": duration, "rate": 24.0 }
        }
    })
}

fn identified_video_track(name: &str, clips: Vec<serde_json::Value>) -> serde_json::Value {
    json!({
        "OTIO_SCHEMA": "Track.1",
        "name": name,
        "kind": "Video",
        "children": clips,
    })
}

fn audio_track(name: &str, clips: Vec<&str>) -> serde_json::Value {
    let children: Vec<serde_json::Value> = clips
        .into_iter()
        .map(|c| {
            json!({
                "OTIO_SCHEMA": "Clip.2",
                "name": c,
                "metadata": {},
                "media_reference": {
                    "OTIO_SCHEMA": "ExternalReference.1",
                    "target_url": format!("media://{c}.wav")
                },
                "source_range": {
                    "OTIO_SCHEMA": "TimeRange.1",
                    "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0.0, "rate": 24.0 },
                    "duration":   { "OTIO_SCHEMA": "RationalTime.1", "value": 24.0, "rate": 24.0 }
                }
            })
        })
        .collect();
    json!({
        "OTIO_SCHEMA": "Track.1",
        "name": name,
        "kind": "Audio",
        "children": children,
    })
}

#[test]
fn fast_forward_when_head_is_ancestor() {
    let dir = tempdir().unwrap();
    let repo = Repo::init(dir.path()).unwrap();
    let v1 = repo
        .write_timeline(&timeline_with_tracks(vec![video_track("V1", vec!["a"])]))
        .unwrap();
    let c1 = repo.commit(&v1, author(), "v1").unwrap();

    repo.create_branch("feat", "HEAD").unwrap();
    repo.switch_branch("feat").unwrap();
    let v2 = repo
        .write_timeline(&timeline_with_tracks(vec![video_track(
            "V1",
            vec!["a", "b"],
        )]))
        .unwrap();
    let c2 = repo.commit(&v2, author(), "feat: add b").unwrap();

    // From main's perspective, feat is just ahead by one commit.
    let mb = repo.merge_base(&c1, &c2).unwrap();
    assert_eq!(mb.as_deref(), Some(c1.as_str()));
}

#[test]
fn three_way_clean_merge_combines_disjoint_changes() {
    // base: V1 with [a]
    // ours adds A1 track
    // theirs adds clip b to V1
    // merge should produce: V1 with [a, b], A1 added
    let base_tl =
        otio::parse_timeline(&timeline_with_tracks(vec![video_track("V1", vec!["a"])])).unwrap();
    let ours_tl = otio::parse_timeline(&timeline_with_tracks(vec![
        video_track("V1", vec!["a"]),
        audio_track("A1", vec![]),
    ]))
    .unwrap();
    let theirs_tl = otio::parse_timeline(&timeline_with_tracks(vec![video_track(
        "V1",
        vec!["a", "b"],
    )]))
    .unwrap();

    match run_merge(&base_tl, &ours_tl, &theirs_tl) {
        MergeOutcome::Clean(merged) => {
            assert_eq!(merged.tracks.len(), 2);
            // V1 should have 2 clips (theirs's contribution).
            let v1 = merged.tracks.iter().find(|t| t.name == "V1").unwrap();
            assert_eq!(
                v1.children
                    .iter()
                    .filter(|c| matches!(c, vedit_core::model::TrackChild::Clip(_)))
                    .count(),
                2
            );
            // A1 should exist (ours's contribution).
            assert!(merged.tracks.iter().any(|t| t.name == "A1"));
        }
        other => panic!("expected Clean, got {:?}", other),
    }
}

#[test]
fn three_way_conflict_when_both_modify_same_track() {
    let base_tl =
        otio::parse_timeline(&timeline_with_tracks(vec![video_track("V1", vec!["a"])])).unwrap();
    let ours_tl = otio::parse_timeline(&timeline_with_tracks(vec![video_track(
        "V1",
        vec!["a", "b"],
    )]))
    .unwrap();
    let theirs_tl = otio::parse_timeline(&timeline_with_tracks(vec![video_track(
        "V1",
        vec!["a", "c"],
    )]))
    .unwrap();
    match run_merge(&base_tl, &ours_tl, &theirs_tl) {
        MergeOutcome::Conflicts(cs) => {
            assert_eq!(cs.len(), 1);
            assert_eq!(cs[0].track_name, "V1");
        }
        other => panic!("expected Conflicts, got {:?}", other),
    }
}

#[test]
fn merge_commit_has_two_parents() {
    let dir = tempdir().unwrap();
    let repo = Repo::init(dir.path()).unwrap();
    let base_v = repo
        .write_timeline(&timeline_with_tracks(vec![video_track("V1", vec!["a"])]))
        .unwrap();
    let base_c = repo.commit(&base_v, author(), "base").unwrap();

    repo.create_branch("alt", "HEAD").unwrap();

    // main advances to add A1.
    let main_v = repo
        .write_timeline(&timeline_with_tracks(vec![
            video_track("V1", vec!["a"]),
            audio_track("A1", vec![]),
        ]))
        .unwrap();
    let main_c = repo.commit(&main_v, author(), "main").unwrap();

    // alt advances to add clip b.
    repo.switch_branch("alt").unwrap();
    let alt_v = repo
        .write_timeline(&timeline_with_tracks(vec![video_track(
            "V1",
            vec!["a", "b"],
        )]))
        .unwrap();
    let alt_c = repo.commit(&alt_v, author(), "alt").unwrap();

    // Merge alt into main using the public commit_with_parents API.
    repo.switch_branch("main").unwrap();
    let mb = repo.merge_base(&main_c, &alt_c).unwrap().unwrap();
    assert_eq!(mb, base_c);

    // Use the engine to compute the merged timeline.
    let base_tl = otio::parse_timeline(&repo.read_timeline(&base_v).unwrap()).unwrap();
    let main_tl = otio::parse_timeline(&repo.read_timeline(&main_v).unwrap()).unwrap();
    let alt_tl = otio::parse_timeline(&repo.read_timeline(&alt_v).unwrap()).unwrap();
    let merged = match run_merge(&base_tl, &main_tl, &alt_tl) {
        MergeOutcome::Clean(m) => m,
        other => panic!("expected Clean, got {:?}", other),
    };

    let merged_value = serde_json::to_value(&merged).unwrap();
    let merged_hash = repo.write_timeline(&merged_value).unwrap();
    let merge_commit_hash = repo
        .commit_with_parents(
            &merged_hash,
            vec![main_c.clone(), alt_c.clone()],
            author(),
            "Merge alt into main",
        )
        .unwrap();

    let merge_commit = repo.read_commit(&merge_commit_hash).unwrap();
    assert_eq!(merge_commit.parents.len(), 2);
    assert_eq!(merge_commit.parents[0], main_c);
    assert_eq!(merge_commit.parents[1], alt_c);
}

#[test]
fn changed_clip_ids_are_reported_relative_to_first_parent() {
    let dir = tempdir().unwrap();
    let repo = Repo::init(dir.path()).unwrap();
    let base_v = repo
        .write_timeline(&timeline_with_tracks(vec![identified_video_track(
            "V1",
            vec![
                identified_clip("clip-a", 24.0),
                identified_clip("clip-b", 24.0),
            ],
        )]))
        .unwrap();
    repo.commit(&base_v, author(), "base").unwrap();

    let changed_v = repo
        .write_timeline(&timeline_with_tracks(vec![identified_video_track(
            "V1",
            vec![
                identified_clip("clip-a", 24.0),
                identified_clip("clip-b", 12.0),
            ],
        )]))
        .unwrap();
    let changed_c = repo.commit(&changed_v, author(), "trim b").unwrap();

    assert_eq!(
        repo.changed_clip_ids(&changed_c).unwrap(),
        vec!["clip-b".to_string()]
    );
}

#[test]
fn changed_clip_id_merge_allows_same_track_non_overlap_and_returns_metadata() {
    let dir = tempdir().unwrap();
    let repo = Repo::init(dir.path()).unwrap();
    let base_v = repo
        .write_timeline(&timeline_with_tracks(vec![identified_video_track(
            "V1",
            vec![
                identified_clip("clip-a", 24.0),
                identified_clip("clip-b", 24.0),
            ],
        )]))
        .unwrap();
    let base_c = repo.commit(&base_v, author(), "base").unwrap();

    repo.create_branch("source", "HEAD").unwrap();

    let target_v = repo
        .write_timeline(&timeline_with_tracks(vec![identified_video_track(
            "V1",
            vec![
                identified_clip("clip-a", 12.0),
                identified_clip("clip-b", 24.0),
            ],
        )]))
        .unwrap();
    let target_c = repo.commit(&target_v, author(), "trim a").unwrap();

    repo.switch_branch("source").unwrap();
    let source_v = repo
        .write_timeline(&timeline_with_tracks(vec![identified_video_track(
            "V1",
            vec![
                identified_clip("clip-a", 24.0),
                identified_clip("clip-b", 12.0),
            ],
        )]))
        .unwrap();
    let source_c = repo.commit(&source_v, author(), "trim b").unwrap();

    let outcome = repo
        .merge_changed_clip_ids("source", "main", author(), "merge source into main")
        .unwrap();

    let clean = match outcome {
        ChangedClipIdMergeOutcome::Clean(clean) => clean,
        other => panic!("expected clean changed-clip-id merge, got {other:?}"),
    };
    assert_eq!(clean.source_ref, "source");
    assert_eq!(clean.target_ref, "main");
    assert_eq!(clean.parents, vec![target_c.clone(), source_c.clone()]);
    assert_eq!(clean.source_changed_clip_ids, vec!["clip-b".to_string()]);
    assert_eq!(clean.target_changed_clip_ids, vec!["clip-a".to_string()]);

    let merge_commit = repo.read_commit(&clean.commit_hash).unwrap();
    assert_eq!(merge_commit.parents, vec![target_c, source_c]);

    let merged_value = repo.read_timeline(&merge_commit.timeline).unwrap();
    let merged = otio::parse_timeline(&merged_value).unwrap();
    let clips: Vec<_> = merged.tracks[0]
        .children
        .iter()
        .filter_map(|child| match child {
            vedit_core::model::TrackChild::Clip(clip) => Some((
                clip.clip_id.as_deref().unwrap(),
                clip.source_range.unwrap().duration.value,
            )),
            _ => None,
        })
        .collect();
    assert_eq!(clips, vec![("clip-a", 12.0), ("clip-b", 12.0)]);
    assert_eq!(
        repo.merge_base(&clean.parents[0], &clean.parents[1])
            .unwrap(),
        Some(base_c)
    );
}

#[test]
fn changed_clip_id_merge_returns_typed_conflict_for_overlap() {
    let dir = tempdir().unwrap();
    let repo = Repo::init(dir.path()).unwrap();
    let base_v = repo
        .write_timeline(&timeline_with_tracks(vec![identified_video_track(
            "V1",
            vec![
                identified_clip("clip-a", 24.0),
                identified_clip("clip-b", 24.0),
            ],
        )]))
        .unwrap();
    repo.commit(&base_v, author(), "base").unwrap();

    repo.create_branch("source", "HEAD").unwrap();

    let target_v = repo
        .write_timeline(&timeline_with_tracks(vec![identified_video_track(
            "V1",
            vec![
                identified_clip("clip-a", 12.0),
                identified_clip("clip-b", 24.0),
            ],
        )]))
        .unwrap();
    let target_c = repo.commit(&target_v, author(), "trim a target").unwrap();

    repo.switch_branch("source").unwrap();
    let source_v = repo
        .write_timeline(&timeline_with_tracks(vec![identified_video_track(
            "V1",
            vec![
                identified_clip("clip-a", 18.0),
                identified_clip("clip-b", 24.0),
            ],
        )]))
        .unwrap();
    repo.commit(&source_v, author(), "trim a source").unwrap();

    let outcome = repo
        .merge_changed_clip_ids("source", "main", author(), "merge source into main")
        .unwrap();

    match outcome {
        ChangedClipIdMergeOutcome::ClipIdConflicts(conflict) => {
            assert_eq!(conflict.source_ref, "source");
            assert_eq!(conflict.target_ref, "main");
            assert_eq!(conflict.overlapping_clip_ids, vec!["clip-a".to_string()]);
            assert_eq!(conflict.source_changed_clip_ids, vec!["clip-a".to_string()]);
            assert_eq!(conflict.target_changed_clip_ids, vec!["clip-a".to_string()]);
        }
        other => panic!("expected changed-clip-id conflict, got {other:?}"),
    }
    assert_eq!(
        repo.branch_target("main").unwrap().as_deref(),
        Some(target_c.as_str())
    );
}
