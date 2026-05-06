# vedit

Version control for video timelines.

```
$ vedit diff intro_v1.otio intro_v2.otio
  Moved "b_roll_03" before "interview_take_2"
  Effects on "interview_take_2" changed (0 → 1)
  Trimmed "drone_shot_04" by 1.80s (in)
  Added crossfade between "title_card" and "drone_shot_04" (12 frames)
  Added audio track "A1"
```

That output is the whole point. Editors should not have to scrub two timelines side-by-side just to understand what changed.

## The problem

Open a video project from last week. What changed since the v3 export? Nobody knows. The project file is a binary blob. `git diff` returns garbage. You scrub the timeline trying to spot what's different. Eventually you give up and just trust the file.

This is how every video team works. It's bad.

It's also a problem for AI video tools. When an agent generates an edit, regenerates a scene, or branches an alternate cut, the system needs a structured answer to "what changed?" — not a binary blob to compare. Today, every team building an AI video product reinvents the same primitive: snapshot, branch, diff, merge. They shouldn't have to.

## What vedit is

A CLI and Rust library that reads video timelines and produces a structured semantic diff: trims, moves, additions, removals, transitions, replaced media, effect changes, track-level deltas. The same engine drives a human-readable rendering and a machine-readable JSON output.

It works on [OpenTimelineIO](https://github.com/AcademySoftwareFoundation/OpenTimelineIO) (OTIO) files. OTIO is the open standard for video timelines, supported by DaVinci Resolve natively and by Premiere and Final Cut via adapters. If your editor or your agent emits OTIO, vedit works on its output.

vedit is editor-agnostic. It does not replace your NLE. It sits next to it, the way git sits next to your text editor.

Built for AI agents that edit video, the developers building them, and human editors who want to know what changed.

## Quickstart

```
$ git clone https://github.com/explicit09/vedit.git && cd vedit
$ cargo build --release
$ alias vedit="$PWD/target/release/vedit"

# Show what changed between two timelines, no repo required.
$ vedit diff examples/intro_v1.otio examples/intro_v2.otio
  Moved "b_roll_03" before "interview_take_2"
  Effects on "interview_take_2" changed (0 → 1)
  Trimmed "drone_shot_04" by 1.80s (in)
  Added crossfade between "title_card" and "drone_shot_04" (12 frames)
  Added audio track "A1"

# Same engine, machine-readable output for AI agents.
$ vedit diff examples/intro_v1.otio examples/intro_v2.otio --json
[ { "op": "moved", "clip": { "name": "b_roll_03", ... }, ... }, ... ]

# Initialize a repo, snapshot a timeline, walk history.
$ mkdir my_project && cd my_project
$ cp /path/to/timeline.otio .
$ vedit init
Initialized empty vedit repository in /path/to/my_project/.vedit
$ vedit commit timeline.otio -m "Initial cut"
[main (root) 7af80fe] Initial cut
$ # ...edit timeline.otio in your NLE, export OTIO again...
$ vedit commit timeline.otio -m "Add crossfade"
[main 65b58e8] Add crossfade
$ vedit log
65b58e8  Add crossfade  (HEAD -> main)
7af80fe  Initial cut
$ vedit show HEAD
commit 65b58e8
Author: ...
Date:   2026-05-06T09:18:23Z

    Add crossfade

  Trimmed "drone_shot_04" by 1.80s (in)
  Added crossfade between "title_card" and "drone_shot_04" (12 frames)
$ vedit checkout 7af80fe -o earlier.otio
Wrote timeline at 7af80fe to earlier.otio
```

## Built for AI agents

The `--json` output is the primary surface for programmatic consumers. Each change is a tagged enum variant with structured data: `trimmed`, `moved`, `added`, `removed`, `replaced`, `effects_changed`, `transition_added`, `transition_removed`, `track_added`, `track_removed`. Agents can branch on `op`, read clip identity from `clip.name` and `clip.media_reference`, and act on the deltas without ever parsing the OTIO files themselves.

The same engine powers the prose output, so what an agent sees and what a human sees are projections of the same truth.

## Status

**v0.1 + v0.2 work.** `vedit diff` reads two OTIO files, matches clips by content fingerprint, and emits structured changes. `vedit init`/`commit`/`log`/`show`/`checkout` give you a real local repo with a content-addressed object store. 35 tests pass, including against the AcademySoftwareFoundation OTIO samples (`multitrack`, `transition`, `effects`, `nested_example`, `premiere_example`, `screening_example`, `generator_reference_test`).

What's not in yet: branches beyond `main`, merge, remotes. Those land in v0.3 and v0.4.

## Roadmap

**v0.1 — semantic diff** ✓ Done.
- `vedit diff <before> <after>` with prose and `--json` output
- Content-fingerprint matcher (no metadata IDs required)
- Detects trim, move, add, remove, replace, effects, transitions, tracks
- 12 corpus tests, validated against real-world OTIO samples

**v0.2 — local repo and snapshots** ✓ Done.
- `vedit init` creates a `.vedit/` content-addressed object store (gzipped JSON, SHA-256, canonical key ordering)
- `vedit commit <timeline.otio> -m "msg"` snapshots and advances `main`
- `vedit log` walks history newest-first
- `vedit show <ref>` renders a commit's diff against its parent
- `vedit checkout <ref> -o <path>` writes the timeline at any commit back to disk
- Media files are referenced by URL, never stored — vedit is not a media manager
- Author info auto-resolved from `VEDIT_AUTHOR_*` env vars or `git config`
- 23 unit and integration tests covering the storage, ref resolution, and full workflow

**v0.3 — branches.**
- `vedit branch <name>`, `vedit checkout` switching branches, `vedit branches` listing
- Diff and log work across branches

**v0.4 — merge.**
- `vedit merge <branch>` with conflict surfacing
- Non-overlapping edits auto-merge; overlapping edits fail loudly with a structured conflict report

```
$ vedit merge trailer_cut social_cut
  Conflict: both branches modified "intro_sequence"
  Conflict: both branches retimed "drone_shot_04" differently
  Auto-merged: 7 non-overlapping edits
```

**Later** — editor adapters, visual diff reports, remotes, language bindings (Python, Node).

## Why this is hard

Three reasons.

**Identity without metadata.** Editors strip third-party metadata from OTIO files on round-trip — Resolve discards every non-`Resolve_OTIO` namespace. So vedit cannot rely on "stash a UUID in the clip's metadata and trust it survives." Instead, the matcher infers identity from content: `(media_reference, source_range)` is a strong fingerprint, `(media_reference, name)` is a weak fallback. This is the same approach git uses for rename detection — content match, not ID tracking.

**Semantic diff, not text diff.** Diffing OTIO as JSON gives garbage. Reordering one clip changes line numbers everywhere. The diff has to operate on timeline semantics: clips, tracks, effects, transitions. This is the actual technical contribution of the project.

**Conflicts must surface, not auto-resolve.** Creative decisions are not commutative. If two editors retime the same clip differently, that is a conflict the human has to resolve, not something a CRDT silently merges. The merge engine has to know the difference between "these edits don't overlap" and "these edits both touch the same object" and fail loudly on the second.

## Why now

AI is about to start generating and modifying video edits at scale. When an agent regenerates a scene, you need to know what it changed. The current answer is "open the project and squint." That does not hold.

The infrastructure layer for versioned edits does not exist. Frame.io is review and approval, not version control. Perforce is enterprise and proprietary. Nothing open source operates at the level of timeline semantics, and the AI tools that do exist are each rolling their own snapshot/diff systems.

If a layer like this exists, it becomes the substrate that editors and AI agents both write to. That's the bet.

## Prior art

[**vit**](https://github.com/LucasHJin/vit) is the closest project. It targets human editors collaborating in DaVinci Resolve, with a Resolve panel plugin that splits timelines into domain-specific JSON files (`cuts.json`, `color.json`, `audio.json`) and uses Gemini for cross-domain merge resolution. It is excellent for that workflow.

vedit is for the layer below. Where vit is a Resolve-integrated tool for humans, vedit is editor-agnostic infrastructure designed to be consumed programmatically — by AI tools, automation pipelines, and any system that produces or modifies OTIO timelines. vit identifies clips by track/position; vedit identifies them by content fingerprint, which is what makes the editor-agnostic story work.

Both projects can coexist. They serve different layers.

## Non-goals

- Not a video editor.
- Not cloud storage.
- Not a rendering service.
- Not a collaboration platform.
- Not a media asset manager.

vedit is the version control layer. Everything else is somebody else's product.

## Contributing

Open. The diff engine works and the corpus is small enough that adding cases is the highest-leverage contribution. Bug reports against real-world OTIO files are also valuable — if vedit misreads your timeline, file an issue with the input.

## License

Apache 2.0. See [LICENSE](LICENSE).
