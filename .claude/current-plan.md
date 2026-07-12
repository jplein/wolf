# Plan: Native command-line UI + message toasts

## Status: COMPLETE ✅

## Finalization

Technical-writer (agent id: a089b4e62f6ee6992) synced all docs: `architecture.md` (Decision 7
realized; attach flags; `Snapshot.cmdline`/`pum`/`toast` contract + handled/ignored events +
msg_show fold + toast-clear-on-input; macOS/gnome phase status; deferred-list pruned), the
wolf-core/wolf-ffi/runtime/wolf-macos/wolf-gnome READMEs (+ src/tests READMEs), and
`completions-decision-report.md` (Option C shipped; hybrid marked SUPERSEDED). Documented two
as-built deviations from the plan: macOS `MainWindow` keeps one row of vertical slack for the
overlays (not a literal removed strip), and GTK KEEPS its bottom strip (now painted from
`Snapshot.cmdline`/`toast`, popup/multiline deferred per OD4). Orchestrator ran `prettier
--write` on all 12 touched Markdown files → all already conformant (`proseWrap: never`).

Minor follow-up logged for the user (NOT a blocker): macOS `ToastView` resets its scroll offset
only when the line COUNT changes, not on same-length content change (cosmetic, manual-smoke path).

## Problem statement

Wolf currently attaches to Neovim with `ext_linegrid` + `ext_multigrid` + `rgb`,
and paints the command line / messages from the outer grid (grid 1) as a
single-row "cmdline strip" at the bottom of the editor area. We are moving both
the command line and messages to Neovim's native `ext_*` UI protocol.

This applies to BOTH front-ends: macOS (`wolf-macos`, via `wolf-ffi`) and GTK
(`wolf-gnome`, consuming `wolf-core` directly). Core + FFI changes are shared.

### Feature 1 — Autocomplete (Option C: fully native, Tab-triggered)

- Add `ext_popupmenu` + `ext_messages` to the UI attach in
  `crates/wolf-core/src/nvim.rs` (`ui_attach`, ~line 130). `ext_messages`
  implicitly enables `ext_linegrid` + `ext_cmdline` and sets `cmdheight=0`
  (confirmed in the bundled nvim `api-ui-events.txt:49`).
- Neovim owns match state + Tab-cycling; keys flow through the normal key path
  unchanged. We do NOT use `getcompletion()` (the old hybrid plan in
  `completions-decision-report.md` is superseded by Option C).
- Fold `cmdline_show` / `cmdline_pos` / `cmdline_hide` into a new
  `Snapshot.cmdline` field; fold `popupmenu_show` / `popupmenu_select` /
  `popupmenu_hide` into a new `Snapshot.pum` field, in
  `crates/wolf-core/src/redraw.rs`. `popupmenu_show` carries `grid=-1` when
  `ext_cmdline` is active, with `col` = byte offset into the cmdline text — the
  popup anchors to the cmdline.
- `runtime/nvim/init.lua` must `set wildoptions+=pum` so cmdline completion
  presents as a vertical popup rather than the horizontal wildmenu.

### Feature 2 — Message toast

- Fold `msg_show` into a new `Snapshot.toast` field (kind + content chunks). Also
  render `msg_history_show` (so `:messages` still works) into the same surface.
- Suppress kind `search_count`. For real prompts (`confirm` / `return_prompt`),
  pass keys through rather than dismissing.
- Dismiss the toast on the next user action (key or click) — NO timer. A new
  `msg_show` is NOT a user action; it replaces/updates the current toast (honor
  `replace_last` / `id`).

### Placement (front-ends)

- Native command line at the BOTTOM of the editor area (where the strip is now).
  Completion popup renders directly ABOVE the cmdline, anchored at the byte col.
- The current bottom cmdline strip becomes dead (grid 1 empties, cmdheight=0) and
  is replaced by the native cmdline widget + toast overlay.
- Message toast appears BOTTOM-LEFT, adjacent to the command line. NOT
  cursor-anchored. Height capped at ~10 lines.
- Toast scroll exemption (decision #1): mouse-wheel/trackpad scroll WHILE
  HOVERING the toast is the single user action that does NOT dismiss it; every
  other key/click dismisses.

### FFI

Bridge the new cmdline / pum / toast snapshot state over swift-bridge
(`crates/wolf-ffi`) following the existing opaque-handle + accessor pattern, for
macOS. GTK consumes `wolf-core` directly. Regenerate committed bindings after any
bridge change.

### Testing (prefer real-nvim integration tests over mocks)

- Core, real nvim: `:` then `e Fi` via `send_key` in a temp dir with
  `File A.md` / `File B.md` / `Something else.md`; assert `Snapshot.cmdline`
  (firstc `:`, content `e Fi`), Tab → `popupmenu_show` folded into
  `Snapshot.pum` containing the `File *` entries; cmdline clears on `<Esc>`.
- Core, real nvim: `:Inspect` / `nvim_echo` kind=list_cmd multi-line → assert
  `Snapshot.toast` captures content; `:messages` → `msg_history_show` content;
  toast clears on next input event.
- FFI: extend the macOS real-nvim FFI test to read cmdline / pum / toast across
  the bridge.

### Docs (source of truth)

Update `architecture.md` decision 7 + deferred lists (`ext_cmdline` +
`ext_popupmenu` + `ext_messages` become realized; the bottom cmdline strip is
replaced); affected `src/README.md` files; reconcile
`completions-decision-report.md` (Option C, not hybrid). `prettier --write` any
edited Markdown; no hard line breaks within paragraphs.

## Constraints

- Documentation is source of truth; keep every touched dir's README +
  `architecture.md` in sync.
- Markdown: no hard line breaks within a paragraph; run `prettier --write` after
  editing.
- wolf-core's one hard rule: no GUI toolkit in its dependency tree.
- macOS builds wolf-core/wolf-ffi via Bazel; regenerate committed swift-bridge
  bindings after any bridge change (`bazel run //crates/wolf-ffi:gen_bindings`).
- Prefer e2e/integration tests that spawn a REAL pinned nvim; requires the flake
  devShell so `WOLF_NVIM_BIN` is set.
- FFI discriminant contracts are APPEND-ONLY (new bridge surface here is additive
  accessors, no discriminant renumbering).

## Orchestrator rulings on the architect's open questions (BINDING — all agents follow these)

- **OD1 — ACCEPT the chunk asymmetry.** cmdline is flattened to a single `text()`
  String across the FFI; the toast keeps STYLED chunks resolved in Rust against the
  `HighlightTable` (message color is semantically load-bearing — emsg red, warnings,
  list highlights — decision 6). Core keeps chunk structure internally for both.
- **OD2 — ACCEPT.** Expose pum `word`/`kind`/`menu`/`info` over the FFI; the macOS
  popup shows `word` primarily with `kind` as a small trailing hint; `menu`/`info`
  unused this pass.
- **OD3 — ACCEPT.** The macOS toast is a dedicated `NSView` subview (`ToastView`) so
  `scrollWheel` can hit-test its frame for the scroll-over-toast exemption.
- **OD4 — ACCEPT the gnome scope.** wolf-gnome gets minimal-but-correct: the bottom
  strip painted from `Snapshot.cmdline` (+ the last toast line when the cmdline is
  idle, so grid 1 emptying never leaves it blank/stale). DEFER the Adw completion
  popup and the multi-line scrollable toast; document the deferral in the gnome README.
- **OD5 — ACCEPT.** Ignore `cmdline_block_*` in v1 (interactive `:function` blocks).
- **OD6 — ACCEPT.** Ignore `cmdline_special_char` in v1.
- **OD7 — ACCEPT.** Toast message `id`: handle the integer form only; a string id is
  treated as `0` (no replace-by-id).
- **OD8 — OVERRIDE the architect's rec #8: SCROLL DOES dismiss (except over the toast).**
  The user's exemption is specifically "scroll WHILE HOVERING the toast"; a scroll that
  is NOT over the toast is a deliberate user action and MUST dismiss. Mechanism that
  makes this clean and consistent: **core clears the toast on any sent `Key`, `Mouse`,
  OR `Scroll` input event** (still gated by `!toast.is_prompt()`); the scroll-over-toast
  exemption is realized entirely in the FRONT-END by NOT forwarding that particular
  wheel gesture to nvim (so no `Scroll` input reaches core → no clear). So: scroll over
  the editor → forwarded → dismisses; scroll over the toast → consumed by `ToastView` →
  never forwarded → toast stays and scrolls. Update the api.rs pump dismiss-scan to
  include `InputEvent::Scroll` alongside `Key`/`Mouse`.
- **OD9 — ACCEPT.** The native command line is a real dedicated `CmdlineView` subview
  (not painted into the flat `PaneContainerView`). PREFERENCE (soft, coder's call): have
  the cmdline widget + toast OVERLAY the bottom of the editor area only when active
  rather than permanently reserving a band, so the editor grid keeps its full
  `cols`×`rows` when no command line / message is showing (matches the `cmdheight=0`
  spirit). If overlay-vs-reserve causes layout thrash, a thin reserved band is an
  acceptable fallback — note whichever is chosen in the docs.

None of OD1–OD9 require re-consulting the user; they are consequences of the locked
decisions (OD8 is the one correction, made to honor the user's explicit scroll-over-toast
exemption more precisely than the architect's draft).

## Architect's plan (authoritative — implement against this + the OD rulings above)

### 1. Summary

Move Neovim's command line, completion popup, and messages off the grid onto the native
`ext_*` UI protocol. Core attaches with `ext_popupmenu` + `ext_messages` (the latter
implicitly enabling `ext_cmdline` and setting `cmdheight=0`), folds the new redraw events
into three new `Snapshot` fields (`cmdline`, `pum`, `toast`), and clears the toast on the
next dismissing input (Key/Mouse/Scroll — OD8). `wolf-ffi` gains additive opaque-handle
accessors for the three surfaces (toast chunks pre-resolved against the highlight table in
Rust, like `FfiCell`). `wolf-macos` replaces the dead grid-painted cmdline strip with a
native `CmdlineView`, a `CompletionPopupView` anchored above it, and a bottom-left
scrollable `ToastView`. `wolf-gnome` gets a minimal-but-correct cmdline (bottom strip from
`Snapshot.cmdline` + last toast line when idle), popup/multiline-toast deferred (OD4).
Realizes architecture decision 7's "going native later" path — additive to core.

### 2. Events handled vs ignored; fold semantics

**Attach flags** (`nvim.rs` `ui_attach`, ~line 130): add `ext_popupmenu=true`,
`ext_messages=true`. `ext_messages` implicitly sets `ext_linegrid`+`ext_cmdline` and forces
`cmdheight=0` → grid 1 empties, no message grid allocated → the front-ends' "paint the
strip from grid 1" path goes dead and must be replaced.

HANDLED (folded):
- `cmdline_show` `[content, pos, firstc, prompt, indent, level]` — set `cmdline.active=true`;
  parse `content` (`[attr_id, text, hl_id]` tuples) into `Vec<CmdlineChunk>`; store `firstc`,
  `prompt`, `pos`, `indent`, `level`.
- `cmdline_pos` `[pos, level]` — update `cmdline.pos` (+ `level`).
- `cmdline_hide` `[level, abort]` — `cmdline.active=false`; clear content/pos. (recursion
  level>1 collapsed to one surface — v1 limitation.)
- `popupmenu_show` `[items, selected, row, col, grid]`; item `[word, kind, menu, info]` —
  `pum.visible=true`; parse items; store `selected`,`row`,`col`,`grid`. `grid==-1` ⇒ anchor
  to cmdline, `col` a BYTE offset into cmdline text.
- `popupmenu_select` `[selected]` — update `pum.selected`.
- `popupmenu_hide` `[]` — `pum.visible=false`; clear items.
- `msg_show` `[kind, content, replace_last, history, append, id, trigger]` — fold into
  `toast` per rules below.
- `msg_clear` `[]` — `toast.messages.clear()`, `visible=false`.
- `msg_history_show` `[entries, prev_cmd]`; entry `[kind, content, append]` — clear toast,
  push one `Message` per entry (honoring `append`), `visible=true` (so `:messages` shows).

`msg_show` fold into `toast.messages: Vec<Message>`:
1. `kind=="search_count"` → suppress (return, no toast change).
2. `kind=="empty"` w/ empty content → clear toast, return.
3. split `content` text on `"\n"` into `lines: Vec<Vec<ToastChunk>>`.
4. `append==true` + current message exists → extend last message's last line with
   `lines[0]`, push remaining lines (`:echon`).
5. else `replace_last==true` → replace the last `Message` in place.
6. else `id!=0` and a visible message shares `id` → replace that message.
7. else push new `Message { kind, id, lines }`; `visible=true`.
`history` ignored (render regardless). `id` integer form only (OD7).

**Toast-clear-on-input (core contract):** in the `api.rs` input-pump task (has snapshot
access; `input.rs::apply` does not). Before `input::apply_batch`, if the batch contains any
`InputEvent::Key`/`Mouse`/`Scroll` (OD8) AND `!snap.toast.is_prompt()` (last kind !=
"confirm"), take the write lock, `snap.clear_toast()`, drop, then apply. Clear BEFORE apply
so a fresh `msg_show` from that same key repopulates afterward. `Resize`/`FocusWin`/`Open`/
tabpage events do NOT clear. Scroll-over-toast exemption is front-end (never forwards that
wheel → no `Scroll` event → no clear).

IGNORED (documented in the redraw ignore list, like `win_float_pos` today):
`cmdline_special_char` (OD6), `cmdline_block_show/append/hide` (OD5), `msg_showmode`,
`msg_showcmd` (showcmd off), `msg_ruler` (ruler off).

### 3. New types — `crates/wolf-core/src/render.rs`

Each derives `Clone, Debug, Default, PartialEq, Eq`:
- `CmdlineChunk { text: String, attr_id: u64 }`.
- `Cmdline { active: bool, firstc: String, prompt: String, content: Vec<CmdlineChunk>,
  pos: usize, indent: usize, level: u32 }` + `Cmdline::text(&self) -> String` (concat).
- `PumItem { word: String, kind: String, menu: String, info: String }`.
- `Pum { visible: bool, items: Vec<PumItem>, selected: i64, row: i64, col: i64, grid: i64 }`.
- `ToastChunk { text: String, attr_id: u64 }`.
- `Message { kind: String, id: i64, lines: Vec<Vec<ToastChunk>> }`.
- `Toast { visible: bool, messages: Vec<Message> }` + helpers `lines()` (flatten all
  messages' lines in order), `kind()` (last message's kind or `""`), `is_prompt()` (last
  kind == "confirm").
Add to `Snapshot`: `pub cmdline: Cmdline`, `pub pum: Pum`, `pub toast: Toast`. Add
`Snapshot::clear_toast(&mut self)`. Default derives cover construction; no `seq` bump
outside the redraw batch (folds run inside the batch write guard; `seq` bumps on `flush`).

### 4. Files to create

- `crates/wolf-core/tests/cmdline_pum.rs` — real-nvim: temp dir with `File A.md`/`File B.md`/
  `Something else.md`; `:` then `e Fi`; poll `cmdline` (active, firstc `:`, `text()=="e Fi"`);
  `Tab` → poll `pum` visible + items containing `File *`; `<Esc>` → cmdline inactive, pum
  hidden.
- `crates/wolf-core/tests/messages.rs` — real-nvim: `nvim_echo`/`:echomsg` multi-line
  `list_cmd` → assert `toast.visible` + `toast.lines()` content + `kind()`; `:messages` →
  `msg_history_show` populates toast; send a key → toast clears; a search → `search_count`
  suppressed.
- `crates/wolf-ffi/tests/cmdline_ui.rs` — real-nvim bridge (mirror `theme.rs`: `mod common`,
  SKIP-when-unset): `:e Fi` → `handle.cmdline()` (active/firstc/text/pos); `Tab` →
  `handle.pum()` (visible/selected/item_count + item `word()`); echo → `handle.toast()`
  (visible/kind/line count + chunk `text()`/`fg()`); key → toast cleared.
- macOS: `Sources/WolfMacOS/CmdlineView.swift`, `CompletionPopupView.swift`,
  `ToastView.swift` (all dedicated `NSView` subviews — OD3/OD9). Optionally pure seams in
  `Sources/WolfKeyMap/` (`CmdlineAnchor.swift` byte-pos→x, `ToastLayout.swift` line cap) for
  `WolfKeyMapTests` coverage; AppKit wiring stays compile+manual-smoke.

### 5. Files to modify

- `nvim.rs` `ui_attach` — add the two flags; update doc comment.
- `redraw.rs` — match arms + fold fns for all 9 handled events; shared
  `parse_content_chunks(&Value)` (elem[0]=attr id via `as_u64` default 0, tolerating the
  attrs-dict form; elem[1]=text) + `split_on_newline`; extend the module ignore-list doc.
- `api.rs` — clone snapshot into the pump; toast-clear-on-input scan (Key/Mouse/Scroll,
  OD8) before `apply_batch`.
- `input.rs` — optional `InputEvent::is_toast_dismissing()` helper (locality); no functional
  change.
- `runtime/nvim/init.lua` — `vim.opt.wildoptions:append("pum")`.
- `crates/wolf-ffi/src/lib.rs` — additive bridge decls (opaque `FfiCmdline`, `FfiPum`,
  `FfiPumItem`, `FfiToast`, `FfiToastLine`, `FfiToastChunk`) + `WolfCoreHandle::cmdline()/
  pum()/toast()`; hand-written impls (strict `///`+`#[must_use]` bar like `FfiPaneStatus`).
  `FfiCmdline`: active/firstc/prompt/text/pos/indent/level. `FfiPum`: visible/selected/
  anchor_grid/anchor_row/anchor_col/items(). `FfiPumItem`: word/kind/menu/info. `FfiToast`:
  visible/kind/lines(). `FfiToastLine`: chunks(). `FfiToastChunk`: text/fg/bg/special/flags
  — RESOLVED IN RUST against `HighlightTable` at pack time (like `FfiCell`). Then
  `bazel run //crates/wolf-ffi:gen_bindings`; commit refreshed `generated/wolf-ffi.swift`+`.h`
  (`wolf_ffi_wake.h` untouched — no new wake channel).
- `crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift` — delete
  `paintCmdlineStrip`/`paintStripRow` + their `draw(_:)` call; reconcile `stripHeight`/
  `paneAreaHeight`/`syncGridSizes` (prefer overlay-when-active, OD9); in `onRenderWake()`
  refresh the 3 overlays from `handle.cmdline()/pum()/toast()`; in `scrollWheel(_:)` hit-test
  the visible `ToastView` frame and forward the wheel to the toast (not nvim) when inside.
- `WolfWindowController.swift`/`MainWindow.swift`/`EditorSplitViewController.swift` — add +
  position the 3 subviews as bottom-pinned/anchored overlays of the editor area; refresh on
  each render wake.
- `crates/wolf-gnome/src/pane_view.rs` (+`app.rs` if needed) — rewrite `draw_cmdline`: paint
  `firstc + text` w/ caret at `pos` from `snap.cmdline` when active; when inactive paint the
  last `toast` line (so the emptied grid 1 never shows blank). Defer Adw pum + multiline
  toast (OD4), noted in the gnome README.

### 6. Test targets (match existing conventions)

- `crates/wolf-core/BUILD.bazel` — add per-test `rust_test` targets for `cmdline_pum` and
  `messages` mirroring the `theme`/`tabpages` entries (`srcs=["tests/common/mod.rs",
  "tests/<name>.rs"]`, `crate_root`, `tags=["no-sandbox"]`, nvim-tree + init.lua `data`,
  `WOLF_TEST_*` env). Cargo auto-discovers on Linux.
- `crates/wolf-ffi/BUILD.bazel` — add `//crates/wolf-ffi:cmdline_ui` cloned from the `theme`
  target (data = `//bazel/nvim:nvim_tree` + `//runtime/nvim:init.lua`; env
  `WOLF_TEST_INIT_LUA`/`WOLF_TEST_NVIM_DIR`; deps `[":wolf_ffi"]+all_crate_deps(normal_dev=
  True)`).
- macOS AppKit view wiring = compile + manual smoke; optional pure-seam tests in
  `WolfKeyMapTests`. GTK wiring = compile + manual smoke.

### 7. Implementation order (avoids compile breakage)

1. `render.rs` types + Snapshot fields + `clear_toast` (additive; still compiles).
2. `redraw.rs` folds + arms + parse helpers.
3. `api.rs` (+ optional `input.rs` helper) toast-clear-on-input.
4. `nvim.rs` attach flags (events now flow).
5. `init.lua` `wildoptions+=pum`.
6. Core tests + BUILD targets.
7. `wolf-ffi` bridge decls + impls → regenerate + commit `generated/`.
8. `wolf-ffi` test + BUILD target.
9. `wolf-macos` new views + strip removal + controller wiring (consumes regenerated bindings).
10. `wolf-gnome` `draw_cmdline` rewrite.
11. Docs + `prettier --write`.

### 8. Docs to update

`architecture.md` decision 7 (ext_cmdline/popupmenu/messages realized; grid strip replaced)
+ the Deferred/open list + the macOS/gnome phase-status paragraphs + the "strip truncates
multi-row prompts" v1-limitation bullet; `crates/wolf-core/src/README.md` (render/redraw/
input/api/nvim); `crates/wolf-ffi/README.md` + src doc (new accessors); `runtime/nvim/
README.md` (`wildoptions+=pum` + attach note); `crates/wolf-macos/Sources/README.md` +
`crates/wolf-gnome/src/README.md`/`README.md` (new views + gnome deferral); reconcile
`completions-decision-report.md` to Option C. `prettier --write` every edited Markdown; no
hard line breaks within paragraphs.

### Risks

1. `parse_content_chunks` must tolerate BOTH chunk forms (`cmdline_show` chunks may carry an
   attrs-dict in elem[0]; `msg_show` carries `attr_id` int + `hl_id`). Default attr id to 0
   when elem[0] is a map.
2. Overlay-vs-reserve for the bottom cmdline band (OD9) — coder picks; document the choice.
3. Toast chunk color resolution must use the SAME `HighlightTable` snapshot as `FfiCell`
   (resolve at pack time in Rust, never in Swift).

Architect agent id: a85fb816c876b071d

## Progress log

- (init) Problem statement written; invoking architect.
- Architect plan complete (above). Orchestrator rulings OD1–OD9 recorded (OD8 overrides the
  architect's scroll rec). Invoking test-writer.
- Test-writer complete. Files: `crates/wolf-core/tests/cmdline_pum.rs`,
  `crates/wolf-core/tests/messages.rs`, `crates/wolf-ffi/tests/cmdline_ui.rs` + BUILD
  targets (`//crates/wolf-core:cmdline_pum`, `:messages`, `//crates/wolf-ffi:cmdline_ui`).
  CODER MUST MATCH these test-assumed signatures (override the plan where they differ):
  * `Toast::lines(&self) -> Vec<Vec<ToastChunk>>` (OWNED, not `Vec<&Vec<…>>`) and
    `Toast::kind(&self) -> String` (tests compare `== "list_cmd"` — String or &str both
    work, but return `String` to match). `Cmdline::text(&self) -> String`.
  * `Snapshot.cmdline/pum/toast` pub fields with the plan §3 field names.
  * FFI `FfiToastChunk::fg() -> FfiColor` with pub `.set`/`.rgb` fields (resolved in Rust);
    `FfiCmdline::pos()` integer type unpinned (tests assert `> 0` only — pick `u32`).
  * Coder should add `pub use render::{Cmdline, CmdlineChunk, Pum, PumItem, Toast, Message,
    ToastChunk}` for the front-ends.
  Test-writer adaptations the coder/reviewer should know: (1) `replace_last` is covered
  indirectly by `consecutive_messages_do_not_accumulate` (typing `:` for the 2nd command
  is itself a dismissing key per OD8, so the toast shows only the latest) — a true
  same-keystroke `replace_last` isn't drivable headlessly; the coder must STILL implement
  the real `replace_last`/`append`/`id` fold semantics per plan §2 (they're exercised by
  real nvim messages, just not uniquely pinned by a keystroke test). (2) `search_count`
  suppression is a tolerant negative assertion. (3) filename assertions de-escape
  backslashes + use substring match. (4) `messages.rs` uses `nvim_echo(..., { kind =
  "list_cmd" })`; if the pinned nvim rejects the `kind` opt the test fails loudly — coder
  should verify the pinned 0.12.3 accepts it (it does per the bundled runtime). Test-writer
  added a local `start_core_in(cwd)` helper in `cmdline_pum.rs` (mirrors `file_search.rs`)
  because no `common` helper pins nvim's cwd. Test-writer agent id: a67922de552e8bf0e.
  Invoking coder.
- Coder complete (agent id: a2b4f80035ccd8b89). ALL tests green: `//crates/wolf-core:cmdline_pum`,
  `:messages`, `:all` 19/19; `//crates/wolf-ffi:cmdline_ui`, `:all` 26/26. macOS
  `bazel build //crates/wolf-macos:Wolf` PASS; bindings regenerated, `wolf_ffi_wake.h`
  byte-identical; wolf-core GUI-free. wolf-gnome `draw_cmdline` rewritten but
  COMPILE-UNVERIFIED on this host (no cargo/Bazel target — reviewer/CI cover it). Chose
  overlay-when-active layout (OD9 pref); macOS overlays wired as `PaneContainerView`
  subviews (not the controller). Pre-existing unrelated clippy error at
  `crates/wolf-ffi/src/lib.rs:1286` (`wolf_ffi_state_flush`, committed in 54a0a4d; newer host
  clippy flags `unnecessary_map_or`) — coder correctly did NOT touch it (the `is_none_or` fix
  needs Rust ≥1.82, risks the pinned build). ORCHESTRATOR RULING: leave line 1286 alone
  (out of scope, pre-existing, pinned-toolchain risk); surface to user in final report.
- Coder recommended 4 test additions; orchestrator ACCEPTS ALL 4 → test-writer 2nd pass:
  1. FFI: an `emsg`-kind message resolves a chunk `fg()` DISTINCT from the default fg
     (uniquely pins OD1 — toast keeps STYLED color, not flattened).
  2. Core: a direct fold unit test (construct a `Snapshot`, drive `msg_show`-shaped events
     through the fold path) deterministically covering `msg_show` steps 4–6 —
     `append` fusing onto the last line, `replace_last` in place, id-match replacement
     (currently only push + clear-on-input are observably covered).
  3. Core: a `confirm`-kind toast is NOT cleared by a passing input event (covers the
     `!is_prompt()` gate branch in `api.rs` / OD8).
  4. FFI or core: cmdline `pos` is a BYTE offset — type a multi-byte char and assert `pos`
     equals byte length, not char count (pins the caret contract both front-ends use).
  Invoking test-writer for these 4 only (do not touch the passing tests).
- Test-writer 2nd pass complete (agent id: a70ef8d4d2493ad04). Added: (#2) an inline
  `#[cfg(test)] mod tests` in `crates/wolf-core/src/redraw.rs` (SOURCE file gained a test
  module — runs under the crate's lib unit-test target) with `msg_show_append_fuses_onto_last_line`,
  `msg_show_replace_last_updates_in_place`, `msg_show_matching_id_replaces_that_message`
  (calls module-private `msg_show(&mut Snapshot, Vec<Value>)`); (#4)
  `cmdline_pos_is_a_byte_offset` in `cmdline_pum.rs` (types `:eé`, asserts `pos == 3`);
  (#3) `confirm_prompt_is_not_dismissed_by_input` in `messages.rs` (real `:call confirm()`);
  (#1) `emsg_toast_keeps_styled_color` in `cmdline_ui.rs` (emsg fg distinct from plain echo).
  No signature divergences. NOTE for coder: the redraw.rs unit-test module must be executed
  by the crate's lib unit-test target (`//crates/wolf-core:wolf-core` unit tests or the cargo
  `--lib` tests) — confirm it runs. Invoking coder to run the full suite.
- Coder 2nd run complete (agent id: a970a02a830607901). ALL green, NO app-code changes needed
  (impl already satisfied the 4 new tests). Redraw unit-test module runs via
  `//crates/wolf-core:wolf_core_lib_test` (75 unit tests incl. the 3 new `redraw::tests::msg_show_*`).
  `//crates/wolf-core:cmdline_pum`/`messages`/`all` 19/19; `//crates/wolf-ffi:cmdline_ui`/`all`
  26/26. Confirmed: `cmdline.pos = pos as usize` stores nvim's BYTE pos verbatim; fold steps 4–6
  correct; emsg styled color resolves; `!is_prompt()` gate holds. Pre-existing unrelated noise
  (ffi lib.rs:1286 clippy, state.rs:112 test warning) untouched. Invoking reviewer.
- Reviewer APPROVED (agent id: a8f264df3ba9f5fb9) — nice-to-haves only, no blockers/should-fix.
  Verified: all new render types + derives; the full msg_show fold steps 1–7 (incl. OD7
  integer-id-only) + `parse_content_chunks` tolerating both chunk forms; byte-offset
  `cmdline.pos`; ignore-list doc; folds inside batch guard, no out-of-band `seq` bump; pump
  clear BEFORE apply on Key/Mouse/Scroll (OD8) gated by `!is_prompt()`, lock dropped before
  `.await`; attach retains multigrid/rgb; GUI-free core; FFI toast colors resolved IN RUST
  like FfiCell, strict `///`+`#[must_use]`, no discriminant change, wake.h byte-identical;
  macOS 3 subviews + overlay-when-active + scroll-over-toast exemption + popup anchored above
  at byte col + 10-line cap; GTK draw_cmdline paints cmdline/last-toast-line (no blank).
  ORCHESTRATOR RULINGS on the 3 nice-to-haves — ALL DECLINED (no code change):
  * `return_prompt` absent from `is_prompt()` is CORRECT (not a msg_show kind; a single-key
    hit-enter is answered by the pass-through key while the toast clears; persistence is only
    needed for multi-key `confirm`). Plan-conformant.
  * ToastView scroll-offset reset-on-line-count (not content) — cosmetic, manual-smoke path,
    reachable only via the scroll-exempt gesture. Logged as a minor follow-up for the user.
  * all-empty non-`empty` content empty band — reviewer confirms unreachable. Declined.
  Invoking technical-writer for docs (source of truth).
