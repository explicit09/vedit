//! Review artifact metadata.
//!
//! Review artifacts describe generated review outputs and their provenance.
//! The schema is intentionally separate from commit objects: commits identify
//! version-control history, while review artifacts identify downstream review
//! packages, renders, and reasoning trails derived from that history.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewArtifact {
    /// Always `"vedit.review_artifact.1"` for this schema version.
    pub schema: String,
    /// Path or URI for the generated review render.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render_path: Option<String>,
    /// UTC generation timestamp, formatted as RFC 3339 / ISO 8601 text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    /// vedit commit hash that the review output was generated from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_commit: Option<String>,
    /// Timeline object hash that the review output was generated from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline: Option<String>,
    /// Freeform labels for downstream review package routing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Human-facing summary/header associated with the reviewed commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_header: Option<String>,
    /// Reasoning or explanation body attached to the review package.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_body: Option<String>,
}

impl ReviewArtifact {
    pub const SCHEMA: &'static str = "vedit.review_artifact.1";

    pub fn new() -> Self {
        Self {
            schema: Self::SCHEMA.to_string(),
            render_path: None,
            generated_at: None,
            source_commit: None,
            timeline: None,
            tags: Vec::new(),
            commit_header: None,
            reasoning_body: None,
        }
    }

    pub fn with_render_path(mut self, render_path: impl Into<String>) -> Self {
        self.render_path = Some(render_path.into());
        self
    }

    pub fn with_generated_at(mut self, generated_at: impl Into<String>) -> Self {
        self.generated_at = Some(generated_at.into());
        self
    }

    pub fn with_source_commit(mut self, source_commit: impl Into<String>) -> Self {
        self.source_commit = Some(source_commit.into());
        self
    }

    pub fn with_timeline(mut self, timeline: impl Into<String>) -> Self {
        self.timeline = Some(timeline.into());
        self
    }

    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_commit_header(mut self, commit_header: impl Into<String>) -> Self {
        self.commit_header = Some(commit_header.into());
        self
    }

    pub fn with_reasoning_body(mut self, reasoning_body: impl Into<String>) -> Self {
        self.reasoning_body = Some(reasoning_body.into());
        self
    }
}

impl Default for ReviewArtifact {
    fn default() -> Self {
        Self::new()
    }
}
