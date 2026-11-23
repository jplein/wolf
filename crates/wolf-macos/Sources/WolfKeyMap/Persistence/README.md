# wolf-macos/Sources/WolfKeyMap/Persistence

The pure decisions behind the window state of [application-state.md](../../../../../specs/application-state.md): what each live view writes to it, in what order, how a pane layout encodes, and how windows, tabs and layouts come back from it on launch. The directory compiles into the [`WolfKeyMap`](../README.md) module, not a module of its own.

Coverage lives in [`Tests/WolfKeyMapTests/`](../../../Tests/WolfKeyMapTests/README.md).

## Files

#### `PaneLayoutCodec.swift`

Purpose: The pure codec between the live `PaneTree` layout and the shapes that persist or cross the FFI boundary.

**Public API**

- `SavedPaneNode` — a `Hashable` mirror of the split tree keyed by per-leaf buffer path. `leafCount` is how many nvim window handles the rebuild path zips onto the leaves. `isEmptyLeaf` is the suppress-from-persistence predicate for a lone pane: true only for an unnamed leaf that is neither a preview nor a terminal, so a lone pane bound to a file, a preview or a terminal still persists. `zoomedLeafIndex` is the first zoomed leaf's depth-first index into the rebuilt leaf ids.
- `encode(_:paths:previews:terminals:zoomed:)` — maps leaf ids to buffer paths; a leaf missing from `paths` becomes `""`. A terminal leaf's path is the directory its shell starts in. It marks the leaf whose id is `zoomed`, and the drawer's id marks none, so a zoomed drawer persists unzoomed.
- `postOrderTokens(_:)` and its inverse `decodeTokens(_:)` — the post-order stream the FFI app-state builder replays; `decodeTokens` returns `nil` on a malformed stream.

**Decisions**

- There is no Swift renderer for the layout-rebuild document. The restore hands the restored `FfiAppTab` to `WolfCoreHandle.rebuild_tab`, so Rust's `tab_rebuild_spec` renders it and the format cannot drift between front-ends ([the rebuild-layout wire format](../../../../../notes/wolf-core-decisions.md#the-rebuild-layout-wire-format)).
- A `preview` leaf stores the source Markdown file's absolute path, not the scratch buffer's, because restore reopens it through `wolf_preview_file` rather than `:edit`.

**Gotchas**

- The token stream is a contract with `wolf-ffi`'s app-state builder and cannot be changed on one side alone.

#### `RestoreSeam.swift`

Purpose: The pure restore planner: one planned tab per saved tab, each to start its own Neovim, so tabs on one directory are never merged ([why](../../../../../notes/wolf-macos-decisions.md#one-neovim-per-tab)).

**Public API**

- `plan(windows:home:isDirectory:) -> [PlannedTab]` — in window then tab order. `isDirectory` is the seam's only injected impurity (the app passes a `FileManager` check, tests pass a set lookup).
- `SavedTab` and `SavedWindow` are the inputs; `SavedTab` is also what a live view contributes to a commit, through [`SessionSeam`](#sessionseamswift). A `PlannedTab` retains its `groupIndex` and `positionInGroup` so the window-assembly phase can drop it back into the right OS window and tab group.
- `PlannedTab.launchCwd` and `hasFolder` are [`launchDecision`](#stateseamswift)'s: the saved directory when it still is one, else `home` in the empty state. `PlannedTab.cwd` still carries the original saved string.
- `selectedPosition(active:materialized:)` — the saved active position when its tab materialized, else the first that did, `nil` when none did.

**Decisions**

- `layout` is a passthrough: it rides from `SavedTab` into the matching `PlannedTab` untouched.

#### `SessionSeam.swift`

Purpose: The write side of state persistence, ported from wolf-gnome's [`state_seam.rs`](../../../../wolf-gnome/src/README.md#state_seamrs): what each live view contributes to the window state, and whether a commit or a close writes anything. The rules are in [wolf-macos-decisions.md](../../../../../notes/wolf-macos-decisions.md#state-is-a-tree-and-the-front-end-owns-every-write).

**Public API**

- `gatherTab(...) -> TabGather` — `.refuse` when a folder view's cwd is `""` or any leaf is `.unreported` or missing from `content`. `zoomed` is [`encode`](#panelayoutcodecswift)'s.
- `mayGather(...)`, `contribution(mayGather:gather:cache:)` — the gathered tab when the gate is open and the gather ready, else `cache`, which the caller stores as its next cache. `gather` is not evaluated behind a shut gate.
- `windowState(live:stacking:orphans:)` — the windows a commit writes, frontmost first by [`StackingOrder`](#stackingorderswift), minimized ones last, orphans after the live windows of their class. A window keeps only its contributing tabs, or drops; `activeTab` falls back to its most recently focused contributing tab.
- `closeCommit(terminating:otherLiveViews:)`, `committedWindows(liveViews:windows:)` and `orphanedSlots(savedTabCounts:materialized:)` — `nil` from `committedWindows` means commit nothing.
- `CloseBurst(scheduleEnd:read:commit:)` and its `close(terminating:otherLiveViews:)` — `closeCommit` over one runloop turn's closes, which ends when `scheduleEnd`'s closure runs.

**Decisions**

- A partial read refuses the whole tab: a leaf with no reported window would persist `""` over the restored file.
- The cache starts as the restored tab, so a view not yet read or behind a shut gate still writes its saved files.
- A keep commits the tree read at its burst's first close, while a tabbed window's earlier tabs are live: its close button closes each tab in turn, so a later read keeps only the last.

#### `StackingOrder.swift`

Purpose: The window stacking order and per-window tab recency of [application-state.md](../../../../../specs/application-state.md) §3, kept from focus and minimize events rather than asked of AppKit.

**Public API**

- `focus(_:)`, `minimize(_:)`, `deminimize(_:)`, `remove(_:)` — the coordinator's events. A window is passed as its tab ids in tab-bar order.
- `init(seed:)` — the order a launch restores: the windows in seed order, each with its active tab first.
- `order(_:)` — window indices frontmost first: windows that are not minimized before those that are, then by best-ranked tab, a window with no ranked tab last of its class. `recency(_:)` is a window's tab indices, most recently focused first, and `isMinimized(_:)` whether every one of its tabs is marked.

**Decisions**

- The order is kept over tab ids, not tab groups: a macOS tab is an `NSWindow`, and a tab group can change identity on a tear-off or merge. A window ranks by its best-ranked tab, and its recency is the list filtered to its tabs.
- Minimize marks every tab of the group, and focusing a tab unmarks it, so a missed deminiaturize cannot keep a focused window behind.
- `order` puts minimized windows last itself, so the invariant holds for windows the list never ranked.

#### `StateSeam.swift`

Purpose: The pure decision seams for the macOS state path — tracking a window's normal frame, validating a saved one, and deciding whether a window opens a folder or the empty state. Foundation and CoreGraphics only; the AppKit glue feeds it real window frames, `NSScreen` visible frames and a `FileManager`-backed directory check.

**Public API**

- `restoredWindowFrame(x:y:width:height:screenVisibleFrames:) -> CGRect?` — `nil` means discard the saved geometry and keep a new window's placement. It rejects two cases: a non-positive width or height, which is the "geometry never reported" sentinel; and a frame intersecting no screen's visible frame, which would restore the window fully offscreen after a display is disconnected.
- `normalFrame(previous:frame:zoomed:fullScreen:) -> CGRect?` — `frame`, unless the window is zoomed or full screen, which keep `previous`: the window state saves the un-zoomed, non-fullscreen frame.
- `launchDecision(restored:home:isDirectory:) -> LaunchDecision` — `.folder(cwd:)` only when `restored` is non-empty and still names a directory, since it may have been deleted or replaced by a plain file since the last run; otherwise `.emptyState(nvimCwd: home)`.

**Decisions**

- `launchDecision` never returns the process working directory. Nvim always needs a cwd, so the empty state launches it at `home`; that is a passive cwd the caller deliberately does not persist or record in the recent-dirs MRU, and a process-cwd fallback would pollute it.
- `isDirectory` is injected rather than reaching for `FileManager`, which is what keeps the seam pure; tests pass a set lookup.

#### `WindowStateCodec.swift`

Purpose: Converts between the front-end's persistence types and the `wolf_ffi_app_state_*` surface, in the shape [application-state.md](../../../../../specs/application-state.md) §2 defines.

**Public API**

- `submit(_:)` — writes `Window`s in list order, frontmost first, each with its normal frame, `activeTab`, `minimized` and `recency`, the tab indices most recently selected first that §4's project-state rule reads. The store writes the window state and the project states in one transaction. It does not flush.
- `load(_:) -> [LoadedWindow]` — frontmost first, a window with no tabs dropped. A `frame` is `nil` when the saved one has no position. Each `LoadedTab` carries `saved`, the tab as saved, which a view caches, and `rebuild`, the layout and bridge tab after `FfiAppTab.restored()`, `nil` with no root.

**Decisions**

- `SavedTab` and `SavedPaneNode` stay the gather, cache and plan types, the zoom a mark on its leaf. The codec converts the mark to and from the tab's `zoomed` pane index, the leaf's depth-first index, so `SessionSeam`, `PaneLayoutCodec` and `PaneZoom` need not know the bridge's shape.
- An empty tab (`cwd == ""`) writes no root, whatever its cache holds. A folder tab with no layout writes a lone edit pane on no file, which reads back as no layout, so a round trip is the identity.
- The per-tab extras are written as absent: `focused`, cursor, `top_line`, scroll, the sidebar's mode, query and toggles, and the drawer.
