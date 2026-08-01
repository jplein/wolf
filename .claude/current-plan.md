# Per-pane file navigation history (back/forward)

## Status

**Phase: tests** — architecture complete and approved. Test-writer next.

## Problem statement

Wolf has no per-pane navigation history. Every recency store is keyed at a coarser scope: `mru.rs` per-project, `recent_dirs.rs` / `command_mru.rs` global, `buffer_picker.rs` per-Neovim-instance (= per tab). Per-pane state is strictly _current_ state, with nothing ordered or history-like.

Add a per-pane back/forward stack over the files, previews, and terminals visited in that pane, surfaced as toolbar buttons and a new top-level History menu.

Explicitly NOT cursor-position history — Neovim's own jumplist (`<C-o>` / `<C-i>`) stays untouched.

---

# PART A — LOCKED DECISIONS (authoritative on WHAT)

Settled with the user before implementation. **Do not re-litigate.**

- History lives in **wolf-core**, single source of truth, keyed by nvim **window handle** (`i64`). Swift holds **no copy**.
- **Not persisted** across restarts.
- **Push trigger:** ANY change to a pane's current file, including Neovim-internal navigation (`:e`, `gd`, buffer picker).
- **Truncate on new navigation** (browser semantics). `A -> B -> C` with `A` active; navigating to `D` leaves `A -> D`.
- **Keep every visit** (literal visit log). `A -> B -> A -> B` = four entries.
- **Terminal drawer EXCLUDED** (no history). A `:terminal` in a **normal split** IS in that pane's history.
- **Untitled buffers tracked** by bufnr, labeled `Untitled`.
- Stack cap **100** (oldest dropped, cursor adjusted). Menu cap **10 DISTINCT rows**.
- `NavTarget` is the dedup key: `Edit(B)` and `Preview(B)` are DISTINCT entries.
- **Push detection rule:** present in `previews` → `Preview`; present in `buffers` → `Edit`/`Terminal`/`Untitled`; in **NEITHER → no entry, no push, no prune**. This last clause is what makes the two notifications' arrival order irrelevant during a preview transition. **Do not drop it.**
- **Feedback-loop suppression:** back/forward/jump record `expected: Option<NavTarget>` per window; the next derived change matching it is ABSORBED, a non-matching one PUSHES. **Write this test first.**
- **`buffers` map must stay behaviorally byte-identical.** Add a PARALLEL `win_buffers` map; do not change `buffers`' type.
- **Menu:** Back (⌘\[) / Forward (⌘\]) / separator / rows newest-first, ≤10 distinct, checkmark on current. Rows CLICKABLE, jumping to that stack index; the checkmarked row targets the **cursor's own index** so clicking it is a true no-op.
- **Toolbar order:** `traffic lights → toggle sidebar → sidebar separator → back → forward → title`.
- **Label disambiguation:** PREFIX form, minimal-growth. Worked example `foo/README.md`, `bar/README.md`, `bar/some-other-file.md` → `some-other-file.md`, `bar/README.md`, `foo/README.md`. Deeper case `a/b/README.md` vs `c/b/README.md` must grow to TWO segments. Pathless rows excluded from collisions, numbered `Terminal`, `Terminal 2`, `Untitled`, `Untitled 2`.
- wolf-gnome gets core support for free; **no gnome UI in this pass**.

**Superseded brief item:** `TabTitleSeam.swift` + its tests were DELETED by the just-merged `96d2c97 Toolbar` (the title now shows the cwd). The Rust implementation is the ONLY implementation — there is no duplication to document. The old algorithm is a **shape reference only** at `git show cceca62:crates/wolf-macos/Sources/WolfKeyMap/TabTitleSeam.swift` (it renders SUFFIX form; this needs PREFIX form). Do not restore or call it.

**Build:** `bazelisk`, never cargo. `rustfmt_core` + `rustfmt_ffi` must pass.

---

# PART B — IMPLEMENTATION PLAN (authoritative on HOW)

## B1. Summary

A new pure module `crates/wolf-core/src/nav_history.rs` holds the data model, the browser-semantics stack ops, and the PREFIX-form label pipeline. `runtime/nvim/init.lua`'s `wolf_buffers` emit grows three elements (`bufnr`, `is_terminal`, `is_drawer`) feeding a new parallel `Snapshot.win_buffers` (the existing `buffers` map left byte-identical). `RedrawHandler` folds a derived per-window `NavTarget` after each `wolf_buffers` / `wolf_preview` emit into an `Arc<Mutex<NavHistory>>` sibling of the snapshot (mirroring `TrackedCwd` / `RecentDirs`), pushing on genuine changes and absorbing self-caused ones via a per-pane `expected`. Six new `WolfCoreHandle` FFI methods surface it to Swift, which draws a History menu (`NSMenuDelegate`-rebuilt) plus two toolbar buttons after the sidebar tracking separator.

## B2. Ownership and fold placement

**Fold call sites** — two, both `crates/wolf-core/src/redraw.rs`:

- `RedrawHandler::apply_buffers` (`redraw.rs:517`) — tail, after `snap.buffers = map; snap.win_buffers = win_map;`, under the write guard.
- `RedrawHandler::apply_previews` (`redraw.rs:437`) — tail, after `snap.previews = map;`, under the write guard.

Both call one new private `RedrawHandler::fold_nav_targets(&self, snap: &Snapshot)`. It must run in **both** places because the derived target joins BOTH maps and either notification can complete a transition.

**Ownership: NOT on `Snapshot`** — `Snapshot` is render state, derives `Clone`, is read under the paint `RwLock` every frame. History is out-of-band app state mutated from FFI accessors that also do blocking RPC.

**Ownership: NOT solely on `RedrawHandler`** — it is `Clone`, re-cloned on restart; a _writer_, not an owner.

**Follow the `IntentQueue` / `CommandQueue` / `TrackedCwd` / `RecentDirs` pattern** (`redraw.rs:30-63`, `api.rs:108-148`, `api.rs:261-268`):

```rust
// redraw.rs, next to IntentQueue / CommandQueue / TrackedCwd
pub type NavHistoryCell = Arc<parking_lot::Mutex<crate::nav_history::NavHistory>>;
```

- `WolfCore.nav_history: NavHistoryCell` (new field, `api.rs`)
- `RedrawHandler.nav_history: NavHistoryCell` (new field + new 7th param of `RedrawHandler::new`, inserted after `recent_dirs`, before `wake`)
- Created in `WolfCore::start` next to `intents` / `commands`, cloned into `RedrawHandler::new`

`restart_nvim` clones the SAME handler, so history survives a respawn by construction. Acceptable — the new nvim reissues fresh window handles, so stale panes are never queried. Document it in the module docs.

## B3. `win_close` pruning

**Core DOES see it:** `redraw.rs:857` routes `"win_close"` → `fn win_close(snap, params)` at `redraw.rs:1168`. **But the payload is `[grid]`, not a window handle.** The handle is recoverable _before_ removal: `snap.windows.get(&grid).map(|w| w.win)`.

`apply_event` is a free function with only `&mut RwLockWriteGuard<Snapshot>`, so thread a collector:

- `fn apply_event(snap, name, params, closed_wins: &mut Vec<i64>) -> bool`
- `fn win_close(snap, params, closed_wins: &mut Vec<i64>)` — resolve `snap.windows.get(&grid)` → `w.win`, push onto `closed_wins` **before** `snap.windows.remove(&grid)`
- `RedrawHandler::apply_redraw` declares `let mut closed_wins: Vec<i64> = Vec::new();` before the batch loop; after the loop (guard held, no await) calls `self.nav_history.lock().prune_many(&closed_wins)` when non-empty

**Known limit — document, do not fix.** `win_hide` (`redraw.rs:1149`, fired for every window of a tabpage being left) already removes the `windows` entry. A `win_close` for a grid whose entry `win_hide` removed leaves the handle unrecoverable and that pane's history orphaned. nvim never reuses window handles, so an orphan is only memory, bounded at 100 entries per orphaned pane per nvim-process lifetime. **Explicitly do NOT add an "absent from `wolf_buffers` for N emits" reaper** — a window in a non-current tabpage is absent from both maps by construction, and pruning it would violate the locked no-prune rule.

## B4. How back/forward moves the pane

| target | mechanism | status |
| --- | --- | --- |
| `Preview { source }` | `WolfCore::preview_file(win, path)` (`api.rs:666`) → `NvimProcess::preview_file` (`nvim.rs:508`) → `_G.wolf_preview_file(win, path)` (`init.lua:1706`). Window-scoped `nvim_win_set_buf`, re-emits previews + current-file, returns source bufnr (`>0`) or `0`. | **Reuse verbatim** — this is the link-follow path; no change needed |
| `Terminal`/`Untitled { bufnr }` | `_G.wolf_switch_buffer(win, buf)` (`init.lua:1309`) — window-scoped `nvim_win_set_buf` + per-`(winid, bufnr)` `winrestview`. `pcall`-wrapped, silent, returns nothing → a WIPED buffer is an invisible no-op | **New thin wrapper needed** |
| `Edit { path }` | Nothing window-scoped exists. `InputEvent::Open` is `:edit` on the CURRENT window and records the file MRU. Closest precedent: `wolf_edit_source`'s local `edit_into` (`init.lua:1648`) | **New helper needed** |

**Two new Lua globals** in `runtime/nvim/init.lua`, immediately after the `wolf_setup_view_tracking` block (so `_G.wolf_switch_buffer` is defined):

```
_G.wolf_history_set_buf(win, bufnr) -> bufnr | 0
    -- 0 unless nvim_win_is_valid(win) AND nvim_buf_is_valid(bufnr)
    -- else _G.wolf_switch_buffer(win, bufnr) (set_buf + view restore), return bufnr

_G.wolf_history_open(win, path) -> bufnr | 0
    -- 0 for an empty path or invalid win
    -- local buf = vim.fn.bufadd(path); vim.fn.bufload(buf)
    -- delegate to _G.wolf_history_set_buf(win, buf)
```

Contract notes for the Lua comment AND `runtime/nvim/README.md`:

- `bufadd(path)` returns the EXISTING buffer when the name matches, so navigating back to an unsaved `Untitled N.md` (which HAS a real cwd-joined name — `wolf_name_untitled`, `init.lua:1424`) rebinds the same modified buffer rather than reading a nonexistent file. **Load-bearing.**
- Window-scoped `nvim_win_set_buf`, **never `:edit`** — no disk re-read, no `current_file`/file-MRU churn, exactly like `wolf_switch_buffer`.
- A buffer WIPED since recording yields `0`, which core turns into a cursor revert (B6).

**Rust plumbing** — mirror `NvimProcess::preview_file` (`nvim.rs:508`) exactly:

```rust
// crates/wolf-core/src/nvim.rs
pub fn history_open(&self, win: i64, path: &str)
    -> impl Future<Output = Result<i64, NvimError>> + Send + 'static;
pub fn history_set_buf(&self, win: i64, bufnr: i64)
    -> impl Future<Output = Result<i64, NvimError>> + Send + 'static;
```

Both routed through `WolfCore::block_on_rpc` (bounded by `rpc_timeout`), returning `0` on error like every sibling.

## B5. `wolf_buffers` extension

**Lua** (`init.lua:327-351`). The preview-scratch skip at `:336` **STAYS** (it is what makes the two maps disjoint). Change only the tuple at `:337`:

```lua
list[#list + 1] = {
  win,
  vim.api.nvim_buf_get_name(buf),          -- element 2, UNCHANGED
  buf,                                      -- element 3: bufnr
  vim.bo[buf].buftype == "terminal",        -- element 4: is_terminal
  vim.b[buf].wolf_terminal == true,         -- element 5: is_drawer
}
```

Element 5 is an **[architect's call, ORCHESTRATOR-APPROVED]**: the locked decisions named only `bufnr` + `is_terminal`, but the drawer must be excluded while a split `:terminal` must be included, and `b:wolf_terminal` (set at `init.lua:1033`) is the only marker distinguishing them. Carrying it here avoids core tracking `DrawerOpen` intents.

**New type** in `crates/wolf-core/src/render.rs`, beside `PreviewInfo` / `TabFiles`:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WinBuffer {
    pub bufnr: i64,
    pub path: Option<PathBuf>,   // canonicalized; None for an unnamed buffer
    pub is_terminal: bool,
    pub is_drawer: bool,
}
```

**New snapshot field**, immediately after `pub buffers` (`render.rs:719`): `pub win_buffers: HashMap<i64, WinBuffer>,`

Doc comment must state: keyed by WINDOW handle; includes UNNAMED windows (unlike `buffers`); excludes preview scratch panes (they are in `previews`); wholesale-replaced per emit; does NOT bump `seq`; defaults to empty. `Snapshot` derives `Default` and `Snapshot::new` uses `..Self::default()` — **no constructor change required**.

**Parser** (`redraw.rs::apply_buffers`, `:517`). Build BOTH maps in the SAME pre-guard pass, assign both under the guard. The `buffers` arm must be **character-for-character the current logic**: skip a missing/non-string element 2, skip an EMPTY path, `std::fs::canonicalize(...).unwrap_or_else(|_| PathBuf::from(path))`. The `win_buffers` arm inserts for EVERY tuple decoding a `win`: `bufnr` from `ext_handle(pair.get(2))` defaulting `0`; `path` = `None` when empty else the same canonicalized value (**reuse the computed one — do not canonicalize twice**); `is_terminal` / `is_drawer` from `Value::as_bool` defaulting `false`. A legacy 2-element tuple yields `bufnr: 0, is_terminal: false, is_drawer: false`.

**EVERY existing consumer of `Snapshot.buffers`** (verified by ripgrep over `crates/**/*.rs`) — the coder must verify none regress:

1. `crates/wolf-core/src/api.rs:1004` — `WolfCore::pane_status(grid)`, the **only** production consumer. Reached over FFI as `WolfCoreHandle::pane_status_for_grid` (`wolf-ffi/src/lib.rs:2252`), consumed by the macOS pane statusbar and by `PaneLayoutCodec` persistence (via `abs_path()`).
2. `crates/wolf-core/tests/pane_buffers.rs` — the real-nvim test.

Nothing in `wolf-ffi`, `wolf-macos`, or `wolf-gnome` reads `Snapshot.buffers` directly. Regression surface is exactly `pane_status` + `pane_buffers.rs` + `//crates/wolf-ffi:pane_status`.

## B6. Feedback-loop suppression mechanics

**Where `expected` lives:** a field on `PaneHistory` (`expected: Option<NavTarget>`), per WINDOW, inside the `NavHistory` cell. Not on `Snapshot`, not on the handler.

**Set:** inside `PaneHistory::back` / `forward` / `jump` — the same mutation that moves `cursor`. Set BEFORE `WolfCore` issues the nvim RPC, so a fold racing the RPC's autocmds already sees it.

**Cleared:** on the very next `observe` for that window, unconditionally, on all branches; by any subsequent `back`/`forward`/`jump` (overwritten); by `revert` on a failed navigation. **Consulted at most once; never a gate on pushing.**

**`observe(win, target)` — the exact rule:**

```
pane = panes.entry(win).or_default()
1. entries empty             -> seed: entries = [target], cursor = 0, expected = None
2. current() == Some(target) -> expected = None; return   (idempotent re-emit / our move landed)
3. expected == Some(target)  -> expected = None; return   (LOCKED absorb rule)
4. otherwise                 -> push(target)              (genuine navigation; push clears expected)
```

**Stale-expected rule:** because `expected` never blocks a push (branch 4 fires regardless), a stale `expected` can never wedge history. If Back to A fails silently and the pane stays at B, the next genuine navigation to C hits branch 4, truncates ahead of the moved cursor, appends C, clears `expected`. **History self-heals in one step — no timer, no epoch, no reaper.** State this explicitly in the module docs.

**Failed moves are also actively reverted.** `back`/`forward`/`jump` return `Option<NavMove { from, to, target }>`; `WolfCore` issues the RPC and, when the helper reports `0`, calls `NavHistory::revert(win, from: mv.to, to: mv.from)`, which restores the cursor **only if it is still `mv.to`** (a concurrent `observe` that already reconciled wins) and clears `expected`. Closes the realistic failures: a wiped `Terminal`/`Untitled` bufnr, a deleted/non-markdown `Preview` source.

**Known limit — document, do not fix.** Two Back presses issued faster than the first `wolf_buffers` emit is folded leave the first emit non-matching (`expected` already overwritten), producing one spurious push. The locked rule specifies exactly this ("a non-matching one is treated as a genuine navigation and pushes"). The window is sub-millisecond because `history_back` blocks on the RPC before returning to the main thread; worst outcome is slightly odd history, never a wedge. Record in `nav_history.rs` module docs and `crates/wolf-core/src/README.md`.

## B7. FFI plumbing

This crate is **swift-bridge**, not uniffi: one `#[swift_bridge::bridge] mod ffi` at `crates/wolf-ffi/src/lib.rs:314`, Swift/C bindings **committed** under `crates/wolf-ffi/generated/`, regenerated by `bazel run //crates/wolf-ffi:gen_bindings`. There is no `.udl`.

`FfiHistoryEntry` **must be an opaque `extern "Rust" type`**, not a `swift_repr = "struct"` — the repeated in-file rule is "`FfiPreview` is opaque so a `Vec<FfiPreview>` is supported"; swift-bridge cannot vectorize a repr-struct.

Add to the EXISTING `extern "Rust" { type WolfCoreHandle; ... }` block, immediately after `preview_file` / `previews` (`:608-613`):

```rust
fn history_can_back(&self, win: i64) -> bool;
fn history_can_forward(&self, win: i64) -> bool;
fn history_back(&self, win: i64) -> bool;
fn history_forward(&self, win: i64) -> bool;
fn history_jump(&self, win: i64, index: u32) -> bool;
fn history_entries(&self, win: i64) -> Vec<FfiHistoryEntry>;
```

New opaque block modeled on `FfiPreview` (`:639-662`), placed right after it:

```rust
extern "Rust" {
    type FfiHistoryEntry;
    fn display(&self) -> String;      // disambiguated label, e.g. "bar/README.md" / "Preview: alpha.md"
    fn index(&self) -> u32;           // STACK index to hand back to history_jump
    fn is_current(&self) -> bool;     // draws the checkmark
}
```

Rust impls beside `FfiPaneStatus` (`lib.rs:3238`) / `FfiPreview` (`:3088`):

```rust
pub struct FfiHistoryEntry { display: String, index: u32, is_current: bool }
impl FfiHistoryEntry {
    pub fn display(&self) -> String;  pub fn index(&self) -> u32;  pub fn is_current(&self) -> bool;
    fn from_menu_row(row: wolf_core::MenuRow) -> Self;   // u32::try_from(row.index).unwrap_or(0)
}
```

`WolfCoreHandle` methods follow the established `self.core.borrow().as_ref().map_or(default, |c| …)` shape: `false` / `Vec::new()` when the core is gone.

**[architect's call, ORCHESTRATOR-APPROVED]** `history_back`/`forward`/`jump` return `bool` (did the pane actually move). Locked list left it unspecified; free, already computed by the revert path, gives tests an assertion point.

**Regenerate `crates/wolf-ffi/generated/wolf-ffi.swift` + `wolf-ffi.h` and commit.** `wolf_ffi_wake.h` unchanged (no new wake channel; history rides the existing render wake).

## B8. Swift wiring

**Pure seam** — new `crates/wolf-macos/Sources/WolfKeyMap/HistorySeam.swift`, Foundation-only, following `PickerSeam` / `RestoreSeam`:

```swift
public enum HistorySeam {
  public struct Row: Equatable { public let title: String; public let index: Int; public let isCurrent: Bool }
  public static let emptyPlaceholderTitle = "No History"
  public static let backIdentifier    = "com.wolf.editor.toolbar.back"
  public static let forwardIdentifier = "com.wolf.editor.toolbar.forward"
  public static let backSymbolName    = "chevron.backward"
  public static let forwardSymbolName = "chevron.forward"

  /// Zips the three parallel FFI-derived arrays into menu rows, preserving order.
  /// Mismatched lengths truncate to the shortest; a negative index is dropped;
  /// at most ONE row keeps `isCurrent` (the FIRST, if several arrive).
  public static func rows(displays: [String], indices: [Int], currentFlags: [Bool]) -> [Row]

  /// The locked toolbar ordering: sidebar toggle, tracking separator, back, forward.
  public static func itemIdentifiers(toggleSidebar: String, trackingSeparator: String) -> [String]
}
```

**[architect's call, ORCHESTRATOR-APPROVED]** One seam file rather than two — the concerns are small and share the identifier constants, keeping the hand-maintained BUILD `srcs` lists to one added entry each. `chevron.backward`/`chevron.forward` over `chevron.left`/`chevron.right` because they are RTL-aware; over `NSImage.Name.goBackTemplate` because the toolbar is already SF-Symbol-styled `.iconOnly` unified.

**Menu** — `crates/wolf-macos/Sources/WolfMacOS/AppMenu.swift`:

- Extend `struct InstalledMenus` with `let history: NSMenu`.
- In `install(target:)`, build `historyMenuItem(target:)` and insert **between** `viewMenuItem` and `windowItem`. Return its submenu.
- `private static func historyMenuItem(target:) -> NSMenuItem`: Back (`keyEquivalent: "["`, `.command`, action `#selector(AppDelegate.historyBackMenuItem(_:))`), Forward (`"]"`, `historyForwardMenuItem`), `.separator()`, then a `placeholderItem()`-style disabled `HistorySeam.emptyPlaceholderTitle` so it is never empty before the first rebuild.
- `static func rebuildHistoryMenu(_ menu: NSMenu, rows: [HistorySeam.Row], target: AppDelegate)`: `removeAllItems()`, re-add Back / Forward / separator, then the placeholder (empty rows) or one item per row — `title: row.title`, `action: #selector(AppDelegate.historyJumpMenuItem(_:))`, `target: target`, `tag: row.index`, `state: row.isCurrent ? .on : .off`. Doc comment must note that `tag` here is a STACK INDEX, not a `command_kind` discriminant — the distinct selector keeps it unambiguous.

Do **not** route through `handleCommandMenuItem` / `CommandDispatcher`: no `wolf_core::Command` variant is added, so these are front-end-only items like `toggleSidebar`.

**`NSMenuDelegate` owner** — `AppDelegate.swift`. It is already `NSMenuDelegate` (`:19`) and already delegates `openRecentMenu` (`:97`). Mirror exactly:

- New field `private var historyMenu: NSMenu?`; in `applicationDidFinishLaunching` set `historyMenu = menus.history; historyMenu?.delegate = self`.
- `menuNeedsUpdate(_:)` (`:608`) currently `guard menu === openRecentMenu else { return }`. Restructure into an identity dispatch: `openRecentMenu` branch unchanged; a new `historyMenu` branch resolves `controllers.first { $0.owns(effectiveKeyWindow) } ?? controllers.first` and calls `AppMenu.rebuildHistoryMenu(menu, rows: source?.historyRows() ?? [], target: self)`.
- Three new actions — `historyBackMenuItem(_:)`, `historyForwardMenuItem(_:)`, `historyJumpMenuItem(_:)` — each resolving the key controller through the existing picker-panel-aware `effectiveKeyWindow` (`:73`) and calling `historyBack()` / `historyForward()` / `historyJump(index: sender.tag)`.
- `validateMenuItem(_:)` (`:638`) currently early-returns `true` for any non-`handleCommandMenuItem` action. Add three explicit branches **before** that guard: back → `keyController?.historyCanBack ?? false`, forward → `historyCanForward ?? false`, jump → `true`.

**Resolving the active pane's win handle.** `PaneContainerView.activePane` is `private` and `PaneBinding.win` is in a `private struct` (`:38, :211`), but the file already does `activePane.flatMap { panes[$0]?.win }` internally (`:572`, `:599`). Add an internal accessor near `var paneCount`:

```swift
var activeWin: Int64 { activePane.flatMap { panes[$0]?.win } ?? 0 }
```

Then on `WolfWindowController` (mirroring `paneCount` / `isVisualMode` at `:707-733`):

```swift
var historyWin: Int64            // paneContainer?.activeWin ?? 0, falling back to handle?.active_win() ?? 0
var historyCanBack: Bool         // guard historyWin != 0; handle?.history_can_back(historyWin) ?? false
var historyCanForward: Bool
func historyBack()               // _ = handle?.history_back(historyWin); refocusEditor()
func historyForward()
func historyJump(index: Int)     // guard index >= 0; handle?.history_jump(historyWin, UInt32(index))
func historyRows() -> [HistorySeam.Row]   // maps handle.history_entries(historyWin) through HistorySeam.rows
```

Prefer the container's own `activeWin` over `handle.active_win()`: the handle is shared across a project's tabpages, while `activePane` is per-VIEW and is what the user sees. The drawer's pane is in `panes` but never gets history in core, so `can_back` correctly returns `false` when it is focused.

**Toolbar** — `MainWindow.swift`:

- New `HistoryNavigating` protocol (`historyBack()`, `historyForward()`, `historyCanBack`, `historyCanForward`) declared in `MainWindow.swift`; `WolfWindowController` conforms; `MainWindow` holds `weak var historyNav: HistoryNavigating?`, assigned in `WolfWindowController.build()` right after the window is constructed. **[architect's call, ORCHESTRATOR-APPROVED]** — explicit delegation rather than a `target: nil` responder-chain hop, because `WolfWindowController` is not an `NSWindowController` and is not in the chain.
- `static let backItemIdentifier = NSToolbarItem.Identifier(HistorySeam.backIdentifier)`, `forwardItemIdentifier` likewise.
- `toolbarDefaultItemIdentifiers` and `toolbarAllowedItemIdentifiers` (`:189-195`) both return the four identifiers, derived from `HistorySeam.itemIdentifiers(...)` mapped back to `NSToolbarItem.Identifier`.
- `toolbar(_:itemForItemIdentifier:willBeInsertedIntoToolbar:)` (`:197`) gains two arms building an `NSToolbarItem` each: `image = NSImage(systemSymbolName:accessibilityDescription:)`, `label`/`paletteLabel`/`toolTip` = "Back"/"Forward", `isNavigational = true`, `autovalidates = false`, `target = self`, `action = #selector(historyBackToolbarAction(_:))` / `historyForwardToolbarAction(_:)`. Two `@objc` forwarders on `MainWindow` call `historyNav?.historyBack()` / `historyForward()`.
- `func refreshHistoryToolbar(canBack: Bool, canForward: Bool)` looks the two items up in `toolbar?.items` by identifier and sets `isEnabled`. `autovalidates = false` is what makes `isEnabled` stick.

**Enable/disable refresh** — `WolfWindowController.onRenderWake()` (`:351`), immediately after `paneContainer?.onRenderWake()` (so `activePane` is reconciled for this frame):

```swift
window?.refreshHistoryToolbar(canBack: historyCanBack, canForward: historyCanForward)
```

Two cheap lock-free snapshot reads per wake — no RPC, same cost class as the existing `handle?.cwd()` read on the next line.

## B9. `nav_history.rs` in detail

Pure, GUI-free, IO-free (path math only), sibling of `buffer_picker.rs` / `pane_status.rs`.

Constants: `pub const MAX_ENTRIES: usize = 100;` `pub const MAX_MENU_ROWS: usize = 10;`

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NavTarget {
    Edit { path: PathBuf },
    Preview { source: PathBuf },
    Terminal { bufnr: i64 },
    Untitled { bufnr: i64 },
}
impl NavTarget { pub fn path(&self) -> Option<&Path>; }   // Edit->path, Preview->source, else None

pub struct NavMove { pub from: usize, pub to: usize, pub target: NavTarget }
pub struct MenuRow { pub display: String, pub index: usize, pub is_current: bool }

#[derive(Clone, Debug, Default)]
pub struct PaneHistory { entries: Vec<NavTarget>, cursor: usize, expected: Option<NavTarget> }

#[derive(Clone, Debug, Default)]
pub struct NavHistory { panes: HashMap<i64, PaneHistory> }
```

`PaneHistory` — all `pub` (the suite is an external integration test):

- `push(&mut self, target)` — if non-empty `entries.truncate(cursor + 1)`; push; `cursor = len - 1`; then cap: `let overflow = len.saturating_sub(MAX_ENTRIES); if overflow > 0 { entries.drain(..overflow); cursor -= overflow; }`; `expected = None`.
- `observe(&mut self, target)` — the four-branch rule from B6.
- `can_back` (`cursor > 0`) / `can_forward` (`cursor + 1 < entries.len()`).
- `back` / `forward` / `jump(index)` → `Option<NavMove>` — bounds-check, record `from = cursor`, move `cursor`, `expected = Some(target.clone())`, return the move. **`jump` to the current index returns `None`** (a true no-op, per the locked checkmark rule).
- `revert(&mut self, from: usize, to: usize)` — only when `cursor == from`: `cursor = to; expected = None`.
- `entries()`, `cursor()`, `current()`, `expected()` accessors.
- `menu_rows(&self, limit: usize, home: Option<&Path>) -> Vec<MenuRow>`.

`NavHistory`: `observe(win, target)`, `prune(win)`, `prune_many(&[i64])`, `can_back(win)`, `can_forward(win)`, `back(win)`, `forward(win)`, `jump(win, index)`, `revert(win, from, to)`, `menu_rows(win, limit, home)`, `pane(win)`.

Free pure functions:

- `pub fn derive_target(buf: Option<&WinBuffer>, preview_source: Option<&Path>) -> Option<NavTarget>` — precedence: preview source present → `Preview`; else no `WinBuffer` → `None`; else `is_drawer` → `None`; else `is_terminal` → `Terminal { bufnr }`; else `path: Some` → `Edit { path }`; else → `Untitled { bufnr }`.
- `pub fn disambiguate(paths: &[PathBuf], home: Option<&Path>) -> HashMap<PathBuf, String>` — steps 2/3, directly unit-testable.

**The label pipeline (`menu_rows`), in this exact order:**

1. **Row selection.** Walk `entries` newest-first (`len-1 → 0`), keeping the FIRST occurrence of each distinct `NavTarget`, recording the index it was drawn from; stop at `limit` DISTINCT rows. Then, if any kept row's target equals `entries[cursor]`, **override that row's `index` to `cursor`** and set `is_current = true` (so clicking the checkmark is a true no-op). If the cursor's target did not survive the cap, no row is checkmarked — document this.
2. **Distinct paths.** Collect `row.target.path()` over kept rows into a de-duplicated `Vec<PathBuf>`. `Preview(X)` and `Edit(X)` contribute the SAME path once.
3. **Minimal-growth prefix labels** (`disambiguate`). Group distinct paths by `file_name()` (lossy). A singleton renders bare. For a colliding group, for `k = 2, 3, …`: the candidate is the last `k` components joined with `/`; a path with fewer than `k` components contributes its FULL absolute form. Stop at the first `k` where every candidate in the group is distinct; the loop terminates at `k == max_components_in_group` (paths are distinct by construction). Finally, any label that is a full absolute path gets home abbreviated to `~`.
4. **Decorate.** `Edit` → `label(path)`. `Preview` → `"Preview: " + label(source)`. Pathless rows are EXCLUDED from steps 2-3 and numbered by first appearance in the newest-first ROW order: first `Terminal` → `"Terminal"`, second → `"Terminal 2"`, …; likewise `"Untitled"`, `"Untitled 2"`, ….

## B10. Files to create

| path                                                             | kind                |
| ---------------------------------------------------------------- | ------------------- |
| `crates/wolf-core/src/nav_history.rs`                            | source              |
| `crates/wolf-core/tests/nav_history.rs`                          | headless test       |
| `crates/wolf-core/tests/nav_history_live.rs`                     | real-nvim test      |
| `crates/wolf-ffi/tests/history.rs`                               | real-nvim test      |
| `crates/wolf-macos/Sources/WolfKeyMap/HistorySeam.swift`         | source              |
| `crates/wolf-macos/Tests/WolfKeyMapTests/HistorySeamTests.swift` | headless Swift test |

## B11. Files to modify

**Rust core**

- `crates/wolf-core/src/lib.rs` — `pub mod nav_history;` (alphabetically after `mru`); module doc bullet; `pub use nav_history::{derive_target, MenuRow, NavHistory, NavMove, NavTarget, PaneHistory, MAX_ENTRIES, MAX_MENU_ROWS};`; add `WinBuffer` to the `pub use render::{…}` list.
- `crates/wolf-core/src/render.rs` — `WinBuffer` struct; `Snapshot.win_buffers` field after `buffers`.
- `crates/wolf-core/src/redraw.rs` — `NavHistoryCell` type + doc in the `IntentQueue`/`TrackedCwd` house style; `RedrawHandler` field + 7th `new` param; update the `BUFFERS_EVENT` const doc (`:71-77`) for the 5-element tuple and second map; `apply_buffers` builds both maps + calls the fold; `apply_previews` calls the fold; new `fold_nav_targets`; `apply_event`/`win_close` collector; `apply_redraw` prune.
- `crates/wolf-core/src/nvim.rs` — `history_open`, `history_set_buf`.
- `crates/wolf-core/src/api.rs` — `nav_history` field; create+clone in `WolfCore::start`; six public methods after `preview_file` (`:675`); private `apply_nav`.

`fold_nav_targets` body: collect the union of `snap.win_buffers.keys()` and `snap.previews.keys()` into a `Vec<i64>`; take `self.nav_history.lock()` once; for each win, `derive_target(snap.win_buffers.get(&win), snap.previews.get(&win).map(|p| p.source_path.as_path()))` and, when `Some`, `hist.observe(win, target)`. **A window in NEITHER map is simply not in the union — no entry, no push, no prune, the locked rule realized structurally.**

`api.rs` lock discipline to state in doc comments: the nav mutex is NEVER held across `block_on_rpc` and NEVER held while the snapshot lock is held from this side. The fold takes them in the opposite order but has no await/RPC between, so there is no lock-order cycle — write it down.

**nvim runtime** — `runtime/nvim/init.lua`: 5-element tuple in `wolf_setup_buffer_tracking` (`:327`) + extend the block comment; the two new `_G.wolf_history_*` helpers.

**FFI** — `crates/wolf-ffi/src/lib.rs` (bridge decls + impls); regenerate and commit `generated/wolf-ffi.swift` + `generated/wolf-ffi.h`.

**Swift** — `AppMenu.swift`, `AppDelegate.swift`, `MainWindow.swift`, `WolfWindowController.swift`, `PaneContainerView.swift` (per B8).

**BUILD**

- `crates/wolf-core/BUILD.bazel` — two new `rust_test` targets.
- `crates/wolf-ffi/BUILD.bazel` — one new `rust_test` target.
- `crates/wolf-macos/BUILD.bazel` — `Sources/WolfKeyMap/HistorySeam.swift` into `wolf_key_map.srcs` (alphabetically, between `GlyphStyle.swift` and `KeyMap.swift`); `Tests/WolfKeyMapTests/HistorySeamTests.swift` into `WolfKeyMapTests.srcs` (between `GlyphStyleTests.swift` and `KeyMapTests.swift`). The `swift_format_srcs` filegroup is a glob needing no change — and per CLAUDE.md it does NOT make the test run.

## B12. Implementation order

Each phase compiles on its own. Gates: **4 before 5; 5 before 6; 6+7 before 8.**

1. **Data model** — `nav_history.rs` + `lib.rs`. Write `tests/nav_history.rs` + BUILD target here and get it green FIRST; the feedback-loop-suppression case is the first test.
2. **Snapshot shape** — `render.rs` `WinBuffer` + `win_buffers` + re-export. `Default`-derived, nothing breaks.
3. **nvim/Lua emit** — 5-element tuple. Harmless to the current parser (extra elements ignored).
4. **Core fold** — `redraw.rs` parser, `NavHistoryCell`, handler field + `new` param, `fold_nav_targets`, `win_close` collector + prune. `api.rs` must change in the SAME commit (the `RedrawHandler::new` arity change is a hard break).
5. **Lua nav helpers + core nav API** — `wolf_history_open`/`wolf_history_set_buf`; `nvim.rs` futures; `api.rs` six methods + `apply_nav`. Then `tests/nav_history_live.rs` + BUILD target.
6. **FFI** — bridge decls + impls, regenerate + commit generated bindings. Then `wolf-ffi/tests/history.rs` + BUILD target.
7. **Swift seam** — `HistorySeam.swift` + BUILD srcs + `HistorySeamTests.swift` + test BUILD srcs. Independent of phase 6 (Foundation-only).
8. **Swift AppKit** — `PaneContainerView.activeWin` → `WolfWindowController` accessors/actions → `MainWindow` toolbar → `AppMenu` History menu → `AppDelegate` delegate/actions/validation.
9. **Docs** — all READMEs + `architecture.md`, then `prettier --write`.

## B13. Test plan

### `crates/wolf-core/tests/nav_history.rs` — HEADLESS

BUILD target `nav_history`, mirroring `//crates/wolf-core:palette_rank` exactly (no `data`, no `env`, no `tags`, no `tests/common/mod.rs`; `srcs = ["tests/nav_history.rs"]`, same `crate_root` shape, `size = "small"`, `edition = "2021"`, `deps = [":wolf_core"] + all_crate_deps(normal = True, normal_dev = True)`).

- **Feedback-loop suppression (WRITE FIRST).** `push(A); push(B)`; `back()` returns `NavMove{from:1,to:0,target:A}` and sets `expected = A`; `observe(A)` does NOT push (entries still `[A, B]`, cursor 0, `expected` cleared). Then `observe(C)` DOES push → `[A, C]`, cursor 1.
- Truncate-on-new-navigation: `A,B,C` cursor 0 → `observe(D)` → `[A, D]` cursor 1.
- Keep-every-visit: `A,B,A,B` → four entries, no dedup in the STACK.
- Idempotent re-emit: `observe(X)` twice pushes once.
- Empty-stack seed: first `observe` seeds without push-truncate.
- Cap 100 + cursor math: push 105 → `len == 100`, cursor 99, oldest 5 dropped; then with cursor mid-stack, push over the cap and assert `cursor` shifted by exactly the drained count and never underflows.
- `can_back`/`can_forward` at both ends and mid-stack.
- `jump`: valid index moves and sets `expected`; `jump(cursor)` → `None`; out-of-range → `None`, pane untouched.
- `revert`: restores when `cursor == from`; NO-OP when a racing `observe` already moved the cursor.
- Stale-`expected` self-heal: `back()` sets `expected = A`, then `observe(B)` (failed-nav) pushes B, truncates, clears; a subsequent `observe(A)` then pushes normally — proving no permanent wedge.
- `derive_target` matrix: preview wins over buffer; `is_drawer` → `None`; `is_terminal && !is_drawer` → `Terminal`; named → `Edit`; unnamed → `Untitled`; both absent → `None`.
- `NavTarget` distinctness: `Edit(B) != Preview(B)`.
- `prune`/`prune_many` drop exactly that pane; others untouched; unknown win is a no-op.
- **Menu rows.** Distinct-row cap of 10 over a repeat-heavy stack; newest-first order; exactly one `is_current`; the checkmarked row's `index == cursor` even when a NEWER instance of the same target exists; no checkmark when the cursor's target falls outside the cap; row indices are real stack indices usable by `jump`.
- **Label disambiguation.** The worked example; the two-parent-segment case (`a/b/README.md` vs `c/b/README.md` → three components each); `Preview(X)` and `Edit(X)` contribute ONE path (same label body, one `"Preview: "`-prefixed); `Terminal`/`Untitled` excluded from collisions and numbered by first appearance; the `~` home-abbreviation fallback; a single-component root-level path.

### `crates/wolf-core/tests/nav_history_live.rs` — REAL NVIM

BUILD target `nav_history_live`, mirroring `//crates/wolf-core:pane_buffers` exactly (`srcs = ["tests/common/mod.rs", "tests/nav_history_live.rs"]`, `crate_root = "tests/nav_history_live.rs"`, `tags = ["no-sandbox"]`, `data = ["//bazel/nvim:nvim_tree", "//runtime/nvim:init.lua"]`, the three `WOLF_*` env entries, `deps = [":wolf_core"] + all_crate_deps(normal = True, normal_dev = True)`).

- `:e a.md` then `:e b.md` builds `Edit(a) → Edit(b)`; `can_back` true, `can_forward` false.
- **`history_back` actually moves the pane** (assert via `pane_status(grid).abs_path()` / `Snapshot.win_buffers`), and the resulting `wolf_buffers` emit is ABSORBED — stack still 2 deep with cursor 0, NOT 3 deep. **Load-bearing feedback-loop assertion.**
- `history_forward` returns to `b`; `can_forward` then false.
- Truncate: back to `a`, `:e c.md` → `[a, c]`.
- Preview transition: `:e a.md` then `open_preview("active")` → `Preview(a)` as a DISTINCT entry after `Edit(a)`; assert NO spurious entry for the transient "in neither map" frame (stack length exactly 2).
- Preview link-follow: `preview_file(win, b.md)` on a preview pane → `Preview(b)` appended; `history_back` restores `Preview(a)` (asserted via `Snapshot.previews[win].source_path`).
- `:terminal` in a normal split → `Terminal { bufnr }` in THAT pane's history; the DRAWER (`toggle_terminal`) produces NO history (`history_entries(drawer_win).is_empty()`, `can_back == false`).
- Unnamed buffer: a raw `:enew` yields `Untitled { bufnr }` labeled `"Untitled"`. **Note in the test header** that Cmd-N's `wolf_new_untitled` NAMES the buffer `<cwd>/Untitled.md`, so it is an `Edit` entry, not an `Untitled` one — cover both.
- `win_close` pruning: `:vsplit`, navigate in the new pane, `<C-w>c`, assert that window's `history_entries` is empty.
- Failed restore: record a `Terminal`/`Untitled` bufnr, `:bwipeout` it, `history_back` returns `false` and the cursor is REVERTED (`can_back` still true).
- Two panes have INDEPENDENT stacks.
- `Snapshot.win_buffers` correctness: bufnr matches nvim; unnamed windows PRESENT with `path: None`; preview panes ABSENT.

### `crates/wolf-core/tests/pane_buffers.rs` — EXTEND (already wired, no BUILD change)

Assert `Snapshot.buffers` is **behaviorally byte-identical** after the tuple extension: unnamed windows still ABSENT, paths still canonicalized, preview panes still skipped, wholesale replacement still drops closed windows; and `pane_status(grid)` returns exactly what it did for named / unnamed / preview / no-window grids.

### `crates/wolf-ffi/tests/history.rs` — REAL NVIM

BUILD target `history`, mirroring `//crates/wolf-ffi:preview` exactly (`srcs = ["tests/common/mod.rs", "tests/history.rs"]`, `crate_root`, `tags = ["no-sandbox"]`, `data`, the two `WOLF_TEST_*` env entries — wolf-ffi targets omit `WOLF_NVIM_BIN` — `deps = [":wolf_ffi"] + all_crate_deps(normal_dev = True)`).

`history_can_back`/`can_forward` follow the core; `history_entries` returns opaque rows whose `display()`/`index()`/`is_current()` match; `history_jump` moves the pane and returns `true`; out-of-range returns `false`; `history_back` returns `true` and moves; **a shut-down core yields `false`/`false`/`false`/empty `Vec`** for every method (the neutral-fallback contract every sibling asserts); a drawer window and an unknown `win` yield empty/false.

### `crates/wolf-macos/Tests/WolfKeyMapTests/HistorySeamTests.swift` — HEADLESS SWIFT

**Must be added to the hand-maintained `srcs` of `//crates/wolf-macos:WolfKeyMapTests`** — it is NOT a glob.

`rows` zips in order preserving newest-first; mismatched lengths truncate to the shortest; a negative index is dropped; several `isCurrent` flags keep only the first; empty input → `[]`; `itemIdentifiers` returns exactly `[toggleSidebar, trackingSeparator, back, forward]` in that order; the identifier and SF-Symbol constants are the expected strings.

### Verification

`bazelisk test //crates/wolf-core/... //crates/wolf-ffi/... //crates/wolf-macos:WolfKeyMapTests` and **confirm each NEW target actually appears in the run** — cargo would mask a missing Rust target and there is no cargo fallback at all for the Swift file. Also `bazelisk test //tools/lint:rustfmt_core //tools/lint:rustfmt_ffi` (hard requirements, not advisory).

## B14. Dependencies

**None.** `parking_lot`, `std::collections::HashMap`, `std::path` are already core deps. No new Swift packages. The `wolf-core` GUI-toolkit denylist is unaffected.

## B15. Lint considerations

- `nav_history.rs`: every public item needs a `///` doc; `#[must_use]` on `can_back`, `can_forward`, `entries`, `cursor`, `current`, `expected`, `menu_rows`, `derive_target`, `disambiguate`, `NavTarget::path`.
- `fold_nav_targets` — build the window union into a local `Vec` BEFORE taking the nav mutex; never iterate a `MutexGuard` in a `match`/`for` scrutinee (`clippy::significant_drop_in_scrutinee`). No `.await` inside, so `await_holding_lock` does not fire — add an explicit comment saying so, matching `:510-516`.
- `api.rs::apply_nav` — bind `self.nvim.borrow()` to a LOCAL, never letting a `RefCell` guard cross an `.await`/`block_on`, exactly as `preview_file` (`:667`) does (`clippy::await_holding_refcell_ref`).
- `FfiHistoryEntry::from_menu_row` — `u32::try_from(row.index).unwrap_or(0)`, not `as` (`clippy::cast_possible_truncation`).
- `#[allow(clippy::too_many_arguments)]` NOT needed on `RedrawHandler::new` at 7 params (threshold is 8) — re-check if anything else is added.
- swift-format: run `//tools/lint` fix before committing.
- `prettier --write` on every touched `.md`; no hard line breaks inside a paragraph.

## B16. Documentation

- `crates/wolf-core/src/README.md` — new `## nav_history.rs` section; update the dependency-order line to place `nav_history` after `render`, before `redraw`; update the `lib.rs` re-export list; add `win_buffers` + `WinBuffer` to the `Snapshot` field list; update `redraw.rs` with the fold, the extended `wolf_buffers` payload, the `win_close` prune and its `win_hide` limit; update `api.rs` with the six methods.
- `crates/wolf-ffi/src/README.md` + `crates/wolf-ffi/README.md` — the six bridge methods + `FfiHistoryEntry`.
- `crates/wolf-macos/Sources/README.md` + `crates/wolf-macos/README.md` — History menu (position, items, key equivalents, `NSMenuDelegate` rebuild); toolbar list `traffic lights → toggle sidebar → sidebar separator → back → forward → title`; `HistorySeam`.
- `runtime/nvim/README.md` — the 5-element `wolf_buffers` tuple and the two new helpers (new section beside `wolf_switch_buffer` / `wolf_preview_file`).
- `crates/wolf-core/tests/README.md` — new headless + real-nvim sections; extend the per-pane buffer-tracking section for `win_buffers`.
- `crates/wolf-ffi/tests/README.md` — the new `history` target.
- `architecture.md` — a `wolf-core` responsibility bullet; the `wolf-ffi` method list; the `wolf-macos` menu list (add History) and toolbar description.
- `README.md` — one sentence only if the top-level feature list warrants it; keep minimal.

---

---

# PART C — AMENDMENTS DISCOVERED DURING TEST-WRITING

## C1. Drawer emit suppression — REQUIRED, supersedes part of B5

The test-writer found, and the orchestrator verified in `runtime/nvim/init.lua`, that **B5's `is_drawer` mechanism does not work as specified.** `_G.wolf_toggle_terminal` (`init.lua:999`) has two defects on the OPEN path:

1. `vim.cmd("botright " .. height .. "split")` (`:1026`) fires `WinNew`/`WinEnter` **synchronously**. `wolf_setup_buffer_tracking` listens on exactly those events, so it emits with the NEW drawer window showing the **previous** buffer → an `Edit(prev)` push for the drawer window.
2. On the FRESH path, `vim.cmd("terminal")` (`:1032`) fires `BufEnter` **before** `vim.b.wolf_terminal = true` on the next line (`:1033`), so that emit sees `is_drawer == false, is_terminal == true` → a `Terminal{n}` push. No further `wolf_buffers` emit follows to correct it. (The REUSE path at `:1028` is already fine — `termbuf` carries the tag from its original creation.)

Tagging the window after the split does NOT fix this: the split's own emit has already fired. And because `observe` pushes on every _change_, an intermediate wrong push persists in the stack even if a later emit is correct. **The emits must be suppressed, not corrected afterwards.**

**Required fix (coder):**

- Add a module-local suppression flag consulted by `wolf_setup_buffer_tracking`'s `emit()` (`init.lua:329`): when set, `emit()` returns immediately without notifying.
- In `_G.wolf_toggle_terminal`, set the flag at the top of the OPEN path (step 3, before the `botright split`), and in a `pcall`-safe manner clear it after ALL tagging is done (after `vim.b[termwin_buf].wolf_full_width = true` at `:1036`), then call `emit()` exactly once so core sees the fully-tagged final state.
- The flag must be cleared even if an intermediate step errors — otherwise buffer tracking dies for the rest of the session. Use a `pcall` or an explicit reset before every `return`.
- Additionally set `vim.w.wolf_terminal = true` on the drawer window right after the split, and OR the window var into the tuple's `is_drawer` (`is_drawer = vim.b[buf].wolf_terminal == true or vim.w[win].wolf_terminal == true`). Belt-and-braces: it keeps the drawer identifiable even if the shared terminal buffer is ever surfaced elsewhere.
- The HIDE path (`nvim_win_close`, `:1019`) needs no suppression — the drawer window is gone and `win_close` prunes it; the surviving panes' buffers are unchanged so no push occurs.

The locked decision ("terminal drawer EXCLUDED, no history") is unchanged. The tests were written to the locked spec and will fail until this fix lands — that is correct and intended.

## C2. API shapes pinned by the test-writer

The tests compile against these; implement to them exactly.

1. `PaneHistory` accessors return **borrows**: `entries() -> &[NavTarget]`, `cursor() -> usize`, `current() -> Option<&NavTarget>`, `expected() -> Option<&NavTarget>`. A cloning accessor will not compile against the suite.
2. `NavHistory::pane(&self, win) -> Option<&PaneHistory>`.
3. `WolfCore` methods: `history_can_back(win) -> bool`, `history_can_forward(win) -> bool`, `history_back(win) -> bool`, `history_forward(win) -> bool`, `history_jump(win, index: usize) -> bool`, `history_entries(win) -> Vec<MenuRow>`. Note **`usize`** on the core side (so `core.history_jump(win, row.index)` type-checks) vs **`u32`** on the FFI side per B7.
4. `history_entries` already applies `MAX_MENU_ROWS` and the home abbreviation — callers read finished display strings.
5. `disambiguate` is imported as `wolf_core::nav_history::disambiguate`; the module path must stay public. Adding it to the crate-root re-export too is harmless.
6. `MenuRow` / `NavMove` are read field-wise only — no `Debug`/`PartialEq` derives required. `NavTarget` needs `Debug + PartialEq` (already in B9).
7. **"Fewer than `k` components ⇒ full absolute form" counts _normal_ path components** (root `/` not counted). Load-bearing for the `~` fallback test, which forces `k == 4` so `/Users/alice/README.md` (3 components) falls back to absolute and renders `~/README.md`.
8. `observe` branch 3 is unreachable single-threaded through the public API (branch 2 catches it first). **Implement it anyway** — it is the cross-thread safety net. The tests assert the observable absorb contract, not which branch fires.
9. `jump(cursor)` is a no-op: cursor and entries unchanged. The tests do not pin what happens to `expected`.
10. Swift `rows()`: "negative index dropped" and "only the first `isCurrent` kept" are asserted independently; whether a dropped row consumes the checkmark budget is deliberately unpinned.
11. **Live tests never assert absolute stack depth** — the startup buffer is a named `<cwd>/Untitled.md`, so every pane's history begins with an extra `Edit`. They assert newest-first row prefixes, relative growth, and `can_forward`.

## C3. Outstanding BUILD wiring owed by the coder

The test-writer added every test target but deliberately did NOT add `Sources/WolfKeyMap/HistorySeam.swift` to `wolf_key_map.srcs` — a `srcs` entry for a nonexistent file breaks the build immediately. **The coder must add it** when creating the file, alphabetically between `GlyphStyle.swift` and `KeyMap.swift`.

## C4. Non-issue

A SourceKit diagnostic reports `No such module 'XCTest'` in `HistorySeamTests.swift`. That is the editor resolving the file outside its Bazel context; the `swift_test` target supplies XCTest. No action.

---

---

# PART D — AMENDMENTS DISCOVERED DURING IMPLEMENTATION

Both deviations below were reviewed and **APPROVED by the orchestrator**. They are now part of the spec.

## D1. `wolf_buffers` emits are coalesced per tick, and `TermOpen` joins the augroup

Part B/C1 assumed a synchronous emit per autocmd. Measured against the pinned nvim 0.12.3: during `:terminal`, `buftype` is **not** `"terminal"` at `BufEnter`/`BufWinEnter`/`BufFilePost` — only at `TermOpen`. Emitting on each would push `Untitled{n}` → `Edit("term://…")` → `Terminal{n}`: three history entries for one user action.

Implemented: `TermOpen` added to the augroup, and emits coalesced onto the next event-loop tick via `vim.schedule`. Coalescing reads the latest state when it runs, so no emit is lost, and it makes C1's drawer suppression robust rather than order-dependent. Lua-side only; `pane_buffers` and every other `wolf_buffers` consumer stay green.

## D2. `NavHistory::observe` drops a repeat of the last target already folded for that window

An **emit-level** filter sitting ABOVE `PaneHistory::observe`. The locked four-branch rule in `PaneHistory` is **unmodified**, and all 27 headless tests pass untouched.

Necessary because nvim fires the driving autocmds 2–4 times per navigation **across several ticks**, so per-tick Lua coalescing cannot merge them all. Without the filter, a late emit still reporting the pre-move target arrives after the cursor has moved, hits branch 4, and pushes — destroying the move:

```
history_back win=1000 entries=[Untitled.md, a.md, b.md] cursor=2
observe b.md  cursor=1   <- late repeat of the state we LEFT -> branch 4 PUSH (truncates a.md's future)
observe b.md  cursor=2   <- absorbed
observe a.md  cursor=2   <- PUSH  => [U,a,b,a]
```

`expected` structurally cannot cover this: it names the target being moved **to**, and the locked rule deliberately requires a non-matching change to push (that is what the self-heal relies on). "Did the pane's derived target actually change since the last fold" separates the two cases exactly.

This is a faithful reading of the locked push trigger — which is ANY **change** to a pane's current file. A repeated emit of the same file is not a change; Part B's `fold_nav_targets` was underspecified in folding every emit unconditionally.

Fixed `back_moves_the_pane_and_the_resulting_emit_is_absorbed`, `two_panes_have_independent_stacks`, and `preview_transition_and_link_follow_are_tracked` in one step.

## D3. Test-writer round 2 — orchestrator decisions

| coder recommendation                                                                                            | decision                                                                                                                                     |
| --------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| Fix `a_failed_back_returns_false_and_reverts_the_cursor` (broken premise: `:e` reuses the empty unnamed buffer) | **ACCEPT** — route to test-writer                                                                                                            |
| Add a headless test for the D2 repeat filter                                                                    | **ACCEPT** — new behavior, no direct coverage                                                                                                |
| Add a live assertion that a split `:terminal` grows the stack by exactly ONE                                    | **ACCEPT** — guards the D1 fix against regression                                                                                            |
| Add AppKit-level coverage for phase 8 (menu rebuild, toolbar enablement, `validateMenuItem`)                    | **DECLINE** — matches the repo's existing posture that only pure Foundation seams are Swift-tested; `HistorySeam` already carries that layer |

**Not routed:** `rustfmt.fix` reformatted the two untracked test files (line-wrapping only, assertions verified unchanged). `rustfmt_core` does not check test files, so this is cosmetic — not worth the churn.

## D4. Non-issues

Editor diagnostics reporting undefined `vim` in `init.lua` and `No such module 'WolfFFI'` / `'WolfKeyMap'` / `'XCTest'` in Swift files are SourceKit/lua-ls resolving outside the Bazel context. The Bazel targets supply these. No action.

---

## Progress log

- **Architecture** — COMPLETE. All seven architect's calls reviewed and approved by the orchestrator (marked ORCHESTRATOR-APPROVED inline).
- **Tests written** — COMPLETE (round 1). 4 files created, 1 extended, 4 BUILD targets added. See Part C.
- **Implementation** — COMPLETE. 8 phases. See Part D for two approved deviations.
- **Test status** — `nav_history` 27/27 PASS, `pane_buffers` PASS, `//crates/wolf-ffi:history` 2/2 PASS, `WolfKeyMapTests` 283 PASS (9 `HistorySeam`), `rustfmt_core`/`rustfmt_ffi`/`clippy`/`swift_format` PASS. `nav_history_live` 7/8 — one test with a broken premise, going back to the test-writer (D3).
- **Tests round 2** — COMPLETE. All three D3-accepted items landed; no implementation changes were required.
- **Test status (final)** — ALL GREEN. `nav_history` 31/31, `nav_history_live` 8/8, `pane_buffers` PASS, `//crates/wolf-ffi:history` 2/2, `//crates/wolf-ffi:pane_status` 1/1, `WolfKeyMapTests` 283 (10 `HistorySeam`), `rustfmt_core`/`rustfmt_ffi`/`clippy`/`swift_format` PASS, full `//...` sweep 86/86. Flake check `--runs_per_test=5` on the three real-nvim targets: 15/15 green.
- **Review** — COMPLETE. Approved with minor notes; all nine plan-conformance checks hold. See Part E for triage.
- **Fixes round 3** — COMPLETE. All six E1 fixes landed; 12/12 live, 86/86 sweep, 10/10 flake runs. No BUILD size bump needed (6.8s against the 60s `small` budget).
- **Tests round 4** — COMPLETE. Both new tests passed on the first run; no implementation change needed. `//crates/wolf-core:nav_history_live` now 14/14 in 8.2s, flake check 5/5 (dev 0.0s), no size bump needed.
- **Documentation** — COMPLETE (prose). 11 files updated. Two follow-ups in Part H.
- **Final cleanup** — COMPLETE.

## FEATURE COMPLETE

All phases done. Final state: 86/86 `//...`, 4/4 manual-tagged lint targets, 31 headless + 14 live + 2 FFI + 10 Swift-seam tests for this feature.

### H2 answered — the re-emit split, measured

Probe B found that during a background bind nvim makes the **target window and buffer temporarily current** inside the autocmd callbacks, then restores focus without firing again:

```
during bg bind: BufEnter(ev.buf=3 current_buf=3 current_win=1000 ft="")
                BufWinEnter(ev.buf=3 current_buf=3 current_win=1000 ft="")
after  bg bind: current_buf=2 current_win=1001 ft="markdown"
```

That splits the two C-3 re-emits:

- **`_G.wolf_emit_current_file()` is LOAD-BEARING.** `WolfCurrentFile`'s `emit()` reads `nvim_get_current_buf()` (`init.lua:280`), so the autocmd-driven emit describes the BACKGROUND pane — for a preview scratch that is `wolf_current_file("", "", true, …)`, leaving `is_markdown_active` false and `is_preview_active` true — and nothing corrects it once focus snaps back.
- **`_G.wolf_emit_previews()` is HARDENING.** `emit_previews` is a full `nvim_list_wins()` sweep that never reads the current window, is already on `BufWinEnter`, and core wholesale-replaces the map — so a stale entry drops on its own.

Also measured: `:help BufWinEnter`'s already-displayed carve-out is **per-window, not per-buffer** — it suppresses events only on a no-op rebind into a window already showing that exact buffer. And a `b:var` write fires NO autocmd at all, which is why `wolf_toggle_full_width`'s re-emit is load-bearing for a different reason (this vindicates `init.lua:477`, checked and left alone).

### Known minor incompleteness — accepted

`runtime/nvim/README.md:198` describes `wolf_history_set_buf`'s re-emits as "both guarded, both idempotent". Accurate and self-consistent, but it does not record the load-bearing/hardening split above. The split IS recorded in the `init.lua` comment a maintainer editing that function would see. Incompleteness, not divergence — fill it in if desired.

### Unrelated flake on the record

`//crates/wolf-core:swap_recovery` failed once during a full sweep (process died before any per-test line printed — an nvim-spawn hiccup under parallelism), then passed on re-run, on `--runs_per_test=5 --nocache_test_results` (5/5), and on the second full sweep. Not causal to this feature (the only change in that round was Lua comments). Pre-existing, same class as the `input_keys` flake seen in round 1.

---

# PART H — FINAL CLEANUP

## H1. Four remaining stale "fires no window event" comments

Part G1 settled empirically that a background `nvim_win_set_buf` DOES fire `[BufEnter, BufWinEnter]`. The technical writer corrected the one instance in `wolf_open_preview`, but `grep` finds four more, **including one in a helper written this session**:

| line            | location                                                |
| --------------- | ------------------------------------------------------- |
| `init.lua:1475` | **`wolf_history_set_buf` — ours, written this session** |
| `init.lua:1928` | preview re-emit rationale                               |
| `init.lua:1983` | active-buffer re-assert rationale                       |
| `init.lua:2034` | `wolf_setup_preview`'s emit-closure exposure            |

`init.lua:477` carries a related claim about a full-width flag flip — verify whether it is the same false assertion or a genuinely different case.

## H2. Are the C-3 re-emits load-bearing or just hardening?

The technical writer could not verify the justification and documented them conservatively as "retained, idempotent, order-independent" — the right call, and the docs are accurate either way. But the real reason should be recorded.

`BufWinEnter` is in BOTH the `wolf_buffers` augroup (`init.lua:443`) and the `WolfPreview` window-scan augroup (`init.lua:2023`), and the preview scan is a full `nvim_list_wins()` sweep whose payload core wholesale-replaces — so it would drop a stale `previews` entry on its own. That makes the re-emits look like hardening, not load-bearing.

**The likely exception:** `:help BufWinEnter` does not fire when the buffer is ALREADY displayed. The preview reuse path may hit exactly that. Determine it empirically, then make every comment state the measured truth.

## H3. `prettier --write` still owed

The technical-writer session had no Bash tool. `.prettierrc` is `{"proseWrap": "never"}` and every paragraph was written as a single unbroken line, so the run should be idempotent — but CLAUDE.md requires it and it has not been run.

## H4. Resolved — no action

- **F3 is DONE.** The orchestrator verified `nav_history_live.rs` directly: it carries `## Surface under test`, `## Terminal DRAWER exclusion, as shipped`, and `## What this does and does not discriminate`. The technical writer read a stale version.
- **`crates/wolf-core/README.md`** was correctly added despite being absent from B16 — `src/README.md` defers to it for the `WolfCore` surface, so it would have been missing six methods.
- **Test-count drift** in `crates/wolf-core/tests/README.md` (four → six `pane_buffers` tests) corrected.

## Final test status — ALL GREEN

| target                                                  | result                        |
| ------------------------------------------------------- | ----------------------------- |
| `//crates/wolf-core:nav_history`                        | 31/31                         |
| `//crates/wolf-core:nav_history_live`                   | 14/14                         |
| `//crates/wolf-core:pane_buffers`                       | 6/6                           |
| `//crates/wolf-ffi:history`                             | 2/2                           |
| `//crates/wolf-ffi:pane_status`                         | 1/1                           |
| `//crates/wolf-macos:WolfKeyMapTests`                   | 283 assertions, 10 seam tests |
| `bazelisk test //...`                                   | 86/86                         |
| `rustfmt_core`, `rustfmt_ffi`, `clippy`, `swift_format` | 4/4                           |

Coverage now reaches all three `apply_nav` failure routes — the invalid-bufnr gate, the `filereadable` gate, and the post-guard throw — plus background-window navigation.

---

# PART G — DOCUMENTATION PHASE

## G1. Empirically settled: background `nvim_win_set_buf` DOES fire `BufWinEnter`

The coder probed the pinned nvim 0.12.3 directly rather than inferring from green tests:

```
focus_unchanged=true  events_after_background_set_buf=[BufEnter,BufWinEnter]
```

`BufWinEnter` is in the `wolf_buffers` autocmd list (`runtime/nvim/init.lua:443`), so `Snapshot.buffers` — and therefore `pane_status` — updates on its own after a background bind. **This resolves a contradiction that has been sitting in `init.lua`:** `wolf_setup_current_file_tracking`'s claim is CORRECT; **`wolf_open_preview`'s claim that a background bind fires no window event is STALE/WRONG** and must be corrected.

The anticipated C-3 gap therefore does not exist — `_G.wolf_emit_buffers()` was NOT added, and the `wolf_emit_previews` re-emit is still load-bearing for the `previews`-drop half (no autocmd clears a stale `previews` entry).

## G2. Facts the docs must capture

- **C-6, in spec:** Back to a file deleted from disk reports SUCCESS. `Edit` targets route through `bufadd` + `bufload`, which create an empty `[New]` buffer — matching plain `:e missing.md`. The revert path covers wiped bufnrs and dead preview sources, not deleted `Edit` paths. Worth a user-facing sentence.
- **The `win_hide` orphan limit** (`redraw.rs:1316-1323`, `nav_history.rs:70-74`): a `win_close` for a grid whose entry `win_hide` already removed leaves that pane's history orphaned. Memory only, bounded at 100 entries per orphaned pane per nvim-process lifetime. An "absent for N emits" reaper was deliberately NOT added — it would violate the locked no-prune rule.
- **The double-Back race** (B6): two Back presses faster than the first emit is folded can produce one spurious push. Matches the locked rule; sub-millisecond window; never a wedge.
- **History survives an nvim restart** by construction (the handler is cloned), but the new nvim reissues fresh window handles so stale panes are never queried.
- **D1/D2:** `wolf_buffers` emits are coalesced per tick via `vim.schedule` and `TermOpen` is in the augroup; `NavHistory::observe` drops a repeat of the last folded target for that window.
- **`TabTitleSeam` is gone** — the Rust prefix-form disambiguation in `nav_history.rs` is the ONLY implementation. There is no duplication to document.

## G3. Skipped deliberately

The C-3 test's doc comment hedges that the `pane_status` half might also arrive via ordinary autocmds — per G1 that is exactly what happens. The comment is conservative, not wrong. Not worth a round trip.

---

# PART F — C-1 CORRECTION AND REMAINING COVERAGE

## F1. C-1 was NOT a live bug — correction of the Part E triage

The coder disproved it empirically: reverted all three Lua fixes, ran the target, and got **12 passed / 0 failed**, including `back_to_a_symlinked_path_rebinds_the_same_buffer`. Two independent nvim 0.12.3 mechanisms already covered the scenario:

1. **nvim resolves DIRECTORY symlinks in buffer names itself.** `:e link/doc.md` (where `link -> real`) yields `nvim_buf_get_name(0) == "<cwd>/real/doc.md"` — already the canonical form core stores.
2. **Buffer lookup is by file id (device+inode), not name** (`buflist_new` → `buflist_findname_file_id`). With a _file_ symlink, `:e filelink.md` keeps the link name, yet a later `:e real/doc.md` reuses the SAME buffer.

So `bufadd(canonical_path)` returns the existing buffer whenever the file exists on disk, regardless of name divergence. The only genuine name-only gap is a **never-written** buffer — and there `canonicalize` fails, so `redraw.rs:640` stores the raw path and the names match anyway.

**The C-1 fix is RETAINED as defensive hardening** — it makes the rebind independent of which nvim mechanism happens to apply, and covers a buffer written under one name whose file was later replaced or moved. But it is hardening, not a bug fix, and `back_to_a_symlinked_path_rebinds_the_same_buffer` does **not** discriminate it (passes with and without).

**Decision: do NOT build an elaborate discriminating test for C-1.** Pinning it would require defeating nvim's own dir-symlink resolution _and_ file-id matching simultaneously — a contrived scenario with poor value. The existing test is retained as harmless general rebind coverage, but its doc comment must stop claiming to guard C-1.

## F2. Remaining coverage gaps — ACCEPTED

Both are code paths introduced this session by the E1 fixes, with no test:

- **C-2** — a bind that throws _after_ the validity checks returns `0` → `apply_nav` returns `false` → cursor reverted. Reachable by setting `'winfixbuf'` on the target window (nvim ≥ 0.10) or opening the cmdwin before `history_back`.
- **C-3** — the background-window re-emits. Every existing test acts on nvim's _current_ window, so the re-emits run but are never asserted. Needs a bind into a window that is NOT current, asserting `Snapshot.previews` drops the stale entry and `pane_status(grid)` reports the new file with no further input.

## F3. Comment accuracy — ACCEPTED

Per CLAUDE.md, documentation that differs from the implementation is a bug. Two stale comments in `crates/wolf-core/tests/nav_history_live.rs`:

- The `back_to_a_symlinked_path_rebinds_the_same_buffer` doc comment claims to guard C-1; per F1 it does not.
- The module header still carries a "NOTE to coder — the terminal DRAWER emit ordering" block describing pre-C1 behavior, obsolete since C1 landed.

---

# PART E — REVIEW FINDINGS AND ORCHESTRATOR TRIAGE

The reviewer verified all nine plan-conformance items hold: the union-based "in NEITHER map" rule with no leaking caller, `Snapshot.buffers` byte-identical with both consumers unchanged, `expected` never gating a push, D2's filter strictly above `PaneHistory::observe`, C1's `pcall`-restored flag checked at BOTH arm and delivery time, clean lock discipline with no cycle, correct checkmark/cap semantics, correct PREFIX disambiguation, correct toolbar order, and all four BUILD targets correctly mirrored.

## E1. Fixes required

**C-1 (medium) — `wolf_history_open` can create a DUPLICATE buffer under symlinks.** `runtime/nvim/init.lua:1488` calls `vim.fn.bufadd(path)` where `path` came from `std::fs::canonicalize` (`redraw.rs:640`). Neovim does NOT symlink-resolve buffer names, so B4's "bufadd returns the EXISTING buffer when the name matches" only holds when nvim's name is already fully resolved.

Failure: project reached via a symlink (`~/Projects` → `/Volumes/work/projects`). nvim's buffer is `/Users/me/Projects/foo.md`; the stack holds `/Volumes/work/projects/foo.md`. Back calls `bufadd` on the resolved path → a **second, distinct** buffer loaded fresh from disk. Unsaved edits in the original are invisible; `bufload` against a live swap can raise nvim's ATTENTION prompt.

Invisible to the live tests because `fresh_cwd()` makes nvim's own names already canonical. **Fix:** in `wolf_history_open`, first scan `nvim_list_bufs()` for a buffer whose `vim.fn.resolve(nvim_buf_get_name(b))` equals `path` and prefer it; fall back to `bufadd`.

**C-2 (low) — `wolf_history_set_buf` reports success when the bind silently failed.** `init.lua:1468-1469` calls `_G.wolf_switch_buffer`, whose body is entirely inside an error-swallowing `pcall` (`:1425-1437`), then unconditionally returns `bufnr`. A `nvim_win_set_buf` that throws after the validity checks (cmdwin, `'winfixbuf'`, modified-buffer abandon) yields success, so `apply_nav` returns `true` and the cursor is NOT reverted — the pane diverges from the history cursor until the next genuine navigation. **Fix:** propagate `wolf_switch_buffer`'s `pcall` result and return `0` on failure.

**C-3 (low) — the two new helpers do not re-emit, unlike their siblings.** `wolf_preview_file` (`:1914-1919`) and `wolf_edit_source` (`:1859-1864`) both explicitly call `_G.wolf_emit_previews()` + `_G.wolf_emit_current_file()` after a window-scoped bind, precisely because a background-window bind fires no window event. The new helpers do neither. When `win` is not nvim's current window, `Snapshot.buffers` / `win_buffers` / `previews` stay stale: wrong file in the pane statusbar, and a stale `previews` entry keeps `derive_target` returning `Preview` for that window (precedence). **Fix:** add the same two re-emits, mirroring the siblings.

**C-4 (low) — `PaneHistory::revert` does not bounds-check `to`.** `nav_history.rs:285-291` guards only on `cursor == from`. The reviewer could not construct a reachable out-of-range case and nothing panics (`entries.get`, bounded indexing), but the invariant is implicit. **Fix:** add the bounds check to make it explicit.

**C-5 (very low) — `UInt32(index)` can trap.** `WolfWindowController.swift:797` converts after only an `index >= 0` guard. Unreachable today (every tag originates from a `u32`). **Fix:** `UInt32(exactly:)` or a clamp.

**V-1 (minor) — `validateMenuItem` resolves the controller inconsistently.** `AppDelegate.swift:686,688` use `controllers.first { $0.owns(effectiveKeyWindow) }` with NO `?? controllers.first` fallback, while `menuNeedsUpdate` (`:622`) and all three actions (`:635`, `:641`, `:652`) DO fall back. With no Wolf window key, the menu renders rows and clicking one works, but Back/Forward and ⌘\[ / ⌘\] grey out. **Fix:** add the fallback for internal consistency.

## E2. Tests required

- **T-1 (the notable gap) — the B4 bufadd-rebind contract is untested**, despite B4 and `nvim.rs:540-542` both calling it LOAD-BEARING. `unnamed_buffers_are_untitled_entries_named_ones_are_edits` only asserts labels; it never modifies a buffer or navigates back to it. Add a live test: type into the startup `Untitled.md`, `:e a.md`, `history_back(win)`, assert the buffer is still MODIFIED (or `bufnr` unchanged). This also guards C-1 against regression.
- **T-2 — drawer toggle round-trip.** C1's claim that the HIDE path needs no suppression is unasserted. Capture a survivor pane's `history_entries` length before `toggle_terminal` (open), after, and after a second `toggle_terminal` (hide); assert unchanged throughout.
- **T-3 — `revert` is untested for `Preview` targets.** Only the wiped-`Terminal` route through `wolf_history_set_buf` is covered. B6 names a deleted/non-markdown `Preview` source as the other realistic failure, and it goes through a DIFFERENT helper (`wolf_preview_file`). Delete the source and assert `history_back == false` + cursor reverted.

## E3. Accepted without change

- **C-6 — Back to a file deleted from disk reports success.** `bufadd` + `bufload` create an empty `[New]` buffer, matching plain `:e missing.md`. B6 only claimed the revert path covers wiped bufnrs and dead preview sources. **In spec** — but the technical writer should note it in the user-facing docs.
- **T-4 — `NavHistory::prune` (single-window) has no production caller**; only `prune_many` is wired. Covered headlessly. Informational only.

## Verification command note

`//tools/lint:*` targets are `manual`-tagged and **do NOT appear in `bazelisk test //...`** (which resolves 86 targets, none from `tools/`). Any final verification must name them explicitly:

```
bazelisk test //... //tools/lint:rustfmt_core //tools/lint:rustfmt_ffi //tools/lint:clippy //tools/lint:swift_format
```

- **Implementation** — pending
- **Review** — pending
- **Documentation** — pending
