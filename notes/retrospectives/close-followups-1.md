# Retrospective: close follow-ups, wording, the leg (d) premise and the confirm smoke

Last verified at commit `1f453297307f2fb61ed1745cf2d136bee252a9fd`.

The task carried out recommendation 4 of [close-commands-2-4](close-commands-2-4.md) and the other leftovers of the close-commands-2 reviews, with no change of behaviour. Its commits are `71fc8c2` (tests and code comments) and `1f45329` (docs). It took five invocations and no round trip, about $10.80 and 36 minutes of agent time: `architect-1` $5.44 (89 turns), `test-writer-1` $1.63, `coder-1` $1.11, `technical-writer-1` $0.83 and `reviewer-1` $1.80.

## What went well

- **The instructions named every site by path and line.** They came from the review and retrospective findings of close-commands-2, which quoted the false sentences. Plan 1 could then give each downstream agent the exact text to write, so `test-writer-1`, `coder-1` and `technical-writer-1` together cost $3.57, under the architect alone.
- **A premise value came from an assertion, and the one that did not was marked.** Plan 1 §3 tied leg (d)'s expected `winnr('$')` to `compose.rs`'s asserted `IMCHORD_WINDOWS_SPLIT`, and §4 marked "the app exits 0 when its window closes over a modified buffer" as unmeasured. `coder-1` measured it in its first remote run before any falsifier. This is what recommendation 2 of [close-commands-2-4](close-commands-2-4.md) asks for, done by the plan although the architect definition does not yet say it.
- **The plan gave the gate's clippy form.** Plan 1 §3 told the test writer to use `cargo clippy --workspace --all-targets -- -D warnings`, and `test-writer-1` ran it in its second remote command. The `clippy::type_complexity` miss of [close-commands-2-4](close-commands-2-4.md) did not recur, though the test writer definition still offers only the narrower forms.
- **The smoke had nothing to wire.** `smoke_headless/main.rs` already declares `mod close_tab;`, and the new test went into that file, so there was no Bazel or Cargo target to add. The `RELAUNCH_FIXTURE_NAMES` table in `fixture_guards.rs` took the new readback file as one row beside its siblings.
- **Falsifiers ran in a remote snapshot.** `coder-1` made both disabling edits inside `regina on linux -- sh -c '…'` and showed the local files' checksums unchanged. Both reds came out as plan 1 §4 predicted (exit 222 and exit 30), and the review checked them against the command logs.
- **Both gates passed on their first run on both workers.**

## What did not go well

- **The plan was half the cost, mostly in tracing.** `architect-1` spent $5.44 and 13 minutes of the task's $10.80. Most of its 89 tool calls traced the confirm across `tab_view.rs`, `tabs_ui.rs`, `window.rs`, `api.rs` and `nvim.rs` to rule out a pure seam, and read `panes.rs` (3,273 lines) and `shell.rs` (2,417 lines) in ranges. This is the large-file finding of [sidebar-preview-3](sidebar-preview-3.md) recommendation 7, come up again.
- **Three agents searched for a free exit-code range.** `architect-1` grepped `exit 22[0-9]` across `smoke_headless/`, `test-writer-1` grepped `\b22[0-9]\b` again, and `reviewer-1` checked `CTN_EXITS` against `CT`, `CW` and `Q` by hand. No document or table lists the smokes' `*_EXITS` ranges, and none says they must not collide. They already do: `zoom_pane.rs`'s `OWN_EXITS` (`82..=95`) and `drawer_divider.rs`'s `RESIZE_OWN_EXITS` (`95..=101`) share `95`.
- **The done-condition grep was classified by hand three times.** `git grep -n 'close_window' crates runtime` gives 37 hits in 10 files. `architect-1` sorted them into allowed classes for plan 1 §3, and `coder-1` and `reviewer-1` each repeated the sort. A one-off sweep is cheap enough that nothing in the repository need change for it.
- **Three recommendations of [close-commands-2-4](close-commands-2-4.md) have not reached the definitions.** The test writer, architect and coder system prompts of this task carry none of recommendations 1 to 3. Here the plan made up for 1 and 2, and `coder-1` polled its background gates with an `until grep` loop, so nothing was lost.

## Recommendations

1. **Keep the smokes' exit-code ranges in one place.** Move every `*_EXITS` range of `crates/wolf-gnome/tests/smoke_headless/` into one table in `smoke_headless/main.rs`, or have `fixture_guards.rs` collect them, with a test that no two overlap. That would have spared `test-writer-1`'s and `reviewer-1`'s searches here, and it would catch the `95` that `zoom_pane.rs` and `drawer_divider.rs` share. It builds on recommendation 3 of [app-state-fixes-3](app-state-fixes-3.md), which asks for one table per file.
2. **Carry out recommendations 1 to 3 of [close-commands-2-4](close-commands-2-4.md).** This task shows they work when the plan states them, which is the cost the definitions would save every plan.
