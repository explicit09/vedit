# How vedit works

This is the technical companion to the README. If you're deciding whether to use vedit, the README is enough. This doc is for people who want to understand the design or build on top of it.

## What problem vedit solves

Video projects have two fundamentally different things mashed together:

- **The edit** — the timeline. Cuts, trims, transitions, effects, audio levels. Kilobytes of structured data describing creative decisions.
- **The media** — the actual `.mov` files. Gigabytes to terabytes of opaque pixels.

Existing tools treat both as one. The edit is locked inside an editor's project file (binary, undiffable, single-user) and the media lives next to it. There's no good way to ask "what changed since last week?" because the project file is opaque, and `git diff` on it returns garbage.

vedit separates them. The edit becomes versioned, diffable, branchable, mergeable — like source code. The media stays where it is, referenced by URL but never copied. vedit only cares about edit decisions.

## How it works

Six things, all borrowed from Git:

**Content-addressed object store.** Every commit and every timeline snapshot is hashed by SHA-256 of its canonical JSON. Two commits with identical content collapse to one stored object.

**Commit graph.** Each commit has parents. The graph of commits is a DAG — directed, acyclic. `vedit log` walks it. `vedit merge` finds the lowest common ancestor.

**Refs.** A branch is a pointer to a commit hash. `refs/heads/main` is literally a file containing `sha256:abc...`. Switching branches moves a HEAD pointer.

**Snapshots, not diffs.** Each commit stores a complete snapshot. Diffs are computed at read time by comparing two snapshots.

**Three-way merge.** Find the merge base, compute changes on each side, combine if non-overlapping, conflict if overlapping.

**Working directory separate from object store.** Your `.otio` file is the working copy; `.vedit/` is the database.

What's different from Git: vedit treats timelines as structured objects from day one (clips, tracks, transitions), so its diff says "trimmed drone_shot_04 by 1.8s" instead of "line 247 changed." And vedit identifies clips by content fingerprint (media reference + source range, with a name fallback) instead of by file path.

## On disk

```
your-project/
├── timeline.otio          # the working copy — edit this in your NLE
└── .vedit/
    ├── HEAD               # "ref: refs/heads/main"
    ├── refs/heads/
    │   ├── main           # "sha256:abc..."
    │   └── alt_cut        # "sha256:def..."
    └── objects/
        └── 7c/
            └── f990f...   # gzipped JSON — a commit or a timeline
```

Everything is plain files. You can `tar` `.vedit/`, email it, `cp -r` it. Inspect any object with `zcat .vedit/objects/7c/f990f... | jq`.

Media files are not in `.vedit/`. The OTIO file references them by URL. Your editor's media stays wherever it already lives.

## Two engineering bets that make vedit work

**Identity without metadata.** Every editor we tested strips third-party metadata from OTIO files on round-trip — DaVinci Resolve discards every namespace except its own. So vedit cannot rely on "stash a UUID in the clip's metadata and trust it survives." Instead, the matcher infers identity from content: `(media_reference, source_range)` is a strong fingerprint, `(media_reference, name)` is a weak fallback. Same approach Git uses for rename detection.

When multiple clips share a fingerprint (the same media reused on the timeline several times — extremely common in real Resolve projects), the matcher pairs them by closest position, then runs longest-increasing-subsequence over the resulting matches to suppress spurious "moved" reports caused by insertions or deletions shifting absolute indices.

**Conflicts surface, not auto-resolve.** Creative decisions are not commutative. If two editors retime the same clip differently, that's a conflict the human resolves — not something a CRDT silently merges. vedit's merge engine is conservative: anything ambiguous becomes a conflict. The failure mode is "false positive conflict," never silent corruption.

## The merge algorithm

Three-way merge for ordered trees, simplified from Lindholm's [3DM](https://www.cis.upenn.edu/~bcpierce/courses/dd/papers/3dm-thesis.ps).

For each track present in any of `base` / `ours` / `theirs`:

- Neither side touched it → keep base.
- Only one side touched it → keep that side.
- Both touched it the same way → keep one.
- Both touched it differently → conflict.

Today, conflicts are reported at track granularity. Two editors touching different clips on the same track will produce a spurious conflict. Refining to clip granularity is the next priority — see [GitHub issues](https://github.com/explicit09/vedit/issues).

Three conflict types: `BothModified`, `BothAdded`, `DeleteVsModify`. Conflicts are structured (JSON, agent-readable). There is no in-place resolution UX yet — the user picks a branch, edits to incorporate the other side's changes, and re-merges.

## The diff engine

Output shape is the same for humans and agents — a list of structured changes:

- `Trimmed { clip, before_range, after_range }`
- `Moved { clip, from_index, to_index, after_neighbor, before_neighbor }`
- `Added { clip, track, index }`
- `Removed { clip, track, index }`
- `Replaced { clip, before_media, after_media }`
- `EffectsChanged { clip, before, after }`
- `TransitionAdded` / `TransitionRemoved` / `TransitionChanged`
- `TrackAdded` / `TrackRemoved`

The CLI renders these as prose; `--json` and the Python bindings hand them back as structured objects. Same engine, two surfaces.

The renderer collapses video/audio mirror pairs (most edits in Resolve happen on a video clip and its synced audio simultaneously), keeping one prose line tagged `(with synced audio)`. The JSON output stays uncollapsed — agents see per-track resolution.

## Round-trip fidelity

vedit re-emits canonical OTIO when you `checkout` a commit. The recovered file is **semantically identical** to the original — every clip, range, media reference, transition, effect, and Resolve metadata block is preserved. It is **not byte-identical**: whitespace, key ordering, and floating-point string representations get normalized. In one round-trip on a real Resolve project the file shrank ~35%, and the recovered timeline opened cleanly when re-imported.

Editor-internal data that doesn't appear in OTIO (color grades, render-cache hints, Fusion compositions in some cases) is never seen by vedit and isn't preserved. Treat vedit as a snapshot tool for your timeline, not a replacement for your `.drp` / `.prproj` project file.

## Comparison with related projects

[**vit**](https://github.com/LucasHJin/vit) is the closest project. It targets human editors collaborating in DaVinci Resolve, with a Resolve panel plugin that splits timelines into domain-specific JSON files (cuts, color, audio) and uses Gemini for cross-domain merge resolution. It's excellent for that workflow.

vedit is a different layer. Where vit is a Resolve-integrated tool for humans, vedit is editor-agnostic infrastructure designed to be consumed programmatically — by AI tools, automation pipelines, and any system that produces or modifies OTIO timelines. vit identifies clips by track/position; vedit identifies them by content fingerprint, which is what makes the editor-agnostic story work.

Both projects can coexist. They serve different layers.

[**Pijul**](https://pijul.org/) takes a different approach to merge entirely — patches as graph operations, conflict-free representation, partial ordering. Beautiful theory, wrong fit for video, where the timeline is ultimately a totally-ordered sequence and creative conflicts should surface to humans.

## What vedit is not

Not a video editor. Not media storage. Not a rendering service. Not a collaboration platform. Not an asset manager. vedit is the version-control layer for one OTIO file. Everything above is somebody else's product.

## License

Apache 2.0.
