//! Object encoding for the content-addressed store.
//!
//! Every object's identity is the SHA-256 of its canonical JSON form,
//! lowercase hex, prefixed with `sha256:`. On disk, objects are stored
//! gzipped at `objects/<first-2-chars>/<rest-of-hash>`.

use crate::atomic;
use anyhow::{Context, Result, anyhow};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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
    if path.exists() && read(root, &h).is_ok() {
        return Ok(h);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let canonical = canonicalize(value);
    let bytes = serde_json::to_vec(&canonical)?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&bytes)?;
    let compressed = encoder.finish()?;
    atomic_write_bytes(&path, &compressed)?;
    Ok(h)
}

/// Read a JSON object out of the store, ungzipping and parsing.
pub fn read(root: &Path, hash: &str) -> Result<Value> {
    let path = object_path(root, hash)?;
    let file = std::fs::File::open(&path).with_context(|| format!("opening {}", path.display()))?;
    let mut decoder = GzDecoder::new(file);
    let mut bytes = Vec::new();
    decoder.read_to_end(&mut bytes)?;
    let value: Value =
        serde_json::from_slice(&bytes).with_context(|| format!("parsing object {hash}"))?;
    let actual = self::hash(&value);
    if actual != hash {
        return Err(anyhow!(
            "object hash mismatch: expected {hash}, got {actual}"
        ));
    }
    Ok(value)
}

fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;

    let tmp_path = create_temp_path(path);
    let mut tmp = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .with_context(|| format!("creating temp file {}", tmp_path.display()))?;
    tmp.write_all(bytes)
        .with_context(|| format!("writing {}", tmp_path.display()))?;
    tmp.sync_all()
        .with_context(|| format!("syncing {}", tmp_path.display()))?;
    drop(tmp);

    if let Err(e) = atomic::replace_file(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }
    sync_parent_dir(parent);
    Ok(())
}

fn create_temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("object");
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{file_name}.tmp.{}.{}", std::process::id(), n))
}

fn sync_parent_dir(path: &Path) {
    if let Ok(dir) = std::fs::File::open(path) {
        let _ = dir.sync_all();
    }
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

    #[test]
    fn read_rejects_object_whose_contents_do_not_match_hash() {
        let dir = tempdir().unwrap();
        let expected = json!({ "x": 1 });
        let wrong = json!({ "x": 2 });

        let expected_hash = write(dir.path(), &expected).unwrap();
        let wrong_hash = write(dir.path(), &wrong).unwrap();
        let expected_path = object_path(dir.path(), &expected_hash).unwrap();
        let wrong_path = object_path(dir.path(), &wrong_hash).unwrap();

        std::fs::copy(wrong_path, expected_path).unwrap();

        let err = read(dir.path(), &expected_hash).unwrap_err();
        assert!(
            err.to_string().contains("object hash mismatch"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn write_repairs_corrupt_existing_object_path() {
        let dir = tempdir().unwrap();
        let v = json!({ "x": 1 });
        let h = hash(&v);
        let path = object_path(dir.path(), &h).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not a gzip object").unwrap();

        let written = write(dir.path(), &v).unwrap();
        assert_eq!(written, h);
        assert_eq!(read(dir.path(), &h).unwrap(), json!({ "x": 1 }));
    }
}
