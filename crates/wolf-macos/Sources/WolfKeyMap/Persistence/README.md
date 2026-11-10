# wolf-macos/Sources/WolfKeyMap/Persistence

The pure decisions behind `state.json`: what each live view writes to it, how a pane layout encodes, and how windows, projects and layouts come back from it on launch. The directory compiles into the [`WolfKeyMap`](../README.md) module, not a module of its own.

Coverage lives in [`Tests/WolfKeyMapTests/`](../../../Tests/WolfKeyMapTests/README.md).

## Files

#### `PaneLayoutCodec.swift`

Purpose: The pure codec between the live `PaneTree` layout and the shapes that persist or cross the FFI boundary.

**Public API**

- `SavedPaneNode` — a `Hashable` mirror of the split tree keyed by per-leaf buffer path. `leafCount` is how many nvim window handles the rebuild path zips onto the leaves. `isEmptyLeaf` is the suppress-from-persistence predicate for a lone pane: true only for an unnamed leaf that is neither a preview nor a terminal, so a lone pane bound to a file, a preview or a terminal still persists.
- `encode(_:paths:previews:terminals:)` — maps leaf ids to buffer paths; a leaf missing from `paths` becomes `""`. A terminal leaf's path is the directory its shell starts in.
- `postOrderTokens(_:)` and its inverse `decodeTokens(_:)` — the post-order stream the FFI push-builder replays; `decodeTokens` returns `nil` on a malformed stream.

**Decisions**

- There is no Swift renderer for the layout-rebuild document. The restore hands the loaded `FfiPaneNode` back to `WolfCoreHandle.rebuild_layout`, so Rust's `rebuild_layout_spec` stays its only renderer and the format cannot drift between front-ends ([the rebuild-layout wire format](../../../../../notes/wolf-core-decisions.md#the-rebuild-layout-wire-format)).
- A `preview` leaf stores the source Markdown file's absolute path, not the scratch buffer's, because restore reopens it through `wolf_preview_file` rather than `:edit`.

**Gotchas**

- The token stream is a contract with `wolf-ffi`'s push-builder and cannot be changed on one side alone.

#### `RestoreSeam.swift`

Purpose: The pure restore planner. It rebuilds projects from the saved (window, tab, working directory) tree, because project identity is not persisted — see [wolf-macos-decisions.md](../../../../../notes/wolf-macos-decisions.md) for that model.

**Public API**

- `plan(windows:home:isDirectory:) -> RestorePlan` — `isDirectory` is the seam's only injected impurity (the app passes a `FileManager` check, tests pass a set lookup). Projects come back in first-encounter order.
- `SavedTab` and `SavedWindow` are the inputs; `SavedTab` is also what a live view contributes to a commit, through [`SessionSeam`](#sessionseamswift); `PlannedTab`, `ProjectPlan` and `RestorePlan` the outputs. A `PlannedTab` retains its `groupIndex` and `positionInGroup` so the window-assembly phase can drop it back into the right OS window and tab group.

**Decisions**

- An empty-state tab (`cwd == ""`) and a tab whose saved directory has since been deleted each get a single-tab project of their own rather than sharing one, matching "a new window is a new project". Their `launchCwd` is `home`, while `PlannedTab.cwd` still carries the original saved string.
- The seam does not canonicalize. The caller persists an already-canonical working directory via `cwd_for_tabpage`, so grouping is on the string exactly as saved.
- `layout` is a passthrough: it rides from `SavedTab` into the matching `PlannedTab` untouched and never affects grouping, which is by working directory alone.

#### `SessionSeam.swift`

Purpose: The write side of state persistence, ported from wolf-gnome's [`state_seam.rs`](../../../../wolf-gnome/src/README.md#state_seamrs): what each live view contributes to `state.json`, and whether a commit or a close writes anything. The rules are in [wolf-macos-decisions.md](../../../../../notes/wolf-macos-decisions.md#state-is-a-tree-and-the-front-end-owns-every-write).

**Public API**

- `gatherTab(...) -> TabGather` — `.refuse` when a folder view's cwd is `""` or any leaf is `.unreported` or missing from `content`. A `.terminal(cwd:)` leaf persists as a terminal leaf at `cwd`.
- `mayGather(...)` and `contribution(mayGather:gather:cache:)` — the gathered tab when the gate is open and the gather ready, otherwise `cache`, which the caller stores as its next cache. `gather` is not evaluated behind a shut gate.
- `closeCommit(terminating:otherLiveViews:)`, `committedWindows(liveViews:windows:)` and `orphanedSlots(savedTabCounts:materialized:)` — `nil` from `committedWindows` means commit nothing.
- `CloseBurst(scheduleEnd:read:commit:)` and its `close(terminating:otherLiveViews:)` — `closeCommit` over the closes of one runloop turn, which ends when the closure given to `scheduleEnd` runs.

**Decisions**

- A partial read refuses the whole tab: a leaf with no reported window would persist `""` over the restored file.
- The cache starts as the restored tab, so a view not yet read or behind a shut gate still writes its saved files.
- A keep commits the tree read at its burst's first close, while a tabbed window's earlier tabs are still live: its close button closes each tab in turn, so a later read keeps only the last tab.
- A walk with no live view or no window would erase the session, so it commits nothing. Orphans are never retried, but are carried into every commit.

#### `StateSeam.swift`

Purpose: The pure decision seams for the macOS state-restore path — validating a saved window frame, and deciding whether a window opens a folder or the empty state. Foundation and CoreGraphics only; the AppKit glue feeds it real window frames, `NSScreen` visible frames and a `FileManager`-backed directory check.

**Public API**

- `restoredWindowFrame(x:y:width:height:screenVisibleFrames:) -> CGRect?` — `nil` means discard the saved geometry and fall back to the default centered placement. It rejects two cases: a non-positive width or height, which is the "geometry never reported" sentinel; and a frame intersecting no screen's visible frame, which would restore the window fully offscreen after a display is disconnected.
- `launchDecision(restored:home:isDirectory:) -> LaunchDecision` — `.folder(cwd:)` only when `restored` is non-empty and still names a directory, since it may have been deleted or replaced by a plain file since the last run; otherwise `.emptyState(nvimCwd: home)`.

**Decisions**

- `launchDecision` never returns the process working directory. Nvim always needs a cwd, so the empty state launches it at `home`; that is a passive cwd the caller deliberately does not persist or record in the recent-dirs MRU, and a process-cwd fallback would pollute it.
- `isDirectory` is injected rather than reaching for `FileManager`, which is what keeps the seam pure; tests pass a set lookup.
