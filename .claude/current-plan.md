# Plan: Search UI in the macOS sidebar

## Status: COMPLETE ✓

Search UI shipped: bundled pinned rg 15.2.0 (`bazel/rg`, real SHAs) → `Contents/Resources/rg/rg` resolved via `WOLF_RG_BIN`; wolf-core `search.rs` (rg `--json` streaming session, versioned-snapshot + 4th wake channel, cancel-on-Drop) + pure `search_tree.rs` (case-insensitive file sort, line sort, `file_label`, ≤2-line balanced-context window); `OpenAt` (edit + set_cursor) + one-shot `:ignorecase` read; FFI `FfiSearch`/`FfiSearchFile`/`FfiSearchMatch` + bindings regenerated; macOS sidebar header (folder/magnifier toggles) switching FileTreeView↔SearchView, Enter-only search, 3 toggle buttons, NSAttributedString results (bold match + dimmed context, wrapped, ellipsis), click→open-at-line. Reviewer APPROVED (code correct across 13 areas); docs synced (6 READMEs, prettier-clean). **All tests GREEN under bazelisk (fresh/uncached):** core `wolf_core_lib_test` (11 inline parse tests), `search_projection`, `search`, `search_no_rg` (graceful degrade), ffi `search`, macOS `WolfKeyMapTests` (3 seam suites) + `bundle_layout_test`, `bazel/rg:rg_version_test`. rustfmt_core/ffi + clippy(-Dwarnings) + swift_format PASS. App builds & signs.

## Problem statement

Add a ripgrep-backed **content search UI** to the macOS AppKit sidebar of the Wolf app (a Neovim-based Markdown editor). Today the sidebar shows only a file tree. We add a second panel — a search UI — toggled by icons at the top of the sidebar.

### Requirements (from the user)

**Sidebar header / toggle**
- At the top of the sidebar, add two icons: a **folder** (show the file tree) and a **magnifying glass** (show the search UI).
- Toggling swaps the sidebar content between the existing file tree and the new search UI.

**Search UI**
- A text field at the top to enter the search text.
- Below it, three toggle buttons controlling behavior:
  - **case sensitive** — initial state keyed off the embedded Neovim's `:ignorecase` setting.
  - **use regex** — ON by default.
  - **search all files** — OFF by default (default: only search Markdown files).
- Search is triggered **only when the user presses Enter** — do NOT search as the user types.

**Results tree**
- A tree view showing results, streamed in as they arrive but kept sorted.
- **Files**: flat list, each displayed as `<base filename> - <relative path of its parent directory>`, e.g. for `foo/bar/baz.md` → `baz.md - foo/bar`.
- Each file entry is **expanded by default** but can be **collapsed** to hide its matching lines.
- **Matching lines** under each file:
  - No syntax highlighting, no Markdown directive formatting (`**bold**` shows literally).
  - Long lines **wrapped to no more than two lines** in the sidebar.
  - If the full line does not fit in two lines, show up to an approximately **equal amount of context on either side** of the match string.
  - The **matching portion** is shown **bold**; the non-matching portion is non-bold and **dimmed slightly**.
- Clicking a matching line → **open the file and jump the cursor to the matching line/column**.

**Sorting**
- Sort files by **relative path** of the file containing the match, **case-insensitively**.
- Matching lines within a file sorted by **line number**.

### Implementation constraints (from the user)
- Use `rg` (ripgrep) under the hood, via its **JSON output** streamed in.
- Ship a **bundled** ripgrep inside the app bundle; do NOT use the system rg if present.
- Pin ripgrep to **version 15.2.0**.
- Results stream in but the list stays sorted (see Sorting above).

### Product decisions (confirmed with the user)
1. **Result click** → open the matched file AND jump the cursor to the matching line/column.
2. **Case toggle** → read `:ignorecase` from Neovim to set the button's initial state, but toggling the button affects only this search; it does NOT write back to Neovim's option.
3. **File filter** → default restricts rg via `--glob` to `*.md`/`*.markdown`; "search all files" removes the glob. (Not `rg --type markdown`.)

---

## Prior exploration (already done — architect should build on this, not redo it)

Three exploration passes established the following. All paths relative to repo root.

### A. Sidebar / UI (where the search panel goes)
- The sidebar is a real `NSSplitViewItem(sidebarWithViewController:)`, NOT the `Drawer*` files (those are the bottom terminal strip — irrelevant).
- Container: `crates/wolf-macos/Sources/WolfMacOS/EditorSplitViewController.swift` — `init(treeView:paneContainer:)` builds the sidebar via `HostViewController(hostedView: treeView, pinTopToSafeArea: false)` (line ~35), wrapped as `NSSplitViewItem(sidebarWithViewController:)`. In sidebar mode `HostViewController` adopts the hosted view directly as `view`.
- The `FileTreeView` is created in `WolfWindowController.swift` (`let tree = FileTreeView(...)`, ~line 225) and threaded through `MainWindow` → `EditorSplitViewController`.
- `FileTreeView.swift`: an `NSOutlineView` in an `NSScrollView`, single-column, headerless, `.sourceList` style. Row model `final class TreeItem` cached by `relPath` for stable NSOutlineView identity. Cells are `NSTableCellView` + one `NSTextField`; **styling today is plain font-swap only — NO attributed strings / per-range bold or dim anywhere**. Search results (bold match + dimmed context) will need `NSAttributedString` cells (`.secondaryLabelColor`/`.tertiaryLabelColor` for dim). No existing helper to reuse.
- **No sidebar header/toolbar exists today** — the scroll view is full-bleed. The header strip with the two toggle icons is new.
- SF Symbols are the icon convention: `NSImage(systemSymbolName:accessibilityDescription:)` + `NSImage.SymbolConfiguration` (used already: `"magnifyingglass"` in PickerView/BufferPickerView/CommandPaletteView). Natural pair: `"folder"` + `"magnifyingglass"`. No asset catalog.
- Whole-sidebar collapse is `sidebarItem.isCollapsed` (KVO-observed in WolfWindowController ~lines 331-335 → `FileTreeView.setSidebarVisible`). Content-panel switching (tree vs search) does NOT exist yet — new.
- Approach: introduce a `SidebarContainerView` (or container VC) hosting a top header strip (two SF-Symbol `NSButton`s) + a content area that switches (via `isHidden` or `NSTabView`) between `FileTreeView` and a new `SearchView`.

### B. FFI surface & core streaming (how search data flows Rust→Swift)
- Bridge crate: `crates/wolf-ffi/src/lib.rs` (single `#[swift_bridge::bridge] mod ffi` + glue). Generated bindings committed under `crates/wolf-ffi/generated/`; regenerate via `bazel run //crates/wolf-ffi:gen_bindings`.
- Streaming pattern to MIRROR: `FfiFileSearch`. There are three parallel **wake channels** — a token-carrying `extern "C" fn(uint64_t)` in a global `AtomicPtr`, declared in `crates/wolf-ffi/generated/wolf_ffi_wake.h`: `wolf_ffi_set_wake` (render), `wolf_ffi_set_filesearch_wake` (picker, lib.rs:1167), `wolf_ffi_set_tree_wake` (tree, lib.rs:1209). Swift registers one process-global thunk per channel (`WakeBridge.swift`, `PickerWakeBridge.swift`, `TreeWakeBridge.swift`) that hops to main and routes by token via `WindowRegistry`.
- Data is a **versioned snapshot behind a lock**, pulled on each wake — NOT a callback-per-item. `crates/wolf-core/src/files.rs`: `FileSearch` holds `Arc<RwLock<FileSearchState { entries, complete, truncated, version, seen }>>`. Background walk `spawn_traversal` (files.rs:234) on `spawn_blocking` appends `TRAVERSAL_BATCH=256` chunks under a briefly-held write lock (`append_batch`, files.rs:342), bumps `version`, drops lock, fires `wake()`. `Drop` cancels via a shared `AtomicBool`.
- Swift pulls via opaque accessors: `ranked(query,max)`, `version()` (refresh gate), `children_of(dir)`. `version()` gates reloads (`TreeRefreshGate`, `PickerRefreshCoalescer`).
- **Design implication:** new opaque `FfiSearch` handle wrapping a `SearchSession { Arc<RwLock<SearchState { matches, version, complete }>> }`; a **fourth** wake channel `wolf_ffi_set_search_wake` in `wolf_ffi_wake.h` + `registered_search_wake(token)`; a new `SearchWakeBridge.swift`; pull-based accessors (`results(max) -> Vec<FfiSearchMatch>`, `version()`). The rg subprocess streams JSON lines into the locked `SearchState` in batches exactly like `spawn_traversal`.
- **Subprocess precedent:** the ONLY `std::process::Command` in core is spawning nvim (`crates/wolf-core/src/nvim.rs:1431`, piped stdio, stderr drained to tracing). This is the model for spawning `rg --json` and streaming stdout.
- **Bundled-binary path:** Rust reads `WOLF_NVIM_BIN` env var (nvim.rs:107); Swift resolves `Bundle.main.resourcePath + "/nvim/bin/nvim"` and `setenv("WOLF_NVIM_BIN", …, 0)` (Project.swift:82,291). For rg: mirror with `WOLF_RG_BIN` + Swift `rgBinPath()`.
- **wolf-core structure:** modules are single files in dependency order `render → layout → state → input → redraw → nvim → files → mru → rank → tree → api`. New `search.rs` sibling to `files.rs` owns the rg subprocess + `SearchSession`/`SearchState`. `WolfCore::start_search(query, opts, wake)` added to `api.rs` next to `start_file_search` (api.rs:1061).
- **Pure projection:** `crates/wolf-core/src/tree.rs` is a pure GUI-free projection of the flat entry list into a hierarchy (`TreeNode` enum, `children_of`, `TreeIndex`). A search-results tree (file → line-match children) is a new pure projection, structurally analogous — good target for pure unit tests.
- **Search root:** no separate "project root" abstraction — root at `WolfCore::resolve_search_root()` (api.rs:1265 → `live_cwd()` else config fallback), same as the picker, so it tracks `:cd`/`:lcd`/`:tcd` and never blocks on a wedged nvim.
- **`:ignorecase` read:** there is currently NO public "eval"/`nvim_get_option_value` helper (documented in `crates/wolf-core/tests/appearance.rs:8`). Options are read either via a blocking RPC (template: `current_dir()` at nvim.rs:236 = `nvim_call_function("getcwd", [])`) OR folded out-of-band into the render snapshot via an init.lua autocmd + `rpcnotify` (like `wolf_cwd`/`wolf_theme`). Architect decides which; out-of-band (e.g. `OptionSet ignorecase` → notification) stays wedge-independent but is more surface. `runtime/nvim/init.lua` sets no ignorecase/smartcase today.

### C. Bazel bundling (pinned rg 15.2.0)
- Mirror the nvim mechanism end-to-end:
  - `bazel/nvim/extensions.bzl`: module extension with two per-arch `http_archive`s (`_VERSION`, two `_SHA256` pins), each with a `build_file_content` overlay that globs + `copy_to_directory` (preserves exec bit) with `replace_prefixes` remapping the archive top dir to an inner `nvim/`. → clone to `bazel/rg/extensions.bzl`: `rg_macos_arm64` + `rg_macos_x86_64` from `https://github.com/BurntSushi/ripgrep/releases/download/15.2.0/ripgrep-15.2.0-aarch64-apple-darwin.tar.gz` and `…-x86_64-apple-darwin.tar.gz`; top dir `ripgrep-15.2.0-<triple>/` contains `rg`; `replace_prefixes` → `rg/` giving `rg/rg`.
  - `bazel/nvim/BUILD.bazel`: `alias(name="nvim_tree", actual=select({@platforms//cpu:arm64/x86_64}))` + version smoke test. → `bazel/rg/BUILD.bazel`: `alias(name="rg_tree", …)` + optional version test.
  - `MODULE.bazel` (~lines 152-160): `nvim = use_extension(...)` + `use_repo(...)`. → add `rg = use_extension("//bazel/rg:extensions.bzl","rg")` + `use_repo(rg, "rg_macos_arm64","rg_macos_x86_64")`. Regenerate `MODULE.bazel.lock`.
  - `crates/wolf-macos/BUILD.bazel` `resources` (~lines 175-185): add `"//bazel/rg:rg_tree"` → lands at `Contents/Resources/rg/rg`.
  - `crates/wolf-macos/Sources/WolfMacOS/Project.swift` (~lines 289-298 + 79-84): add `rgBinPath()` (`resourcePath + "/rg/rg"`, guard `isExecutableFile`) + `setenv("WOLF_RG_BIN", …, 0)`.
  - Rust reader mirrors nvim.rs:107 (`std::env::var("WOLF_RG_BIN")`).
  - `crates/wolf-macos/bundle_layout_test.sh`: add an rg `-x` exec-bit check (mirror the nvim check).
  - `flake.nix` (~lines 86-139): add `pkgs.ripgrep` + `export WOLF_RG_BIN="${pkgs.lib.getExe pkgs.ripgrep}"` for the Linux/cargo path.

### Test wiring reminders (from CLAUDE.md — critical)
- Bazel does NOT auto-discover tests. Every new Rust `tests/*.rs` needs a matching `rust_test` target in the crate's `BUILD.bazel` (real-nvim tests need `//bazel/nvim:nvim_tree` + `//runtime/nvim:init.lua` runfiles, wired `WOLF_*` env, `tags=["no-sandbox"]`; headless tests need none). A real-rg test also needs `//bazel/rg:rg_tree` staged + `WOLF_RG_BIN` wired.
- Every new Swift `*Tests.swift` must be added to the explicit `srcs` list of `//crates/wolf-macos:WolfKeyMapTests` (NOT a glob).
- Prefer end-to-end/integration tests that start a real nvim (and now a real rg) over mocks.
- Confirm new tests actually run under `bazelisk test`.

---

## Status update: ARCHITECTURE COMPLETE — TEST-WRITING NEXT

## Architect's plan

### 1. Summary
ripgrep-backed content-search panel in the macOS sidebar. New sidebar header strip with two SF-Symbol toggle buttons (folder / magnifying-glass) switches the sidebar body between the existing `FileTreeView` and a new `SearchView`. Search field (Enter-triggered), three toggle buttons (case-sensitive seeded from nvim `:ignorecase`, use-regex default ON, search-all-files default OFF = Markdown only), streaming kept-sorted results tree (files → matching lines, bold match + dimmed context, wrapped to ≤2 balanced-context lines, click opens file at line/col). Data path mirrors the `FileSearch`/`FfiFileSearch` streaming pattern exactly: new `wolf-core` `search` module owns an `rg --json` subprocess feeding a versioned snapshot behind an `RwLock`, a fourth FFI wake channel drives refreshes, pure projection/display logic lives in a headlessly-testable `wolf-core` module + small Foundation-only Swift seams. Pinned rg 15.2.0 bundled via new `bazel/rg` package, mirroring `bazel/nvim`.

### 2. Files to CREATE

**Rust — wolf-core**
- `crates/wolf-core/src/search.rs` — rg subprocess + streaming session (sibling of `files.rs`; same lock/wake/spawn_blocking/cancel-on-Drop shape).
  - `SearchOptions { query: String, use_regex: bool, case_sensitive: bool, all_files: bool }` (Clone, Debug).
  - `SearchMatch { abs_path: PathBuf, rel_path: String, line_number: u64, column: u32, line_text: String, match_ranges: Vec<(u32,u32)> }`. `rel_path` `/`-separated relative to root (fallback absolute, mirror `files::display_for`); `line_text` trailing `\n`/`\r\n` stripped; `match_ranges` byte `[start,end)` into `line_text`; `column` = first submatch byte start (0-based). Clone, Debug, PartialEq, Eq.
  - `SearchState { matches: Vec<SearchMatch>, complete: bool, error: Option<String>, version: u64 }` (mirror `FileSearchState`; matches accumulate in rg emission order; sort at projection time; version starts 1, bumps per batch).
  - `SearchReadGuard` — owned read guard (copy `FileSearchReadGuard`: `ArcRwLockReadGuard<RawRwLock, SearchState>` + Deref).
  - `SearchSession { state: Arc<RwLock<SearchState>>, cancel: Arc<AtomicBool>, child: Arc<Mutex<Option<std::process::Child>>> }` with `read() -> SearchReadGuard`, `cancel()` (set flag AND `child.kill()`), `Drop` calls `cancel()`, `from_parts(...)` ctor.
  - Internal serde parse structs (cfg(test)-tested): `#[serde(tag="type", content="data", rename_all="snake_case")] enum RgEvent { Begin(RgBegin), Match(RgMatch), End(RgEnd), Summary(Value), Context(Value) }`; `RgBegin { path: RgText }`; `RgMatch { path: RgText, lines: RgText, line_number: Option<u64>, absolute_offset: u64, submatches: Vec<RgSubmatch> }`; `RgSubmatch { #[serde(rename="match")] text: RgText, start: usize, end: usize }`; `RgText { text: Option<String> }` (deserialize only `text`; SKIP events whose required text is `None` = non-UTF-8 → documented limitation, no base64 dep).
  - `fn parse_rg_line(&str) -> Option<RgEvent>` (from_str, None on error). `fn match_from_event(root: &Path, m: &RgMatch) -> Option<SearchMatch>` (strip newline, rel_path via display_for-style helper, column = first submatch start, collect ranges; None if line_number/line/path text missing).
  - `pub(crate) fn spawn_search(root, opts, rg_bin, state, cancel, child_slot, wake, handle: tokio::runtime::Handle)` — analog of `spawn_traversal`. On `handle.spawn_blocking`: build Command, spawn piped stdout/stderr (stderr → tracing like nvim.rs), store Child in child_slot, `BufReader::lines()` loop; per line check cancel (break + kill), parse, on Match push into batch; every `SEARCH_BATCH` (~128) OR on each `End` → `append_batch` under brief write lock (append + version+=1), drop, `wake()`. On EOF/Summary append remainder, `complete = true` unless cancelled, final `wake()`. Non-zero exit / spawn error → `state.error` (still complete).
  - `fn append_batch(state, batch, complete)` — copy `files::append_batch` (no `truncated`).
  - **rg command line** (order): `<WOLF_RG_BIN> --json --no-config --hidden  [ -F | nothing ]  [ -s | -i ]  [ --glob '*.md' --glob '*.markdown' | nothing ]  -e <query> -- <root>`. `-F` when NOT use_regex; `-s` when case_sensitive else `-i` (NOT smart-case); globs when NOT all_files; `--no-config` for determinism; `--hidden` mirrors `files.rs` `.hidden(false)` (rg still honors .gitignore + excludes .git); `-e <query>` protects leading dash; `--` separates root.
- `crates/wolf-core/src/search_tree.rs` — pure GUI-free projection + display (sibling of `tree.rs`; width is an integer CHAR budget, no pixels).
  - `SearchFileGroup { abs_path: PathBuf, rel_path: String, label: String, match_indices: Vec<usize> }`.
  - `pub fn project(matches: &[SearchMatch]) -> Vec<SearchFileGroup>` — group by abs_path, sort groups by rel_path **case-insensitively** (`to_ascii_lowercase` compare + rel_path tie-break, mirror `tree::node_cmp`), sort each group's `match_indices` by line_number. Sort-on-read.
  - `pub fn file_label(rel_path: &str) -> String` — `"<basename> - <relative parent dir>"`; `foo/bar/baz.md` → `"baz.md - foo/bar"`; root file `baz.md` → `"baz.md"` (no `" - "` when parent empty). `/`-splitting.
  - `ContextWindow { text: String, ranges: Vec<(u32,u32)>, truncated_left: bool, truncated_right: bool }` — visible substring, bold ranges adjusted RELATIVE to `text` (byte offsets), per-side ellipsis flags.
  - `pub fn context_window(line: &str, ranges: &[(u32,u32)], budget: usize) -> ContextWindow` — THE 2-line balanced-context computation, centered on FIRST match range, roughly equal context each side. Operate on char boundaries (never split a scalar). If line char count ≤ budget → whole line, ranges unchanged, both truncated=false. Else `remaining = budget - match_span_chars`, `left = remaining/2`, `right = remaining-left`, window `[match_start-left, match_end+right)` clamped to `[0,len]`; if one side clamps, spend leftover on the other ("balanced but greedy at edges"); slice text; recompute each intersecting range → window-relative byte offsets (drop fully-outside; clamp partial); set truncated_* from whether window starts/ends inside line. Ellipsis glyph added by Swift (kept out of Rust so offsets stay clean); AppKit wraps `text` across ≤2 lines.
  - `#[cfg(test)] mod tests`.
- Register in `crates/wolf-core/src/lib.rs`: `pub mod search;` (after files) + `pub mod search_tree;` (after tree); re-export `SearchOptions, SearchMatch, SearchSession, SearchReadGuard` and `SearchFileGroup, ContextWindow, project, file_label, context_window`.

**Swift — WolfKeyMap seams (Foundation only, headlessly tested)**
- `Sources/WolfKeyMap/SearchToggleSeam.swift` — `public enum SearchToggleSeam { static func initialCaseSensitive(ignorecase: Bool) -> Bool  (= !ignorecase); static let defaultUseRegex = true; static let defaultAllFiles = false }`.
- `Sources/WolfKeyMap/SearchBudgetSeam.swift` — `public enum SearchBudgetSeam { static func charBudget(sidebarWidth: Double, charAdvance: Double, lines: Int = 2) -> Int }` = `max(1, floor(width/advance)) * lines` (guard advance ≤ 0 → floor).
- `Sources/WolfKeyMap/SearchHighlightSeam.swift` — `public enum SearchHighlightSeam`, `public struct SearchSpan { utf16Start: Int; utf16Length: Int; bold: Bool }`, `static func spans(text: String, boldByteRanges: [(Int,Int)]) -> [SearchSpan]` — map Rust BYTE offsets → UTF-16 spans (for NSAttributedString/NSRange), ordered gap-filling, every char covered once, bold inside a match range else non-bold(dim).

**Swift — AppKit views / bridges (smoke-only, not headless)**
- `Sources/WolfMacOS/SidebarContainerView.swift` — `final class SidebarContainerView: NSView`. Top header strip (~30pt) with two SF-Symbol `NSButton`s (`"folder"`, `"magnifyingglass"`, mutually-exclusive toggle pair) + content area hosting BOTH `FileTreeView` and `SearchView`, switched via `isHidden`. `init(tree: FileTreeView, search: SearchView)`, `mode` enum (.tree/.search), forwards sidebar `isCollapsed` visibility to active child. Becomes the sidebar's hosted NSView (manages own layout like FileTreeView's scroll view filling the pane).
- `Sources/WolfMacOS/SearchView.swift` — `final class SearchView: NSView, NSOutlineViewDataSource, NSOutlineViewDelegate`, modeled on `FileTreeView`. Subviews: NSTextField search field (`action = #selector(runSearch)`, Return ONLY — do NOT wire `controlTextDidChange`); three toggle NSButtons seeded from `SearchToggleSeam` + `handle.ignorecase()`; NSOutlineView in NSScrollView. State: `search: FfiSearch?` (dropped/replaced each run to cancel prior rg), `TreeRefreshGate`, `PickerRefreshCoalescer` (reuse verbatim like FileTreeView), row-model caches `SearchFileItem`/`SearchLineItem` (stable identity), current char budget. `runSearch()` builds SearchOptions, `handle.start_search(...)`, clears caches, reloadData. Search wake → `scheduleRefresh()` → version-gated `refresh()` rebuilds rows from `files(budget:)`, auto-expands new file items (expanded by default; collapse preserved by identity). Cells: file rows = `file.label`; line rows = NSAttributedString from `SearchHighlightSeam.spans`, `maximumNumberOfLines=2`, `.byWordWrapping`, ellipsis prefix/suffix when truncated; variable row height via `heightOfRowByItem`. Line click → `handle.open_at(absPath, line, column)` then return focus to paneContainer. `close()` drops search + cancels coalescer.
- `Sources/WolfMacOS/SearchWakeBridge.swift` — `enum SearchWakeBridge { static func install() }` calling `wolf_ffi_set_search_wake({ token in DispatchQueue.main.async { WindowRegistry.shared.project(for: token)?.onSearchWake() } })`. Verbatim clone of `TreeWakeBridge.swift`.

**Bazel — bundled ripgrep 15.2.0**
- `bazel/rg/extensions.bzl` — clone of `bazel/nvim/extensions.bzl`. `_VERSION="15.2.0"`, two per-arch `_SHA256` pins (**coder MUST compute real SHA256s — leave clear TODO**), `http_archive`s `rg_macos_arm64`/`rg_macos_x86_64` from `https://github.com/BurntSushi/ripgrep/releases/download/15.2.0/ripgrep-15.2.0-aarch64-apple-darwin.tar.gz` and `…-x86_64-apple-darwin.tar.gz`. Archive top dir `ripgrep-15.2.0-<triple>/` contains `rg`; overlay globs `["{archive_root}/**"]` + `copy_to_directory(replace_prefixes={"{archive_root}":"rg"})` → `rg/rg` (exec bit preserved).
- `bazel/rg/BUILD.bazel` — `alias(name="rg_tree", actual=select({"@platforms//cpu:arm64":"@rg_macos_arm64//:rg_tree","@platforms//cpu:x86_64":"@rg_macos_x86_64//:rg_tree"}), visibility=["//visibility:public"])` + `sh_test(name="rg_version_test", srcs=["rg_version_test.sh"], data=[":rg_tree"])` (asserts rg/rg exists, executable, `rg --version` = `ripgrep 15.2.0`).
- `bazel/rg/rg_version_test.sh` (mirror `nvim_version_test.sh`).
- `bazel/rg/README.md` (mirror `bazel/nvim/README.md`: targets, overlay/replace_prefixes, bump steps).

**Tests to create** — `crates/wolf-core/tests/search.rs` (real-nvim+real-rg); `crates/wolf-ffi/tests/search.rs` (FFI surface, model on `tree_search.rs`); `Tests/WolfKeyMapTests/{SearchToggleSeam,SearchBudgetSeam,SearchHighlightSeam}Tests.swift`.

### 3. Files to MODIFY

**wolf-core**
- `src/input.rs` — add `InputEvent::OpenAt { path: PathBuf, line: u64, col: u64 }` next to `Open`. In `apply_event` (match ~L426) add arm: run `edit <fnameescape(path)>` (reuse Open arm's command/fnameescape) then `nvim.call("nvim_win_set_cursor", [0, [line, col]])`. rg line_number 1-based = cursor line; rg column/submatch start 0-based byte = cursor col; nvim clamps.
- `src/api.rs` — extend MRU-record loop (~L359-367) to `InputEvent::Open { path } | InputEvent::OpenAt { path, .. }`. Add `pub fn open_at(&self, path, line, col)` → `send_input(InputEvent::OpenAt{...})`. Add `pub fn start_search(&self, opts: SearchOptions, wake: WakeFn) -> SearchSession`: `root = resolve_search_root()`; `rg_bin = env::var("WOLF_RG_BIN")` — unset/empty → warn + immediately-complete SearchSession with `error=Some("WOLF_RG_BIN unset")` (graceful degrade); build shared state/cancel/child_slot, call `search::spawn_search(root, opts, rg_bin, state.clone(), cancel.clone(), child_slot.clone(), wake, self.runtime.handle().clone())`, return `SearchSession::from_parts(...)`. Add `pub fn ignorecase(&self) -> bool` via `block_on_rpc("ignorecase", nvim.ignorecase())`, false on error/timeout.
- `src/nvim.rs` — add `pub fn ignorecase(&self) -> impl Future<Output=Result<bool,NvimError>> + Send + 'static` via `nvim.call("nvim_get_option_value", [ "ignorecase", {} ])` → `as_bool().unwrap_or(false)`. Model on `current_dir()` (L236).

**wolf-ffi** (`crates/wolf-ffi/src/lib.rs`)
- Bridge `mod ffi`: opaque `FfiSearch`, `FfiSearchFile`, `FfiSearchMatch`; on `WolfCoreHandle`: `start_search(query, use_regex, case_sensitive, all_files) -> FfiSearch`, `ignorecase() -> bool`, `open_at(path: String, line: u64, col: u64)`. `FfiSearch`: `version() -> u64`, `files(budget: u32) -> Vec<FfiSearchFile>`. `FfiSearchFile`: `label()`, `rel_path()`, `abs_path()`, `matches() -> Vec<FfiSearchMatch>`. `FfiSearchMatch`: `line_number() -> u64`, `column() -> u32`, `display_text() -> String`, `truncated_left()/right() -> bool`, and bold ranges as parallel `range_starts()/range_ends() -> Vec<u32>` (avoid extra opaque type).
- Glue: `pub struct FfiSearch { search: Option<SearchSession>, cache: RefCell<SearchProjectionCache> }` memoizing sorted `Vec<SearchFileGroup>` + per-match `ContextWindow`s keyed on `(version, budget)` (analog of FfiFileSearch tree_cache). `files(budget)` reads snapshot briefly, runs `project` + `context_window` per match under guard, clones out `Vec<FfiSearchFile>` (windowed matches), drops guard. `start_search` builds SearchOptions → `core.start_search(opts, registered_search_wake(self.wake_token))`; core-gone → `FfiSearch{search:None,..}`. `ignorecase()` → `borrow().as_ref().map_or(false,|c| c.ignorecase())`. `open_at` → core.open_at. Fourth wake channel: `static SEARCH_WAKE_FN: AtomicPtr<()>` + `#[no_mangle] pub extern "C" fn wolf_ffi_set_search_wake(Option<WakeCallback>)` + `fn registered_search_wake(token) -> WakeFn` (verbatim clones of FILESEARCH/TREE trio L1143-1219).
- `crates/wolf-ffi/generated/wolf_ffi_wake.h` — add `void wolf_ffi_set_search_wake(void (*wake)(uint64_t token));`.
- **Regenerate + commit bindings**: `bazel run //crates/wolf-ffi:gen_bindings`; commit refreshed `generated/*.swift` (+ `.h`).

**Swift front-end wiring**
- `EditorSplitViewController.swift` — `init` takes `SidebarContainerView` (e.g. `init(sidebar:paneContainer:)`), hosts it as sidebar HostViewController view (L35). Split-item config unchanged.
- `MainWindow.swift` — inits take `SidebarContainerView`, thread to EditorSplitViewController. `sidebarItem` accessor unchanged.
- `WolfWindowController.swift` — ~L225 build `SearchView(handle:paneContainer:)` alongside FileTreeView, wrap both in `SidebarContainerView(tree:search:)`, store props, pass container to MainWindow (L228-234). `sidebarVisibilityObservation` (L331-335) relays visibility to container (forwards to active child); keep tree relay. Add search `close()`/teardown on window close.
- `Project.swift` — add `func onSearchWake()` fanning to all views' search panels (mirror `onTreeWake` L272; version-gated + coalesced so idle views no-op). Add `static func rgBinPath() -> String?` = `Bundle.main.resourcePath + "/rg/rg"` guarded by `isExecutableFile` (mirror `nvimBinPath` L291). In `start(...)` (~L79-84) after WOLF_NVIM_BIN setenv add `if let rg = rgBinPath() { setenv("WOLF_RG_BIN", rg, 0) }`.
- `AppDelegate.swift` — install `SearchWakeBridge.install()` alongside existing WakeBridge/PickerWakeBridge/TreeWakeBridge installs.

**Bazel bundling wiring**
- `MODULE.bazel` (after nvim block L108-109): `rg = use_extension("//bazel/rg:extensions.bzl","rg")` + `use_repo(rg, "rg_macos_arm64","rg_macos_x86_64")`. Regenerate `MODULE.bazel.lock` (`--lockfile_mode=update`).
- `crates/wolf-macos/BUILD.bazel` — `resources` (~L179) add `"//bazel/rg:rg_tree"` → `Contents/Resources/rg/rg`. `wolf_key_map` `srcs` (L51-83) add the 3 seam files (alphabetical). `WolfKeyMapTests` `srcs` (L240+) add the 3 `Search*Tests.swift`. (`WolfMacOS` glob L94 auto-picks SearchView/SidebarContainerView/SearchWakeBridge.)
- `crates/wolf-macos/bundle_layout_test.sh` — add rg exists+executable check (mirror check (a)); update summary.
- `flake.nix` — `linuxShell.packages` (~L86-134) add `pkgs.ripgrep`; `shellHook` (~L136+) add `export WOLF_RG_BIN="${pkgs.lib.getExe pkgs.ripgrep}"` beside WOLF_NVIM_BIN.
- `crates/wolf-core/BUILD.bazel` + `crates/wolf-ffi/BUILD.bazel` — new `rust_test` targets (§Test plan).

### 4. Design decisions
- **`:ignorecase` via blocking RPC (not out-of-band notification).** Value needed only ONCE when panel first shown to seed the toggle; #2 says no write-back and no live tracking. One-shot bounded `block_on_rpc` mirrors `current_dir()`; degrades to false on wedged nvim. Out-of-band would be far more surface (init.lua + redraw parse + snapshot field + accessor) for no benefit.
- **Sort-on-read (not stable-insert).** rg emits per-file grouped (ascending line within file) but not cross-file rel-path ordered. `matches` accumulate in emission order; `project` sorts at projection time, memoized per `(version, budget)`. Keeps hot streaming path lock-cheap (append + version bump); O(F log F) only on UI reload. Mirrors FfiFileSearch flat entries + per-version TreeIndexCache.
- **Cursor placement.** rg first-submatch start (0-based byte) + 1-based line_number → `nvim_win_set_cursor(0,[line,col])`. `OpenAt` does edit then set_cursor in one input-pump step (ordered/atomic). No open-at-position path exists today.

### 5. Dependencies
No new crates (`serde`/`serde_json` already wolf-core deps; `ignore` already used; rg is a subprocess). New Bazel extension repo pair via `bazel/rg`. Nix adds `pkgs.ripgrep` (Linux/cargo only). **FLAG: the two `_SHA256` pins are placeholders — coder must download the 15.2.0 aarch64+x86_64 apple-darwin tarballs, `shasum -a 256`, then regenerate MODULE.bazel.lock.**

### 6. Open-at-line-and-column
FileTreeView opens via `handle.open(absPath)` (bare `:edit`, no cursor). New surface: `InputEvent::OpenAt{path,line,col}` → `WolfCore::open_at` → FFI `open_at` → SearchView calls on line-row click, then returns first-responder to paneContainer. MRU recording extended to cover OpenAt.

### 9. Implementation ORDER (avoid compile/link breaks)
1. wolf-core open-at + ignorecase (InputEvent::OpenAt + input.rs arm; open_at; NvimProcess::ignorecase + WolfCore::ignorecase; extend MRU match). Compiles standalone.
2. wolf-core `search_tree.rs` (pure) + re-exports + cfg(test) tests. Standalone.
3. wolf-core `search.rs` (types/spawn_search/parse) + re-exports + `WolfCore::start_search` + cfg(test) parse tests.
4. `bazel/rg` package (extensions.bzl w/ TODO SHAs, BUILD, version test, README) + MODULE.bazel wiring + lockfile regen. Needed before rg-staging test + bundle.
5. wolf-ffi: wake channel + header decl; FfiSearch/FfiSearchFile/FfiSearchMatch + cache + accessors; start_search/ignorecase/open_at; bridge decls. Then **regenerate + commit bindings**.
6. Swift seams + `wolf_key_map` srcs wiring. Foundation-only.
7. Swift UI: SearchView, SidebarContainerView, SearchWakeBridge; thread EditorSplitViewController→MainWindow→WolfWindowController; Project.onSearchWake + rgBinPath + WOLF_RG_BIN; AppDelegate install. (Depends on step-5 bindings.)
8. Bundle wiring: macOS BUILD resources += rg_tree; bundle_layout_test.sh; flake.nix.
9. Tests: core/search.rs + target; ffi/search.rs + target; 3 Swift seam tests + WolfKeyMapTests srcs. `bazelisk test` to confirm each new target executes.
10. Docs + prettier.

### 10. Lint
- serde structs have read-but-unused fields (absolute_offset, End/Summary/Context payloads) → local `#[allow(dead_code)]` or trim fields. `child.kill()` on Drop ignores errors (`let _ =`). Hand-written FFI glue passes stricter bar (`#[must_use]` on accessors like FfiFileSearch). Swift seams `public`, `import Foundation` only (stay headless). rustfmt on core/ffi REQUIRED (not advisory) — run on new modules.

## Test plan (from architect §7)

**Pure headless Rust unit tests** (run by EXISTING lib test target — cfg(test) modules compile into the lib; confirm they run under `bazelisk test //crates/wolf-core:wolf_core_lib_test` or the crate's lib-test target name):
- `search.rs` cfg(test): begin/match/end/summary decode; multi-submatch → multiple ranges; column = first submatch start; CRLF/LF newline stripping (ranges still correct); non-UTF-8 (`{"bytes":…}` no text) → SKIPPED (None); malformed line → None; rel_path relative to root + absolute fallback.
- `search_tree.rs` cfg(test): `file_label` (`foo/bar/baz.md`→`baz.md - foo/bar`; root `baz.md`→`baz.md`); `project` (case-insensitive file sort `Foo.md` vs `bar.md`; line-number sort within file; grouping by abs_path); `context_window` (line < budget = whole line no truncation; match at start = truncate right only; match at end = truncate left only; match middle of long line = balanced both truncated; exact-fit boundary; leftover-budget spent on other side when one side clamps; multibyte/emoji never splits scalar + ranges adjusted).

**Real-subprocess Rust integration** — `crates/wolf-core/tests/search.rs` (NEW): real nvim + real rg (no mocks). Reuse `tests/common/mod.rs`. Seed temp cwd with `.md`/`.markdown`/`.txt` across nested dirs with known match/no-match lines; start WolfCore rooted there; `start_search` with various options; poll snapshot until complete; assert: matches stream + end complete; groups sorted case-insensitively by rel-path; lines sorted by line number; ranges/column correct; `all_files=false` excludes `.txt`, `=true` includes; `use_regex=false` literal (metachar matches literally), `=true` real regex; `case_sensitive` true vs false differs on mixed-case query; `open_at` round-trip (call, poll `current_file()` until == opened path canonicalized, core stays responsive). Wire `rust_test(name="search", srcs=["tests/common/mod.rs","tests/search.rs"], crate_root="tests/search.rs", tags=["no-sandbox"], data=["//bazel/nvim:nvim_tree","//runtime/nvim:init.lua","//bazel/rg:rg_tree"], env={the three WOLF_TEST_/WOLF_NVIM_BIN keys as in `file_search` PLUS "WOLF_RG_BIN":"$(rootpath //bazel/rg:rg_tree)/rg/rg"}, deps=[":wolf_core"]+all_crate_deps(...))` mirroring `file_search` (L55-90). Harness passes WOLF_RG_BIN through, OR coder adds `rg_bin_or_skip` loud-SKIP helper (mirror `nvim_bin_or_skip`).

**FFI-surface integration** — `crates/wolf-ffi/tests/search.rs` (NEW): model EXACTLY on `crates/wolf-ffi/tests/tree_search.rs`; real nvim + real rg, loud-SKIP when unavailable. Exercise `start_search` → `FfiSearch::version`/`files(budget)`, `FfiSearchFile::label/rel_path/abs_path/matches`, `FfiSearchMatch::line_number/column/display_text/ranges/truncated_*`, `ignorecase`, `open_at` round-trip. Wire `rust_test(name="search",…)` in `crates/wolf-ffi/BUILD.bazel` mirroring `tree_search` block, adding `//bazel/rg:rg_tree` to data + WOLF_RG_BIN to env.

**Swift headless seam tests** (wired into `//crates/wolf-macos:WolfKeyMapTests` srcs):
- `SearchToggleSeamTests.swift` — `initialCaseSensitive` inverts; the two default constants.
- `SearchBudgetSeamTests.swift` — `charBudget` = floor(width/advance)*2; degenerate advance≤0 guard; 2× line factor.
- `SearchHighlightSeamTests.swift` — `spans`: ASCII single/multiple; adjacent/edge ranges; multibyte(accented)+emoji correct UTF-16 offsets; gap-filling coverage (every char in exactly one span); empty ranges → one non-bold span.

Every new `*Tests.swift` → `WolfKeyMapTests` srcs; every new seam `.swift` → `wolf_key_map` srcs (else silently never compiled/run). AppKit views (SearchView/SidebarContainerView/SearchWakeBridge) = compile-verified + manual-smoke only (need window server), like FileTreeView.

**Bundle test** — extend `bundle_layout_test.sh` with `Contents/Resources/rg/rg` exists+executable; optionally `//bazel/rg:rg_version_test`.

## Documentation to update (§8)
- `crates/wolf-core/src/README.md` — document `search.rs` + `search_tree.rs`; add `InputEvent::OpenAt`, `WolfCore::start_search`/`open_at`/`ignorecase`; update dependency-order line + lib.rs re-export list.
- `crates/wolf-core/README.md` — add search feature if module/feature enumerated.
- `crates/wolf-ffi/src/README.md` — `FfiSearch`/`FfiSearchFile`/`FfiSearchMatch`, fourth wake channel, `start_search`/`ignorecase`/`open_at`; note FOUR wake channels.
- `crates/wolf-ffi/README.md` — surface overview + regen note.
- `crates/wolf-macos/README.md` + `Sources/README.md` — `SidebarContainerView`, `SearchView`, `SearchWakeBridge`, 3 seams; update AppKit-free file count + imported-generated-types list.
- `bazel/rg/README.md` — new (mirror `bazel/nvim/README.md`).
- Root `README.md` — add search panel if user-facing features listed.
- NO `runtime/nvim` change (ignorecase via blocking RPC, not autocmd).

## Test ownership split (orchestrator decision)
- **test-writer owns** all EXTERNAL test files: `crates/wolf-core/tests/search.rs` (real nvim+rg), `crates/wolf-ffi/tests/search.rs` (FFI surface), a NEW headless standalone `crates/wolf-core/tests/search_projection.rs` exercising the PUBLIC re-exported pure fns `project`/`file_label`/`context_window` (no nvim, no rg — plain `rust_test`, needs no data/env/no-sandbox), and the 3 Swift `Search*Tests.swift`. Plus ALL their BUILD/`srcs` wiring.
- **coder owns** the INLINE `#[cfg(test)] mod tests` blocks for the PRIVATE rg-JSON parse fns (`parse_rg_line`, `match_from_event`) inside `search.rs` — these are inseparable from the private module (standard Rust inline-unit-test idiom) and run under the existing lib-test target. The `search_projection.rs` external file covers the public pure fns; the inline block covers only the private parse/skip cases (non-UTF-8 `{"bytes"}`→None, malformed→None, event decode, CRLF stripping, column/ranges).
- Reviewer verifies BOTH exist and run.

## Progress log
- **test-writer** (done): created external tests `crates/wolf-core/tests/search.rs` (real nvim+rg: streaming/sort/ranges, all_files filter, regex literal-vs-regex, case-sensitivity, open_at round-trip; local `rg_bin_or_skip` loud-SKIP), `crates/wolf-core/tests/search_projection.rs` (headless: file_label, project case-insensitive sort + line sort + grouping, 7 context_window cases incl. multibyte/emoji), `crates/wolf-ffi/tests/search.rs` (FFI surface incl. ignorecase + open_at, modeled on tree_search.rs), and Swift `SearchToggleSeamTests/SearchBudgetSeamTests/SearchHighlightSeamTests.swift`. BUILD wiring added: core `search` (no-sandbox, data+env incl. `//bazel/rg:rg_tree`+`WOLF_RG_BIN`) & `search_projection` (plain headless) targets; ffi `search` target; 3 Swift tests into `WolfKeyMapTests` srcs.
  - **Coder MUST match these test-bound contracts** (test-writer's flagged assumptions):
    - `//bazel/rg:rg_tree` must exist (impl step 4) — core/ffi `search` targets reference it and won't build until then.
    - Public struct literals used by tests → these fields MUST be `pub`: `SearchOptions{query,use_regex,case_sensitive,all_files}`, `SearchMatch{abs_path,rel_path,line_number,column,line_text,match_ranges}`, `SearchFileGroup{abs_path,rel_path,label,match_indices}`, `ContextWindow{text,ranges,truncated_left,truncated_right}`. Re-export `project,file_label,context_window,SearchFileGroup,ContextWindow,SearchOptions,SearchMatch,SearchSession`.
    - `SearchSession::read()` guard derefs to a view exposing public `matches: Vec<SearchMatch>`, `complete: bool`, `version: u64`.
    - `WolfCore::start_search(SearchOptions, WakeFn) -> SearchSession`; `WolfCore::open_at(PathBuf,u64,u64)`.
    - Core test reads current file via a snapshot accessor named `current_file` on the read snapshot — VERIFY the actual accessor name in the codebase (`current_file()`); align the test if the real name differs (coder may adjust the test call site to the real accessor — this is a test-writer oversight to reconcile, not a contract to invent).
    - FFI (crate-root `pub` like `FfiTreeNode`): `WolfCoreHandle::{start_search(String,bool,bool,bool)->FfiSearch, ignorecase()->bool, open_at(String,u64,u64)}`; `FfiSearch::{version()->u64, files(u32)->Vec<FfiSearchFile>}`; `FfiSearchFile::{label,rel_path,abs_path,matches}`; `FfiSearchMatch::{line_number,column,display_text,truncated_left,truncated_right,range_starts,range_ends}`.
    - Swift seams: `SearchBudgetSeam.charBudget(sidebarWidth:charAdvance:lines:)` with `lines: Int = 2` default; `charAdvance<=0` → positive floor `>= lines`. `SearchHighlightSeam.spans(text:boldByteRanges:[(Int,Int)]) -> [SearchSpan]`, `SearchSpan{utf16Start,utf16Length,bold}`. Add the 3 seam source files to `wolf_key_map` srcs (else WolfKeyMapTests won't compile).
    - Budget is a CHAR count (not bytes) throughout (`context_window`/`charBudget`/`files(budget)`); emoji/multibyte assertions depend on char-boundary slicing.
    - `ignorecase()==false` core/ffi assertions assume captive init.lua sets no ignorecase/smartcase (true today).

## Implementation notes (coder — DONE, all green)
- Implemented all 8 impl-order steps. rg SHAs computed & pinned: aarch64 `3750b2e93f37e0c692657da574d7019a101c0084da05a790c83fd335bad973e4`, x86_64 `af7825fcc69a2afc7a7aea55fc9af90e26421d8f20fe59df32e233c0b8a231c1`. Archive top-dir verified → `replace_prefixes`→`rg/rg`. `MODULE.bazel.lock` regenerated.
- FFI bindings regenerated & on disk (`generated/wolf-ffi.swift`/`.h` carry FfiSearch/FfiSearchFile/FfiSearchMatch + start_search/ignorecase/open_at; `wolf_ffi_wake.h` declares `wolf_ffi_set_search_wake`).
- Inline `#[cfg(test)]` in search.rs = 11 parse tests (event decode, multi-submatch, CRLF/LF strip, non-UTF-8 path/line skip, missing line_number skip, malformed→None, absolute-fallback rel_path). Clippy `too_many_arguments` resolved by bundling shared Arcs into private `SearchHandles` (not suppressed).
- **Test results (fresh, `--nocache_test_results`, all PASSED):** `//crates/wolf-core:wolf_core_lib_test` (11 inline search tests printed ok), `:search_projection`, `:search` (real nvim+rg), `//crates/wolf-ffi:search` (real nvim+rg), `//crates/wolf-macos:WolfKeyMapTests` (3 new seam suites), `//crates/wolf-macos:bundle_layout_test` (rg at Contents/Resources/rg/rg), `//bazel/rg:rg_version_test` (ripgrep 15.2.0).
- **Lint:** rustfmt_core, rustfmt_ffi, clippy (`-Dwarnings`), swift_format all PASSED. App `//crates/wolf-macos:Wolf` builds & signs.
- **Coder notes:** No `current_file` reconciliation needed (real API matched tests unchanged — no test edits). `rustfmt.fix` whitespace-normalized the two untracked test-writer files (cosmetic, still pass). SourceKit "No such module WolfFFI/WolfKeyMap" diagnostics are IDE-only (no Bazel module context) — inert; the real bazelisk build compiled the Swift.
- **Coder-suggested optional coverage (orchestrator to decide):** a wolf-core test that `start_search` with `WOLF_RG_BIN` unset returns an immediately-`complete` SearchSession with `error == Some("WOLF_RG_BIN unset")` (graceful-degrade path only implicitly covered today).

## Review notes (reviewer — DONE)
- **Verdict: code CORRECT & spec-conformant across all 13 focus areas** (rg command toggles, JSON parse base/skip cases, case-insensitive file sort + line sort, file_label format incl. bare-basename root case, context_window balanced windowing + char-boundary slicing + range re-basing, byte→UTF-16 span mapping incl. emoji surrogates + gap-fill, streaming/cancel/child-reap no-zombie, open_at edit+set_cursor + MRU records OpenAt, ignorecase read-once no write-back, Enter-only search + expand-default/collapsible + click→open_at→refocus + 4th wake installed & fanned, rg 15.2.0 real SHAs → bundled executable resolved via WOLF_RG_BIN not system rg, all test targets wired incl. inline parse tests, bindings regenerated on disk). Cited file:line throughout.
- **BLOCKING B1 = documentation only** (CLAUDE.md hard rule): `crates/wolf-core/src/README.md` (no search.rs/search_tree.rs sections; lib.rs list + input/nvim/api sections omit OpenAt/open_at/start_search/ignorecase), `crates/wolf-ffi/src/README.md` (still says "three wake channels" — now four; FfiSearch* undocumented), `crates/wolf-macos/README.md` + `Sources/README.md` (SearchView/SidebarContainerView/SearchWakeBridge + 3 seams undocumented; AppKit-free count + imported-types list stale). → routed to technical-writer.
- **NON-BLOCKING:** N1 graceful-degrade (WOLF_RG_BIN unset) untested → ORCHESTRATOR APPROVED adding an isolated real-nvim/no-rg test target. N2 root README → folded into technical-writer. N3 `--hidden` intentional (mirrors files.rs) → no action.

## Post-review actions (orchestrator decisions)
- technical-writer: fix B1 (4 source READMEs + root README check) + prettier.
- test-writer: add N1 graceful-degrade test in an ISOLATED target (real nvim, NO rg / WOLF_RG_BIN intentionally unset) to avoid env-var races across parallel test threads. Then coder runs it.
