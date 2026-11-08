# Retrospective: watch live file-search roots with a `notify` watcher

Last verified at commit `2cec66d34d3ec440dff3839d42a026b71794b8c7`.

The task added a `notify` watcher to `SearchRegistry` in `crates/wolf-core/src/files.rs`. A dedicated `wolf-fs-watch` thread watches each live session's root, drops events the walk would not list, and forwards the rest to `notify_fs_changed`. Six watcher cases joined `crates/wolf-core/tests/file_search_rescan.rs`, and three in-source tests joined `files.rs`. The work is commits `7b40265` (the coder's, carrying the test writer's cases) and `2cec66d`.

It ran once through the flow with no rework: no rejection, no failed gate, no discrepancy. The five invocations cost about $21.33. `architect-1` was the largest ($7.59, 72 turns, 26 minutes), most of it reading `notify`'s and `ignore`'s sources and release notes, then `coder-1` ($5.96, 110 turns), `reviewer-1` ($3.59), `technical-writer-1` ($2.32) and `test-writer-1` ($1.88).

## What went well

- **The GUI-free rule was written down with its check.** `notes/wolf-core-decisions.md` "The one hard rule" gives the forbidden names, `objc` among them, and both commands. That let plan 1 see that the release the task's rule selected, `notify` 8.2.1, pulls `objc2-core-*` crates, and pin `=8.2.0` instead before any code was written. `coder-1` then ran both checks and got `clean`.
- **`walk_files` could serve as a test oracle.** The walk is a plain function, so `watch_filter_admits_exactly_what_the_walk_lists` compares the watcher's filter with the real walk over one fixture instead of a hand-written list. Falsifiers F9 and F10 in `coder-1`'s report each turned it red on a single file.
- **Nothing needed wiring.** `//crates/wolf-core:file_search_rescan` already listed the test file, and `//crates/wolf-core:wolf_core_lib_test` takes its sources from the crate, so the three in-source tests ran with no `BUILD.bazel` edit. The target ran in 7.4 s, so its `size` stayed `"small"`.
- **An existing env-lock pattern was reused.** `crates/wolf-core/tests/file_search_bounds.rs`'s `lock_env`, with poison recovery, became `file_search_rescan.rs`'s `WATCH_ENV_LOCK`. Plan 1 could name it, and `test-writer-1` copied it.
- **Both gates passed on both workers on their first run.** `coder-1` ran `cargo fmt`, `clippy`, the tests and the full Linux `./gates.sh` through `regina on linux` before claiming `passed`.

## What did not go well

No step was redone. Five things cost effort, and two of them came up in earlier tasks.

- **The GUI-free guard is not a gate.** Neither `gates.sh` nor any test runs the `cargo tree` or `bazel query` check. Had plan 1 followed the task's version rule to the letter, `notify` 8.2.1 would have put `objc2-core-foundation` and `objc2-core-services` into `wolf-core` with every gate green. `architect-1` caught it only by reading the crate's release notes and crates.io feature map (its commands 13 to 18).
- **Plan 1 asked the test writer to run the tests again.** Its §6 step 1 had `test-writer-1` record (a) to (f) red before the code, which the test writer's definition forbids. `test-writer-1` left it to the coder, and `coder-1` wrote the code first, so no red-before-code run exists; review 1 §1 accepted the falsifiers instead. This is the conflict [open-items-1](open-items-1.md) and [open-items-3](open-items-3.md) recorded, and `architect-1`'s system prompt still does not say the test writer runs no test.
- **`regina on linux` output took seven commands to use.** It appends stderr after stdout, so the `cat Cargo.lock` that the task text and the coder's definition both give returned the lockfile followed by cargo's progress lines and two nix warnings, which `coder-1` cut by hand (its commands 15 to 18). Reading the new crates' minimum Rust versions then failed twice, on a missing `jq` and on `cargo metadata` output over the 1 MiB cap, before a third command read the registry's `Cargo.toml` files (commands 19 to 21).
- **The task's lockfile list rested on a stale sentence.** `build.md` said to commit `MODULE.bazel.lock` after every repin, so the instructions listed it in the done criterion, though this repin did not move it. `coder-1` (commands 26 and 27) and review 1 §1 each proved that nothing was missing. `technical-writer-1` corrected `build.md` to commit it "when the repin moves it", so this cause is gone.
- **Three agents read the same dependency sources three ways.** `architect-1` downloaded the `ignore` and `notify` crates from crates.io with `curl`, `coder-1` read them from the Linux worker's cargo registry (commands 23, 24, 28 to 30), and `technical-writer-1` found them under Bazel's `external` directory. Each checked the same `ignore` 0.4.26 `dir.rs` rules, and only the last way needs neither the network nor the Linux worker.
- **The macOS build log names no targets.** `coder-1` spent six commands (76 to 81) finding out whether `//crates/wolf-macos:Wolf` linked against FSEvents, ending with a build of its own. This came up again: it is recommendation 5 of [open-items-9](open-items-9.md), repeated as recommendation 4 of [open-items-10](open-items-10.md).

Review 1 left three notes on the landed work:

- No test pins `canonical_event_path`'s rule, which resolves the parent and keeps the last component so a new symlink stays listed. A later change to the whole-path `canonicalize` that `redraw.rs` uses would fail nothing.
- A Linux session rooted at `$HOME`, GNOME's default with no folder open, re-walks on each nvim swap-file create or delete and each `state.json` save, because `~/.local/state/wolf` is neither ignored nor in `skip_dirs`. Only the plan's fourth manual check would show it.
- The opening paragraph of `file_search_rescan.rs`'s module doc still describes only the nvim trigger path.

## Recommendations

1. **Gate the GUI-free guard.** Add the `cargo tree -e no-dev -p wolf-core` check, grepped against the forbidden names in `notes/wolf-core-decisions.md`, to the Linux gates of `gates.sh`, and the `bazel query` form to its macOS gates, each failing on a match. Keep the name list in one place both read. In this task only `architect-1`'s reading of a changelog stood between the task's version rule and a silent breach of the crate's one hard rule.
2. **Say in the architect's definition that the test writer runs no test.** This came up again: it is recommendation 3 of [open-items-1](open-items-1.md) and recommendation 1 of [open-items-3](open-items-3.md). In this task the cost was the red-before-code run, which nobody made.
3. **Give a clean lockfile command for `regina on linux`.** In `.claude/skills/regina/agents/coder.md`, where it gives `regina on linux -- sh -c 'cargo update -p <crate> && cat Cargo.lock'`, send cargo's stderr to `/dev/null`, and say that the command's stderr follows its stdout. Add a command that prints each locked crate's `rust-version` without `jq`, for example by reading the registry's `Cargo.toml` files. That would have saved `coder-1` six of its seven commands.
4. **Say where a pinned crate's source is on macOS.** Add one line to `build.md` "Repinning Rust dependencies": the source of every crate in `Cargo.Bazel.lock` is under `$(bazel info output_base)/external/`, named after the crate and version. `architect-1` and `coder-1` would then have read `ignore` and `notify` there instead of downloading them or going through the Linux worker.
5. **Pin the symlink rule of `canonical_event_path`.** Add a watcher case to `crates/wolf-core/tests/file_search_rescan.rs` that creates `link.md` pointing at a listed `other.md` with `std::os::unix::fs::symlink` and polls until `link.md` is listed, as review 1 §3 proposes. The rule then survives a later tidy-up toward `redraw.rs`'s form.
6. **Keep Wolf's own state out of a `$HOME` walk.** Add `state_dir()` to the default `skip_dirs` in `crates/wolf-core/src/files.rs`, or ignore it in the walk, so that a session rooted at `$HOME` stops re-walking on swap files and state saves. Review 1 §2 is the evidence, and GNOME starts at `$HOME` by default.
