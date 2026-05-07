# pyvedit

Python bindings for [**vedit**](https://github.com/explicit09/vedit) — version control for video timelines (OpenTimelineIO).

> **Heads-up on the name.** The PyPI distribution is `pyvedit`. The Python module is `vedit`. So you `pip install pyvedit` but write `import vedit`. The `vedit` PyPI name was already taken by an unrelated 2016 project; we kept the project's actual name (`vedit`) in code and only changed the install line.

## Install

```
python3 -m venv .venv && source .venv/bin/activate
pip install pyvedit
```

## Use it

```python
import vedit

repo = vedit.Repo.init("./project")

# Agent generates a timeline as an OTIO dict.
timeline = my_agent.generate(prompt="opening montage, fast cuts")
repo.commit(timeline, message="agent v0")

# Branch off and try a different approach.
repo.create_branch("alt", at="HEAD")
repo.switch_branch("alt")
alt_timeline = my_agent.generate(prompt="opening montage, slower")
repo.commit(alt_timeline, message="agent v1 — slower")

# What changed between the two?
for change in repo.diff_refs("main", "alt"):
    print(change.op, change.to_dict())
# trimmed {'clip': {'name': 'shot_03', ...}, 'before': {...}, 'after': {...}}
# added   {'clip': {'name': 'reaction_take_2', ...}, ...}

# Or diff two timelines directly without a repo.
changes = vedit.diff(timeline_a, timeline_b)
```

There's also a CLI (`vedit init`, `vedit commit`, `vedit diff`, `vedit branch`, `vedit merge`, `vedit watch`) — same engine. See the [GitHub repo](https://github.com/explicit09/vedit) for setup.

## What it does

vedit is the snapshot / branch / diff / merge layer for OTIO timelines. Built for AI video tools that need version control without reinventing it, and for editors who want a real edit history beyond their NLE's undo stack.

The semantic diff understands timeline operations, not byte differences:
- `Trimmed "drone_shot_04" by 1.80s (in)`
- `Moved "interview_take_2" before "b_roll_03"`
- `Added crossfade between "title_card" and "drone_shot_04" (12 frames)`

Three-way merge produces structured conflicts. Backed by a content-addressed object store and a commit graph — same shape as Git, applied to video timelines instead of source files. Rust core, Python bindings via [maturin](https://www.maturin.rs/).

## API surface

```python
# Repo lifecycle
vedit.Repo.init(path)         # create a new repo
vedit.Repo.open(path)         # open at exact path
vedit.Repo.discover()         # walk up to find .vedit/

# Commit / read
repo.commit(timeline_dict, message="...", author_name=None, author_email=None)
repo.read_timeline(ref)       # returns OTIO dict
repo.read_commit(ref)         # returns commit dict
repo.resolve(ref)             # full hash from any ref

# Branches
repo.create_branch(name, at="HEAD")
repo.switch_branch(name)
repo.delete_branch(name)
repo.list_branches()          # [(name, hash), ...]
repo.current_branch()         # str or None

# History
repo.log()                    # [(hash, commit_dict), ...] from HEAD
repo.log("alt")               # from a specific ref

# Diff
repo.diff_refs("main", "alt") # list of Change objects
vedit.diff(before_dict, after_dict)  # standalone, no repo

# Errors
vedit.VeditError              # all errors from the core
```

## Links

- **GitHub:** [explicit09/vedit](https://github.com/explicit09/vedit)
- **Architecture notes:** [docs/ARCHITECTURE.md](https://github.com/explicit09/vedit/blob/main/docs/ARCHITECTURE.md)
- **DaVinci Resolve setup:** [docs/RESOLVE.md](https://github.com/explicit09/vedit/blob/main/docs/RESOLVE.md)
- **Issues:** [explicit09/vedit/issues](https://github.com/explicit09/vedit/issues)
- **License:** [Apache 2.0](https://github.com/explicit09/vedit/blob/main/LICENSE)
