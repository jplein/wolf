# Merge `feat/command-line-ui` into `main`

## Problem statement

We are merging the `feat/command-line-ui` worktree branch into `main`. `main`
already contains `feat/markdown-preview` (merged previously). The merge is in
progress in the working tree (`git merge --no-commit --no-ff feat/command-line-ui`)
and has 13 conflicted files. The two branches are almost entirely **additive**
relative to each other:

- **main / HEAD** brought the Markdown **preview** feature: `PreviewInfo`,
  `Snapshot.previews`, `Snapshot.markdown_ticks`, `current_filetype`,
  `current_is_preview`, and the FFI surface (`open_preview`, `edit_source`,
  `render_preview`, `preview_file`, `previews`, `FfiPreview`,
  `is_markdown_active`, `is_preview_active`).
- **feat/command-line-ui** brings the native **command line / completion popup /
  message toast** (`ext_cmdline` / `ext_popupmenu` / `ext_messages`): `Cmdline`,
  `CmdlineChunk`, `Pum`, `PumItem`, `Message`, `ToastChunk`, `Toast`,
  `Snapshot.cmdline` / `.pum` / `.toast`, the FFI `cmdline()` / `pum()` /
  `toast()` methods and their `FfiCmdline`/`FfiPum`/`FfiToast` opaque types, plus
  new Swift views `CmdlineView.swift`, `CompletionPopupView.swift`,
  `ToastView.swift` and the tests `cmdline_pum.rs`, `messages.rs`, `cmdline_ui.rs`.

**Because the two feature sets touch the same import lists, the same `Snapshot`
struct, the same FFI `extern` block, and the same `PaneContainerView`, they
collide — but almost every collision is resolved by KEEPING BOTH SIDES.** No
semantic content from either branch may be dropped.

## Goal

Resolve all 13 conflicts keeping both feature sets intact, regenerate the
swift-bridge FFI bindings, get the full Bazel test suite and all linters green,
commit the merge, and mark `feat/command-line-ui` done in
`worktree-merge-tasks.md`.

## Conflicted files & resolution strategy

### A. Additive Rust conflicts — KEEP BOTH SIDES (union)
1. `crates/wolf-core/src/lib.rs` (1 hunk) — re-export list. Produce the
   alphabetical **union** of both `pub use` lists (HEAD adds `PreviewInfo`; feat
   adds `Cmdline, CmdlineChunk, Message, Pum, PumItem, Toast, ToastChunk`).
2. `crates/wolf-core/src/redraw.rs` (1 hunk) — `use` import list. Union of both.
3. `crates/wolf-core/src/render.rs` (2 hunks) — (a) new struct type defs: keep
   HEAD `PreviewInfo` AND feat `CmdlineChunk`/`Cmdline`/`PumItem`/`Pum`/
   `ToastChunk`/`Message`/`Toast`; (b) `Snapshot` fields: keep HEAD
   `current_filetype`/`current_is_preview`/`previews`/`markdown_ticks` AND feat
   `cmdline`/`pum`/`toast`.
4. `crates/wolf-ffi/src/lib.rs` (3 hunks) — (a) import union; (b) `extern "Rust"`
   fn decls: keep both blocks; (c) impl methods: keep both. Watch for the shared
   trailing braces so the union is syntactically valid.

### B. Generated FFI — REGENERATE (do not hand-merge)
5. `crates/wolf-ffi/generated/wolf-ffi.h`
6. `crates/wolf-ffi/generated/wolf-ffi.swift`
   Resolve `src/lib.rs` first, then regenerate via
   `bazel run //crates/wolf-ffi:gen_bindings` (the tests document this exact
   flow), which overwrites the conflicted files with the correct union output.
   These are EXCLUDED from swift-format.

### C. Hard conflict — PaneContainerView.swift
7. `crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift` (2 hunks).
   `feat` reindented the whole file to **4-space**; `main`/HEAD is canonical
   **2-space** (default swift-format, no `.swift-format` file in repo). Do NOT
   accept the 4-space side. **Base = HEAD (2-space, includes preview code); port
   in feat's command-line/completion/toast additions, reindented to 2-space.**
   Whitespace-insensitive semantic delta saved at
   `scratchpad/panecontainer-semantic.diff` (625 lines) — the real additions are
   the three overlay subviews (`cmdlineView`, `completionPopupView`,
   `toastView`): their stored-property decls, `init` construction +
   `addSubview`, layout, and per-wake sync logic. Architect must enumerate every
   feat-only region to port. After porting, `swift-format` must pass.

### D. Documentation conflicts — KEEP BOTH, integrate prose
8. `architecture.md` (1 hunk)
9. `crates/wolf-core/src/README.md` (4 hunks)
10. `crates/wolf-core/tests/README.md` (1 hunk)
11. `crates/wolf-ffi/src/README.md` (1 hunk)
12. `crates/wolf-macos/README.md` (2 hunks)
13. `crates/wolf-macos/Sources/README.md` (2 hunks)
    Integrate both branches' documentation additions. Reformat each with
    `prettier --write` after resolving (per CLAUDE.md).

### E. Orchestrator-owned
- `.claude/current-plan.md` — this file (conflict already superseded).

## Verification
- `bazel test //...` (the suite starts real nvim processes — integration tests).
- All linters/formatters: clippy, rustfmt, swift-format (advisory Bazel targets
  added in recent commits), prettier for Markdown.
- No content from either branch dropped; both preview AND cmdline/pum/toast
  features fully present.

## Notes on team usage
- **test-writer: SKIPPED.** The merge already carries its own tests
  (`cmdline_pum.rs`, `messages.rs`, `cmdline_ui.rs` from feat + preview tests
  from main). No new tests are authored for a merge; verification is running the
  union of both suites. Revisit only if the coder finds a genuine coverage gap
  created by the resolution itself.

## Status log
- [done] Architect produced detailed per-hunk resolution spec (below).
- [done] Coder resolved all 13 conflicts, regenerated FFI bindings. `bazel test //...` = 58/58 PASS; `bazel test //tools/lint:check` = all 4 PASS (clippy, rustfmt_core, rustfmt_ffi, swift_format). Swift sanity greps confirmed. Coder decisions: WolfKeyMap README count set to actual tree union (23); reverted pre-existing rustfmt drift in unrelated test files (out of merge scope). Orchestrator declined the optional new "coexistence" test (merges author no new tests; both feature sets independently covered & green).
- [done] Reviewer APPROVED — all 6 areas PASS, no content dropped from either branch, all shared-brace closures correct, Swift 2-space with both feature sets coexisting.
- [done] Technical writer verified all READMEs + architecture.md match the merged implementation; no doc changes needed.
- [in progress] Orchestrator committing merge + marking worktree done.

---

# DETAILED RESOLUTION SPEC (from architect — follow literally)

## A. Additive Rust conflicts — exact merged text

### 1. `crates/wolf-core/src/lib.rs` (1 hunk) — `pub use render::{…}` union:
```rust
pub use render::{
    Cell, Cmdline, CmdlineChunk, Cursor, CursorShape, DefaultColors, Grid, HighlightTable, HlAttr,
    Message, ModeInfo, PreviewInfo, Pum, PumItem, ResolvedStyle, Rgb, SharedSnapshot, Snapshot,
    ThemeColors, ThemeGroup, Toast, ToastChunk, Viewport, WinInfo,
};
```

### 2. `crates/wolf-core/src/redraw.rs` (1 hunk) — `use crate::render::{…}` union (note: has `ToastChunk` but NOT `Toast`; keeps `PreviewInfo`):
```rust
use crate::render::{
    Cell, Cmdline, CmdlineChunk, CursorShape, DefaultColors, HlAttr, Message, ModeInfo, PreviewInfo,
    Pum, PumItem, Rgb, SharedSnapshot, Snapshot, ThemeColors, ThemeGroup, ToastChunk, Viewport,
    WinInfo,
};
```

### 3. `crates/wolf-core/src/render.rs` (2 hunks) — both "keep both, HEAD then feat", both with SHARED CLOSING BRACE:
- **Hunk 1 (~459–630):** Keep HEAD `PreviewInfo` struct block (~460–475) and ADD a `}` after `pub source_path: PathBuf,` to close it. Blank line. Then keep feat's whole block (~477–628: `CmdlineChunk`, `Cmdline`+impl, `PumItem`, `Pum`, `ToastChunk`, `Message`, `Toast`+impl). The existing single `}` (~630) now closes feat's `impl Toast`. Delete the 3 markers.
- **Hunk 2 (~682–724):** `Snapshot` fields. Keep HEAD's four (`current_filetype`, `current_is_preview`, `previews`, `markdown_ticks`) then feat's three (`cmdline`, `pum`, `toast`), then existing `pub seq`. Delete markers only. No shared-brace issue.

### 4. `crates/wolf-ffi/src/lib.rs` (3 hunks):
- **Hunk 1 (~47–62) import union:**
```rust
use wolf_core::{
    rank_buffers, rank_matches_top_n, rank_palette, BufInfo, BufferRow, Cmdline, Command,
    CommandMru, CoreConfig, CursorShape, FileSearch, FocusDir, Grid, HighlightTable, InputEvent,
    Key, KeyEvent, LayoutIntent, MouseAction, MouseButton, MouseEvent, PaletteRow, PaletteRowKind,
    PaletteWindow, PaneStatus, PreviewInfo, Pum, ResolvedStyle, Rgb, ScrollDirection, ScrollEvent,
    SplitOrientation, StateStore, TabState, Toast, WakeFn, WinInfo, WindowState, WolfCore,
    WolfState,
};
```
- **Hunk 2 (~481–556) extern decls, SHARED BRACE:** Inside the FIRST (main-handle) extern block, after `fn is_visual_mode(&self) -> bool;`, keep HEAD's 7 decls (`is_markdown_active`, `is_preview_active`, `edit_source`, `open_preview`, `render_preview`, `preview_file`, `previews`), THEN add feat's 3 decls (`cmdline`, `pum`, `toast`) — feat's 3 must be MOVED UP into the main block — then close main block with the `}` (~522). Then keep HEAD's new `extern "Rust" { type FfiPreview; … }` block (~524–541); the `}` (~556) closes it. Delete 3 markers.
- **Hunk 3 (~1911–2060) impl methods, SHARED BRACE:** Keep HEAD's 7 methods (~1912–2016) and ADD a `}` after `previews()`'s `.collect()`. Then keep feat's 3 methods (`cmdline`, `pum`, `toast`, ~2018–2059). Line ~2061 `}` closes feat's `toast()`; ~2062 `}` closes the `impl`. Delete markers.
- Rest of ffi lib.rs merged cleanly (FfiPreview struct, FfiCmdline/FfiPum/FfiToast opaque types) — no action.

## B. Generated FFI — REGENERATE (files 5 & 6):
Do NOT hand-merge `crates/wolf-ffi/generated/wolf-ffi.h` and `.swift`. After src/lib.rs resolves & compiles, run `bazel run //crates/wolf-ffi:gen_bindings` — overwrites both, clears their conflict markers. Excluded from swift-format.

## C. `PaneContainerView.swift` — reconstruct, don't pick sides
feat reindented whole file 4-space; HEAD is canonical 2-space. Procedure:
1. `git show HEAD:crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift > <path>` — clean compiling 2-space file with all preview code.
2. Port feat's additions (below), reindented 4→2. Use feat's side in the ORIGINAL conflicted file (between `=======` and `>>>>>>>`, ~lines 398–439 and ~522–1261) as authoritative for exact bodies. Semantic diff at `scratchpad/panecontainer-semantic.diff`.
3. Run swift-format fix at end.

**C.1 feat-only ADDITIONS (locate by HEAD anchor):**
1. Three stored props after `lastTitlebarTheme`, before init: OD9 comment + `private let cmdlineView: CmdlineView`, `private let completionPopupView: CompletionPopupView`, `private let toastView: ToastView`.
2. In `init(handle:metrics:tabpageHandle:)`, after `self.controllerTabpageHandle = tabpageHandle` before `super.init`: `self.cmdlineView = CmdlineView(metrics: metrics)`, `self.completionPopupView = CompletionPopupView(metrics: metrics)`, `self.toastView = ToastView(metrics: metrics)`.
3. Same init, after `wantsLayer = true` before NotificationCenter observer: feat comment + `addSubview(toastView)`, `addSubview(cmdlineView)`, `addSubview(completionPopupView)` (order matters, popup last).
4. In `applyMetrics(_:)` after metric applied to panes: `cmdlineView.applyMetrics(newMetrics)`, `completionPopupView.applyMetrics(newMetrics)`, `toastView.applyMetrics(newMetrics)`, then `refreshOverlays()`.
5. In `scrollWheel(with:)` after `point` computed, before pane-forward loop: feat's `if !toastView.isHidden, toastView.frame.contains(point) { … toastView.scrollBy(deltaY:) ; return }` (OD8).
6. New `private func refreshOverlays()` (reads `handle.read_frame_for_grid(Self.cmdlineGridId)`, resolves default fg/bg, then `handle.cmdline()`/`pum()`/`toast()`, updates the 3 views, lays them out along bottom). Place near HEAD's `syncPreviewWebViews()`.
7. New `private func pumAnchorX(pum: FfiPum, cmdline: FfiCmdline) -> CGFloat` helper.
8. In `draw(_:)` where HEAD calls `paintCmdlineStrip(ctx)`: replace with feat's 3-line comment.

**C.2 feat MODIFICATIONS/REMOVALS (adopt feat — OD9 supersedes HEAD grid strip):**
9. REMOVE `stripHeight` computed prop; change `paneAreaHeight` from `max(0, bounds.height - stripHeight)` to `bounds.height`. Grep: zero `stripHeight` refs after.
10. REMOVE `paintCmdlineStrip(ctx)` call in `draw(_:)` (→ comment from #8).
11. REMOVE methods `paintCmdlineStrip(_:)` and `paintStripRow(_:row:stripRect:ctx:)` entirely.
12. Add `refreshOverlays()` after EACH `syncPreviewWebViews()` call (HEAD calls it at 2 sites: active-pane refresh + end of `onRenderWake`). Also honor 2 pure-add `refreshOverlays()` sites in resize/wake-refresh path (semantic diff 592a491, 598a498).
13. Doc-comment prose: adopt feat's wording for `paneRects()` doc (drop "cmdline strip … stripHeight" clause) and the grid-1 sizing comment ("under ext_messages it empties / cmdheight=0").

**C.3 HEAD-only KEEP (already present from pristine HEAD — confirm survive):** `PreviewPlacementInputs`/`EditPlacementInputs` structs + `previewPlacementInputs()`/`editPlacementInputs()`; preview maps (`previewWebViews`, `previewDimOverlays`, `lastPreviewLoads`, `previewNavDelegates`, `previewFocusHandlers`, `previewSourcePaths`, `pendingPreviewFragments`, `previewNamesByGrid`, `previewDisplayPathsByGrid`, `previewReloadCoalescer`); methods `syncPreviewWebViews`, `reloadDirtyPreviews`, `ensurePreviewWebView`, `focusPreviewPane`, `applyPreviewDim`, `removePreviewWebView`, `previewSourcePath`, `scrollPreview`, `previewFile`, `previewDidFinishLoad`; the `Preview: <name>` statusbar override; drawScrollbars web-view suppression (`if previewNamesByGrid[grid] != nil { continue }`); `import WebKit`. The 3 feat view files (`CmdlineView`/`CompletionPopupView`/`ToastView`.swift) already exist — just confirm compile.

## D. Documentation conflicts (keep both prose; `prettier --write` each after):
- **8. `architecture.md` (~65–69) DIVERGENT:** base = feat's paragraph (has 3-overlay + scroll-over-toast wording); re-insert HEAD's Preview + Edit Source menu items after the Buffer Picker `⌘E` entry.
- **9. `crates/wolf-core/src/README.md` (4 hunks) MIXED:** H1(~59–67) keep HEAD `PreviewInfo` bullet + feat 3 bullets; merge the two `Snapshot {…}` signature lines into one union in struct order (`… theme, current_filetype, current_is_preview, previews, markdown_ticks, cmdline, pum, toast, seq }`). H2(~75–86) keep HEAD's 4 field bullets + feat's cmdline/pum/toast bullet; merge divergent `seq` bullet (out-of-band preview updates DON'T bump seq; cmdline/pum/toast fold IS in redraw batch so DOES) and merge Accessors bullet (`is_markdown_active`/`is_preview_active` + feat `clear_toast()`). H3(~189–193) NvimProcess: adopt feat `ui_attach` (adds ext_popupmenu+ext_messages+cmdheight=0) but re-insert HEAD's `setup_preview()` + `render_preview/open_preview/edit_source/preview_file`. H4(~348–352) api.rs: base HEAD paragraph + insert feat's toast-clear-on-input (OD8) sentence.
- **10. `crates/wolf-core/tests/README.md` (~38–42):** additive union — HEAD preview/markdown test files + feat `cmdline_pum.rs`/`messages.rs`/`cmdline_ui.rs`.
- **11. `crates/wolf-ffi/src/README.md` (~9–17) DIVERGENT:** opaque types = HEAD `FfiPreview` + feat `FfiCmdline`/`FfiPum`/`FfiPumItem`/`FfiToast`/`FfiToastLine`/`FfiToastChunk`; `read_frame_for_grid` adopt feat wording (grid 1 default fg/bg overlays paint against); WolfCoreHandle methods union (HEAD preview methods + feat `cmdline()`/`pum()`/`toast()`).
- **12. `crates/wolf-macos/README.md` (2 hunks) DIVERGENT largest:** both hunks (~87–92, ~129–133) — base = feat (OD9 overlay reality) + re-insert HEAD preview prose (live Markdown preview View>Preview, preview-pane statusbar/click-to-focus `PreviewClickFocusHandler`/`PreviewSeam.clickFocusJS`, Edit-command paragraph). Command-palette count: union = HEAD's higher count with Preview+Edit Source listed. Drop HEAD's stale "bottom cmdline strip" sentences in favor of feat overlay sentences.
- **13. `crates/wolf-macos/Sources/README.md` (2 hunks ~7–11, ~51–62):** likely pure-add module entries — HEAD `PreviewClickFocusHandler`/`PreviewNavigationDelegate` + feat `CmdlineView`/`CompletionPopupView`/`ToastView`. If enclosing sentence edited both sides, merge like README above.

## E. Implementation order (avoid compile breakage):
1. render.rs → 2. lib.rs re-exports + redraw.rs imports → 3. build/test core → 4. ffi/src/lib.rs (3 hunks) → 5. `bazel run //crates/wolf-ffi:gen_bindings` (resolves files 5–6) → 6. PaneContainerView.swift → 7. 6 doc files + `prettier --write` each → 8. formatters/linters → full test suite.

## F. Verification commands:
- `bazel run //crates/wolf-ffi:gen_bindings`
- `bazel test //...` (real-nvim integration; includes `//crates/wolf-ffi:cmdline_ui`, core `cmdline_pum`/`messages`, preview `:preview`/`:edit`)
- Linters (all `manual`/tagged): rustfmt `bazel test //tools/lint:rustfmt` / fix `:rustfmt.fix`; clippy `bazel test //tools/lint:clippy` (hard-fail `--config=lint`) / fix `:clippy.fix`; swift-format `bazel test //tools/lint:swift_format` / fix `:swift_format.fix`; all checks `bazel test //tools/lint:check`; all fixes `bazel run //tools/lint:fix`.
- `prettier --write` each resolved doc.
- Swift sanity greps: `stripHeight` & `paintCmdlineStrip` GONE; `refreshOverlays`/`cmdlineView`/`completionPopupView`/`toastView`/`syncPreviewWebViews` PRESENT.

## G. No new external packages. No new lint directives to author (all unioned symbols are used by one branch).
