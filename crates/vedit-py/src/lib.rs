//! Python bindings for vedit.
//!
//! Mirrors the Rust core API but takes Python dicts for OTIO timelines so
//! agents don't have to round-trip through JSON files. Errors map to a
//! single `vedit.VeditError` exception type.

use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pythonize::{depythonize, pythonize};
use serde_json::Value;
use std::path::PathBuf;
use vedit_core::commit::Author;
use vedit_core::diff as core_diff;
use vedit_core::otio;
use vedit_core::repo;

create_exception!(vedit, VeditError, PyException);

fn map_err<E: std::fmt::Display>(e: E) -> PyErr {
    VeditError::new_err(e.to_string())
}

/// Convert a Python object (typically a dict) into a serde_json::Value.
fn py_to_value(_py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    depythonize::<Value>(obj).map_err(map_err)
}

/// Convert a serde_json::Value into a Python object.
fn value_to_py<'py>(py: Python<'py>, v: &Value) -> PyResult<Bound<'py, PyAny>> {
    pythonize(py, v).map_err(map_err)
}

#[pyclass(name = "Repo", module = "vedit")]
struct PyRepo {
    inner: repo::Repo,
}

#[pymethods]
impl PyRepo {
    /// Create a new repo at <workdir>/.vedit/. Errors if it already exists.
    #[staticmethod]
    fn init(workdir: PathBuf) -> PyResult<Self> {
        let inner = repo::Repo::init(&workdir).map_err(map_err)?;
        Ok(Self { inner })
    }

    /// Open a repo by walking up from <start> looking for .vedit/.
    #[staticmethod]
    #[pyo3(signature = (start = None))]
    fn discover(start: Option<PathBuf>) -> PyResult<Self> {
        let path = match start {
            Some(p) => p,
            None => std::env::current_dir().map_err(map_err)?,
        };
        let inner = repo::Repo::discover(&path).map_err(map_err)?;
        Ok(Self { inner })
    }

    /// Open a repo whose .vedit/ lives at exactly <path>/.vedit. Convenience
    /// wrapper over discover() when the caller knows the workdir.
    #[staticmethod]
    fn open(workdir: PathBuf) -> PyResult<Self> {
        let inner = repo::Repo::discover(&workdir).map_err(map_err)?;
        Ok(Self { inner })
    }

    /// Filesystem path to the .vedit/ directory.
    #[getter]
    fn root(&self) -> PathBuf {
        self.inner.root.clone()
    }

    /// Snapshot a timeline (Python dict) and create a commit on the
    /// current branch. Returns the new commit's hash.
    #[pyo3(signature = (timeline, message, author_name = None, author_email = None))]
    fn commit(
        &self,
        py: Python<'_>,
        timeline: &Bound<'_, PyAny>,
        message: &str,
        author_name: Option<String>,
        author_email: Option<String>,
    ) -> PyResult<String> {
        let value = py_to_value(py, timeline)?;
        let timeline_hash = self.inner.write_timeline(&value).map_err(map_err)?;
        let author = Author {
            name: author_name.unwrap_or_else(|| "agent".to_string()),
            email: author_email.unwrap_or_else(|| "agent@vedit".to_string()),
        };
        let h = self
            .inner
            .commit(&timeline_hash, author, message)
            .map_err(map_err)?;
        Ok(h)
    }

    /// Diff between two refs in this repo (branch name, commit hash, HEAD).
    /// Returns a list of Change objects.
    fn diff_refs(
        &self,
        before_ref: &str,
        after_ref: &str,
    ) -> PyResult<Vec<PyChange>> {
        let before_hash = self.inner.resolve(before_ref).map_err(map_err)?;
        let after_hash = self.inner.resolve(after_ref).map_err(map_err)?;
        let before_commit = self.inner.read_commit(&before_hash).map_err(map_err)?;
        let after_commit = self.inner.read_commit(&after_hash).map_err(map_err)?;
        let before_value = self
            .inner
            .read_timeline(&before_commit.timeline)
            .map_err(map_err)?;
        let after_value = self
            .inner
            .read_timeline(&after_commit.timeline)
            .map_err(map_err)?;
        let before_tl = otio::parse_timeline(&before_value).map_err(map_err)?;
        let after_tl = otio::parse_timeline(&after_value).map_err(map_err)?;
        let changes = core_diff::diff(&before_tl, &after_tl);
        Ok(changes.into_iter().map(PyChange::from).collect())
    }

    /// Read the timeline at a given ref as a Python dict.
    fn read_timeline<'py>(&self, py: Python<'py>, refstr: &str) -> PyResult<Bound<'py, PyAny>> {
        let h = self.inner.resolve(refstr).map_err(map_err)?;
        let commit = self.inner.read_commit(&h).map_err(map_err)?;
        let value = self
            .inner
            .read_timeline(&commit.timeline)
            .map_err(map_err)?;
        value_to_py(py, &value)
    }

    /// Read the commit at <ref> as a Commit dict.
    fn read_commit<'py>(&self, py: Python<'py>, refstr: &str) -> PyResult<Bound<'py, PyAny>> {
        let h = self.inner.resolve(refstr).map_err(map_err)?;
        let commit = self.inner.read_commit(&h).map_err(map_err)?;
        let v = serde_json::to_value(&commit).map_err(map_err)?;
        value_to_py(py, &v)
    }

    /// Resolve a ref string (HEAD, branch, short hash, full hash) to a
    /// full hash.
    fn resolve(&self, refstr: &str) -> PyResult<String> {
        self.inner.resolve(refstr).map_err(map_err)
    }

    /// Walk commits from <ref> (default HEAD) newest-first.
    /// Each entry is a (hash, commit_dict) tuple.
    #[pyo3(signature = (refstr = None))]
    fn log<'py>(
        &self,
        py: Python<'py>,
        refstr: Option<&str>,
    ) -> PyResult<Vec<(String, Bound<'py, PyAny>)>> {
        let entries = self.inner.log(refstr).map_err(map_err)?;
        let mut out = Vec::with_capacity(entries.len());
        for (h, c) in entries {
            let v = serde_json::to_value(&c).map_err(map_err)?;
            out.push((h, value_to_py(py, &v)?));
        }
        Ok(out)
    }

    /// Create a branch at <start_ref> (default HEAD). Returns the commit
    /// hash the branch was created at.
    #[pyo3(signature = (name, at = "HEAD"))]
    fn create_branch(&self, name: &str, at: &str) -> PyResult<String> {
        self.inner.create_branch(name, at).map_err(map_err)
    }

    /// Delete a branch. Refuses to delete the current branch.
    fn delete_branch(&self, name: &str) -> PyResult<()> {
        self.inner.delete_branch(name).map_err(map_err)
    }

    /// List branches as (name, hash) tuples, sorted alphabetically.
    fn list_branches(&self) -> PyResult<Vec<(String, String)>> {
        self.inner.list_branches().map_err(map_err)
    }

    /// Return the current branch name, or None if HEAD is detached.
    fn current_branch(&self) -> PyResult<Option<String>> {
        self.inner.current_branch().map_err(map_err)
    }

    /// Switch HEAD to an existing branch.
    fn switch_branch(&self, name: &str) -> PyResult<()> {
        self.inner.switch_branch(name).map_err(map_err)
    }

    fn __repr__(&self) -> String {
        format!("Repo(root={:?})", self.inner.root)
    }
}

/// One semantic change between two timelines.
#[pyclass(name = "Change", module = "vedit")]
#[derive(Clone)]
struct PyChange {
    inner: core_diff::Change,
}

impl From<core_diff::Change> for PyChange {
    fn from(inner: core_diff::Change) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyChange {
    /// The change's discriminator: "trimmed", "moved", "added", "removed",
    /// "replaced", "effects_changed", "transition_added",
    /// "transition_removed", "track_added", "track_removed".
    #[getter]
    fn op(&self) -> &'static str {
        match self.inner {
            core_diff::Change::TrackAdded { .. } => "track_added",
            core_diff::Change::TrackRemoved { .. } => "track_removed",
            core_diff::Change::Trimmed { .. } => "trimmed",
            core_diff::Change::Moved { .. } => "moved",
            core_diff::Change::Added { .. } => "added",
            core_diff::Change::Removed { .. } => "removed",
            core_diff::Change::EffectsChanged { .. } => "effects_changed",
            core_diff::Change::Replaced { .. } => "replaced",
            core_diff::Change::TransitionAdded { .. } => "transition_added",
            core_diff::Change::TransitionRemoved { .. } => "transition_removed",
        }
    }

    /// Full structured payload as a Python dict. Same shape the CLI emits
    /// with --json. This is the most flexible accessor; per-field
    /// properties exist for ergonomics on common cases.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let v = serde_json::to_value(&self.inner).map_err(map_err)?;
        value_to_py(py, &v)
    }

    fn __repr__(&self) -> String {
        format!("Change(op={:?})", self.op())
    }
}

/// Compute the diff between two OTIO timelines passed as Python dicts.
/// Returns a list of Change objects. No repo required.
#[pyfunction]
fn diff<'py>(
    py: Python<'py>,
    before: &Bound<'py, PyAny>,
    after: &Bound<'py, PyAny>,
) -> PyResult<Vec<PyChange>> {
    let before_value = py_to_value(py, before)?;
    let after_value = py_to_value(py, after)?;
    let before_tl = otio::parse_timeline(&before_value).map_err(map_err)?;
    let after_tl = otio::parse_timeline(&after_value).map_err(map_err)?;
    let changes = core_diff::diff(&before_tl, &after_tl);
    Ok(changes.into_iter().map(PyChange::from).collect())
}

#[pymodule]
fn vedit(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyRepo>()?;
    m.add_class::<PyChange>()?;
    m.add_function(wrap_pyfunction!(diff, m)?)?;
    m.add("VeditError", m.py().get_type::<VeditError>())?;
    Ok(())
}
