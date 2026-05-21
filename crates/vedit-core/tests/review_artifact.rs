use serde_json::json;
use vedit_core::review_artifact::ReviewArtifact;

#[test]
fn review_artifact_serializes_versioned_provenance_fields() {
    let artifact = ReviewArtifact::new()
        .with_render_path("renders/review.mp4")
        .with_generated_at("2026-05-21T07:29:30Z")
        .with_source_commit(
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .with_timeline("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        .with_tags(["review", "agent"])
        .with_commit_header("Trim intro, add crossfade")
        .with_reasoning_body("The edit removes dead air before the first title card.");

    let value = serde_json::to_value(&artifact).unwrap();

    assert_eq!(
        value,
        json!({
            "schema": "vedit.review_artifact.1",
            "render_path": "renders/review.mp4",
            "generated_at": "2026-05-21T07:29:30Z",
            "source_commit": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "timeline": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "tags": ["review", "agent"],
            "commit_header": "Trim intro, add crossfade",
            "reasoning_body": "The edit removes dead air before the first title card."
        })
    );
}

#[test]
fn review_artifact_deserializes_legacy_artifacts_with_missing_optional_fields() {
    let artifact: ReviewArtifact = serde_json::from_value(json!({
        "schema": "vedit.review_artifact.1",
        "source_commit": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    }))
    .unwrap();

    assert_eq!(artifact.schema, ReviewArtifact::SCHEMA);
    assert_eq!(
        artifact.source_commit.as_deref(),
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert_eq!(artifact.render_path, None);
    assert_eq!(artifact.generated_at, None);
    assert_eq!(artifact.timeline, None);
    assert!(artifact.tags.is_empty());
    assert_eq!(artifact.commit_header, None);
    assert_eq!(artifact.reasoning_body, None);
}

#[test]
fn empty_review_artifact_omits_optional_fields() {
    let value = serde_json::to_value(ReviewArtifact::new()).unwrap();

    assert_eq!(
        value,
        json!({
            "schema": "vedit.review_artifact.1"
        })
    );
}
