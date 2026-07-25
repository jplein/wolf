# Current Plan — Terminal defaults to full width

## Status: COMPLETE ✅

One-file code change (runtime/nvim/init.lua) + real-nvim test + docs. Tests green (ffi:terminal 2/2,
ffi:all 31/31). Docs prettier-clean. NOT committed (per standing instruction).

## Problem statement

The Terminal (the drawer pane, a third window type alongside editor and preview) should use
FULL WIDTH by default, instead of the ~90-char centered cap that all grid panes get today.

## Design (settled — reuses the just-built per-buffer full-width mechanism)

The terminal buffer is the shared, tagged `b:wolf_terminal` buffer created in
`_G.wolf_toggle_terminal()` (`runtime/nvim/init.lua` ~line 602). The drawer pane resolves its
width in the front-end via `full_width_for_win(binding.win)` exactly like an editor pane
(reviewer-confirmed: `fullWidth(forGrid:)` falls back to `full_width_for_win` for non-preview
panes). So defaulting the terminal buffer's `b:wolf_full_width = true` makes the drawer render
full-width through the EXISTING channel — NO Swift changes, and it stays consistent (the View-menu
checkmark correctly reads "on" when the terminal is focused; still user-toggleable).

### Changes

1. **`runtime/nvim/init.lua`, `_G.wolf_toggle_terminal()`**: when binding the terminal buffer into
   the drawer window (BOTH the freshly-created `:terminal` path AND the recreated/persisted-buffer
   path), set `vim.b[termbuf].wolf_full_width = true` so full-width is the terminal's default.
   Then, before/near the existing `wolf_emit_drawer(w)` call, add a guarded
   `if _G.wolf_emit_full_width then _G.wolf_emit_full_width() end` so the front-end learns the
   drawer window's full-width state immediately (the per-window `wolf_full_width` autocmds can
   otherwise fire BEFORE the var is set on fresh creation — `:terminal` enters the buffer at
   line 634 before line 635 sets the tag, so an explicit re-emit is needed for determinism).
   Do NOT change the toggle semantics — a user may still flip it via `:WolfFullWidth`.

### Test (real Neovim — mirror `crates/wolf-ffi/tests/terminal.rs` + `full_width.rs`)

Open the terminal (via the toggle path used by `terminal.rs`), then assert the terminal is
full-width by default: `handle.full_width_for_win(drawer_win) == true` (resolve the drawer window
the same way `terminal.rs` observes the terminal window/grid) and/or observe `b:wolf_full_width`
on the terminal buffer is true. Confirm a normal editor buffer is still `false` (default cap
unchanged). Reuse the poll helpers from the existing ffi tests.

### Docs

- `runtime/nvim/README.md`: note that `wolf_toggle_terminal` sets `b:wolf_full_width = true` on the
  terminal buffer (full-width default) and re-emits `wolf_full_width`.
- `full-width-decisions.md`: add the terminal-defaults-to-full-width decision (third window type).
- `architecture.md`: if the terminal/drawer or content-width section names the width behavior,
  note the terminal's full-width default.
- `prettier --write` all edited Markdown; no hard line breaks in paragraphs.

## Progress log

- Orchestrator: small-change plan written (Option A: default `b:wolf_full_width=true` on the
  terminal buffer). Invoking test-writer, then coder, then technical-writer.
- Test-writer DONE: `terminal_drawer_defaults_to_full_width` in `crates/wolf-ffi/tests/terminal.rs`
  (resolves drawer win from the DRAWER_OPEN intent; baseline editor=false, drawer=true, checkmark).
- Coder DONE: init.lua `wolf_toggle_terminal` sets `vim.b[termwin_buf].wolf_full_width = true` after
  binding the terminal buffer, and re-emits BOTH `wolf_emit_current_file()` (drives the checkmark via
  the separate wolf_current_file channel — necessary; only re-emitting full_width left the checkmark
  stale) and `wolf_emit_full_width()`. No Swift/Rust changes. Tests: ffi:terminal 2/2, ffi:full_width
  clean, ffi:all 31/31. Toggle semantics unchanged (still user-toggleable). Invoking technical-writer.
