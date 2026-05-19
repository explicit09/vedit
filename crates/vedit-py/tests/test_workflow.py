"""End-to-end test of the vedit Python API.

Run from the repo root with the venv active:
    pytest crates/vedit-py/tests/

Or via maturin:
    cd crates/vedit-py && maturin develop --release && pytest tests/
"""

import json
import os
import pathlib
import subprocess
import time
import tempfile

import pytest

import vedit


REPO_ROOT = pathlib.Path(__file__).resolve().parents[3]


@pytest.fixture(scope="session")
def vedit_bin() -> pathlib.Path:
    subprocess.run(
        ["cargo", "build", "--bin", "vedit"],
        cwd=REPO_ROOT,
        check=True,
        text=True,
        capture_output=True,
    )
    return REPO_ROOT / "target" / "debug" / "vedit"


def write_timeline(path: pathlib.Path, name: str, n_clips: int) -> None:
    path.write_text(json.dumps(make_timeline(name, n_clips)), encoding="utf-8")


def run_cli(vedit_bin: pathlib.Path, cwd: pathlib.Path, *args: str) -> subprocess.CompletedProcess:
    env = {
        **os.environ,
        **{
            "VEDIT_AUTHOR_NAME": "test",
            "VEDIT_AUTHOR_EMAIL": "test@example.com",
        },
    }
    return subprocess.run(
        [str(vedit_bin), *args],
        cwd=cwd,
        text=True,
        capture_output=True,
        env=env,
    )


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


def test_python_exposes_richer_effect_and_transition_changes():
    before = make_timeline("doc", 2)
    after = make_timeline("doc", 2)

    before_children = before["tracks"]["children"][0]["children"]
    after_children = after["tracks"]["children"][0]["children"]

    before_children[0]["effects"] = [
        {
            "OTIO_SCHEMA": "Effect.1",
            "name": "blur",
            "metadata": {"radius": 4},
        }
    ]
    after_children[0]["effects"] = [
        {
            "OTIO_SCHEMA": "Effect.1",
            "name": "blur",
            "metadata": {"radius": 12},
        }
    ]

    before_children.insert(
        1,
        {
            "OTIO_SCHEMA": "Transition.1",
            "name": "crossfade",
            "in_offset": {"OTIO_SCHEMA": "RationalTime.1", "value": 6.0, "rate": 24.0},
            "out_offset": {"OTIO_SCHEMA": "RationalTime.1", "value": 6.0, "rate": 24.0},
            "metadata": {},
        },
    )
    after_children.insert(
        1,
        {
            "OTIO_SCHEMA": "Transition.1",
            "name": "dip to black",
            "in_offset": {"OTIO_SCHEMA": "RationalTime.1", "value": 12.0, "rate": 24.0},
            "out_offset": {"OTIO_SCHEMA": "RationalTime.1", "value": 12.0, "rate": 24.0},
            "metadata": {},
        },
    )

    changes = {change.op: change.to_dict() for change in vedit.diff(before, after)}

    effect = changes["effects_changed"]
    assert effect["before"][0]["metadata"]["radius"] == 4
    assert effect["after"][0]["metadata"]["radius"] == 12

    transition = changes["transition_changed"]
    assert transition["before_name"] == "crossfade"
    assert transition["after_name"] == "dip to black"
    assert transition["before_duration"]["value"] == 12.0
    assert transition["after_duration"]["value"] == 24.0


def test_python_reads_commit_made_by_cli(vedit_bin):
    with tempfile.TemporaryDirectory() as tmp:
        workdir = pathlib.Path(tmp)
        timeline_path = workdir / "timeline.otio"
        write_timeline(timeline_path, "cli", 2)

        assert run_cli(vedit_bin, workdir, "init").returncode == 0
        commit = run_cli(vedit_bin, workdir, "commit", "timeline.otio", "-m", "cli initial")
        assert commit.returncode == 0, commit.stderr

        repo = vedit.Repo.open(workdir)
        log = repo.log()
        assert len(log) == 1
        assert log[0][1]["message"] == "cli initial"
        assert len(repo.read_timeline("HEAD")["tracks"]["children"][0]["children"]) == 2


def test_cli_reads_commits_made_by_python(vedit_bin):
    with tempfile.TemporaryDirectory() as tmp:
        workdir = pathlib.Path(tmp)
        repo = vedit.Repo.init(workdir)
        repo.commit(make_timeline("py", 2), message="python initial")
        repo.commit(make_timeline("py", 3), message="python adds clip")

        log = run_cli(vedit_bin, workdir, "log")
        assert log.returncode == 0, log.stderr
        assert "python adds clip" in log.stdout
        assert "python initial" in log.stdout

        show = run_cli(vedit_bin, workdir, "show", "HEAD")
        assert show.returncode == 0, show.stderr
        assert "python adds clip" in show.stdout
        assert 'Added "clip_2"' in show.stdout


def test_watch_once_commits_changed_export(vedit_bin):
    with tempfile.TemporaryDirectory() as tmp:
        workdir = pathlib.Path(tmp)
        timeline_path = workdir / "timeline.otio"
        write_timeline(timeline_path, "watch", 1)
        assert run_cli(vedit_bin, workdir, "init").returncode == 0

        proc = subprocess.Popen(
            [
                str(vedit_bin),
                "watch",
                "timeline.otio",
                "--once",
                "--interval",
                "20",
                "--settle",
                "20",
                "--message-prefix",
                "watch:",
            ],
            cwd=workdir,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env={
                **os.environ,
                "VEDIT_AUTHOR_NAME": "test",
                "VEDIT_AUTHOR_EMAIL": "test@example.com",
            },
        )
        time.sleep(0.1)
        write_timeline(timeline_path, "watch", 2)
        stdout, stderr = proc.communicate(timeout=5)

        assert proc.returncode == 0, stderr
        assert "Watching timeline.otio" in stdout
        assert "watch: Initial commit: 1 track(s), 2 clip(s)" in stdout

        repo = vedit.Repo.open(workdir)
        assert len(repo.log()) == 1
        assert repo.log()[0][1]["message"] == "watch: Initial commit: 1 track(s), 2 clip(s)"


def test_python_observes_two_parent_merge_commit(vedit_bin):
    """
    v0.6 doesn't expose merge through Python yet, but a merge commit
    created via the CLI must be readable through the Python API and
    must report two parents. Pins the cross-tool contract for v0.6.
    """
    with tempfile.TemporaryDirectory() as tmp:
        workdir = pathlib.Path(tmp)
        timeline_path = workdir / "timeline.otio"

        # base
        write_timeline(timeline_path, "base", 1)
        assert run_cli(vedit_bin, workdir, "init").returncode == 0
        assert run_cli(vedit_bin, workdir, "commit", "timeline.otio", "-m", "base").returncode == 0

        # branch alt at base
        assert run_cli(vedit_bin, workdir, "branch", "alt").returncode == 0

        # main: add an audio track (independent of V1 changes)
        main_tl = make_timeline("base", 1)
        main_tl["tracks"]["children"].append({
            "OTIO_SCHEMA": "Track.1",
            "name": "A1",
            "kind": "Audio",
            "children": [],
        })
        timeline_path.write_text(json.dumps(main_tl), encoding="utf-8")
        assert run_cli(vedit_bin, workdir, "commit", "timeline.otio", "-m", "main: add A1").returncode == 0
        head_main = run_cli(vedit_bin, workdir, "log").stdout.splitlines()[0].split()[0]

        # alt: add a clip to V1 (independent of A1)
        assert run_cli(vedit_bin, workdir, "checkout", "alt").returncode == 0
        write_timeline(timeline_path, "base", 2)
        assert run_cli(vedit_bin, workdir, "commit", "timeline.otio", "-m", "alt: add clip").returncode == 0
        head_alt = run_cli(vedit_bin, workdir, "log").stdout.splitlines()[0].split()[0]

        # back to main, merge alt
        assert run_cli(vedit_bin, workdir, "checkout", "main").returncode == 0
        merge = run_cli(vedit_bin, workdir, "merge", "alt")
        assert merge.returncode == 0, merge.stderr
        assert "Merge branch 'alt' into main" in merge.stdout

        # Python sees the merge commit with two parents.
        repo = vedit.Repo.open(workdir)
        log = repo.log()
        assert len(log) >= 3, log
        merge_hash, merge_commit = log[0]
        assert "Merge branch 'alt' into main" in merge_commit["message"]
        parents = merge_commit["parents"]
        assert len(parents) == 2, parents
        # Resolve the short CLI hashes to full hashes for comparison.
        assert repo.resolve(head_main) == parents[0]
        assert repo.resolve(head_alt) == parents[1]


def test_cli_negative_cases_are_pinned(vedit_bin):
    with tempfile.TemporaryDirectory() as tmp:
        workdir = pathlib.Path(tmp)
        valid = workdir / "valid.otio"
        garbage = workdir / "garbage.json"
        empty = workdir / "empty.otio"
        write_timeline(valid, "valid", 1)
        garbage.write_text("{not json", encoding="utf-8")
        empty.write_text("", encoding="utf-8")

        diff = run_cli(vedit_bin, workdir, "diff", "garbage.json", "valid.otio")
        assert diff.returncode != 0
        assert "parsing" in diff.stderr

        assert run_cli(vedit_bin, workdir, "init").returncode == 0
        commit_empty = run_cli(vedit_bin, workdir, "commit", "empty.otio")
        assert commit_empty.returncode != 0
        assert "parsing" in commit_empty.stderr

        commit_valid = run_cli(vedit_bin, workdir, "commit", "valid.otio", "-m", "initial")
        assert commit_valid.returncode == 0, commit_valid.stderr
        merge_missing = run_cli(vedit_bin, workdir, "merge", "missing_branch")
        assert merge_missing.returncode != 0
        assert "missing_branch" in merge_missing.stderr
