# vedit

Version control for video edits.
Track every version of your timeline. Branch off, compare, and merge cuts.

```
$ vedit diff intro_v1.otio intro_v2.otio
  Moved "b_roll_03" before "interview_take_2"
  Trimmed "drone_shot_04" by 1.80s (in)
  Added crossfade between "title_card" and "drone_shot_04" (12 frames)
  Added audio track "A1"
```

That's a real edit being summarized in plain English. No screenshots, no scrubbing, no opening your editor.

## What it does

vedit watches your video timeline and tells you what changed. Snapshot a version, branch off to try a different cut, compare two versions, merge them back together. It works on any video project that exports [OpenTimelineIO](https://github.com/AcademySoftwareFoundation/OpenTimelineIO) — DaVinci Resolve, Premiere, Final Cut Pro, or an AI tool that generates edits.

It's built for two audiences. **AI agents that generate or modify edits** can commit every output, branch to try alternatives, and read structured diffs to know what they changed. **Human editors** get a real edit history that survives across sessions, plus the option to auto-commit every export. The same engine serves both.

If you've used Git for code, vedit is the same idea for video edits.

Under the hood: a Rust core with a content-addressed object store and a three-way merge engine, plus Python bindings via [maturin](https://www.maturin.rs/). Architecture details in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Install

```
python3 -m venv .venv && source .venv/bin/activate
pip install pyvedit
```

The PyPI distribution is **pyvedit**. The Python module is **vedit** — so you write `import vedit` regardless of how you installed it. (The `vedit` name on PyPI was taken by an unrelated 2016 project; we kept the brand intact and only renamed the install line.)

## Use it from the terminal

```
$ vedit init
$ vedit commit timeline.otio -m "Initial cut"
$ vedit commit timeline.otio -m "Trimmed the intro"
$ vedit show HEAD
```

`vedit show HEAD` prints the diff against the previous commit, in plain English. That's the loop.

## Use it from Python

For AI tools that generate or modify timelines programmatically:

```python
import vedit

repo = vedit.Repo.init("./project")
repo.commit(timeline_dict, message="agent generated v1")

# Try a different approach on a branch.
repo.create_branch("alt")
repo.switch_branch("alt")
repo.commit(other_timeline_dict, message="agent v2 — slower pacing")

# What changed between the two?
for change in repo.diff_refs("main", "alt"):
    print(change.op, change.to_dict())
```

The agent writes timelines as Python dicts and gets back structured `Change` objects — no temp files, no shelling out.

## Auto-commit while you edit

If you're in DaVinci Resolve, you can wire vedit to commit automatically every time you export your timeline. Configure once, then forget about it — every export becomes a version.

```
$ vedit watch timeline.otio
Watching timeline.otio
[main 4758e4a] Initial commit: 1 track(s), 4 clip(s)
[main f2d8815] Trimmed "drone_shot_04" by 1.80s (in)
```

The commit messages write themselves from the diff. Setup takes about five minutes — see [docs/RESOLVE.md](docs/RESOLVE.md).

## More

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — how vedit works under the hood, design choices, comparison with related projects.
- [docs/RESOLVE.md](docs/RESOLVE.md) — DaVinci Resolve setup.
- [GitHub issues](https://github.com/explicit09/vedit/issues) — bugs, ideas, what's planned.

## License

[Apache 2.0](LICENSE). Free to use, fork, build on. If you build something with vedit, I'd love to hear about it — open an issue or send a note.
