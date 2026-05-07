# Roadmap

## Shipped (v0.0.1)

- **Semantic diff** — `vedit diff` reads two OTIO files and emits structured changes (trim, move, add, remove, replace, effects, transitions, tracks). Prose for humans, JSON for agents.
- **Local repo** — `vedit init`, `commit`, `log`, `show`, `checkout`. Content-addressed object store, gzipped JSON, SHA-256 over canonical form.
- **Branches** — `vedit branch`, `branches`, `checkout <branch>`. Divergent histories.
- **Three-way merge** — `vedit merge`. Fast-forward when possible; structured conflict reporting at track granularity when not.
- **Auto-commit** — `vedit watch <file>` auto-commits when the file changes, with a message generated from the diff.
- **Python bindings** — `pip install pyvedit`. `repo.commit(timeline_dict, ...)`, `vedit.diff(a, b)`, etc. The agent-facing surface, no temp files.

Validated against real DaVinci Resolve projects via end-to-end round-trip. 79 automated tests across Rust + Python.

## Up next (v0.6.1, no commitment date)

- **Clip-granular merge conflicts.** Today, two editors touching different clips on the same track will produce a spurious conflict. The fix: refine the merge engine to operate at clip level inside a track instead of treating tracks atomically.
- **Python `repo.merge()` binding.** Currently agents have to shell out to the CLI to merge.
- **Better commit metadata.** Author config in `.vedit/config`, more useful `vedit log` formatting.

## Later

- **`vedit blame`** — for any clip in a timeline, when did it enter the cut, by whom, and what's been done to it since.
- **Editor-side scripts beyond Resolve.** Premiere ExtendScript and FCP X workflows that pair with `vedit watch`.
- **Remotes.** `vedit push` / `pull`. Mostly mechanical once the local model is solid.
- **Node bindings.** Same shape as the Python bindings, for JS/TS-based AI tools.
- **Rust crate on crates.io.** `cargo install vedit` for the CLI without building from source.
- **macOS Intel wheels.** Dropped from v0.0.1 because the GitHub Actions Intel macOS runner pool is unreliable. Add back if anyone needs it.
- **Visual diff renderer.** A small static-HTML report from `vedit diff --html` for sharing with non-technical stakeholders.

## Things vedit will not do

- Become a video editor.
- Store media files. The OTIO file references media by URL; the actual `.mov` / `.wav` files stay on whatever shared storage you already use.
- Use AI to silently auto-resolve creative conflicts. Conflicts surface to humans by design.
