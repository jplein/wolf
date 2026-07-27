# Plan: Merge latest `main` (pane-dividers) into `feat/preview-anonymous-files`

## Task

Merge the current `main` (`2145703`) into `feat/preview-anonymous-files`
(currently at the verified merge commit `9cf46b1`), resolve any conflicts, then run
an `/orchestrate` verification pass: the app BUILDS, all LINTERS pass, all TESTS
pass. Then commit the merge.

`main` is 3 commits ahead of HEAD — the **pane-dividers** feature:
`2145703` Merge origin/main into feat/macos-pane-dividers; `5f8becb` Pane dividers
can be resized, save panes in app state; `03a4647` Resize panes by mouse. This
brings in: mouse pane-divider resizing, full pane-layout persistence, new source
files (`DividerDragSeam.swift`, `DividerHitTest.swift`, `PaneLayoutCodec.swift`),
new Rust tests (`state_layout.rs`, `rebuild_layout.rs`), schema changes in
`wolf-core/src/state.rs`, and **regenerated FFI bindings**
(`crates/wolf-ffi/generated/wolf-ffi.{swift,h}` — the bridge surface changed on
main's side: `push_leaf(path, preview, weight)`, `FfiPaneNode.is_preview()`).

## Known conflicts (from an earlier aborted attempt of this same merge)

Textual auto-merge left exactly 3 conflicts:
1. `.claude/current-plan.md` — throwaway coordination file (orchestrator resolves).
2. `crates/wolf-ffi/src/lib.rs` — ONE region: the `use wolf_core::{...}` import list.
   HEAD lacks `PaneNode, PaneOrientation`; main adds them. Our side changed only
   doc comments (no bridge/import change), so the resolution is main's SUPERSET
   import list (keep `PaneNode, PaneOrientation`). Verify there is no other seam.
3. `crates/wolf-macos/README.md` — ONE region: prose. Union BOTH features' text —
   keep our `⌘N` → `Untitled N.md` (`wolf_new_untitled`) description AND main's
   pane-divider/multi-window/persistence prose. No content dropped.

Everything else auto-merged (generated bindings auto-merged as `M `/staged, but see
CRITICAL below).

## CRITICAL — regenerate FFI bindings

The generated `crates/wolf-ffi/generated/wolf-ffi.{swift,h}` were TEXT-auto-merged.
Main changed the bridge surface; our side did NOT. Before committing, the bindings
MUST be regenerated from the merged Rust source and confirmed to match (byte-diff)
the auto-merged result — a bridge/codegen mismatch would break the macOS build.
The coder must regenerate (the repo's swift-bridge codegen step) and reconcile.

## Watch for semantic (non-textual) merge breaks

- Our feature added Neovim-side Lua helpers + `input.rs` `NewBuffer`/`Chdir` arms +
  the current-file tracker naming the startup buffer `Untitled.md`. Main's pane
  layout/persistence + rebuild_layout may interact (e.g. `leafPaths()` /
  `pane_status` / restored layouts now seeing a named `Untitled.md` startup buffer,
  or `rebuild_layout` re-`:edit`ing it). The coder must run BOTH families' test
  suites and root-cause any failure as a real interaction vs. a stale expectation.
- `api.rs`/`nvim.rs` seam: main may have changed `block_on_rpc` Send/'static bounds
  or `self.nvim` borrowing; our feature's new `input.rs` RPC helpers must still
  compile against the merged core (keep `Ref`/`RefCell` out of `.await`).

## Success criteria

Merged tree: workspace build (incl. macOS app) ✓; full Rust suites
(`//crates/wolf-core/...`, `//crates/wolf-ffi/...`) + Swift `WolfKeyMapTests` ✓,
0 failures, both feature families' tests run; ALL linters GREEN — clippy,
rustfmt_core, rustfmt_ffi, swift_format (rustfmt is REQUIRED, not advisory — fix any
debt scoped to the crate library sources, NOT the test files). Do NOT weaken tests.
Regenerated bindings committed together with the merge.

## Workflow

1. [orchestrator] Perform `git merge --no-ff main`; resolve the 3 conflicts
   (plan file by orchestrator; lib.rs + wolf-macos/README.md guidance to coder).
2. [coder] Resolve lib.rs (import superset) + wolf-macos/README.md (prose union);
   regenerate FFI bindings + reconcile; build; full test suite (both families);
   all 4 linters. Fix semantic merge breaks minimally. Report.
3. [test-writer] Only if a real coverage gap / stale expectation needs a test change
   (route from coder findings).
4. [reviewer] Verify the merge resolution is faithful (no dropped/duplicated content,
   no marker residue, bindings correct, both features intact).
5. [technical-writer] Sync any docs the merge left inconsistent.
6. [orchestrator] Commit the merge; report.

## Status

Merge performed; 3 conflicts resolved (plan by orchestrator; lib.rs import superset
+ wolf-macos/README.md prose union by coder). FFI bindings regenerated → BYTE-
IDENTICAL to the auto-merge (auto-merge was correct). Build (incl. macOS app) ✓;
clippy + rustfmt_core + rustfmt_ffi + swift_format ✓; 73/74 tests pass, both feature
families ran.

ONE semantic-merge failure (predicted by the watch list):
`crates/wolf-core/tests/rebuild_layout.rs::rebuild_perpendicular_tree_with_empty_leaf`
(assertion ~line 221, doc ~184-186/219). `wolf_rebuild_layout` leaves an empty-path
leaf showing the window's INHERITED buffer; on main that startup buffer was
`[No Name]`, but our `VimEnter` naming (`wolf_name_untitled_current`) now names it
`Untitled.md`. Test asserts `NO_NAME`.

DECISION (orchestrator): this is the CORRECT, intended merged behavior — our feature
deliberately eliminates bare `[No Name]` startup buffers, so an empty-leaf pane
inheriting `Untitled.md` is right. NOT an impl bug (forcing `[No Name]` would need a
gratuitous `:enew` contradicting the feature). Stale-expectation test update, same
class as the first merge's `current_file.rs`. TEST-WRITER: update the assertion at
~line 221 to expect basename `Untitled.md` (robust `ends_with`/basename match) and
fix the doc comment (~184-186, 219) to describe the empty leaf inheriting the named
`Untitled.md` startup buffer. Leave the other two subtests
(`rebuild_vertical_split_of_two_files`, `rebuild_editor_and_preview_of_same_markdown`)
unchanged. Then coder re-runs `:rebuild_layout` + full suite; reviewer; tech-writer;
commit.
