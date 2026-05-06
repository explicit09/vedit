# Wiring vedit into DaVinci Resolve

The recommended workflow for human editors. Combine a small Resolve
script that exports the current timeline as OTIO with `vedit watch` to
get one commit per export, with an auto-generated message from the
diff.

## Goal

After setup:

1. You make edits in Resolve as usual.
2. You hit a hotkey (or run a script from the menu).
3. The timeline exports to a fixed path.
4. `vedit watch` notices, runs `vedit commit` with an auto-generated
   message describing what changed, and you see the commit fly by in
   your terminal.

No manual `vedit commit`. No typing commit messages. No leaving Resolve.

## Step 1 — install the export script

Save the following as `~/Library/Application Support/Blackmagic Design/DaVinci Resolve/Fusion/Scripts/Edit/vedit_export.py`
(macOS path; on Windows it's under `%APPDATA%\Blackmagic Design\...`):

```python
"""Export the current Resolve timeline as OTIO to a fixed path.

vedit watch is configured to look at the same path, so running this
script triggers an auto-commit.
"""

import os

# Where vedit watch is looking. Change to suit your project.
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

Adjust `EXPORT_PATH` to point at the timeline file inside your project
directory.

Restart Resolve, or refresh the Scripts menu (`Workspace → Scripts →
Edit`).

## Step 2 — bind the script to a hotkey (optional)

Resolve doesn't have a native "bind script to hotkey" feature, but the
script will appear under `Workspace → Scripts → Edit → vedit_export`
and you can run it with one click. For a real hotkey, use a global
keyboard tool (Karabiner-Elements on macOS, PowerToys on Windows) to
trigger the menu item.

## Step 3 — start vedit watch

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

## Tips

- **Don't worry about half-written files.** vedit's `--settle` window
  (default 200ms) waits for the export to finish before committing.
- **Use branches.** Create a branch before trying an alternate cut:
  `vedit branch alt_cut && vedit checkout alt_cut`. Then export from
  Resolve as before — commits go to `alt_cut` until you switch back.
- **`vedit show HEAD`** any time gives you the full diff against the
  previous commit, in plain English.
- **Skip the auto-message** with `vedit commit timeline.otio -m "..."`
  if you want to write your own. The auto-message is the default, not
  the only option.

## Other editors

This pattern works for any editor that can export OTIO to a known path:

- **Premiere:** ExtendScript can write OTIO via the
  [community OTIO adapters](https://opentimelineio.readthedocs.io/en/latest/tutorials/adapters.html).
  Bind the script to a panel button.
- **FCP:** Apple's `XML` export goes through `otio-fcpx-xml-adapter`.
  Less ergonomic than Resolve but workable.
- **Anything else that emits OTIO:** if it can write to a path on a
  hotkey, `vedit watch` works.
