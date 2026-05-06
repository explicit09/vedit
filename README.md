# vedit

Version control for video timelines.

```
$ vedit diff intro_v1.otio intro_v2.otio
  Trimmed "drone_shot_04" by 1.8s (in)
  Moved "interview_take_2" before "b_roll_03"
  Added crossfade between "title_card" and "scene_01" (12 frames)
  Removed "music_bed_v1", added "music_bed_v2"
  Adjusted volume on "voiceover_master" from -12dB to -8dB
```

That output is the whole point. Editors should not have to scrub two timelines side-by-side just to understand what changed.

## The problem

Open a video project from last week. What changed since the v3 export? Nobody knows. The project file is a binary blob. `git diff` returns garbage. You scrub the timeline trying to spot what's different. Eventually you give up and just trust the file.

This is how every video team works. It's bad.

## What vedit is

A CLI that reads video timelines and tells you what changed, in plain English. Not "line 2,847 of timeline.json differs." Actual edit decisions: clip trimmed, transition added, volume changed, b-roll reordered.

It works on [OpenTimelineIO](https://github.com/AcademySoftwareFoundation/OpenTimelineIO) (OTIO) files. OTIO is the open standard for video timelines, supported by DaVinci Resolve natively and by Premiere and Final Cut via adapters. If your editor speaks OTIO, vedit works on your edits.

vedit is editor-agnostic. It does not replace your NLE. It sits next to it, the way git sits next to your text editor.

Built for editors, post houses, and AI agents that write to timelines.

## Status

Pre-alpha. The first thing being built is `vedit diff` against a hand-built corpus of OTIO file pairs with known semantic differences. Everything else is downstream of that working.

If `vedit diff` feels like a product, the rest follows. If it doesn't, the project is wrong.

## Roadmap

**v0.1** — `vedit diff` between two OTIO files. Human-readable output. No repo, no commits, no history. Just the diff.

**v0.2** — `vedit init` and `vedit commit` snapshot timelines into a content-addressed object store. Media files are hashed and tracked, not stored.

**v0.3** — `vedit log`, `vedit branch`, `vedit checkout`. Branching for alternate cuts.

**v0.4** — `vedit merge` with conflict surfacing.

```
$ vedit merge trailer_cut social_cut

  Conflict: both branches modified "intro_sequence"
  Conflict: both branches retimed "drone_shot_04" differently
  Auto-merged: 7 non-overlapping edits
```

**Later** — Editor adapters, visual diff reports, remotes.

## Why this is hard

Three reasons.

**Stable identity.** Every clip, track, and effect needs a UUID that survives a round-trip through your editor. If Resolve strips the IDs on export, "clip moved" becomes indistinguishable from "clip deleted plus new clip inserted," and semantic merge is impossible. The first real engineering question is whether OTIO metadata survives the editors people actually use.

**Semantic diff, not text diff.** Diffing OTIO as JSON gives garbage. Reordering one clip changes line numbers everywhere. The diff has to operate on timeline semantics: clips, tracks, effects, transitions. This is the actual technical contribution of the project.

**Conflicts must surface, not auto-resolve.** Creative decisions are not commutative. If two editors retime the same clip differently, that is a conflict the human has to resolve, not something a CRDT silently merges. The merge engine has to know the difference between "these edits don't overlap" and "these edits both touch the same object" and fail loudly on the second.

## Why now

AI is about to start generating and modifying video edits at scale. When an agent regenerates a scene, you need to know what it changed. The current answer is "open the project and squint." That does not hold.

The infrastructure layer for versioned edits does not exist. Frame.io is review and approval, not version control. Perforce is enterprise and proprietary. Nothing open source operates at the level of timeline semantics.

If a layer like this exists, it becomes the substrate that editors and AI agents both write to. That's the bet.

## Non-goals

- Not a video editor.
- Not cloud storage.
- Not a rendering service.
- Not a collaboration platform.
- Not a media asset manager.

vedit is the version control layer. Everything else is somebody else's product.

## Contributing

Not yet. The README is ahead of the code. Once `vedit diff` works on the test corpus, contributions open up.

## License

Apache 2.0. See [LICENSE](LICENSE).
