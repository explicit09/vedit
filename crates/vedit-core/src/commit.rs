//! Commit objects.

use anyhow::{Result, bail};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Commit {
    /// Always `"vedit.commit.1"` for now. Future-proofs the on-disk format.
    pub schema: String,
    /// Hash of the timeline snapshot this commit refers to.
    pub timeline: String,
    /// Hashes of parent commits. The initial commit has an empty array;
    /// non-merge commits have one parent; merges have two or more.
    pub parents: Vec<String>,
    /// Primary author, kept for compatibility with older callers.
    pub author: Author,
    /// Primary author followed by any co-authors.
    pub authors: Vec<Author>,
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
        Self::new_with_authors(timeline, parents, vec![author], timestamp, message)
            .expect("single-author commit always has one author")
    }

    pub fn new_with_authors(
        timeline: String,
        parents: Vec<String>,
        authors: Vec<Author>,
        timestamp: String,
        message: String,
    ) -> Result<Self> {
        let Some(author) = authors.first().cloned() else {
            bail!("commit requires at least one author");
        };
        Ok(Self {
            schema: Self::SCHEMA.to_string(),
            timeline,
            parents,
            author,
            authors,
            timestamp,
            message,
        })
    }
}

impl<'de> Deserialize<'de> for Commit {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct CommitWire {
            schema: String,
            timeline: String,
            parents: Vec<String>,
            author: Author,
            #[serde(default)]
            authors: Vec<Author>,
            timestamp: String,
            message: String,
        }

        let wire = CommitWire::deserialize(deserializer)?;
        let authors = if wire.authors.is_empty() {
            vec![wire.author]
        } else {
            wire.authors
        };
        let author = authors
            .first()
            .cloned()
            .ok_or_else(|| serde::de::Error::custom("commit requires at least one author"))?;
        Ok(Self {
            schema: wire.schema,
            timeline: wire.timeline,
            parents: wire.parents,
            author,
            authors,
            timestamp: wire.timestamp,
            message: wire.message,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn author(name: &str, email: &str) -> Author {
        Author {
            name: name.to_string(),
            email: email.to_string(),
        }
    }

    #[test]
    fn new_with_authors_preserves_primary_author_and_all_authors() {
        let primary = author("Alice", "alice@example.com");
        let agent = author("Vedit Agent", "agent@vedit.local");

        let commit = Commit::new_with_authors(
            "timeline".to_string(),
            vec!["parent".to_string()],
            vec![primary.clone(), agent.clone()],
            "2026-05-21T00:00:00Z".to_string(),
            "pair edit".to_string(),
        )
        .unwrap();

        assert_eq!(commit.author, primary);
        assert_eq!(commit.authors, vec![primary, agent]);
    }

    #[test]
    fn deserializes_legacy_single_author_commit_as_single_author_list() {
        let value = json!({
            "schema": Commit::SCHEMA,
            "timeline": "timeline",
            "parents": [],
            "author": {
                "name": "Legacy",
                "email": "legacy@example.com"
            },
            "timestamp": "2026-05-21T00:00:00Z",
            "message": "old"
        });

        let commit: Commit = serde_json::from_value(value).unwrap();

        assert_eq!(commit.author.name, "Legacy");
        assert_eq!(commit.authors, vec![commit.author.clone()]);
    }
}
