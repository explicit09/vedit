"""End-to-end test of the vedit Python API.

Run from the repo root with the venv active:
    pytest crates/vedit-py/tests/

Or via maturin:
    cd crates/vedit-py && maturin develop --release && pytest tests/
"""

import pathlib
import tempfile

import pytest

import vedit


def make_timeline(name: str, n_clips: int) -> dict:
    """Return a minimal but valid OTIO Timeline dict with N video clips."""
    return {
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
                    "children": [
                        {
                            "OTIO_SCHEMA": "Clip.2",
                            "name": f"clip_{i}",
                            "source_range": {
                                "OTIO_SCHEMA": "TimeRange.1",
                                "start_time": {
                                    "OTIO_SCHEMA": "RationalTime.1",
                                    "value": 0.0,
                                    "rate": 24.0,
                                },
                                "duration": {
                                    "OTIO_SCHEMA": "RationalTime.1",
                                    "value": 24.0,
                                    "rate": 24.0,
                                },
                            },
                            "media_reference": {
                                "OTIO_SCHEMA": "ExternalReference.1",
                                "target_url": f"media://clip_{i}.mov",
                            },
                            "effects": [],
                            "metadata": {},
                        }
                        for i in range(n_clips)
                    ],
                }
            ],
        },
    }


def test_module_surface():
    """The four public exports exist."""
    assert hasattr(vedit, "Repo")
    assert hasattr(vedit, "Change")
    assert hasattr(vedit, "diff")
    assert hasattr(vedit, "VeditError")


def test_init_and_commit_returns_hash():
    with tempfile.TemporaryDirectory() as tmp:
        repo = vedit.Repo.init(tmp)
        assert pathlib.Path(repo.root).is_dir()
        assert pathlib.Path(repo.root).name == ".vedit"

        h = repo.commit(make_timeline("v0", 3), message="initial")
        assert h.startswith("sha256:")
        assert len(h) > 20


def test_init_twice_raises():
    with tempfile.TemporaryDirectory() as tmp:
        vedit.Repo.init(tmp)
        with pytest.raises(vedit.VeditError):
            vedit.Repo.init(tmp)


def test_diff_no_repo_returns_change_objects():
    a = make_timeline("doc", 3)
    b = make_timeline("doc", 5)
    changes = vedit.diff(a, b)
    assert len(changes) == 2
    assert all(c.op == "added" for c in changes)
    assert all("clip" in c.to_dict() for c in changes)


def test_full_branching_workflow():
    with tempfile.TemporaryDirectory() as tmp:
        repo = vedit.Repo.init(tmp)
        h0 = repo.commit(make_timeline("v0", 3), message="initial")

        repo.create_branch("alt", at="HEAD")
        assert sorted(name for name, _ in repo.list_branches()) == ["alt", "main"]
        assert repo.current_branch() == "main"

        repo.switch_branch("alt")
        assert repo.current_branch() == "alt"

        h1 = repo.commit(make_timeline("v1", 4), message="add a clip")
        assert h1 != h0

        # main still at h0; alt at h1.
        targets = dict(repo.list_branches())
        assert targets["main"] == h0
        assert targets["alt"] == h1

        # Diff between branches finds the added clip.
        changes = repo.diff_refs("main", "alt")
        assert len(changes) == 1
        assert changes[0].op == "added"
        assert changes[0].to_dict()["clip"]["name"] == "clip_3"

        # Reading a timeline at a ref returns the original dict (lossless
        # after canonicalization).
        alt_tl = repo.read_timeline("alt")
        assert len(alt_tl["tracks"]["children"][0]["children"]) == 4

        main_tl = repo.read_timeline("main")
        assert len(main_tl["tracks"]["children"][0]["children"]) == 3

        # Log walks history newest-first.
        log = repo.log()  # current = alt
        assert len(log) == 2
        assert log[0][0] == h1
        assert log[1][0] == h0

        log_main = repo.log("main")
        assert len(log_main) == 1
        assert log_main[0][0] == h0


def test_cannot_delete_current_branch():
    with tempfile.TemporaryDirectory() as tmp:
        repo = vedit.Repo.init(tmp)
        repo.commit(make_timeline("v", 1), message="v")
        with pytest.raises(vedit.VeditError):
            repo.delete_branch("main")


def test_resolve_short_hash_and_head():
    with tempfile.TemporaryDirectory() as tmp:
        repo = vedit.Repo.init(tmp)
        h = repo.commit(make_timeline("v", 1), message="v")
        assert repo.resolve("HEAD") == h
        assert repo.resolve("main") == h
        # Short hash (without prefix).
        body = h.removeprefix("sha256:")
        assert repo.resolve(body[:10]) == h


def test_change_objects_are_iterable_and_have_op_and_dict():
    a = make_timeline("doc", 3)
    b = make_timeline("doc", 4)
    changes = vedit.diff(a, b)
    for c in changes:
        assert isinstance(c.op, str)
        d = c.to_dict()
        assert isinstance(d, dict)
        assert d["op"] == c.op
