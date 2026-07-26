# Plan: Forward OS window focus to Neovim (external-file reload on app refocus)

## Problem statement

Follow-up to the just-landed external-file-reload feature (`autoread` +
`WolfAutoRead` augroup running `:checktime` on FocusGained/BufEnter/CursorHold/
CursorHoldI in `runtime/nvim/init.lua`). That feature reloads a buffer whose file
changed on disk — and the user confirms it WORKS when the file is edited from
Wolf's own terminal drawer (going terminal-pane → editor-pane fires `BufEnter` →
checktime → reload+repaint, reliably).

It does NOT work when the file is changed by an app OUTSIDE Wolf and the user then
reactivates Wolf (e.g. clicks the Dock icon). Root cause (already established by
investigation): **no Wolf front-end forwards OS window focus to the captive
Neovim.** There is no `nvim_ui_set_focus` call anywhere; on macOS
`WolfWindowController.windowDidBecomeKey` (`WolfWindowController.swift:775`) only
marks the view current + rebinds panes; it never tells nvim its UI regained focus.
So `FocusGained` never fires in the running app, and the `FocusGained` entry in the
autocmd list is effectively dead.

Reproduction (macOS): launch Wolf; edit a file; change that file from an external
terminal; reactivate Wolf via the Dock. Actual: no reload. Expected: reload.

## Goal

When the Wolf app/window regains OS focus, tell the captive Neovim its UI gained
focus (`nvim_ui_set_focus(true)`), so `FocusGained` fires → the existing
`WolfAutoRead` autocmd runs `:checktime` → an externally-changed, unmodified buffer
reloads. Symmetrically report focus LOST on resign so nvim sees a real focus
transition each time (else nvim de-dupes and only the first refocus would fire).

Primary platform: **macOS** (the user's platform / repro). Keep the wolf-core +
wolf-ffi method front-end-agnostic so GNOME can wire the same call cheaply as a
follow-up.

## Design direction (to be refined by the architect)

- **wolf-core:** a new method (e.g. `WolfCore::set_ui_focus(&self, gained: bool)`)
  that calls `nvim_ui_set_focus(gained)` over RPC. `NvimProcess` already wraps
  `nvim_ui_attach` / `nvim_ui_try_resize` (see `crates/wolf-core/src/nvim.rs:146+`);
  add a sibling `set_focus`. Wire a public `WolfCore` API + an `InputEvent` variant
  or direct method, matching how existing UI ops (resize, focus_win) are exposed.
- **wolf-ffi:** expose the new method across the FFI bridge (mirror an existing
  no-payload-ish method like `focus_win` in `crates/wolf-ffi/src/lib.rs`), and
  regenerate/extend the generated header if the FFI uses cbindgen/generated bindings.
- **wolf-macos:** call the new FFI method with `true` on window-become-key
  (`WolfWindowController.windowDidBecomeKey`) and `false` on window-resign
  (add `windowDidResignKey`, or use app active/resign notifications — architect to
  choose the correct AppKit hook so a real transition is reported each refocus).

## Deliverables

1. wolf-core focus method + public surface.
2. wolf-ffi binding (+ generated header if applicable).
3. macOS front-end wiring (become-key → focus true; resign → focus false).
4. An integration test proving the chain end-to-end in REAL nvim: external change +
   `set_ui_focus(true)` → FocusGained → reload. (This also becomes a more FAITHFUL
   trigger for the autoread reload test than the CursorHold-idle proxy.)
5. Docs: `runtime/nvim/README.md` (the FocusGained caveat can now say focus IS
   forwarded on macOS), plus the relevant crate READMEs (`crates/wolf-core`,
   `crates/wolf-ffi`, `crates/wolf-macos`).

## Status

- [x] Architect plan
- [x] Tests written — autoread.rs retriggered via `refocus_ui` (`set_ui_focus(false)`→
      `(true)`); CursorHold-idle helpers deleted; module doc rewritten.
- [~] Implementation + tests pass — coder implemented input.rs (`SetUiFocus` variant +
      `set_ui_focus` helper → `nvim_ui_set_focus`), api.rs (`WolfCore::set_ui_focus`),
      wolf-ffi bridge decl + impl, regenerated `generated/wolf-ffi.{swift,h}`, macOS
      `Project.setUiFocus` + AppDelegate `applicationDidBecomeActive`/`WillResignActive`
      → `pushUiFocusToAllProjects`. VERIFIED: autoread Test 1 (real set_ui_focus trigger)
      **20/20** + 12/12; guard 20/20; swap_exists PASS; bridge_smoke PASS;
      `bazel build //crates/wolf-macos:Wolf` SUCCESS (FFI symbol resolves); rustfmt +
      clippy + swift_format PASS. Coder also updated crate READMEs (prettier clean).
      Test 2 setup race HARDENED (test-writer added `wait_for_row0_contains` polling for
      the `# Original` setup + `ZZUNSAVED` marker). Re-verified: autoread **40/40 each**,
      swap_exists PASS, rustfmt + clippy PASS. Flake gone. (SourceKit `No such module
      'WolfFFI'` diagnostics = editor-LSP noise; Bazel build produces that module + succeeded.)
- [x] Review — reviewer APPROVED all 8 areas. Confirmed: helper calls `nvim_ui_set_focus`
      with one bool (not toast-dismissing, no NvimProcess wrapper); `WolfCore::set_ui_focus`
      fire-and-forget; ffi bridge decl + impl mirror `focus_win`; generated Swift/.h contain
      the symbol and match the decl; `Project.setUiFocus`; AppDelegate wires BOTH edges
      (become-active→true, resign→false — the critical transition edge); `windowDidBecomeKey`
      untouched (no BufEnter regression); tests use `refocus_ui(false→true)`, CursorHold
      helpers gone, owned-locals discipline intact; init.lua + BUILD unchanged. Minor doc
      note: macOS `Sources/README.md` doesn't yet mention the focus wiring (crate-root
      README does) — hand to technical-writer.
- [x] Docs verified — technical-writer filled the one gap (macOS `Sources/README.md`
      now documents the AppDelegate focus fan-out + `Project.setUiFocus`, both-edges
      rationale, `windowDidBecomeKey` untouched) and verified all others accurate,
      including macOS-ONLY scoping in `runtime/nvim/README.md` (GNOME explicitly not-yet-
      wired). `prettier --write` clean on all edited docs.

## MERGE FROM MAIN (user-requested) + verification pass

- [x] Merged `origin/main` (abf6e2a) into `feat/auto-refresh` → merge commit `cb37351`.
      Incoming from main: code_block syntax highlighting in preview, sidebar tree-load
      fix, confirm-prompt/toast fix, plus changes to `nvim.rs`/`redraw.rs`/`render.rs`/
      `lib.rs`/`messages.rs`/ffi `BUILD.bazel` and new tests (code_block_highlight.rs,
      markdown_render.rs, messages.rs, PreviewSeamTests). 3 conflicts resolved:
      `.claude/current-plan.md` (took ours), and two macOS READMEs (`README.md` +
      `Sources/README.md`) — Project.swift bullet edited on BOTH sides; merged to keep
      main's expanded `onTreeWake()` (ALL-sidebars fan-out) AND our `setUiFocus` docs.
      `init.lua` (WolfAutoRead) + `Project.swift` (setBackground/setUiFocus/onTreeWake all
      coexist) auto-merged cleanly. No markers remain; prettier clean.
- [~] Verification pass on merged HEAD `cb37351` — OUR WORK GREEN, 2 inherited-from-main reds.
      GREEN: `//crates/wolf-macos:Wolf` builds+links (FFI `set_ui_focus` + all main changes
      compile together); `//crates/wolf-core:autoread` 30/30 post-merge; all main's new tests
      pass (markdown_render, messages, code_block_highlight, WolfKeyMapTests incl PreviewSeamTests);
      clippy + rustfmt_ffi + swift_format PASS. 61/62 targets green.
      (`bazel //...` can't parse `.worktrees/`; scoped to `//crates/...` + `//tools/lint:*` =
      whole real source tree.)
      RED (both proven PRE-EXISTING on origin/main, byte-identical, untouched by our commits):
      1. `//crates/wolf-ffi:frame_pack` E0451 build break — main's confirm fix made
         `Snapshot.pending_confirm` `pub(crate)` (lone non-pub of 22 fields); frame_pack.rs
         (external test) uses `..Snapshot::default()` FRU which needs all fields visible →
         origin/main itself doesn't build it. Fix options: (a) `pub(crate)`→`pub` on the field
         (1 word, consistent w/ other 21 pub fields) OR (b) edit the test to avoid FRU (keeps
         field crate-private).
      2. `//tools/lint:rustfmt_core` — only main's `redraw.rs` (new confirm unit tests)
         unformatted; advisory per build.md; fix = `bazel run //tools/lint:rustfmt.fix`.
      USER CHOSE: fix both, field→pub.
- [x] Verification GREEN. Fixes applied (uncommitted, on top of merge `cb37351`):
      1. `render.rs`: `pending_confirm` field AND `PendingConfirm` type `pub(crate)`→`pub`
         (the type too — clippy `private_interfaces` under `-D warnings` rejects a pub field
         of a pub(crate) type; pure visibility-widening, no `#[allow]`). Exposes
         `PendingConfirm` in wolf-core's public API — TRADEOFF flagged (alt: `#[allow(
         private_interfaces)]` on the field keeps the type internal).
      2. `redraw.rs`: rustfmt reflow of main's confirm unit tests (formatting only).
      RESULT: `bazel build //crates/wolf-ffi:frame_pack` + `//crates/wolf-macos:Wolf` build;
      `bazel test //crates/... --keep_going` = **62/62 pass**; all 4 lint gates (rustfmt_core,
      rustfmt_ffi, clippy, swift_format) PASS; autoread 10/10 post-fix. WHOLE TREE GREEN.
      Fixes NOT yet committed (awaiting user direction on commit).

## COMPLETE ✅ (pre-merge feature summary below)

Focus forwarding landed: on macOS, reactivating Wolf (Dock click) now fires nvim
`FocusGained` → `WolfAutoRead` `:checktime` → external-file reload. Chain: AppDelegate
`applicationDidBecomeActive`/`WillResignActive` → `pushUiFocusToAllProjects` →
`Project.setUiFocus` → ffi `WolfCoreHandle::set_ui_focus` → `WolfCore::set_ui_focus` →
`InputEvent::SetUiFocus` → `nvim_ui_set_focus(gained)`. Both edges wired (resign→false so
each refocus is a real transition). Proven: autoread Test 1 on the real `set_ui_focus`
trigger 40/40, guard 40/40, swap_exists + bridge_smoke PASS, `bazel build
//crates/wolf-macos:Wolf` SUCCESS, rustfmt+clippy+swift_format PASS. Reviewer APPROVED;
docs synced (macOS-only scope; GNOME follow-up noted).

### GIT STATE (needs user attention)
A subagent committed the FIRST feature as `4b453a3 "Reload changed files"` on a NEW local
branch `feat/auto-refresh` (branched off a main that had fast-forwarded to bda1603) —
WITHOUT the user asking to commit. Nothing pushed. This SECOND feature is left uncommitted
in the working tree. Orchestrator will not commit/push/reset without user direction.

### Remaining optional follow-up
GNOME focus forwarding (a GTK `notify::is-active` handler → the SAME `set_ui_focus` FFI
call) — the core/ffi method is front-end-agnostic and ready; only the GNOME wiring is
unbuilt. Also still available: the modified+externally-changed W12-prompt policy (à la
SwapExists) if a non-blocking modified-case is wanted.

---

## Architect's plan (authoritative)

### wolf-core (fire-and-forget, ordered input channel — NOT block_on_rpc; mirrors Resize)

**`crates/wolf-core/src/input.rs`:**
- Add `InputEvent::SetUiFocus { gained: bool }` after `ToggleFullWidth` (~after line 265),
  doc: `true`→nvim `FocusGained`, `false`→`FocusLost`. `is_toast_dismissing` (line 278)
  is a `matches!` over Key/Mouse/Scroll → new variant auto non-dismissing (correct; leave).
- `apply()` arm (near 431): `InputEvent::SetUiFocus { gained } => set_ui_focus(nvim, *gained).await,`.
- Private helper near `toggle_full_width` (~551):
  `async fn set_ui_focus(nvim: &Nvim, gained: bool) -> Result<(), Box<CallError>>` →
  `nvim.call("nvim_ui_set_focus", vec![Value::from(gained)]).await?; Ok(())`.
  (arity 1, single bool. Do NOT add a `NvimProcess::set_focus` method — inline like Resize
  calls `nvim_ui_try_resize` directly at input.rs:383-391.)
- Update the module doc (1-7) to list `nvim_ui_set_focus`.

**`crates/wolf-core/src/api.rs`:** add on `WolfCore` next to `focus_win` (~after 837):
`pub fn set_ui_focus(&self, gained: bool) { self.send_input(InputEvent::SetUiFocus { gained }); }`.
`WolfCore` already re-exported at crate root — no extra export.

### wolf-ffi (swift-bridge; committed generated/ tree)

**`crates/wolf-ffi/src/lib.rs`:**
- In the `#[swift_bridge::bridge] mod ffi` `WolfCoreHandle extern "Rust"` list, next to
  `fn focus_win(&self, win: i64);` (line 424): add `fn set_ui_focus(&self, gained: bool);`.
- In the hand-written `impl WolfCoreHandle` (after `focus_win` ~1690):
  `pub fn set_ui_focus(&self, gained: bool) { if let Some(core) = self.core.borrow().as_ref() { core.set_ui_focus(gained); } }`.
- **Regenerate:** `bazel run //crates/wolf-ffi:gen_bindings` → rewrites committed
  `generated/wolf-ffi.swift` (adds `public func set_ui_focus(_ gained: Bool)`) +
  `generated/wolf-ffi.h` (adds the C decl). Commit both. No BUILD change, no CARGO repin.
- `crates/wolf-ffi/README.md`: add `set_ui_focus` to the bridge-surface list; `prettier --write`.

### macOS (APP-LEVEL notifications — recommended over per-window; single clean transition)

**`crates/wolf-macos/Sources/WolfMacOS/Project.swift`** (next to `setBackground(dark:)` ~after 125):
`func setUiFocus(gained: Bool) { handle.set_ui_focus(gained) }`.

**`crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift`:**
- `func applicationDidBecomeActive(_:)` → `pushUiFocusToAllProjects(gained: true)`.
- `func applicationWillResignActive(_:)` → `pushUiFocusToAllProjects(gained: false)`.
- private fan-out next to `pushAppearanceToAllProjects` (~after 324):
  `private func pushUiFocusToAllProjects(gained: Bool) { for project in projects { project.setUiFocus(gained: gained) } }`
  (empty-projects launch no-op, like the appearance fan-out).
- Do NOT touch `windowDidBecomeKey` (WolfWindowController.swift:775 — keeps setCurrent +
  markRefocusRebind, the proven BufEnter path). New handlers are additive; no BufEnter regression.
- `crates/wolf-macos/Sources/WolfMacOS/README.md`: document the focus wiring; `prettier --write`.

Why app-level: `applicationDidBecomeActive` fires on Dock reactivation (the exact repro) and
gives ONE unfocused→focused transition. Switching between two Wolf windows of the SAME project
does NOT deactivate the app → no spurious false→true churn on the shared nvim (per-window
become/resign-key would). `applicationWillResignActive` fires only when the whole app loses
focus → the resign-while-sibling-window-key edge cannot occur.

### Integration test — `crates/wolf-core/tests/autoread.rs` (reuse `//crates/wolf-core:autoread`, no BUILD change)

**REPLACE Test 1's trigger** (delete `set_fast_updatetime` + `nudge_cursor_move` helpers +
their module-doc rationale; rewrite the module doc to describe the `set_ui_focus` trigger).
New `external_change_reloads_unmodified_buffer`:
1. `start_core_in` + open `note.md`; assert row0 has `# Original`.
2. sleep `PRE_REWRITE_SLEEP`; rewrite fixture to `RELOADED_DOC` (different byte length).
3. `waiter.drain()`.
4. **Trigger: `core.set_ui_focus(false)` THEN `core.set_ui_focus(true)`** — the `false` first
   forces a genuine unfocused→focused transition regardless of nvim's post-attach initial focus
   (nvim de-dupes FocusGained; a lone `true` may no-op). Models production resign→activate.
5. Poll reading `row_text(0)` / `is_prompt()` into OWNED locals (NEVER hold the snapshot
   read-guard across drain/sleep — the prior starvation bug), bounded by FLUSH_TIMEOUT, until
   `# Reloaded externally` appears; assert `!is_prompt()`.
**Keep guard test** `modified_buffer_is_not_reloaded_by_checktime`; switch ITS trigger to the
same `set_ui_focus(false)`→`(true)` (so both delete the CursorHold helpers). Keep all guard
assertions (`ZZUNSAVED` survives, `never applied` never present, per-iteration dismissing Enter).
No FFI/Swift focus test (headless AppKit focus is impractical — state so); wolf-core test is
load-bearing, `bazel build //crates/wolf-macos:Wolf` proves the Swift symbol resolves.

### Verification commands (coder)
- `bazel run //crates/wolf-ffi:gen_bindings` (regen + commit generated Swift/C before macOS build).
- `bazel test //crates/wolf-core:autoread` (reload + guard on the real set_ui_focus trigger).
- `bazel test //crates/wolf-ffi:bridge_smoke` (bridge still links/round-trips).
- `bazel build //crates/wolf-macos:Wolf` (new FFI symbol resolves in Swift).
- Lint: `cargo fmt --all -- --check`, `cargo clippy -p wolf-core -p wolf-ffi --all-targets`
  (or the repo's `//tools/lint:rustfmt` + `:clippy` bazel targets).

### Risks
- **Transition dedup (primary):** nvim fires FocusGained only unfocused→focused. Both edges
  wired (resign→false, activate→true); test forces false→true. Insulates from nvim's uncertain
  post-attach focus state.
- Multi-window same project: app-level fans one `true`/project; `:checktime` (no arg) checks ALL
  buffers → correct + idempotent; intra-app window switch sends nothing (advantage over per-window).
- Idempotent/harmless; unmodified-only reload (guard test proves); no hermetic-isolation interaction.
- `nvim_ui_set_focus` = stock nvim ≥0.9 (pinned is 0.12.3) — no version guard, no new deps.
