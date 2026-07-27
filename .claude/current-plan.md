# Plan: Verify merge of `main` into `feat/preview-anonymous-files`

## Task

`main` was merged into `feat/preview-anonymous-files` (a real merge — the branches
had diverged: main brought 4 commits, headlined by `:q`/`:qa` → close active
pane/window; this branch had the `Untitled.md` anonymous-buffer + preview feature).
The merge auto-resolved all code; the only conflict was `.claude/current-plan.md`
(this throwaway coordination file), now resolved. Both features coexist in
`runtime/nvim/init.lua` (untitled helpers near the preview helpers; `:q`/`:qa`
`do_quit`/`WolfQuitAll` in `wolf_setup_layout` — different regions).

Now run an `/orchestrate` verification pass to confirm the MERGED tree:

1. **Builds** — the Rust core, FFI, and (where buildable) the macOS app, via bazelisk.
2. **Tests pass** — the full Rust test suites for `//crates/wolf-core/...` and
   `//crates/wolf-ffi/...` (real-nvim integration tests, bundled pinned nvim), and
   the Swift `//crates/wolf-macos:WolfKeyMapTests` target if buildable in this env.
   This is the union of both merged features' tests — the `Untitled` tests
   (`untitled.rs`, updated `preview`/`edit`/`full_width`/`code_block_highlight`/
   `file_actions`/`pane_buffers`/`browser_preview`/`nvim_restart`/`swap_recovery`)
   AND main's `:q`/`:qa` tests (`commands.rs`, `layout_intents.rs`).
3. **Linters pass** — clippy (Rust) and any configured lint/format checks.

## Merge context the verifier must know

- New CLAUDE.md rule (from main): every new test MUST be wired into a Bazel target
  (`BUILD.bazel` `rust_test` for Rust; `srcs` list for Swift `swift_test`) — Bazel
  does NOT auto-discover. Our `crates/wolf-ffi/tests/untitled.rs` already has a
  `//crates/wolf-ffi:untitled` target. Verify it is present and RUNS after the merge
  (main also touched `crates/wolf-core/BUILD.bazel`).
- Main touched `crates/wolf-core/tests/commands.rs`, `crates/wolf-core/tests/
  layout_intents.rs`, `crates/wolf-core/BUILD.bazel`, `build.md`,
  `command-lifecycle.md`, `CLAUDE.md` — all auto-merged. Confirm the core test
  targets still build and pass alongside our changes.

## Success criteria

Full build + full Rust test suite (both crates) green, and ALL linters pass —
clippy, swift-format, AND rustfmt (`rustfmt_core` + `rustfmt_ffi`). Per USER
DIRECTIVE, rustfmt is a REQUIRED linter (NOT advisory); pre-existing rustfmt debt
must be fixed. Do NOT weaken tests to pass.

## USER DECISION (rustfmt)

User: "Also fix the rustfmt debt and don't mark rustfmt as advisory - it's one of
the linters that needs to pass." Action: run rustfmt on the library sources so
`rustfmt_core`/`rustfmt_ffi` pass, KEEPING the fix SCOPED to the library sources
those targets actually check (`crates/wolf-core/src/{api,liveness,nvim,redraw}.rs`,
`crates/wolf-ffi/src/lib.rs`). Do NOT re-introduce the out-of-scope rustfmt churn
on `tests/*.rs` (rustfmt targets don't check tests; that churn was deliberately
reverted for a clean merge). Then confirm ALL linters + full test suite green, and
commit the merge.

## Status

Verification round 1: merged tree BUILDS (incl. macOS app), clippy + rustfmt +
swift-format PASS (coder ran the repo's official `//tools/lint:rustfmt.fix`, pure
formatting — kept). 71/72 test targets pass. ONE semantic-merge failure:

`crates/wolf-core/tests/current_file.rs::unnamed_buffer_has_no_current_file` —
main ADDED the `current_file` Bazel target (the file existed unwired on our tip);
its `unnamed_buffer_has_no_current_file` asserts the STARTUP buffer is unnamed →
`current_file == None`. Our feature deliberately names the startup buffer
`Untitled.md`, so it is now `Some(.../Untitled.md)`. Mutually exclusive with our
(passing, correct) `untitled.rs::startup_names_untitled_markdown_no_auto_write`.

DECISION (orchestrator): do NOT delete — the test's INTENT (current-file tracker
maps a genuinely UNNAMED buffer's empty `expand('%:p')` → `None`) is still valid
coverage. REWRITE it to drive a genuinely unnamed buffer via a raw `:enew`
keystroke sequence (bypasses `wolf_new_untitled`; only `VimEnter` auto-names, so a
post-startup `:enew` stays `[No Name]`), then poll `current_file` until it becomes
`None` and assert — mirroring the sibling fixes already made to
`pane_buffers::unnamed_buffer_reports_no_name` and
`browser_preview::non_markdown_buffer_returns_empty_url`. Then re-run
`//crates/wolf-core:current_file` + full suite; commit the merge once green.
