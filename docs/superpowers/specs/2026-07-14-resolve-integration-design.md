# DaVinci Resolve Integration Design

## Goal

Make Vedit usable from DaVinci Resolve without asking an editor to open a
terminal or manually import or export OTIO. The first release proves one
complete workflow: open Vedit from Resolve, snapshot the active timeline, and
immediately review its version history and latest semantic changes.

## Product boundary

Vedit remains the editor-agnostic version-control engine. The Resolve
integration is a thin host adapter; it does not duplicate diff, commit, branch,
or merge logic.

### V1: snapshot and review

- Launch from `Workspace -> Workflow Integrations -> Vedit` in Resolve Studio.
- Detect the current Resolve project and active timeline.
- Export the active timeline as OTIO to a Vedit-managed local workspace without
  showing a file picker.
- Initialize a per-timeline Vedit repository on first use.
- Create a commit with Vedit's existing auto-generated message.
- Show the active project, timeline, current branch, recent commits, and the
  latest human-readable diff in the integration window.
- Surface actionable errors for no project, no timeline, export failure,
  missing Vedit binary, and command failure.
- Require no terminal and no manual OTIO operation in the normal workflow.

### V2: automatic snapshots

After V1 is proven inside the real Resolve runtime:

- Add an opt-in automatic snapshot toggle.
- Poll the active timeline on a conservative interval because Resolve exposes
  no timeline-edit callback.
- Export to a temporary OTIO file and skip commits with no semantic changes.
- Serialize snapshot attempts so concurrent exports cannot corrupt history.
- Show `Watching`, `Saving`, `Up to date`, and error states in the UI.

Branch switching, timeline restoration, merge resolution, cloud remotes, and
Premiere support remain later releases. Restoration is intentionally excluded
until Vedit can prove non-destructive import behavior on real Resolve projects.

## Approaches considered

### Recommended: Resolve Electron integration plus bundled Vedit CLI

Use Resolve's Workflow Integration API for host access and a sandboxed Electron
window for UI. The main process exports OTIO through Resolve's promise-based
JavaScript API and invokes an architecture-matched Vedit CLI sidecar. This gives
the editor-native experience now while preserving Vedit's reusable core.

### Rejected for V1: Python or Lua UIManager script

This is simpler to prototype but produces a less flexible interface, complicates
packaging Python dependencies, and does not establish the reusable HTML UI shell
needed for history and later conflict review.

### Rejected for V1: local HTTP daemon

A daemon would give every NLE a common protocol, but adds lifecycle, port,
authentication, and installer complexity before the Resolve workflow is proven.
The plugin's engine boundary will remain narrow so a daemon can replace the CLI
sidecar later without changing the UI contract.

## Architecture

```text
Resolve 20.1+ Studio
  -> WorkflowIntegration.node (provided by installed Resolve SDK)
  -> plugin main process
       -> Resolve adapter: identify project/timeline and export OTIO
       -> workspace service: choose managed per-timeline paths
       -> Vedit runner: init, commit, log, show
  -> preload IPC allowlist
  -> sandboxed renderer: status, Snapshot button, history, latest diff
```

The integration files live under `integrations/resolve/`. The renderer receives
plain serializable view models through a minimal preload bridge; it never gets
direct Node, filesystem, child-process, or Resolve access.

## Managed workspace

Each timeline maps to:

```text
~/Movies/Vedit/<project-slug>--<project-key>/timeline--<timeline-key>/
  timeline.otio
  .vedit/
```

Resolve does not expose a stable project ID, so `project-key` is a short hash of
the Resolve project-library identity when available plus the project name.
`timeline-key` is derived from `Timeline.GetUniqueId()`. The timeline directory
does not include its mutable name, so renaming an active timeline cannot split
its history. The project directory remains readable and carries a database-aware
identity key.

The plugin writes exports to a temporary sibling and atomically replaces
`timeline.otio` only after Resolve reports a successful export. Vedit's existing
atomic repository writes protect the commit database.

## Snapshot data flow

1. The renderer requests `snapshotActiveTimeline()` through preload IPC.
2. The main process returns the same in-flight operation while one is running.
3. The Resolve adapter gets the current project and timeline, including names
   and the timeline unique ID.
4. The workspace service creates the managed directory.
5. Resolve exports OTIO to `timeline.otio.pending`.
6. The service atomically replaces `timeline.otio`.
7. The runner executes `vedit init` if `.vedit` is absent, then
   `vedit commit timeline.otio` with that workspace as its current directory.
8. The runner loads history and `vedit show HEAD`.
9. The renderer displays the new commit and latest diff.

OTIO is never presented as a user task. It is an adapter transport format.

## Engine and installation boundary

The repository will include:

- Plain JavaScript, HTML, and CSS plugin source with no runtime npm packages.
- Node built-in tests for services and controller behavior.
- An installer script that builds the release Vedit CLI, copies plugin files to
  Resolve's Workflow Integration Plugins directory, and copies
  `WorkflowIntegration.node` from the installed Resolve 20.1+ developer sample.
- A validation script that verifies required files without starting Resolve.

The Blackmagic native module is not committed or redistributed. Installation
fails with a precise message when the compatible developer module is missing.

## UI

The V1 window is compact and operational rather than Git-like:

- Header: Vedit, connection state, active project/timeline.
- Primary action: `Snapshot timeline`.
- Latest change card: commit message, short hash, time, semantic diff lines.
- History list: newest commits with branch indicator.
- Empty state: explains that the first snapshot is automatic and local.
- Error banner: specific failure plus a retry action.

No repository path, OTIO path, terminal command, or implementation jargon is
shown by default.

## Error handling

- Resolve unavailable: keep the window open and offer reconnect.
- No current project/timeline: disable snapshot and explain what to open.
- Export returns false or produces no file: do not commit; preserve prior state.
- Vedit process fails: capture exit code and stderr, show a concise error, retain
  diagnostic details for an expandable section.
- A snapshot is already running: return the same in-flight operation rather than
  launching another.
- Renderer closes: clean up the Resolve integration interface.

## Testing and proof

Automated tests use dependency injection around Resolve, filesystem, and command
execution. They cover workspace identity, successful first and later snapshots,
empty-state history, export failure, missing timeline, command failure, and
concurrent-click serialization. Renderer tests cover state transitions and safe
text rendering.

V1 is done only after all of the following are observed in the installed app:

1. Resolve lists `Vedit` under `Workspace -> Workflow Integrations`.
2. The Vedit window identifies a real open project and timeline.
3. Clicking `Snapshot timeline` creates a real `.vedit` commit without a file
   dialog or terminal interaction.
4. The window displays that commit and its semantic changes.
5. A second timeline edit and snapshot produces a second, correct diff.

V2 begins only after those five checks pass.

## Compatibility

- V1 target: DaVinci Resolve Studio 20.1 or newer.
- Initial platform: macOS Apple silicon, tested on installed Resolve 20.3.2.
- Renderer security: sandbox and context isolation enabled; no Node integration.
- Plugin source stays portable to Windows, but Windows packaging is not a V1
  completion requirement.
