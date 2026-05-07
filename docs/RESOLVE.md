# Wiring vedit into DaVinci Resolve

The recommended workflow for human editors. Combine a small Resolve
script that exports the current timeline as OTIO with `vedit watch` to
get one commit per export, with an auto-generated message from the
diff.

## Goal

After setup:

1. You make edits in Resolve as usual.
2. You hit a hotkey (or run a one-line script from the menu).
3. The timeline exports to a fixed path.
4. `vedit watch` notices, runs `vedit commit` with an auto-generated
   message describing what changed, and you see the commit fly by in
   your terminal.

No manual `vedit commit`. No typing commit messages. No leaving
Resolve.

## What's known to work

Tested against DaVinci Resolve 20 on macOS:

- **External Resolve scripting via `fusionscript.so`:** does not work
  by default. `scriptapp("Resolve")` returns `None` on a clean
  machine.
- **Resolve's internal Python console:** may show *"Python 3 was not
  found"* unless you've installed and configured Python in Resolve's
  preferences.
- **Resolve's internal Lua console:** works. This is the path the
  documentation below uses.
- **Resolve's `Workspace → Scripts` menu:** works for both Python and
  Lua scripts placed in the right directory.

If your machine has Python configured in Resolve, the Python script at
the bottom of this doc works the same way.

## Step 1 — verify Resolve scripting

In Resolve: `Workspace → Console`. The console pane appears at the
bottom. Switch its language dropdown to **Lua**. Type:

```lua
project = resolve:GetProjectManager():GetCurrentProject()
print(project:GetName())
```

You should see your current project's name. If you do, Lua scripting
is live and you're ready to continue.

## Step 2 — install the export script (Lua)

Save this as `~/Library/Application Support/Blackmagic Design/DaVinci Resolve/Fusion/Scripts/Edit/vedit_export.lua`
(macOS path; on Windows it's under `%APPDATA%\Blackmagic Design\...`):

```lua
-- Export the current Resolve timeline as OTIO to a fixed path.
-- vedit watch is configured to look at the same path, so running this
-- script triggers an auto-commit.

local EXPORT_PATH = os.getenv("HOME") .. "/projects/my_cut/timeline.otio"

local resolve = Resolve()
local project = resolve:GetProjectManager():GetCurrentProject()
local timeline = project:GetCurrentTimeline()
if not timeline then
    print("vedit_export: no current timeline")
else
    local ok = timeline:Export(EXPORT_PATH, resolve.EXPORT_OTIO)
    if ok then
        print("vedit_export: wrote " .. EXPORT_PATH)
    else
        print("vedit_export: export failed")
    end
end
```

Adjust `EXPORT_PATH` to point at the timeline file inside your project
directory.

Restart Resolve, or refresh the Scripts menu (`Workspace → Scripts →
Edit`). The script will appear under that menu.

## Step 3 — bind to a hotkey (optional)

Resolve doesn't natively bind menu items to hotkeys. The script
appears under `Workspace → Scripts → Edit → vedit_export` and runs in
one click. For a real hotkey:

- **macOS:** Karabiner-Elements, or a Keyboard Maestro macro that
  triggers the menu item by name.
- **Windows:** PowerToys Run, or AutoHotkey targeting the menu.

## Step 4 — start vedit watch

In a terminal:

```
cd ~/projects/my_cut
vedit init                              # if you haven't already
vedit watch timeline.otio
```

That's it. Now every time you run the script in Resolve, vedit will
commit the new state. Example terminal output:

```
Watching timeline.otio (interval 500ms, settle 200ms)
[main 4758e4a] Initial commit: 1 track(s), 4 clip(s)
[main f2d8815] 5 edits: 1 trim, 1 move, 1 effect change, 1 transition added, 1 track added
[main a0c1d22] trimmed "drone_shot_04" by 1.80s (in)
```

## Round-trip fidelity

vedit re-emits canonical OTIO when you `checkout` a commit. The
recovered file is **semantically identical** to the original (every
clip, track, range, effect count, and transition is preserved) but
**not byte-identical**. In one round-trip on a real Resolve project
the file shrank from 209KB to 137KB — the dropped bytes were Resolve's
own metadata namespace (`Resolve_OTIO`), whitespace, and key ordering.

The recovered timeline opens cleanly in Resolve. What you may lose on
round-trip:

- Resolve's own per-clip metadata (color memory, cache hints, internal
  IDs)
- Comments and unmodelled fields
- Key ordering within JSON objects (not visible to a human, only to a
  byte-comparison tool)

What is preserved:

- Every clip, with its source range, media reference, and name
- Every track, with its kind and ordering
- Every transition, with its in/out offsets
- Effect counts on each clip
- All commit history

If preserving Resolve's editor-internal metadata matters to your
workflow, treat vedit as a snapshot tool — keep your `.drp` project
file separate. vedit is the timeline-version-control layer, not a
replacement for the project file.

## Tips

- **Don't worry about half-written files.** vedit's `--settle` window
  (default 200ms) waits for the export to finish before committing.
- **Use branches.** Create a branch before trying an alternate cut:
  `vedit branch alt_cut && vedit checkout alt_cut`. Then export from
  Resolve as before — commits go to `alt_cut` until you switch back.
- **`vedit show HEAD`** any time gives you the full diff against the
  previous commit, in plain English.
- **`vedit merge alt_cut`** brings an alternate cut back into main.
  Fast-forwards if possible; otherwise three-way merge with conflicts
  reported at track granularity.
- **Skip the auto-message** with `vedit commit timeline.otio -m "..."`
  if you want to write your own. The auto-message is the default, not
  the only option.

## Python alternative

If your Resolve has Python configured (`Resolve → Preferences →
System → General → External scripting using` set, plus a Python
install Resolve can find), use this instead, saved as
`vedit_export.py` in the same `Scripts/Edit/` directory:

```python
"""Export the current Resolve timeline as OTIO to a fixed path."""

import os

EXPORT_PATH = os.path.expanduser("~/projects/my_cut/timeline.otio")

resolve = app.GetResolve()  # noqa: F821 — provided by Resolve at runtime
project = resolve.GetProjectManager().GetCurrentProject()
timeline = project.GetCurrentTimeline()
if timeline is None:
    print("vedit_export: no current timeline")
else:
    ok = timeline.Export(EXPORT_PATH, resolve.EXPORT_OTIO)
    print(f"vedit_export: wrote {EXPORT_PATH}" if ok else "vedit_export: export failed")
```

The Lua and Python scripts are interchangeable; pick whichever your
Resolve install supports.

## Other editors

This pattern works for any editor that can export OTIO to a known
path:

- **Premiere:** ExtendScript can write OTIO via the
  [community OTIO adapters](https://opentimelineio.readthedocs.io/en/latest/tutorials/adapters.html).
  Bind the script to a panel button.
- **FCP:** Apple's `XML` export goes through `otio-fcpx-xml-adapter`.
  Less ergonomic than Resolve but workable.
- **Anything else that emits OTIO:** if it can write to a path on a
  hotkey, `vedit watch` works.
