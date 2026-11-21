# Retrospective: remove the GNOME header bar's Open button

Last verified at commit `a1231948594e44092e8aa0fc584a7ca5b8c2f898`.

The task removed the `document-open-symbolic` button from `wolf-gnome`'s header bar, with its `open_button` fields in `shell.rs` and its wiring in `window.rs`, and added the cage smoke `smoke_headless/header_bar.rs`. It took two rounds. The ten invocations cost about $14.39; review 1's rejection added the second round, about $4.63 and 18 minutes of agent time. Every gate run passed on both workers on its first try.

## What went well

- **The instructions named every site.** They listed the struct fields, the `set_sensitive` call, the `connect_clicked` block and the two doc passages, and said what must stay. Review 1 found the production code and the docs complete, and neither changed in round 2.
- **The header-bar geometry was already in named helpers.** `tg_search_x`, `tg_header_band` and the `TG_*` widths in `smoke_headless/sidebar_toggles.rs`, and the title-span reading in `titlebar.rs`, let plan 1 derive every assertion's column from existing code. Round 2's fix was one helper, `tg_group_start_x`, built from the same private constants, and `tg_files_x` now sums from it.
- **`regina on linux` let the coder measure everything.** `coder-1` showed the smoke red on the unchanged code and recorded one red per falsifier with checksummed restores. That run is where the launch buffer's real name came from.
- **`fixture_guards.rs` took the new fixture by registration.** Adding `HB_TARGET_FILE` and `HB_READ_NAME` to `RELAUNCH_FIXTURE_NAMES` was the whole cost of keeping the new names clear of other smokes' assertions.
- **The `PANE_ORIGIN_Y_EXPECTED` worry was settled by the gate.** The pointer smokes whose targets rest on the header bar's height run in the Linux `test` step, so no extra check was needed.

## What did not go well

- **Plan 1 checked "no other button" at one side only.** The instructions asked the smoke to assert that the start holds Back/Forward and the toggle group "and no other button". Plan 1 §2 asserted ink at the Search column (A2) and nothing after the group (A3). Its falsifiers H1 to H4 removed things or added one after the group, so none put a button before or between, and review 1's T1 found that such a button shifts the group 40 px right and passes.
- **The smoke's fall-through message rested on an unmeasured Neovim premise.** Plan 1 §2 and §4 said that when `Ctrl+Shift+O` falls through, the readback is empty. `test-writer-1` listed that among its assumptions and ran no test, as its definition requires. `coder-1`'s H4 red read `Untitled.md`, the name `init.lua` gives the startup buffer at `VimEnter`, so the message blamed the picker's ranking for the one failure it was written for.
- **The fact was documented, but not where smoke writers read.** `runtime/nvim/README.md` describes `wolf_name_untitled_current` and the startup `VimEnter` naming, and the Save As smoke's doc in `smoke_headless/editor.rs` says the startup buffer is `<cwd>/Untitled.md`. The task told agents to read `main.rs`'s module doc and `tests/README.md` § Smoke conventions, and neither names the launch buffer. `architect-2` found it with one grep for `Untitled`.
- **`coder-1` saw T2 and returned `passed`.** Its report says the message is wrong and is the test writer's to fix, "nothing in it is wrong enough for a `rejected`". The coder definition says a test believed wrong is a `rejected`, and never a `passed` with a remark. T1 would have caused the round trip anyway, so this cost nothing extra here.
- **Round 2 re-ran all six falsifiers.** Plan 2 §4 had the coder re-run H1 to H4 as well as the new H5 and H6, since the renumbered assertions moved the panic lines and the old reds were no longer verbatim. `coder-2` cost $1.04 and 8.8 minutes, most of it four repeat cage runs that showed nothing new.
- **`crates/wolf-gnome/tests/README.md` is about 120 KB.** `technical-writer-1` touched it 30 times in its transcript and `architect-1` 16 times, for one new entry and two bullet rewrites.

## Recommendations

1. **Name the launch buffer in § Smoke conventions.** In `crates/wolf-gnome/tests/README.md`, add that a fresh launch's buffer is `Untitled.md`, named at `VimEnter` by `wolf_name_untitled_current`, and that a smoke compares against `wolf_gnome::save_ui::UNTITLED_SAVE_NAME` rather than expecting an empty name. Plan 1, `test-writer-1` and the smoke's A4 message all assumed an empty name. The line would have removed T2 and its re-run of H4.
2. **Ask for a falsifier at each place an absence assertion forbids.** In the architect's definition under `.claude/skills/regina/agents/`, beside "one such edit per direction", say that an assertion that nothing else lies in a region needs one edit per position a forbidden item could take, such as before, between and after. Plan 1's H1 to H4 tested after only, and H5 and H6 were what review 1 had to ask for. It would have saved the $4.63 round.
3. **Let the test writer measure a premise about Neovim.** This is recommendation 2 of [nerd-font-3](nerd-font-3.md), and recommendation 1 of [fixes-2](fixes-2.md) and [fixes-3](fixes-3.md), come up again. `test-writer-1` had `regina on linux` and wrote the empty-readback premise down as an assumption, and its definition says to measure such a premise but also to run no test. One scratch launch would have shown `Untitled.md`.
4. **Split the wolf-gnome tests README.** This is recommendation 3 of [sidebar-toggles-3](sidebar-toggles-3.md) and 7 of [nerd-font-3](nerd-font-3.md), come up again. `architect-1`, `test-writer-1`, `technical-writer-1` and `reviewer-1` each read in it, for an entry of a few lines.
