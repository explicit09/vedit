# DaVinci Resolve Integration V1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and prove a DaVinci Resolve Studio integration that snapshots the active timeline and displays Vedit history and semantic changes without manual OTIO operations or Terminal use.

**Architecture:** A sandboxed Electron Workflow Integration uses Resolve's promise JavaScript API in the main process, exposes a narrow preload IPC bridge, and invokes a bundled Vedit CLI sidecar. Pure CommonJS services isolate workspace identity, Resolve access, CLI execution, and snapshot orchestration so Node's built-in test runner can verify behavior without launching Resolve.

**Tech Stack:** Rust Vedit CLI, DaVinci Resolve Studio 20.1+ Workflow Integration API, Electron 36-compatible CommonJS, Node built-ins, HTML/CSS, `node:test`, POSIX shell installer.

## Global Constraints

- No file picker, manual OTIO import/export, or Terminal interaction in the normal V1 workflow.
- V1 targets DaVinci Resolve Studio 20.1+ on macOS Apple silicon and must be tested on installed Resolve 20.3.2.
- The renderer uses sandbox and context isolation with no Node integration.
- `WorkflowIntegration.node` is copied from the installed Resolve SDK and is never committed or redistributed.
- The Vedit core remains editor-agnostic; Resolve-specific behavior lives under `integrations/resolve/`.
- A real Resolve run is required before V1 can be called complete.

---

## File structure

- `integrations/resolve/package.json` — metadata and Node test command.
- `integrations/resolve/manifest.xml` — Resolve plugin identity and entrypoint.
- `integrations/resolve/main.js` — Electron lifecycle, Resolve initialization, and IPC registration.
- `integrations/resolve/preload.js` — minimal renderer API allowlist.
- `integrations/resolve/index.html` — accessible V1 shell.
- `integrations/resolve/styles.css` — compact Resolve-adjacent visual system.
- `integrations/resolve/renderer.js` — DOM events and safe view rendering.
- `integrations/resolve/lib/workspace.js` — stable per-timeline workspace paths and atomic export promotion.
- `integrations/resolve/lib/resolve-adapter.js` — active context lookup and OTIO export.
- `integrations/resolve/lib/vedit-runner.js` — child-process calls and CLI output parsing.
- `integrations/resolve/lib/snapshot-controller.js` — serialized snapshot workflow and view model assembly.
- `integrations/resolve/test/*.test.js` — Node unit and integration tests.
- `integrations/resolve/scripts/install-macos.sh` — build and install plugin, sidecar, and SDK module.
- `integrations/resolve/scripts/validate-install.js` — deterministic installed-layout validation.
- `docs/RESOLVE.md` — editor-facing installation and usage.

### Task 1: Stable per-timeline managed workspace

**Files:**
- Create: `integrations/resolve/package.json`
- Create: `integrations/resolve/lib/workspace.js`
- Create: `integrations/resolve/test/workspace.test.js`

**Interfaces:**
- Produces: `slugify(value: string): string`
- Produces: `projectKey(database: object, projectName: string): string`
- Produces: `timelineWorkspace(root: string, context: object): { directory, timelinePath, pendingPath }`
- Produces: `promoteExport(fsPromises, pendingPath: string, timelinePath: string): Promise<void>`

- [ ] **Step 1: Write failing workspace identity and promotion tests**

```js
const test = require('node:test');
const assert = require('node:assert/strict');
const { slugify, projectKey, timelineWorkspace, promoteExport } = require('../lib/workspace');

test('timelineWorkspace stays stable across timeline renames', () => {
  const a = timelineWorkspace('/tmp/Vedit', { database: { DbType: 'Disk', DbName: 'Local' }, projectName: 'Show', timelineName: 'Cut 1', timelineId: 'timeline-42' });
  const b = timelineWorkspace('/tmp/Vedit', { database: { DbType: 'Disk', DbName: 'Local' }, projectName: 'Show', timelineName: 'Final Cut', timelineId: 'timeline-42' });
  assert.equal(a.directory.split('/').at(-1).split('--').at(-1), b.directory.split('/').at(-1).split('--').at(-1));
  assert.equal(a.timelinePath.endsWith('/timeline.otio'), true);
  assert.equal(a.pendingPath.endsWith('/timeline.otio.pending'), true);
});

test('promoteExport refuses a missing or empty pending export', async () => {
  const fs = { stat: async () => ({ size: 0 }), rename: async () => assert.fail('rename must not run') };
  await assert.rejects(promoteExport(fs, '/tmp/pending', '/tmp/timeline.otio'), /empty/i);
});
```

- [ ] **Step 2: Run the test and verify RED**

Run: `cd integrations/resolve && node --test test/workspace.test.js`

Expected: FAIL with `Cannot find module '../lib/workspace'`.

- [ ] **Step 3: Implement deterministic workspace paths and atomic promotion**

Use `node:crypto.createHash('sha256')` over normalized database identity plus project name. Keep the last 12 hex characters in directory keys, normalize names to lowercase ASCII slugs, and use `fsPromises.rename()` only after `stat().size > 0`.

- [ ] **Step 4: Run workspace tests and verify GREEN**

Run: `cd integrations/resolve && node --test test/workspace.test.js`

Expected: 2 tests pass, 0 fail.

- [ ] **Step 5: Commit the workspace unit**

```bash
git add integrations/resolve/package.json integrations/resolve/lib/workspace.js integrations/resolve/test/workspace.test.js
git commit -m "feat(resolve): add managed timeline workspaces"
```

### Task 2: Vedit CLI sidecar boundary

**Files:**
- Create: `integrations/resolve/lib/vedit-runner.js`
- Create: `integrations/resolve/test/vedit-runner.test.js`

**Interfaces:**
- Consumes: absolute workspace and timeline paths from Task 1.
- Produces: `createVeditRunner({ binaryPath, execFile, fsPromises }): { snapshot, load }`
- Produces: `snapshot(workspace): Promise<{ commitLine, history, detail }>`
- Produces: `load(workspace): Promise<{ history, detail }>`

- [ ] **Step 1: Write failing runner tests**

Test with a temporary directory and an injected `execFile` fake. Assert that the first snapshot calls `init`, then `commit timeline.otio`, `log`, and `show HEAD`, all with `cwd` set to the timeline workspace. Assert that an existing `.vedit` skips `init`, and that non-zero child-process errors retain `stderr` in `error.details`.

- [ ] **Step 2: Run the test and verify RED**

Run: `cd integrations/resolve && node --test test/vedit-runner.test.js`

Expected: FAIL with `Cannot find module '../lib/vedit-runner'`.

- [ ] **Step 3: Implement the runner**

Promisify injected `execFile`, use argument arrays rather than shell strings, cap output at 2 MiB, and parse history lines into `{ hash, message, author, current }`. Keep `show HEAD` as an array of display lines after removing commit metadata; do not parse OTIO in JavaScript.

- [ ] **Step 4: Run runner tests and verify GREEN**

Run: `cd integrations/resolve && node --test test/vedit-runner.test.js`

Expected: runner tests pass with 0 failures.

- [ ] **Step 5: Commit the runner unit**

```bash
git add integrations/resolve/lib/vedit-runner.js integrations/resolve/test/vedit-runner.test.js
git commit -m "feat(resolve): add Vedit sidecar runner"
```

### Task 3: Resolve promise API adapter

**Files:**
- Create: `integrations/resolve/lib/resolve-adapter.js`
- Create: `integrations/resolve/test/resolve-adapter.test.js`

**Interfaces:**
- Produces: `createResolveAdapter({ getResolve, exportType }): { getActiveContext, exportActiveTimeline }`
- Produces: `getActiveContext(): Promise<{ database, projectName, timelineName, timelineId }>`
- Produces: `exportActiveTimeline(path: string): Promise<context>`

- [ ] **Step 1: Write failing adapter tests**

Build small promise-returning fakes for Resolve, ProjectManager, Project, and Timeline. Verify successful context collection and `timeline.Export(path, exportType)`. Verify precise errors with codes `NO_PROJECT`, `NO_TIMELINE`, and `EXPORT_FAILED`.

- [ ] **Step 2: Run the test and verify RED**

Run: `cd integrations/resolve && node --test test/resolve-adapter.test.js`

Expected: FAIL with `Cannot find module '../lib/resolve-adapter'`.

- [ ] **Step 3: Implement the Resolve adapter**

Call `GetProjectManager()`, `GetCurrentDatabase()`, `GetCurrentProject()`, `GetCurrentTimeline()`, `GetName()`, and `GetUniqueId()` in sequence. Throw typed errors with user-safe messages. Treat any export result other than `true` as failure.

- [ ] **Step 4: Run adapter tests and verify GREEN**

Run: `cd integrations/resolve && node --test test/resolve-adapter.test.js`

Expected: adapter tests pass with 0 failures.

- [ ] **Step 5: Commit the adapter unit**

```bash
git add integrations/resolve/lib/resolve-adapter.js integrations/resolve/test/resolve-adapter.test.js
git commit -m "feat(resolve): add Resolve timeline adapter"
```

### Task 4: Serialized snapshot controller

**Files:**
- Create: `integrations/resolve/lib/snapshot-controller.js`
- Create: `integrations/resolve/test/snapshot-controller.test.js`

**Interfaces:**
- Consumes: `resolveAdapter`, `workspaceService`, and `veditRunner` interfaces from Tasks 1-3.
- Produces: `createSnapshotController(dependencies): { inspect, snapshot }`
- Produces: renderer-safe state `{ status, context, history, latest, error }`.

- [ ] **Step 1: Write failing controller tests**

Verify first snapshot order: context -> mkdir -> export pending -> promote -> Vedit snapshot -> state. Call `snapshot()` twice before the first finishes and assert only one export and one commit occur while both callers receive the same result. Verify inspect on an uninitialized workspace returns an empty history without error. Verify adapter and runner errors become renderer-safe errors with optional diagnostic details.

- [ ] **Step 2: Run the test and verify RED**

Run: `cd integrations/resolve && node --test test/snapshot-controller.test.js`

Expected: FAIL with `Cannot find module '../lib/snapshot-controller'`.

- [ ] **Step 3: Implement controller orchestration**

Keep one `inFlight` promise, clear it in `finally`, never expose filesystem paths in normal state, and preserve the last successful history when a later snapshot errors.

- [ ] **Step 4: Run controller tests and verify GREEN**

Run: `cd integrations/resolve && node --test test/snapshot-controller.test.js`

Expected: controller tests pass with 0 failures.

- [ ] **Step 5: Commit the controller unit**

```bash
git add integrations/resolve/lib/snapshot-controller.js integrations/resolve/test/snapshot-controller.test.js
git commit -m "feat(resolve): orchestrate timeline snapshots"
```

### Task 5: Secure Resolve Workflow Integration host

**Files:**
- Create: `integrations/resolve/manifest.xml`
- Create: `integrations/resolve/main.js`
- Create: `integrations/resolve/preload.js`
- Create: `integrations/resolve/test/plugin-contract.test.js`

**Interfaces:**
- Consumes: Tasks 1-4.
- Produces renderer APIs: `window.vedit.inspect()`, `window.vedit.snapshot()`, and `window.vedit.cleanup()`.

- [ ] **Step 1: Write failing static security and contract tests**

Read the source files as text and assert the manifest ID is `com.explicit09.vedit.resolve`, `contextIsolation: true`, `sandbox: true`, `nodeIntegration: false`, exactly three preload methods, and no renderer-facing generic IPC send/invoke method.

- [ ] **Step 2: Run the test and verify RED**

Run: `cd integrations/resolve && node --test test/plugin-contract.test.js`

Expected: FAIL because manifest, main, and preload do not exist.

- [ ] **Step 3: Implement the Electron host**

Initialize `WorkflowIntegration.node` with `InitializePromise`, cache the promise Resolve object, register `vedit:inspect`, `vedit:snapshot`, and `vedit:cleanup` handlers, create a 720x760 window, load `index.html`, and call `CleanUp()` exactly once during shutdown. Resolve the sidecar from `bin/vedit` beside the plugin.

- [ ] **Step 4: Run plugin contract tests and verify GREEN**

Run: `cd integrations/resolve && node --test test/plugin-contract.test.js`

Expected: contract tests pass with 0 failures.

- [ ] **Step 5: Commit the host unit**

```bash
git add integrations/resolve/manifest.xml integrations/resolve/main.js integrations/resolve/preload.js integrations/resolve/test/plugin-contract.test.js
git commit -m "feat(resolve): add secure workflow host"
```

### Task 6: Resolve-native snapshot and history UI

**Files:**
- Create: `integrations/resolve/index.html`
- Create: `integrations/resolve/styles.css`
- Create: `integrations/resolve/renderer.js`
- Create: `integrations/resolve/lib/view-state.js`
- Create: `integrations/resolve/test/view-state.test.js`

**Interfaces:**
- Consumes: the renderer-safe state from Task 4 and preload methods from Task 5.
- Produces: `reduceView(state, event)` for deterministic loading, success, empty, and error states.

- [ ] **Step 1: Write failing view-state tests**

Verify boot -> loading -> ready, snapshot -> saving -> ready, error with retry, empty history copy, and that user/project strings remain data rather than interpolated HTML.

- [ ] **Step 2: Run the test and verify RED**

Run: `cd integrations/resolve && node --test test/view-state.test.js`

Expected: FAIL with `Cannot find module '../lib/view-state'`.

- [ ] **Step 3: Implement the reducer and renderer**

Use `textContent` and DOM node creation for all runtime strings. Render a compact header, project/timeline context, one primary `Snapshot timeline` button, latest-change card, history list, empty state, error banner, and expandable diagnostics. Use system fonts, neutral graphite surfaces, restrained blue for action, and green only for healthy status.

- [ ] **Step 4: Run view tests and all Node tests**

Run: `cd integrations/resolve && npm test`

Expected: all Resolve integration tests pass with 0 failures.

- [ ] **Step 5: Commit the UI unit**

```bash
git add integrations/resolve/index.html integrations/resolve/styles.css integrations/resolve/renderer.js integrations/resolve/lib/view-state.js integrations/resolve/test/view-state.test.js
git commit -m "feat(resolve): add snapshot history UI"
```

### Task 7: macOS installer and installed-layout validation

**Files:**
- Create: `integrations/resolve/scripts/install-macos.sh`
- Create: `integrations/resolve/scripts/validate-install.js`
- Create: `integrations/resolve/test/installer.test.js`
- Modify: `integrations/resolve/package.json`

**Interfaces:**
- Installer inputs: `VEDIT_REPO_ROOT`, `VEDIT_RESOLVE_PLUGIN_ROOT`, and `VEDIT_RESOLVE_SDK_PLUGIN` overrides for tests.
- Installed output: `com.explicit09.vedit.resolve/` with source files, `bin/vedit`, and `WorkflowIntegration.node`.

- [ ] **Step 1: Write a failing installer integration test**

Run the installer against temporary fake plugin and SDK roots with a fake prebuilt sidecar. Assert the exact installed layout, executable mode on `bin/vedit`, omission of tests and docs, and failure with a clear error when the SDK native module is absent.

- [ ] **Step 2: Run the test and verify RED**

Run: `cd integrations/resolve && node --test test/installer.test.js`

Expected: FAIL because the installer does not exist.

- [ ] **Step 3: Implement installer and validator**

Default to `/Library/Application Support/Blackmagic Design/DaVinci Resolve/Workflow Integration Plugins` and the installed `SamplePromisePlugin/WorkflowIntegration.node`. Build only `vedit-cli` in release mode unless `VEDIT_SIDECAR_BIN` is provided. Install through a temporary sibling directory and rename into place. The validator checks manifest, entrypoint, native module, sidecar executable, and architecture.

- [ ] **Step 4: Run installer tests and validation against a temporary install**

Run: `cd integrations/resolve && node --test test/installer.test.js`

Expected: installer tests pass with 0 failures.

- [ ] **Step 5: Commit installation support**

```bash
git add integrations/resolve/scripts integrations/resolve/test/installer.test.js integrations/resolve/package.json
git commit -m "feat(resolve): add macOS plugin installer"
```

### Task 8: Documentation and complete automated verification

**Files:**
- Modify: `docs/RESOLVE.md`
- Modify: `README.md`

- [ ] **Step 1: Replace the manual export/watch path with plugin-first instructions**

Document `integrations/resolve/scripts/install-macos.sh`, Resolve restart, `Workspace -> Workflow Integrations -> Vedit`, Snapshot, managed local storage, compatibility, and troubleshooting. Retain the Lua export/watch workflow under an explicit legacy/advanced heading.

- [ ] **Step 2: Run documentation and repository checks**

Run:

```bash
cargo fmt --check
cargo test -p vedit-core -p vedit-cli
cd integrations/resolve && npm test
git diff --check
```

Expected: Rust tests and Resolve tests pass, formatting and whitespace checks exit 0.

- [ ] **Step 3: Commit documentation**

```bash
git add README.md docs/RESOLVE.md
git commit -m "docs: make Resolve integration the primary workflow"
```

### Task 9: Install and prove V1 in real DaVinci Resolve

**Files:**
- Runtime install only; do not commit `WorkflowIntegration.node` or the built sidecar.

- [ ] **Step 1: Build and install the real plugin**

Run: `integrations/resolve/scripts/install-macos.sh`

Expected: validator reports a complete arm64 plugin at the Resolve Workflow Integration Plugins root.

- [ ] **Step 2: Start Resolve and confirm plugin discovery**

Use Computer Use to open installed DaVinci Resolve 20.3.2 and inspect `Workspace -> Workflow Integrations`. Expected: `Vedit` appears and opens without a crash.

- [ ] **Step 3: Use a disposable Resolve project and timeline**

Create or open a non-production project, ensure an active timeline exists, open Vedit, and confirm the project and timeline names appear. Do not alter an existing production timeline for the proof.

- [ ] **Step 4: Prove first snapshot**

Click `Snapshot timeline`. Confirm no file picker or Terminal appears, the UI shows an initial commit, and the managed workspace contains a real `.vedit` repository whose `vedit log` contains that commit.

- [ ] **Step 5: Prove a semantic second snapshot**

Make one reversible edit in the disposable timeline, click Snapshot again, and confirm the UI and `vedit show HEAD` report the correct semantic change.

- [ ] **Step 6: Record proof and decide V2 gate**

Capture the plugin UI and relevant local log output. If all five runtime criteria from the design pass, mark V1 proven and write the V2 automatic-snapshot plan. If any criterion fails, record the exact failure, add a failing automated regression test where possible, fix it through TDD, reinstall, and repeat the runtime proof.

### Task 10: Final verification and handoff

**Files:**
- Modify only files required by runtime-discovered fixes.

- [ ] **Step 1: Run the full fresh verification suite**

```bash
cargo fmt --check
cargo test -p vedit-core -p vedit-cli
cd integrations/resolve && npm test
node scripts/validate-install.js
git diff --check
git status --short
```

Expected: every command exits 0; status shows no accidental SDK binary or runtime workspace files.

- [ ] **Step 2: Review every V1 requirement against evidence**

Confirm no manual OTIO interaction, real active-timeline detection, real commit creation, history and diff rendering, actionable errors, secure renderer settings, SDK-module non-redistribution, and real Resolve proof.

- [ ] **Step 3: Commit any runtime fixes and report actual status**

Do not claim V1 complete unless both the automated suite and real Resolve proof pass in the current run. Report V2 as started only after the V1 gate is satisfied.
