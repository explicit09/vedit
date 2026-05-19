//! End-to-end test of the v0.2 surface using the core library directly.
//! This exercises init → commit → commit → log → show-equivalent
//! (diff parent) → checkout (equivalent: read_timeline by hash) without
//! shelling out to the CLI binary.

use serde_json::json;
use tempfile::tempdir;
use vedit_core::commit::Author;
use vedit_core::diff::diff;
use vedit_core::otio;
use vedit_core::repo::Repo;

fn timeline_with_clip_count(count: usize, name: &str) -> serde_json::Value {
    let clips: Vec<serde_json::Value> = (0..count)
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
        "name": name,
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

fn author() -> Author {
    Author {
        name: "tester".to_string(),
        email: "test@example.com".to_string(),
    }
}

#[test]
fn full_workflow_init_commit_log_show_checkout() {
    let dir = tempdir().unwrap();
    let repo = Repo::init(dir.path()).unwrap();

    // Two snapshots: 3 clips, then 4 clips.
    let v1 = timeline_with_clip_count(3, "doc");
    let v1_hash = repo.write_timeline(&v1).unwrap();
    let c1 = repo.commit(&v1_hash, author(), "Initial cut").unwrap();

    let v2 = timeline_with_clip_count(4, "doc");
    let v2_hash = repo.write_timeline(&v2).unwrap();
    let c2 = repo.commit(&v2_hash, author(), "Add one clip").unwrap();

    // Log walks newest-first.
    let log = repo.log(None).unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].0, c2);
    assert_eq!(log[1].0, c1);

    // The second commit's parent is the first.
    let commit2 = repo.read_commit(&c2).unwrap();
    assert_eq!(commit2.parents, vec![c1.clone()]);

    // Diffing parent → child reveals the new clip.
    let parent_value = repo.read_timeline(&v1_hash).unwrap();
    let child_value = repo.read_timeline(&v2_hash).unwrap();
    let parent_tl = otio::parse_timeline(&parent_value).unwrap();
    let child_tl = otio::parse_timeline(&child_value).unwrap();
    let changes = diff(&parent_tl, &child_tl);
    assert!(
        changes
            .iter()
            .any(|c| matches!(c, vedit_core::diff::Change::Added { .. })),
        "{:#?}",
        changes
    );

    // Checkout: read the v1 timeline back out, verify it matches what we
    // committed.
    let checked_out = repo.read_timeline(&v1_hash).unwrap();
    // After canonicalization, content must match.
    let original_canonical = serde_json::to_string(&canonical(&v1)).unwrap();
    let recovered_canonical = serde_json::to_string(&canonical(&checked_out)).unwrap();
    assert_eq!(original_canonical, recovered_canonical);

    // HEAD points at c2 via main.
    assert_eq!(repo.resolve("HEAD").unwrap(), c2);
    assert_eq!(repo.resolve("main").unwrap(), c2);
}

#[test]
fn discover_finds_repo_from_subdirectory() {
    let dir = tempdir().unwrap();
    Repo::init(dir.path()).unwrap();
    let nested = dir.path().join("a/b/c");
    std::fs::create_dir_all(&nested).unwrap();
    let repo = Repo::discover(&nested).unwrap();
    assert_eq!(repo.root, dir.path().join(".vedit"));
}

#[test]
fn empty_repo_log_is_empty() {
    let dir = tempdir().unwrap();
    let repo = Repo::init(dir.path()).unwrap();
    let log = repo.log(None).unwrap();
    assert!(log.is_empty());
}

#[test]
fn resolve_head_before_first_commit_errors() {
    let dir = tempdir().unwrap();
    let repo = Repo::init(dir.path()).unwrap();
    let r = repo.resolve("HEAD");
    assert!(r.is_err());
}

fn canonical(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().map(|(k, v)| (k.clone(), canonical(v))).collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut out = serde_json::Map::new();
            for (k, v) in entries {
                out.insert(k, v);
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(canonical).collect())
        }
        _ => v.clone(),
    }
}
