# DaVinci Resolve Integration V2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add opt-in automatic timeline snapshots that never create commits when the Resolve timeline is semantically unchanged.

**Architecture:** The existing plugin keeps its three-method preload boundary. A renderer scheduler requests `snapshot({ skipUnchanged: true })` every 30 seconds while enabled; the main process sanitizes that option. Before promoting the pending OTIO export, the sidecar runner compares it with the last snapshot through `vedit diff --json` and the controller skips no-op commits.

**Tech Stack:** Existing Resolve Electron integration, Node built-ins, Vedit CLI JSON diff, `node:test`.

## Global constraints

- Automatic snapshots are opt-in and default off.
- The interval is 30 seconds and runs only while the Vedit integration is open.
- At most one export/commit operation may run at a time.
- Semantic no-ops must not promote the pending file or create a commit.
- Manual Snapshot continues to create an intentional commit.
- No new generic IPC capability is exposed.

### Task 1: Semantic no-op detection

- [ ] Add failing runner tests for unchanged, changed, and first-snapshot cases.
- [ ] Implement `hasSemanticChanges(workspace, candidatePath)` using `vedit diff ... --json`.
- [ ] Run focused and full Node tests.

### Task 2: Automatic snapshot controller option

- [ ] Add a failing controller test proving `skipUnchanged` prevents promotion and commit.
- [ ] Sanitize the option in the main IPC handler and preload bridge.
- [ ] Preserve the last successful review and return `unchanged: true` on no-op.
- [ ] Run controller and security contract tests.

### Task 3: Opt-in renderer scheduler and control

- [ ] Add failing pure scheduler tests for start, stop, interval, and overlap behavior.
- [ ] Implement a browser/Node-compatible scheduler module.
- [ ] Add an accessible Auto snapshots toggle, status copy, and local preference.
- [ ] Use a 30-second interval and `skipUnchanged: true` ticks.
- [ ] Run view, scheduler, and full Node tests.

### Task 4: Install, document, and verify

- [ ] Include the scheduler in installer and validator tests.
- [ ] Update Resolve documentation with exact automatic-snapshot behavior.
- [ ] Reinstall the production plugin and verify source checksums.
- [ ] Run Rust tests, Node tests, real-sidecar no-op smoke, validation, and git checks.
- [ ] Leave only the locked-screen Resolve UI proof deferred.
