# Retrospective: one shared `quit_app` for the GNOME cage smokes

Last verified at commit `3ee7fe66b2a1306b20cd96727e93dd98e073e796`.

The task replaced every per-scenario `quit_app()` and inline end-of-run Vim quit in `crates/wolf-gnome/tests/smoke_headless/` with one `quit_app "<label>"` in `READINESS_PRELUDE`. It discards `modified`, quits through `:WolfQuitApplication`, and exits 241 or 242. It ran straight through, with no rejection, failed gate, discrepancy or resume. The five invocations cost $16.79 in about 64 minutes: `architect-1` $6.58 (75 turns), `test-writer-1` $5.01 (96), `coder-1` $1.40 (36), `technical-writer-1` $1.28 (36) and `reviewer-1` $2.52 (38).

## What went well

- **`READINESS_PRELUDE` already reached every scenario.** `cage_command` prepends it to each scenario, the packaged and pointer ones included, so plan 1 §1 needed no new support module and no wiring. After this task, a change to how the smokes quit is one edit in `main.rs`, where the request counted about 33 tool calls for the last such sweep.
- **The plan's two greps made the sweep checkable by anyone.** Plan 1 §8 gave a done-grep and a kept-sites grep with the exact lines each must print. `test-writer-1`, `coder-1` and `reviewer-1` each ran them in one command, and review 1 §1 cites them as its conformance check for table A and table B.
- **`regina on linux` carried the Linux-only work.** `coder-1` ran the 25-test green batch, all 17 falsifiers under F1 in three remote snapshots, the package smoke and a base reproduction of row 9's WebKit failure, without editing the local `shell.rs`. Review 1 §4 matched each red against those runs.

## What did not go well

- **Row 7's test could not fail on its quit.** `a_restored_top_line_survives_its_grid_resize_under_cage` checked its readbacks only through `extras.rs`'s `require`, which asserts that files exist and only prints the status. At the base, a hung or unclean quit, codes 152 and 153, still passed it. `test-writer-1` found this and added a final `status.success()` assertion. `a_zoomed_focused_drawer_restores_under_cage` in the same file has the same shape and no such assertion, and its chord quit was out of scope (plan 1 D6).
- **One scenario's exit codes were restated in another file, and the plan missed it.** `preview_render.rs`'s code-block smoke runs `PREVIEW_THEME_SCENARIO` verbatim and wrote out that scenario's codes 11 to 18 in its own failure message. Plan 1's table A did not list the file. `test-writer-1` caught it by grepping for the const, and rewrote the message. This is the scattering that recommendation 1 of [close-followups-1](close-followups-1.md) names, come up again.
- **Choosing two free exit codes took a scan of every smoke.** `architect-1` grepped all `exit` numbers and each candidate from 41 to 252 across `smoke_headless/*.rs` to settle D4. A single table of ranges would have answered it in one read.
- **The Linux gate ran twice.** Plan 1 §6 step 5 told `coder-1` to run `./gates.sh`, and its `regina on linux -- ./gates.sh` took about seven of its 25 minutes before Regina's implementation gate ran it again on both workers. [app-state-fixes-2](app-state-fixes-2.md) found the same, and the cost was wall time only.
- **The tests README stayed at the size ceiling.** `crates/wolf-gnome/tests/README.md` grew 602 bytes to 150,307, three times the `big-file` warning. Its `smoke_headless/main.rs` entry was at 243 of 250 words, so `technical-writer-1` cut three phrases to fit one sentence about the helper (248 words). This is recommendation 1 of [cli-open-2-4](cli-open-2-4.md), come up again.
- **Two minor slips cost nothing.** Plan 1 named row 7's const `RV_STEPS` where it is `RV_SCENARIO` (review 1 §1). `test-writer-1`'s scripted replacement left a doubled quote in `search_toggles.rs`, which its first `cargo fmt --check` on Linux caught before handoff.

## Recommendations

1. **Make `require` in `crates/wolf-gnome/tests/smoke_headless/extras.rs` fail on a non-zero status once every file exists.** It prints `status` but never asserts it, so an `extras.rs` test that writes its evidence before the quit passes on a hung or crashed quit. Row 7 needed a hand-added assertion here, and `a_zoomed_focused_drawer_restores_under_cage` still lacks one. Asserting it in the helper, or in a final call each test makes, closes the gap for every caller at once.
2. **Give `PREVIEW_THEME_SCENARIO` its exit-code text as a `pub(crate) const` in `preview.rs`.** Both `preview.rs`'s and `preview_render.rs`'s failure messages would then format it rather than restate codes 11 to 18. The next change to that scenario's codes would then reach one place, rather than depending on a grep the plan did not make. It is one case of recommendation 1 of [close-followups-1](close-followups-1.md), which asks for one table of every `*_EXITS` range.
3. **Carry out recommendation 1 of [close-followups-1](close-followups-1.md).** This task came up against it twice: `architect-1`'s free-code scan for D4, and `preview_render.rs`'s copied codes.
