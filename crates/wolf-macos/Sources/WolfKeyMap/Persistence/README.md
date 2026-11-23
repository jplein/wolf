# wolf-macos/Sources/WolfKeyMap/Persistence

The pure decisions behind the window state of [application-state.md](../../../../../specs/application-state.md): what each live view writes to it, in what order, how a pane layout encodes, and how windows, tabs and layouts come back from it on launch and on Open Folder. The directory compiles into the [`WolfKeyMap`](../README.md) module, not a module of its own.

Coverage lives in [`Tests/WolfKeyMapTests/`](../../../Tests/WolfKeyMapTests/README.md).

## Files

#### `OpenFolderSeam.swift`

Purpose: The pure decision behind Open Folder ([§6](../../../../../specs/application-state.md#6-opening-a-folder)): which step a directory takes, its canonical form, and the frame a fill applies. The AppKit glue is `AppDelegate.openFolder(_:from:)` ([why](../../../../../notes/wolf-macos-decisions.md#opening-a-folder)).

**Public API**

- `step(directory:current:windows:stacking:) -> Step` — `.focus(tab:)` names the tab id §4's rule picks, through `WindowStateCodec.projectTab`; else `.fill` when `current`'s directory is `""`, else `.newWindow`. `current` is `nil` when no Wolf window is key. A `LiveTab`'s directory is `""` for an empty tab.
- `canonicalDirectory(_:)` — `realpath(3)`, falling back to the path as given.
- `fillFrame(stored:windowTabs:screenVisibleFrames:) -> CGRect?` — `stored`, validated by `StateSeam.restoredWindowFrame`, only when the filled tab is its window's only tab; `nil` keeps the window's frame.

**Decisions**

- The windows are put in `stacking.order` and each takes its selected tab as active, else its most recent one, so the focused tab is §4's and not the first in list order.
- `step` reads live directories, never a tab's cached contribution, which can name another directory.
- `canonicalDirectory` never uses Foundation's `resolvingSymlinksInPath`, which maps `/private/tmp` to `/tmp`, while core's `apply_cwd` canonicalizes to `/private/tmp`: tab directories and project-state keys are core's form.

#### `PaneLayoutCodec.swift`

Purpose: The pure codec between the live `PaneTree` layout and the shapes that persist or cross the FFI boundary.

**Public API**

- `SavedPaneNode` — a `Hashable` mirror of the split tree keyed by per-leaf buffer path. `leafCount` is how many window handles the rebuild zips onto the leaves. `isEmptyLeaf` is the suppress-from-persistence predicate for a lone pane: true only for an unnamed leaf that is neither a preview nor a terminal, whatever its view. `zoomedLeafIndex` is the first zoomed leaf's depth-first index, and `leafViews` each leaf's `view` in that order.
- `SavedPaneView` — an edit leaf's captured `cursor` and `topLine`, or a preview's `scroll`.
- `encode(_:paths:previews:terminals:zoomed:views:)` — maps leaf ids to paths and views, an unmapped id to `""` and an empty view. A terminal leaf's path is the directory its shell starts in. It marks the leaf whose id is `zoomed`, and the drawer's id marks none: the drawer's zoom is `SavedDrawer.zoomed`.
- `postOrderTokens(_:)` and its inverse `decodeTokens(_:)` — the post-order stream the FFI app-state builder replays; `decodeTokens` returns `nil` on a malformed stream.

**Decisions**

- There is no Swift renderer for the layout-rebuild document: `WolfCoreHandle.rebuild_tab` renders the restored `FfiAppTab` in Rust, so the format cannot drift between front-ends ([the rebuild-layout wire format](../../../../../notes/wolf-core-decisions.md#the-rebuild-layout-wire-format)).
- A `preview` leaf stores the source Markdown file's absolute path, not the scratch buffer's, because restore reopens it through `wolf_preview_file` rather than `:edit`.

**Gotchas**

- The token stream is a contract with `wolf-ffi`'s app-state builder and cannot be changed on one side alone.

#### `RestoreSeam.swift`

Purpose: The pure restore planner: one planned tab per saved tab, each to start its own Neovim, so tabs on one directory are never merged ([why](../../../../../notes/wolf-macos-decisions.md#one-neovim-per-tab)).

**Public API**

- `plan(windows:home:isDirectory:) -> [PlannedTab]` — in window then tab order. `isDirectory` is the seam's only injected impurity (the app passes a `FileManager` check, tests pass a set lookup).
- `SavedTab` and `SavedWindow` are the inputs; `SavedTab` is also what a live view contributes to a commit, through [`SessionSeam`](#sessionseamswift). Its `focused` is a `SavedPaneRef`, a depth-first leaf index or the drawer. Its `sidebar` is a `SavedSidebar`, each field `nil` when unsaved, and its `drawer` a `SavedDrawer`, whose `zoomed` is the drawer's zoom, since the drawer is no leaf to mark. A `PlannedTab` retains its `groupIndex` and `positionInGroup` so the window-assembly phase can drop it back into the right OS window and tab group.
- `PlannedTab.launchCwd` and `hasFolder` are [`launchDecision`](#stateseamswift)'s: the saved directory when it still is one, else `home` in the empty state. `PlannedTab.cwd` still carries the original saved string.
- `selectedPosition(active:materialized:)` — the saved active position when its tab materialized, else the first that did, `nil` when none did.

**Decisions**

- `layout` is a passthrough: it rides from `SavedTab` into the matching `PlannedTab` untouched.

#### `SessionSeam.swift`

Purpose: The write side of state persistence, as in wolf-gnome's [`state_seam.rs`](../../../../wolf-gnome/src/README.md#state_seamrs): what each live view contributes to the window state, and whether a commit or a close writes anything. The rules are in [wolf-macos-decisions.md](../../../../../notes/wolf-macos-decisions.md#state-is-a-tree-and-the-front-end-owns-every-write).

**Public API**

- `gatherTab(...) -> TabGather` — `.refuse` when a folder view's cwd is `""` or any leaf is `.unreported` or missing from `content`. `zoomed` and `views` are [`encode`](#panelayoutcodecswift)'s, and `active` and `drawer` give the tab's `focused` and `drawer`; a view without a folder keeps only `sidebar`.
- `terminate(_:capture:commit:flush:teardown:)` — the quit's order: captures, commit, flush, then teardowns, so no Neovim exits before the final write.
- `mayGather(...)`, `contribution(mayGather:gather:cache:)` — the gathered tab when the gate is open and the gather ready, else `cache`, which the caller stores as its next cache. `gather` is not evaluated behind a shut gate.
- `windowState(live:stacking:orphans:)` — the windows a commit writes, frontmost first by [`StackingOrder`](#stackingorderswift), minimized ones last, orphans after the live windows of their class. A window keeps only its contributing tabs, or drops; `activeTab` falls back to its most recently focused contributing tab.
- `closeCommit(terminating:otherLiveViews:)`, `committedWindows(liveViews:windows:)` and `orphanedSlots(savedTabCounts:materialized:)` — `nil` from `committedWindows` means commit nothing.
- `CloseBurst(scheduleEnd:read:commit:)` and its `close(terminating:otherLiveViews:)` — `closeCommit` over the closes of one runloop turn, ended by `scheduleEnd`'s closure.

**Decisions**

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

Purpose: Converts between the front-end's persistence types and the `wolf_ffi_app_state_*` surface, in [§2](../../../../../specs/application-state.md#2-the-state-model)'s shape.

**Public API**

- `submit(_:)` — writes `Window`s frontmost first, each `recency` most recently selected first, which §4's project-state rule reads. One transaction writes the window state and the project states, unflushed.
- `load(_:) -> [LoadedWindow]` — frontmost first, a window with no tabs dropped, a `frame` `nil` without a position. Each `LoadedTab` carries `saved`, as saved, which a view caches, and `rebuild`, the tab after `FfiAppTab.restored()`, `nil` with no root.
- `projectTab(_:windows:)` — the tab §4's rule picks for Open Folder, `nil` when none is open. It feeds the builder without committing.
- `loadProject(_:)` — a directory's project state, read as `load` reads a window, else `nil`.

**Decisions**

- `SavedTab` and `SavedPaneNode` stay the gather, cache and plan types, the zoom a mark on its leaf. The codec converts the mark to and from the tab's `zoomed` pane index, the leaf's depth-first index, and `SavedDrawer.zoomed` to and from a `zoomed` drawer, so `SessionSeam`, `PaneLayoutCodec` and `PaneZoom` need not know the bridge's shape.
- An empty tab (`cwd == ""`) writes its sidebar and no root, focus, zoom or drawer, whatever its cache holds. A folder tab with no layout writes a lone edit pane on no file, which reads back as no layout, so a round trip is the identity.
- A sidebar with any field set writes every field, an unset mode as `UNSET` and an unset query or toggle as `""` or `false`.
