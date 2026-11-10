# wolf-macos/Sources/WolfKeyMap/Sidebar

The pure decisions behind the sidebar's two panels, the file tree and the content search, and the View menu checkmarks that name the visible one. The directory compiles into the [`WolfKeyMap`](../README.md) module, not a module of its own.

Coverage lives in [`Tests/WolfKeyMapTests/`](../../../Tests/WolfKeyMapTests/README.md).

## Files

#### `SearchBudgetSeam.swift`

Purpose: Converts a sidebar pixel width and a monospace per-character advance into an integer character budget for a matched search line wrapped across up to `lines` rows.

**Public API**

- `charBudget(sidebarWidth:charAdvance:lines:) -> Int` — an unmeasurable font (`charAdvance <= 0`) floors the per-row count to 1 rather than dividing by zero.

**Decisions**

- The budget is a character count rather than a pixel width because it is consumed by wolf-core's `context_window`, which slices on character boundaries and so never splits a multibyte scalar.

#### `SearchHighlightSeam.swift`

Purpose: Maps wolf-core's byte-offset bold match ranges onto the UTF-16 spans that `NSAttributedString` and `NSRange` need.

**Public API**

- `SearchSpan` — one run of the display line with a uniform `bold` flag, measured in UTF-16 units.
- `spans(text:boldByteRanges:) -> [SearchSpan]` — `boldByteRanges` are byte `[start, end)` offsets into `text`'s UTF-8, the shape core returns. The result is ordered, contiguous and gap-filling, covering every UTF-16 unit with exactly one span, so a caller can style the whole line by walking the array and needs no fallback for an uncovered remainder. Empty ranges yield a single non-bold span, and an empty string yields none.

**Decisions**

- The seam exists because the two sides index differently: Rust returns byte offsets while AppKit's text ranges are UTF-16, and a scalar outside the basic multilingual plane occupies two UTF-16 units, so the offsets cannot be reused as-is.

#### `SearchToggleSeam.swift`

Purpose: The initial states for the sidebar search panel's three toggle buttons.

**Public API**

- `initialCaseSensitive(ignorecase:) -> Bool` — the inverse of Neovim's `:ignorecase`.
- `defaultUseRegex` — regex search is on by default. `defaultAllFiles` — off by default, so search covers Markdown only.

**Decisions**

- Case sensitivity is seeded from Neovim once, when the panel is first shown; toggling the button never writes back, so the search panel cannot change the editor's `:ignorecase`.

#### `SidebarMenuSeam.swift`

Purpose: The pure decision behind the View menu's Project and Search checkmarks. Foundation only, so it is testable without an `NSMenuItem` or a window.

**Public API**

- `SidebarPanel` — names one sidebar panel, the file tree or the content search. Also used by `WolfWindowController` to name the live panel.
- `isChecked(sidebarVisible:current:item:) -> Bool` — at most one entry is checked; a hidden sidebar or a nil `current` checks nothing.

**Gotchas**

- Checked and enabled are separate concerns. `AppDelegate.validateMenuItem` uses this seam only for the checkmark and always leaves both items enabled, since choosing either while the sidebar is hidden is what reveals it.

#### `SidebarRowStyle.swift`

Purpose: The pure choice of label color for one file-tree sidebar row — the current buffer's mark. Foundation only, so it is testable without an outline view.

**Public API**

- `SidebarLabelColor` — the color role of one row's label: `.normal`, `.current` (the current buffer's accent), or `.selected` (an emphasized selection's contrasting text).
- `labelColor(isCurrent:isEmphasized:) -> SidebarLabelColor` — an emphasized selection wins, because accent text on an accent fill is unreadable; otherwise only the current buffer is accented.
- `markedFile(previewSource:currentFile:) -> String` — which file the sidebar marks. A previewing pane's source wins; an empty or nil source falls back to the edited buffer, and `""` marks nothing.

**Decisions**

- A preview pane's scratch buffer is unnamed, and nvim's `wolf_current_file` reports `""` for it, so `currentFile` alone cannot name what a preview shows. The front-end supplies the per-pane source through `PaneContainerView.activePreviewSourcePath`.
- The accent depends on the current-buffer flag alone, never on the row's selection. AppKit tints a selected source-list label only while the row view is emphasized — a key window whose first responder is the outline — and the sidebar outline never becomes first responder, so a selection tint holds until the first app switch and never returns.

#### `TreeChildrenMemo.swift`

Purpose: A per-directory memo for the sidebar file tree's child projection, so the injected `project` runs at most once per directory per invalidation window. Generic over `Item` and Foundation only, so it is testable without AppKit or a live search.

**Public API**

- `init(project:)` — the injected projection from a directory rel-path to its immediate children.
- `children(ofDir:)` and `invalidate()` — `invalidate` drops every cached array, keeping the dictionary's capacity.

**Decisions**

- It exists because `NSOutlineView` asks for one directory's children in a storm — `numberOfChildrenOfItem`, then `child:index:` once per child — and each callback would otherwise re-run the FFI `children_of(dir)`, an O(N) scan of the flat file list.

**Gotchas**

- `FileTreeView` projects the same `TreeItem` instances it hands out from `itemsByRelPath`, so memoizing does not disturb the outline's stable item identity.
- `FileTreeView` invalidates only inside the `TreeRefreshGate.shouldReload` branch and on reroot. A version-unchanged refresh — an expand, a collapse, a cursor move — keeps serving from the still-valid memo, which is the point; invalidating more eagerly reinstates the storm.

#### `TreeRefreshGate.swift`

Purpose: A refresh gate for a sidebar panel on two axes, visibility and file-search snapshot version. Foundation only, so it is testable without AppKit; `FileTreeView` and `SearchView` each own one, relaying the sidebar's real `isCollapsed` KVO into `setVisible` and `search.version()` into `shouldReload`.

**Public API**

- `requestRefresh() -> Bool` — true while visible. While hidden it records that a catch-up is owed and returns false, and the caller does nothing.
- `setVisible(_:) -> Bool` — true only on a hidden-to-visible transition that has deferred work, so exactly one catch-up refresh runs on reappear.
- `shouldReload(version:) -> Bool` — true only when `version` differs from the last reloaded one, which it then records. The initial last-reloaded version is nil, so the first call at any version, including 0, reloads.
- `resetReloadTracking()` — forgets that version so the next `shouldReload` reloads unconditionally. Called on reroot, where the outline is rebuilt wholesale.

**Decisions**

- Both gates guard the same expense, the outline reload and reveal. Without the visibility gate that work runs while the sidebar is collapsed, so hiding the sidebar does not relieve a freeze; without the version gate a full `reloadData()` runs on every render wake even when nothing structural changed.
