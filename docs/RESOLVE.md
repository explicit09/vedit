# Using Vedit inside DaVinci Resolve

The Vedit Workflow Integration snapshots the active Resolve timeline and shows
its version history without asking you to import or export OTIO, choose a file,
or keep a terminal open.

## Requirements

- DaVinci Resolve **Studio 20.1 or newer**.
- An Apple silicon Mac for V1.
- A local checkout of this repository.

Resolve's Workflow Integration API is a Studio feature. The current package
is macOS arm64; the source is structured for a later Windows package.

## Install

From the repository root:

```bash
./integrations/resolve/scripts/install-macos.sh
```

The installer:

1. Builds the release `vedit` sidecar.
2. Copies the integration to Resolve's Workflow Integration Plugins directory.
3. Copies Blackmagic's compatible `WorkflowIntegration.node` from the Resolve
   developer kit already installed on your Mac.
4. Validates the plugin identity, production files, executables, and arm64
   architecture.

The Blackmagic native module is never stored in this repository or distributed
as part of Vedit.

Restart Resolve after installation. Resolve discovers Workflow Integrations only
during launch.

## Take a snapshot

1. Open a Resolve project and make its timeline active.
2. Choose **Workspace → Workflow Integrations → Vedit**.
3. Confirm that Vedit shows the correct project and timeline.
4. Click **Snapshot timeline**.

Resolve exports the active timeline to Vedit's managed workspace in the
background. Vedit commits it and immediately shows the latest change and recent
history. OTIO is an internal transport format here, not a user workflow.

The first snapshot describes the number of tracks and clips. Later snapshots
describe semantic changes such as trims, moves, replacements, transitions, and
effects.

## Automatic snapshots

Turn on **Auto snapshots** in the Vedit window to check the active timeline
every 30 seconds. The setting is opt-in and stays saved for the next time the
integration opens. Checks run only while the Vedit window is open.

Automatic checks compare the pending Resolve export with the latest timeline
semantically. If no edit decisions changed, Vedit discards the pending export
and creates no commit. If something changed, it uses the same serialized save
and review path as the Snapshot button. Manual snapshots remain intentional and
always create a snapshot when clicked.

## Where history lives

Each Resolve timeline gets a local repository under:

```text
~/Movies/Vedit/<project>/<timeline>/
```

The project folder is readable, while the timeline folder uses a short identity
key so its history remains stable when the timeline is renamed. Media is not
copied. Each repository stores only structured timeline snapshots and Vedit
history.

You can inspect one from Terminal if needed, but normal editor use does not
require it:

```bash
cd ~/Movies/Vedit/<project>/<timeline>
vedit log
vedit show HEAD
```

## What a snapshot includes

Vedit stores the edit information present in Resolve's OTIO export: timeline
structure, clips, source ranges, transitions, supported effects, audio levels,
and Resolve metadata that survives OTIO.

Resolve-internal information that is absent from OTIO is not versioned. This can
include color grades, render-cache state, and some Fusion compositions. Keep
normal Resolve project backups; Vedit is timeline history, not a replacement for
the project database or `.drp` archives.

## Troubleshooting

**Vedit is missing from Workflow Integrations.** Confirm you are using Resolve
Studio 20.1+, run the installer again, and fully restart Resolve. Validate the
installed files with:

```bash
node integrations/resolve/scripts/validate-install.js
```

**The installer cannot find `WorkflowIntegration.node`.** The installed Resolve
developer kit is missing or older than 20.1. Reinstall or update Resolve Studio,
then rerun the Vedit installer. The script intentionally does not download or
redistribute Blackmagic's native module.

**Vedit says to open a project or timeline.** The integration only snapshots the
currently active timeline. Open the project, select a timeline in the Edit or Cut
page, then click **Reconnect**.

**Snapshot failed.** Expand **Technical details** in the Vedit error banner. A
failed export is never committed, and the previous successful history remains
visible.

**A snapshot does not contain a color grade.** Color grades are stored by Resolve
outside the OTIO timeline representation. Continue backing up the Resolve project
alongside Vedit history.

## Advanced fallback: export watcher

The earlier script-and-watcher workflow remains available for unsupported hosts
and custom automation. Export an OTIO file to a stable path using a Resolve Lua
or Python script, then run:

```bash
vedit init
vedit watch timeline.otio
```

That fallback exposes the OTIO file and requires a terminal. Resolve Studio users
should use the Workflow Integration above.

## Other editors

The Resolve plugin is a thin adapter over the editor-agnostic Vedit engine. A
future Premiere integration can provide the same Snapshot and History interface
while translating through Premiere's own host API. The Vedit repository, diff,
branch, and merge logic stays shared.
