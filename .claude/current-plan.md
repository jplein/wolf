# Plan: Per-pane statusbar (macOS front-end)

## Status: COMPLETE ✅ — implemented, tested (39/39 Bazel green), reviewed (approved), perf-nit fixed, docs synced + prettier-clean

### Final outcome
- Feature built end-to-end across `runtime/nvim/init.lua` (`wolf_buffers`), wolf-core (`Snapshot.buffers`, `apply_buffers`, `setup_buffer_tracking`, pure `pane_status`, `WolfCore::pane_status`), wolf-ffi (`FfiPaneStatus`, `pane_status_for_grid`, regenerated bindings), wolf-macos (`StatusbarLayout` seam + `PaneContainerView` statusbar painting/geometry).
- Tests: `//crates/wolf-core:pane_status_unit` (9), `//crates/wolf-core:pane_buffers` (real nvim), `//crates/wolf-ffi:pane_status` (real nvim), `//crates/wolf-macos:WolfKeyMapTests` (StatusbarLayoutTests) — all PASS; broad `//crates/...` = 39/39; clippy clean.
- Perf nit fixed: `$HOME` canonicalization cached via `OnceLock` in `api.rs` (`canonical_home()`), mirroring `render.rs` `empty_grid()`. Tests re-run green.
- Docs: all §10 READMEs + architecture.md accurate; `wolf-core/README.md` gap fixed; `prettier --write` clean (proseWrap: never).
- REMAINING (not automatable): manual GUI smoke on a window server — launch the app, split panes, confirm each pane shows a top statusbar matching the titlebar bg, filename + smaller path, text left-aligned to grid text, inactive panes dimmed.

### Reviewer outcome: APPROVED, no blockers, no correctness bugs. All §1–§7 verified correct.
- should-fix: `crates/wolf-core/README.md` stale (missing `buffers` field, `setup_buffer_tracking`, `pane_status`) — routed to technical-writer.
- nit: `WolfCore::pane_status` canonicalizes `$HOME` per pane per repaint (`api.rs:484-485`) — routed to coder for a OnceLock cache (branch is perf-focused).
- nit: symlinked-prefix uncanonicalized new-buffer edge (mirrors existing current_file) — awareness only, no action.

### Coder outcome
- All 4 steps implemented per plan. `//crates/wolf-core:pane_status_unit` (9 cases), `//crates/wolf-core:pane_buffers` (real nvim), `//crates/wolf-ffi:pane_status` (real nvim), `//crates/wolf-macos:WolfKeyMapTests` all PASS. Broad `bazelisk test //crates/...` = 39/39. Clippy clean. `wolf_macos_lib` builds (compile-verifies PaneContainerView).
- No contract mismatches: `active_grid_id()`, `Snapshot.windows`, `theme_titlebar_fg()` all exist. No test edits needed. Bindings regenerated cleanly via gen_bindings.
- Docs updated by coder (§10) + prettier-clean. (technical-writer to double-check.)
- Diagnostics after: Lua `vim` undefined-global (pre-existing, whole file) + SourceKit `No such module 'WolfFFI'` (headless artifact; Bazel builds fine) — both non-issues.

### Test files (test-writer, done)
- `crates/wolf-core/tests/pane_status_unit.rs` (9 pure cases)
- `crates/wolf-core/tests/pane_buffers.rs` (real nvim; panics loud when nvim bin unset, matching wolf-core convention)
- `crates/wolf-ffi/tests/pane_status.rs` (real nvim; loud SKIP via `common::nvim_bin_or_skip`)
- `crates/wolf-macos/Tests/WolfKeyMapTests/StatusbarLayoutTests.swift` (headless; pins lineFactor=1.25, exact height 37 for (13,10,3,1))

### API contracts the tests pin (coder MUST satisfy)
- `wolf_core::{pane_status, PaneStatus, NO_NAME}` re-exported; `PaneStatus` has PUBLIC fields `filename`/`display_path`.
- `WolfCore::pane_status(&self, grid: u64) -> PaneStatus`; windows with no file ABSENT from `Snapshot.buffers` → returns NO_NAME/"".
- `Snapshot` exposes `windows` (keyed by u64 grid) and `active_grid_id()` (already used by redraw_multigrid.rs — VERIFY it exists).
- `WolfCoreHandle::pane_status_for_grid(&self, grid: u64) -> FfiPaneStatus` with `.filename()`/`.display_path()`.
- `StatusbarLayout` uses `ceil(fontSize*1.25)` lineFactor. If coder changes the factor, update the two pinned Swift tests.

## Problem statement

Add a statusbar strip at the TOP of each split pane in the macOS AppKit front-end.

### Requirements
- Statusbar spans the full pane width, at the top of each pane.
- Background matches the existing macOS titlebar (themed `WolfTitlebar` bg when set, else the titlebar's own fallback).
- Primary line: BASE FILENAME of the pane's buffer file, or the buffer's name if no file (`[No Name]`).
- Below it, SMALLER text: path — cwd-relative if inside cwd, else absolute; `~` prefix if under home (absolute case only).
- Left edge of statusbar text aligns with the pane's grid text left edge (respect `PaneGeometry.paintOffsetX`).

### Scope
- macOS only (`crates/wolf-macos`). GNOME out of scope. Shared plumbing (nvim/core/ffi) may change.

### Constraints
- Docs are source of truth. Prefer real-nvim e2e/integration tests; pure logic gets unit tests; Swift painting is compile+smoke.
- Markdown: no mid-paragraph hard breaks; `prettier --write` after edits. Build: bazelisk (macOS) / cargo (core).

---

## 1. Verified facts (file:line)

- Global-only current file today: `Snapshot.current_file: Option<PathBuf>` (`render.rs:478`), fed by `apply_current_file` (`redraw.rs:274-289`) via `_G.wolf_setup_current_file_tracking` (`init.lua:180-190`, emits `expand('%:p')` on BufEnter/BufWinEnter/BufFilePost).
- `WinInfo` carries `win: i64`, keyed in `Snapshot.windows` by grid id (`render.rs:407-425`, `:461`). Rebuilt from redraw events (`win_pos` `redraw.rs:695-717`; `win_viewport` `:755-759`) → attaching `buf_name` to `WinInfo` is fragile; buffer notification is keyed by WIN id → sibling map keyed by win is robust.
- cwd is a sibling cell (`redraw.rs:56` `TrackedCwd`, `apply_cwd` `:387-409`), read via `WolfCore::live_cwd` (`api.rs:452-454`), already canonicalized.
- `win` handles decode via `ext_handle` accepting plain Integer or Ext (`redraw.rs:455-466`). Lua `nvim_list_wins()` returns plain ints → join to `WinInfo.win`.
- setup-fn wiring: each `setup_*` in `nvim.rs` looks up chan via `nvim_get_api_info`, calls `wolf_setup_*(...)` via `nvim_exec_lua` (`nvim.rs:218-249`); called in order in `api.rs:240-244`.
- Notify dispatch: `RedrawHandler::handle_notify` (`redraw.rs:163-224`); apply then `(self.wake)()` after lock drops. current_file/active_win/theme wake; cwd does not.
- FFI: `read_frame_for_grid` (`lib.rs:1062-1069`); `windows()` maps snapshot windows via `FfiWinInfo::from_win_info` projecting {grid,win,hidden} (`:1120-1131`, `:1796-1828`). `theme_titlebar_bg()`/`theme_titlebar_fg()` exist (`:1313-1332`) → `FfiColor{rgb,set}` (transparent struct `:264-269`). Opaque types allow Vec + accessor methods.
- swift-bridge extern block for `WolfCoreHandle`: `lib.rs:328-426`; opaque decls `FfiWinInfo` `:645-654`. Bindings regenerated by `bazel run //crates/wolf-ffi:gen_bindings` (`src/bin/gen_bindings.rs`), rewrites `generated/wolf-ffi.{swift,h}`; must be committed.
- `dirs` already a wolf-core dep (`Cargo.toml:26`) → `dirs::home_dir()`. Existing relative convention `path.strip_prefix(&cwd)` (`mru.rs:162-167`).
- PaneContainerView geometry (`PaneContainerView.swift`): bottom cmdline strip (`:169-172`); `paneRects()` top `paneAreaHeight` (`:182-185`); `geometry(for:rect)` → `PaneGeometry` (`:191-199`), call sites: mouseDown `:266`, mouseDragged `:280`, mouseUp `:289`, scrollWheel `:335`, syncGridSizes `:396`, paintGrid `:567`. `paintGrid` fills+translates by `rect.minX+geo.paintOffsetX` then dims whole pane (`:566-593`). Grid text left = `rect.minX+paintOffsetX`. `applyTitlebarTheme` `:427-434`. `draw(_:)` `:535-558`.
- `PaneGeometry.swift`: `paintOffsetX = gridRect.x - paneRect.x`, centering cap at maxCols (`:52-73`); `cellAt` clamps out-of-grid points to nearest edge cell (`:82-90`) → strip points map to row 0, acceptable.
- Titlebar fallback: `MainWindow.applyTitlebarBackground` sets `window.backgroundColor` to themed color when set, restores pristine `backgroundColor` when unset (`MainWindow.swift:128-142`) → **`window.backgroundColor` always equals the titlebar bg** → statusbar reads it for an exact match.
- `CellMetrics.font: CTFont` (`CellMetrics.swift:28`); `CTFontGetSize(metrics.font)` = grid point size. View `isFlipped` (`:161`) → top-left NSString drawing upright.
- CORRECTION to problem statement: statusbar bg need not re-derive the titlebar fallback; read `window.backgroundColor` (kept in sync by `applyTitlebarBackground`), with `handle.theme_titlebar_bg()` as explicit fallback when no window.

## 2. Core storage decision

Add sibling map on `Snapshot`: `buffers: HashMap<i64, PathBuf>` (win handle → canonicalized buffer path). Wholesale-replaced on each re-emit (stale/closed windows vanish); joins to `WinInfo.win`; updated WITHOUT bumping `seq` (like `current_file`). Unnamed windows absent from map.

## 3. New notification `wolf_buffers`

One arg: msgpack array of `[win_id, buf_path]` pairs for ALL windows in current tabpage. `win_id` plain int; `buf_path` = `nvim_buf_get_name(buf)` (absolute for file buffer, `""` unnamed). Re-emitted wholesale on each autocmd; core clears+rebuilds map each emit. Initial one-shot emit at setup. Firing autocmds: BufEnter, BufWinEnter, BufFilePost, WinEnter, WinNew, WinClosed.

## 4. Pure display-string function

**Location**: new `crates/wolf-core/src/pane_status.rs`, exported from `lib.rs`.

```
pub const NO_NAME: &str = "[No Name]";
pub struct PaneStatus { pub filename: String, pub display_path: String }
pub fn pane_status(buf_path: &str, cwd: Option<&Path>, home: Option<&Path>) -> PaneStatus
```

Logic (precedence):
1. Empty `buf_path` → `{ filename: NO_NAME, display_path: "" }`.
2. Else `path = Path::new(buf_path)`:
   - `filename` = `path.file_name()` lossy; if `None`, fall back to full `buf_path`.
   - `display_path`:
     - cwd-relative WINS: `cwd=Some(c)` and `path.strip_prefix(c)` OK AND remainder non-empty → remainder lossy. (`path==cwd` → empty remainder → fall through.)
     - else absolute+`~`: `home=Some(h)` and `path.strip_prefix(h)` OK → `"~"` if remainder empty (path==home) else `"~/"+remainder`.
     - else full `buf_path`.
   Caller passes already-canonical cwd/home/buf_path so `strip_prefix` lines up. `~` only in absolute branch.

## 5. FFI surface

Decision (b): new `WolfCoreHandle` method + small opaque return type.
- wolf-core `api.rs`: `pub fn pane_status(&self, grid: u64) -> PaneStatus`. Body: read snapshot; find `WinInfo` with `grid==grid`, get `win`, look up `snapshot.buffers.get(&win)` → lossy or `""`; DROP guard; read `self.live_cwd()`; read `dirs::home_dir()` (canonicalize, fall back to raw); call `pane_status(&buf, cwd.as_deref(), home.as_deref())`. Drop read guard before `live_cwd()` (mirror `mru_recent` `api.rs:409-417`).
- wolf-ffi `lib.rs`: opaque `FfiPaneStatus { filename: String, display_path: String }` with `filename()`/`display_path()`; `pub fn pane_status_for_grid(&self, grid: u64) -> FfiPaneStatus` on `WolfCoreHandle` (empty status when core gone). Bridge decls: `fn pane_status_for_grid(&self, grid: u64) -> FfiPaneStatus;` in `WolfCoreHandle` extern block (near `windows()`/`theme_titlebar_bg()`); new `extern "Rust" { type FfiPaneStatus; fn filename(&self)->String; fn display_path(&self)->String; }` near `FfiWinInfo`.
- Regenerate bindings, commit `generated/wolf-ffi.{swift,h}`.
- Statusbar fg reuses existing `handle.theme_titlebar_fg()` — no new color FFI.

## 6. macOS painting

### 6.1 New pure seam (WolfKeyMap), testable
`crates/wolf-macos/Sources/WolfKeyMap/StatusbarLayout.swift`, Foundation-only, Double-based:
```
public struct StatusbarLayout {
    public let height, primaryLineTop, primaryLineHeight, secondaryLineTop, secondaryLineHeight: Double
    public init(primaryFontSize: Double, secondaryFontSize: Double, verticalPadding: Double, lineGap: Double)
}
```
`primaryLineHeight ≈ ceil(primaryFontSize*lineFactor)`, `secondaryLineHeight ≈ ceil(secondaryFontSize*lineFactor)` (lineFactor const e.g. 1.25); `primaryLineTop = verticalPadding`; `secondaryLineTop = verticalPadding + primaryLineHeight + lineGap`; `height = secondaryLineTop + secondaryLineHeight + verticalPadding`.

### 6.2 PaneContainerView edits — which rect each site uses
- Add constants: statusbar font sizes from `CTFontGetSize(metrics.font)` (primary=base, secondary=`max(9, base*0.8)`), verticalPadding, lineGap, text left inset.
- Add `statusbarHeight: CGFloat` = `CGFloat(StatusbarLayout(...).height)`.
- Add `statusbarRect(ofPane:) -> CGRect` = top strip `(rect.minX, rect.minY, rect.width, statusbarHeight)`.
- Add `gridRect(ofPane:) -> CGRect` = `rect` with `minY += statusbarHeight`, `height = max(0, rect.height - statusbarHeight)`.
- Change every `geometry(for: rect)` → `geometry(for: gridRect(ofPane: rect))`:
  - mouseDown `:266` — pane hit-test stays on full pane rect; `cellAt` uses grid rect.
  - mouseDragged `:280`, mouseUp `:289` — `paneRect(forGrid:)` returns full pane rect; `cellAt` grid rect.
  - scrollWheel `:335` — hit-test full pane rect; `cellAt` grid rect.
  - syncGridSizes `:392-404` — size grid from grid rect → nvim gets REDUCED rows.
- `draw(_:)` loop: per `(id,rect)` call new `paintPane(frame, grid:, paneRect: rect, dimmed: id != activePane, ctx:)`.
- Refactor `paintGrid`: param becomes GRID rect; REMOVE internal inactive-dim (moves to paintPane). Keeps fill grid-default-bg over grid rect, translate by `gridRect.minX+geo.paintOffsetX`/`gridRect.minY+geo.paintOffsetY`, paint cells+cursor.
- New `paintPane`: saveGState; clip to paneRect; `paintStatusbar(into: statusbarRect, grid:, ctx:)`; `paintGrid(frame, into: gridRect, ctx:)`; if dimmed fill whole paneRect with `inactiveDim` scrim; restoreGState. (Dim statusbar WITH pane for consistency.)
- New `paintStatusbar(into strip:, grid:, ctx:)`:
  - Bg: fill strip with `(window as? MainWindow)?.backgroundColor` cgColor; fallback resolve `handle.theme_titlebar_bg()` via `WolfColor.cgColor` else `WolfColor.fallbackBg`.
  - Text left: `geo = geometry(for: gridRect(ofPane: paneRect))`; `textLeft = paneRect.minX + geo.paintOffsetX`. Clip text to strip.
  - Two line rects from `StatusbarLayout` (primary/secondary tops+heights), `x: textLeft, width: strip.maxX - textLeft - trailingInset`.
  - Primary: `handle.pane_status_for_grid(grid).filename()`, `NSFont.systemFont(ofSize: primary)`, primary color.
  - Secondary: `.display_path()`, `NSFont.systemFont(ofSize: secondary)`, secondary color.
  - Colors: primary = `theme_titlebar_fg()` if set (via WolfColor) else `NSColor.labelColor`; secondary = titlebar fg @~0.7 alpha if set else `NSColor.secondaryLabelColor`.
  - Draw via `(text as NSString).draw(in:withAttributes:)` with `.byTruncatingTail` paragraph style.
- `drawScrollbars`: compute track from `gridRect(ofPane: rect)` not full pane rect.
- `drawDividers`: keep full pane rects (span whole pane incl. strip). No change.
- `paintCmdlineStrip`: unchanged. `applyMetrics`: unchanged in shape (statusbarHeight derives from metrics; font-size change reflows via syncGridSizes).
- Every pane (active+inactive) gets its own statusbar.

## 7. Files to create
- `crates/wolf-core/src/pane_status.rs`
- `crates/wolf-core/tests/pane_status_unit.rs`
- `crates/wolf-core/tests/pane_buffers.rs` (real nvim)
- `crates/wolf-ffi/tests/pane_status.rs` (real nvim)
- `crates/wolf-macos/Sources/WolfKeyMap/StatusbarLayout.swift`
- `crates/wolf-macos/Tests/WolfKeyMapTests/StatusbarLayoutTests.swift`

## 8. Files to modify
- `runtime/nvim/init.lua` — add `_G.wolf_setup_buffer_tracking(chan)` mirroring current-file tracker; iterate `nvim_list_wins()` → `[win, nvim_buf_get_name(buf)]`, emit on listed autocmds + one-shot.
- `crates/wolf-core/src/render.rs` — add `pub buffers: HashMap<i64, PathBuf>` to `Snapshot` (default empty); doc as out-of-band, no seq bump; note wholesale re-emit handles staleness.
- `crates/wolf-core/src/redraw.rs` — `const BUFFERS_EVENT: &str = "wolf_buffers";`; match arm calling `apply_buffers(args).await` then `(self.wake)()`; `async fn apply_buffers` (build map before write guard; clear+rebuild; canonicalize non-empty keyed by `ext_handle(win)`; skip empty; no seq bump).
- `crates/wolf-core/src/nvim.rs` — `pub async fn setup_buffer_tracking(&self)` mirroring `setup_current_file_tracking`.
- `crates/wolf-core/src/api.rs` — call `proc.setup_buffer_tracking().await?;` after `setup_current_file_tracking`; add `pub fn pane_status(&self, grid) -> PaneStatus`.
- `crates/wolf-core/src/lib.rs` — `pub mod pane_status;` + `pub use pane_status::{pane_status, PaneStatus, NO_NAME};`.
- `crates/wolf-ffi/src/lib.rs` — `FfiPaneStatus` + accessors; `pane_status_for_grid`; bridge decls.
- `crates/wolf-ffi/generated/wolf-ffi.{swift,h}` — regenerated (do not hand-edit).
- `crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift` — §6.2 edits.
- `crates/wolf-ffi/BUILD.bazel` — `rust_test` `pane_status` (mirror `panes`).
- `crates/wolf-core/BUILD.bazel` — `rust_test` `pane_buffers` (mirror `theme.rs`) + `pane_status_unit` (mirror `palette_rank`).

## 9. Test plan

Pure unit (`pane_status_unit.rs`): empty→([No Name],""); file under cwd nested→(x.md, sub/x.md); direct under cwd→(x.md,x.md); not under cwd but under home→~/...; path==home→~; under neither→absolute; path==cwd→NOT empty relative (absolute or ~); cwd-and-home file→cwd-relative wins; cwd=None,home=None→absolute passthrough.

Core integration (`pane_buffers.rs`, real nvim, mirror `redraw_multigrid.rs`/`current_file.rs` poll): open temp .md → poll `pane_status(active_grid)` until filename+display match; `:vsplit` + `:e` different file B → poll until two grids report DISTINCT filenames (proves per-window); unnamed startup buffer → filename `[No Name]`, path ""; close one split → closed win drops from `buffers`, survivor keeps its file.

FFI integration (`pane_status.rs`, real nvim, mirror `panes.rs`): after attach with temp .md, poll `pane_status_for_grid(grid).filename()`/`.display_path()`; `:vsplit`+second file → grids' filenames differ; loud SKIP when `WOLF_NVIM_BIN` unset.

Swift headless (`StatusbarLayoutTests.swift`): height == expected arithmetic; `secondaryLineTop>primaryLineTop`; heights positive; `primaryLineTop+primaryLineHeight+lineGap==secondaryLineTop`; zero-padding + small-font edges.

Swift painting: compile + manual smoke only.

## 10. Docs to update
- `runtime/nvim/README.md` — `wolf_setup_buffer_tracking` + `wolf_buffers` (payload, autocmds, one-shot).
- `crates/wolf-core/src/README.md` + `crates/wolf-core/README.md` — `pane_status` module + `Snapshot.buffers` + `WolfCore::pane_status`; `setup_buffer_tracking` in nvim-setup list; `wolf_buffers`/`BUFFERS_EVENT` handler.
- `crates/wolf-core/tests/README.md` — `pane_status_unit`, `pane_buffers`.
- `crates/wolf-ffi/README.md` + `crates/wolf-ffi/src/README.md` — `FfiPaneStatus` + `pane_status_for_grid`; gen_bindings note.
- `crates/wolf-ffi/tests/README.md` — `pane_status`.
- `crates/wolf-macos/README.md` + `crates/wolf-macos/Sources/README.md` — per-pane statusbar (top strip reserved, grid inset, `StatusbarLayout` seam, bg from `window.backgroundColor`, text left-aligned via `paintOffsetX`, dimmed w/ inactive).
- `crates/wolf-macos/Tests/README.md` — `StatusbarLayoutTests`.
- `architecture.md` — narrative: `wolf_buffers`→`Snapshot.buffers`→`pane_status`→FFI `pane_status_for_grid`→macOS statusbar; GNOME out of scope.
- Grep first; `prettier --write`; no mid-paragraph breaks.

## 11. Implementation order (never un-buildable)
1. wolf-core pure+storage: `pane_status.rs` (+lib exports); `Snapshot.buffers`; `pane_status_unit`. cargo build + test pure.
2. wolf-core plumbing: `BUFFERS_EVENT`+`apply_buffers`+dispatch; `setup_buffer_tracking`; `api.rs` call + `WolfCore::pane_status`; Lua `wolf_setup_buffer_tracking`; `pane_buffers` test. Build+test.
3. wolf-ffi: `FfiPaneStatus`+`pane_status_for_grid`+bridge; `gen_bindings`+commit; `pane_status` test + BUILD target. Build+test.
4. wolf-macos: `StatusbarLayout.swift`+tests first; then `PaneContainerView.swift`. Swift build + WolfKeyMapTests.
5. Docs + `prettier --write`.

## 12/13. Deps + lint
No new deps (`dirs` already present; AppKit/CoreText imported). swift-bridge `mod ffi` already blanket-allowed (`lib.rs:258`). Build map before write guard in `apply_buffers`; drop read guard before `live_cwd()` in `pane_status`. Prefer extracting statusbar painting to an extension over a SwiftLint disable if file-length flagged.

## Notes / open items for verification during coding
- Confirm `theme_titlebar_fg()` exact name/signature at `lib.rs:~1324`.
- Confirm `WOLF_NVIM_BIN` skip helper name in `crates/wolf-ffi/tests/common`.
- Confirm exact `rust_test` target patterns in both BUILD.bazel files before mirroring.
