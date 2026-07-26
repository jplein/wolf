# Fix: sporadic partial file-tree on inactive macOS windows

## Problem statement

The file-tree sidebar occasionally fails to load the full list of files on macOS.
Most reliable repro: launch Wolf with a two-window state — the frontmost window
loads the full tree; the inactive window loads a partial list. The list is
refreshed only once you open a file in the inactive window.

## Root cause (diagnosed)

The file-tree sidebar streams in asynchronously. Each view's `FileTreeView` runs
its own independent `start_tree_search()` traversal; streaming batch wakes trigger
`refresh()` to rebuild the outline, and the final batch fills the tree completely.

`Project.onTreeWake()` (`crates/wolf-macos/Sources/WolfMacOS/Project.swift`) routes
the tree wake to ONLY `currentView`. On restore, `materializeProjects`
(`crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift`) never calls
`project.setCurrent`, so inactive windows have `currentView == nil` (or pointing at
a sibling tab). Their streaming batch wakes are dropped, leaving a partial tree
until the window becomes key (opening a file → `onRenderWake` → tree refresh shows
the full list).

## Confirmed FFI facts

- Each `start_tree_search()` is fully independent — multiple concurrent traversals
  coexist on one core; a second call never cancels the first. Every view's own
  traversal completes regardless of focus; only the wake notification is lost.
- The tree wake token is per-project and carries no view/session discriminator.

## Fix (proposed — architect to finalize)

Change `Project.onTreeWake()` to fan the tree wake out to ALL attached views'
trees (iterate `views.allObjects`, call `view.tree?.scheduleRefresh()` on each),
not just `currentView`. Each `FileTreeView.refresh()` is already gated on its own
`search.version()` and coalesced via `refreshCoalescer`, so refreshing every view
is cheap and idempotent.

Do NOT change `onWake` (render wakes) or `onPickerWake` — those routing policies
are correct by design (nvim only renders the current tabpage; pickers are
transient on the key window only).

## Testing

Repo prefers end-to-end/integration tests with a real Neovim process. Add/adjust a
test reproducing the partial-tree condition, ideally exercising the per-project
wake fan-out to multiple views. The AppKit NSOutlineView shell is manual-smoke
only; make the routing logic testable via a seam if one fits (consult
`crates/wolf-macos/README.md` and the WolfKeyMap headless test suites).

## Documentation

`architecture.md` says "the project fans every wake out to its currentView; other
views hold their last painted frame until refocused." Accurate for RENDER wakes,
but must be corrected/clarified for the TREE wake. Update `architecture.md`,
`crates/wolf-macos/README.md`, and the `Project.swift` / `TreeWakeBridge.swift`
doc comments so docs and implementation agree.

## Architect's plan (authoritative)

### Code change — `crates/wolf-macos/Sources/WolfMacOS/Project.swift`
In `onTreeWake()` (lines 148-151), replace `currentView?.tree?.scheduleRefresh()`
with a fan-out over all attached views:
```
for view in views.allObjects {
  view.tree?.scheduleRefresh()
}
```
`views` is `NSHashTable<WolfWindowController>.weakObjects()`; `.allObjects` returns
a `[WolfWindowController]` snapshot — the same idiom `detach`/`closeTabpage`
already use. `view.tree` is `private(set) var tree: FileTreeView?` (same module).
Leave `onWake` and `onPickerWake` routing to `currentView` only.

Doc-comment updates in `Project.swift`: file header (8-11), `wakeToken` (35-40),
and `onTreeWake()` (148) — distinguish RENDER/picker wakes (currentView only) from
the TREE wake (fanned to ALL views; each view runs its own independent
`start_tree_search()`, per-project token carries no view discriminator; fan-out is
cheap/idempotent via per-view `search.version()` gate + `refreshCoalescer`).

### Seam / test decision — NO new seam, NO new automated test
The routing change is a constant policy (tree→all, render/picker→current), not an
input-dependent decision, so a `WakeFanout`-style seam would test a hard-coded map
= ceremony, not coverage. Sibling `onWake`/`onPickerWake` are untested manual-smoke
today; the load-bearing idempotence primitives are already pinned by existing
`TreeRefreshGateTests` + `PickerRefreshCoalescerTests`. Verification is doc
alignment + manual smoke (the NSOutlineView shell is manual-smoke by convention).

### Doc updates
- `TreeWakeBridge.swift` header (2-6) + enum doc (12-20): token resolves to the
  PROJECT, which fans refresh to ALL its views' sidebars (not one window).
- `architecture.md` line 272: qualify "the project drives wakes only to
  `currentView`" → "drives RENDER wakes only to `currentView`". Line 204 (file-tree
  sidebar paragraph): add that the tree wake is per-PROJECT and fanned to ALL of a
  project's views' sidebars.
- `crates/wolf-macos/README.md` line 83 + line 101 (`TreeWakeBridge` bullet):
  `onTreeWake()` fans to all attached views. Leave `PickerWakeBridge`/`WakeBridge`.
- `crates/wolf-macos/Sources/README.md` line 50 + line 72: same split.
- Keep Markdown paragraphs as single long lines (no hard breaks); `prettier
  --write` the edited `.md` files.

### Implementation order
1. Edit `Project.swift` `onTreeWake()` body. 2. Update 3 `Project.swift` doc
comments. 3. `TreeWakeBridge.swift` docs. 4. `architecture.md` (204, 272).
5. `crates/wolf-macos/README.md` (83, 101). 6. `Sources/README.md` (50, 72).
7. `prettier --write` the `.md` files. 8. swift-format + build/verify.

### Verification (coder runs)
- `bazel build //crates/wolf-macos:Wolf`
- `bazel test //crates/wolf-macos:WolfKeyMapTests` (regression sanity — unchanged)
- swift-format check via `//tools/lint` (no BUILD srcs edit — no new files)
- `prettier --write` on `architecture.md`, `crates/wolf-macos/README.md`,
  `crates/wolf-macos/Sources/README.md`
- Manual smoke: two-window restore repro — both windows load the FULL tree without
  refocus/opening a file; plus same-project two-tab variant.

Note: doc edits (architecture.md/READMEs/`TreeWakeBridge.swift`) are assigned to
the technical-writer stage; the coder handles `Project.swift` + build/verify.

## Status

- [x] Architect plan
- [~] Tests: NONE (deliberate — see seam/test decision above)
- [x] Implementation (Project.swift): `onTreeWake()` fans to `views.allObjects`;
      3 doc comments updated. `bazel build //crates/wolf-macos:Wolf` OK;
      `WolfKeyMapTests` PASSED (unchanged); `//tools/lint:swift_format` PASSED.
      (SourceKit "No such module 'WolfFFI'" is editor-only noise — bazel build OK.)
- [x] Review — APPROVED. `onTreeWake()` at `Project.swift:161-165` fans to
      `views.allObjects`; `onWake`/`onPickerWake` unchanged; doc comments accurate;
      no new hazards; root cause addressed.
- [x] Documentation: TreeWakeBridge.swift header/enum doc (per-project → all
      views); architecture.md (204 sidebar paragraph, 272 RENDER-wake qualifier,
      + 3 general routing statements at ~43/65/240); both READMEs (onTreeWake fans
      to all views, TreeWakeBridge bullet). prettier --write: all unchanged/clean.

## COMPLETE ✅

Bug fixed: `Project.onTreeWake()` now fans the per-project file-tree wake out to
ALL attached views' sidebars (`for view in views.allObjects { view.tree?
.scheduleRefresh() }`) instead of only `currentView`. This ensures an inactive /
non-current window (currentView == nil on restore, or pointing at a sibling tab)
receives its streaming tree-traversal batch wakes and loads the FULL file list —
no longer a partial tree until you open a file. Render/picker wakes stay
currentView-only by design. Fan-out is cheap/idempotent (per-view `search.version()`
gate + `refreshCoalescer`). No automated test (deliberate — constant routing
policy; idempotence primitives already covered by TreeRefreshGateTests /
PickerRefreshCoalescerTests). build + WolfKeyMapTests + swift_format all green;
reviewer APPROVED; docs synced. NOT committed. Manual smoke (two-window / two-tab
restore) recommended before shipping.
