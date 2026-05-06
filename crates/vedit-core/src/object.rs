//! Object encoding for the content-addressed store.
//!
//! Every object's identity is the SHA-256 of its canonical JSON form,
//! lowercase hex, prefixed with `sha256:`. On disk, objects are stored
//! gzipped at `objects/<first-2-chars>/<rest-of-hash>`.

use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// `sha256:` followed by 64 lowercase hex chars.
pub const HASH_PREFIX: &str = "sha256:";

/// Compute the canonical hash of a JSON value.
pub fn hash(value: &Value) -> String {
    let canonical = canonicalize(value);
    let bytes = serde_json::to_vec(&canonical).expect("canonical JSON serializes");
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    format!("{HASH_PREFIX}{}", hex::encode(digest))
}

/// Recursively reorder all object keys. Arrays preserve order. Numbers
/// are not normalized (we trust serde_json's representation).
fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map
                .iter()
                .map(|(k, v)| (k.clone(), canonicalize(v)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut out = serde_json::Map::with_capacity(entries.len());
            for (k, v) in entries {
                out.insert(k, v);
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize).collect()),
        _ => value.clone(),
    }
}

/// Convert a hash like `sha256:ab12...` into the on-disk path
/// `<root>/ab/12...`.
pub fn object_path(root: &Path, hash: &str) -> Result<PathBuf> {
    let body = hash
        .strip_prefix(HASH_PREFIX)
        .ok_or_else(|| anyhow!("hash missing `{HASH_PREFIX}` prefix: {hash}"))?;
    if body.len() < 3 {
        return Err(anyhow!("hash body too short: {body}"));
    }
    let (head, tail) = body.split_at(2);
    Ok(root.join(head).join(tail))
}

/// Write a JSON object to the store, gzipped. Returns the hash, regardless
/// of whether the object already existed.
pub fn write(root: &Path, value: &Value) -> Result<String> {
    let h = hash(value);
    let path = object_path(root, &h)?;
    if path.exists() {
        return Ok(h);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let canonical = canonicalize(value);
    let bytes = serde_json::to_vec(&canonical)?;
    let file = std::fs::File::create(&path)
        .with_context(|| format!("creating {}", path.display()))?;
    let mut encoder = GzEncoder::new(file, Compression::default());
    encoder.write_all(&bytes)?;
    encoder.finish()?;
    Ok(h)
}

/// Read a JSON object out of the store, ungzipping and parsing.
pub fn read(root: &Path, hash: &str) -> Result<Value> {
    let path = object_path(root, hash)?;
    let file = std::fs::File::open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    let mut decoder = GzDecoder::new(file);
    let mut bytes = Vec::new();
    decoder.read_to_end(&mut bytes)?;
    let value: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing object {hash}"))?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn hash_is_stable_under_key_reorder() {
        let a = json!({ "z": 1, "a": [3, 2, 1], "m": { "y": 1, "x": 2 } });
        let b = json!({ "a": [3, 2, 1], "m": { "x": 2, "y": 1 }, "z": 1 });
        assert_eq!(hash(&a), hash(&b));
    }

    #[test]
    fn hash_changes_when_array_reorders() {
        // Array order is meaningful in OTIO (clip ordering on a track).
        let a = json!([1, 2, 3]);
        let b = json!([3, 2, 1]);
        assert_ne!(hash(&a), hash(&b));
    }

    #[test]
    fn write_then_read_roundtrips() {
        let dir = tempdir().unwrap();
        let v = json!({ "kind": "timeline", "name": "test" });
        let h = write(dir.path(), &v).unwrap();
        let back = read(dir.path(), &h).unwrap();
        // After reading back through canonicalization the keys may be
        // sorted, but the *content* is preserved.
        assert_eq!(back["kind"], v["kind"]);
        assert_eq!(back["name"], v["name"]);
    }

    #[test]
    fn write_is_idempotent() {
        let dir = tempdir().unwrap();
        let v = json!({ "x": 1 });
        let h1 = write(dir.path(), &v).unwrap();
        let h2 = write(dir.path(), &v).unwrap();
        assert_eq!(h1, h2);
    }
}
