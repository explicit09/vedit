//! Commit objects.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Commit {
    /// Always `"vedit.commit.1"` for now. Future-proofs the on-disk format.
    pub schema: String,
    /// Hash of the timeline snapshot this commit refers to.
    pub timeline: String,
    /// Hashes of parent commits. The initial commit has an empty array;
    /// non-merge commits have one parent; merges have two or more.
    pub parents: Vec<String>,
    pub author: Author,
    pub timestamp: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    pub email: String,
}

impl Commit {
    pub const SCHEMA: &'static str = "vedit.commit.1";

    pub fn new(
        timeline: String,
        parents: Vec<String>,
        author: Author,
        timestamp: String,
        message: String,
    ) -> Self {
        Self {
            schema: Self::SCHEMA.to_string(),
            timeline,
            parents,
            author,
            timestamp,
            message,
        }
    }
}
