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

vedit watches your video timeline and tells you what changed. Snapshot a version, branch off to try a different cut, compare two versions, merge them back together. It works on any video project that exports [OpenTimelineIO](https://github.com/AcademySoftwareFoundation/OpenTimelineIO) — DaVinci Resolve, Premiere, Final Cut Pro, or an AI tool that generates edits for you.

If you've used Git for code, vedit is the same idea for video edits.

## Install

```
python3 -m venv .venv && source .venv/bin/activate
pip install pyvedit
```

The PyPI package is `pyvedit`. The Python module is `vedit`.

## Use it

From the terminal:

```
$ vedit init
$ vedit commit timeline.otio -m "Initial cut"
$ vedit commit timeline.otio -m "Trimmed the intro"
$ vedit show HEAD
```

`vedit show HEAD` prints the diff against the previous commit, in plain English. That's the loop.

From Python (for AI tools that generate timelines):

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

## Auto-commit while you edit

If you're using DaVinci Resolve, you can wire vedit to commit automatically every time you export your timeline. See [docs/RESOLVE.md](docs/RESOLVE.md) for the setup — about five minutes once.

```
$ vedit watch timeline.otio
Watching timeline.otio
[main 4758e4a] Initial commit: 1 track(s), 4 clip(s)
[main f2d8815] Trimmed "drone_shot_04" by 1.80s (in)
```

## More

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — how vedit works under the hood, design choices, comparison with related projects.
- [docs/RESOLVE.md](docs/RESOLVE.md) — DaVinci Resolve setup.

## License

Apache 2.0. Free to use, fork, build on. If you build something with vedit, I'd love to hear about it — open an issue or send a note.
