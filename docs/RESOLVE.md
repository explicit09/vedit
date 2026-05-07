# Using vedit with DaVinci Resolve

Set this up once and vedit will save a snapshot of your timeline every time you export from Resolve. You'll get a clean version history — what was in each cut, what changed, when — without ever leaving your editor.

## What you'll have at the end

A terminal window quietly running `vedit watch`. Whenever you click "export" in Resolve (or hit a hotkey for it), vedit notices and saves a new version automatically:

```
[main 4758e4a] Initial cut: 1 track, 4 clips
[main f2d8815] Trimmed "drone_shot_04" by 1.80s
[main a0c1d22] Moved interview clip, added crossfade
```

You don't have to type anything. The commit messages write themselves from what changed.

## Step 1 — make sure Resolve scripting works

Open Resolve. Go to **Workspace → Console**. The console pane appears at the bottom. Switch its dropdown to **Lua**. Type:

```lua
project = resolve:GetProjectManager():GetCurrentProject()
print(project:GetName())
```

You should see your project name. If you do, you're good. If you get an error, scripting may not be enabled — check **Resolve → Preferences → System → General** and make sure scripting is allowed.

(If your Resolve has Python configured instead of Lua, both work the same way — see the Python script at the bottom of this page.)

## Step 2 — install the export script

Save this as a `.lua` file in Resolve's scripts folder:

- **macOS:** `~/Library/Application Support/Blackmagic Design/DaVinci Resolve/Fusion/Scripts/Edit/vedit_export.lua`
- **Windows:** `%APPDATA%\Blackmagic Design\DaVinci Resolve\Fusion\Scripts\Edit\vedit_export.lua`

```lua
-- Export the current Resolve timeline to a fixed path.
-- vedit watch is configured to look at this path,
-- so running this script triggers an automatic snapshot.

local EXPORT_PATH = os.getenv("HOME") .. "/projects/my_cut/timeline.otio"

local resolve = Resolve()
local project = resolve:GetProjectManager():GetCurrentProject()
local timeline = project:GetCurrentTimeline()

if not timeline then
    print("vedit_export: no timeline open")
else
    local ok = timeline:Export(EXPORT_PATH, resolve.EXPORT_OTIO)
    print(ok and ("vedit_export: saved to " .. EXPORT_PATH) or "vedit_export: export failed")
end
```

Change `EXPORT_PATH` to match where your project lives.

Restart Resolve, or refresh its Scripts menu (**Workspace → Scripts → Edit**). The script will appear there.

## Step 3 — bind it to a hotkey (optional)

Resolve doesn't bind menu items to hotkeys natively. The script will appear under **Workspace → Scripts → Edit → vedit_export** and you can run it with one click. If you want a real hotkey:

- **macOS:** Karabiner-Elements or Keyboard Maestro
- **Windows:** PowerToys or AutoHotkey

## Step 4 — start vedit

In a terminal, in your project folder:

```
vedit init                 # only the first time
vedit watch timeline.otio
```

Leave that terminal running while you edit. Now whenever you run the export script in Resolve (from the menu or your hotkey), vedit will save a new version automatically.

## Working with versions

While `vedit watch` is running, use these commands in another terminal whenever you want to look at history:

```
vedit log              # see all your saved versions
vedit show HEAD        # what changed in the most recent version
vedit show <version>   # what changed in a specific version
vedit branches         # list your branches
```

To try out an alternate cut without losing your main version:

```
vedit branch alt_cut         # save a bookmark of where you are
vedit checkout alt_cut       # switch to it
# ...edit and export from Resolve as normal — it commits to alt_cut...
vedit checkout main          # come back to the main cut
```

To bring an alternate cut back into main:

```
vedit merge alt_cut
```

If both branches changed the same track, vedit will tell you and refuse to merge. Pick one branch to keep, manually re-create the changes you want from the other side, and re-merge.

## What gets saved and what doesn't

vedit saves the parts of your edit that live in the OTIO export — clips, cuts, transitions, source ranges, audio levels, effects, the structure of your timeline. That's enough to re-create the timeline exactly as you exported it.

vedit does **not** save things that Resolve keeps inside the project file rather than the OTIO export — color grades, render-cache hints, Fusion compositions in some cases. Treat vedit as a snapshot of your timeline, not a replacement for your `.drp` project file. Keep both.

## Troubleshooting

**Scripts menu doesn't show vedit_export.** Check the path is exactly right (capitalization matters), then restart Resolve. The Scripts menu only refreshes on launch.

**vedit watch says nothing when I export.** Make sure the export path in `vedit_export.lua` matches the path you passed to `vedit watch`. They have to point at the same file.

**vedit watch commits a half-written file.** Shouldn't happen — vedit waits for the file to finish writing before committing. If it does, file an issue with the input.

**The recovered version doesn't have my color grade.** Color grades live in Resolve's project file, not the OTIO export. They aren't saved by vedit. Keep your `.drp` file in normal backup.

## Other editors

The same pattern works for any editor that exports OTIO:

- **Premiere** — ExtendScript can export OTIO via community adapters. Bind the script to a panel button.
- **Final Cut Pro** — Apple's XML export goes through `otio-fcpx-xml-adapter`. Less ergonomic, workable.

Anywhere you can write a small script that exports OTIO to a known path, `vedit watch` will pick it up.

## Python alternative (if your Resolve has Python configured)

If Lua isn't your thing and Resolve's Python console works on your machine, save this as `vedit_export.py` in the same folder:

```python
"""Export the current Resolve timeline as OTIO."""
import os

EXPORT_PATH = os.path.expanduser("~/projects/my_cut/timeline.otio")

resolve = app.GetResolve()  # provided by Resolve at runtime
project = resolve.GetProjectManager().GetCurrentProject()
timeline = project.GetCurrentTimeline()
if timeline is None:
    print("vedit_export: no timeline open")
else:
    ok = timeline.Export(EXPORT_PATH, resolve.EXPORT_OTIO)
    print(f"vedit_export: saved to {EXPORT_PATH}" if ok else "vedit_export: export failed")
```

The Lua and Python versions are interchangeable. Use whichever your Resolve install supports.
