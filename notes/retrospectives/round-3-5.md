# Retrospective: silence rust-overlay's nix warnings, and settle the Linux `Cargo.lock` drift

Last verified at commit `e49920be88672d3382613a73f18f8e4f689a537f`.

The task carried out recommendations 2 and 3 of [retro-fixes-6](retro-fixes-6.md). It locked `rust-overlay` to `51c6dc4`, which drops the two `evaluation warning:` lines and moves the Linux toolchain from 1.96.0 to 1.99.1. It kept `Cargo.lock`, and changed the write-back recipe in `.claude/skills/regina/agents/coder.md` from `cargo update -p <crate>` to `cargo update --workspace`. The change is `c287e7b`, `f9790d2` and `e49920b`.

It took three plans. `coder-1` rejected over a clippy 1.99 lint in a test file, and `technical-writer-1` rejected over a wrong comment in `flake.nix`. Every gate run passed on both workers on its first try, and review 1 approved with minor notes. The eleven invocations cost about $11.40, of which `architect-1` was $4.89 (79 turns, 18.5 minutes).

## What went well

- **The Linux shell's warnings could be traced from macOS.** The "Darwin eval laziness" section of `notes/flake.nix.md` told `architect-1` why the macOS shell printed nothing. Evaluating `.#devShells.x86_64-linux.default.drvPath` with `--option abort-on-warn true --show-trace` then named `lib/mk-aggregated.nix:76` in the locked rust-overlay, without a Linux worker.
- **One lock node moved alone.** `nix flake lock --override-input rust-overlay …` changed exactly three lines of `flake.lock`, and nixpkgs and flake-utils did not move. Because rust-overlay `follows` nixpkgs, the bump could not drag GTK or Neovim with it, so no runtime re-verification was owed.
- **`regina on linux` snapshots made the lockfile experiments safe.** `coder-1` toggled `Cargo.lock` between two valid states, ran six no-op `--precise` updates and added a trial `static_assertions` dependency, all in snapshots, and the workspace lockfiles never changed. One `CARGO_LOG=trace` command showed the mechanism: `second hit on windows-sys v0.48.0`.
- **The Linux gate caught the toolchain's new lint, and nothing else broke.** `cargo clippy -D warnings` under 1.99.1 found `clippy::chunks_exact_to_as_chunks` in `crates/wolf-gnome/tests/release_profile.rs`. Because wolf-gnome has no `BUILD.bazel`, clippy's `as_chunks` rewrite needed no care for the macOS rustc 1.85.0.

## What did not go well

- **The architect could not run the toolchain it was planning to install.** `architect-1`'s system prompt grants no `regina on linux`, while `coder-1`'s and `test-writer-1`'s each mention it six times. So plan 1 listed a test-file lint only as possible and ordered T1 before C3. `coder-1` found the lint, and plan 2 and `test-writer-2` cost about $1.20 more.
- **The architect read cargo's source to infer what one command showed.** 23 of `architect-1`'s 29 web fetches and searches read cargo's resolver and registry code, to explain the `windows-sys` drift without `cargo`. `coder-1` confirmed the same mechanism with one `CARGO_LOG=trace` run. That made `architect-1` 43% of the task's cost, as in every earlier task of this request.
- **The request's premise did not hold.** [retro-fixes-6](retro-fixes-6.md) recommendation 2 assumed a lockfile exists that a no-op `cargo update -p <crate> --precise` leaves alone. With `windows-sys` 0.48.0, 0.60.2 and 0.61.2 in the graph, none does for `notify`, `ignore`, `walkdir` or `winapi-util`. Plan 1 had to rule out the cargo version, the resolver and `rust-version` that the instructions named.
- **A wrong claim sat in a comment and in its mirror, owned by two agents.** `flake.nix:7` and `notes/flake.nix.md` both said nixpkgs supplies the toolchain. `architect-1` read both twice, but plan 1 searched `*.md` for `rust-overlay` and `1.9[0-9]`, which the claim does not contain. `technical-writer-1` could not edit a `.nix` file, so plan 3, `coder-3`, a second implementation gate and `technical-writer-2` cost about $1.28 more.
- **This is the fourth time a technical writer found a code comment it may not fix.** [open-items-2](open-items-2.md), [open-items-3](open-items-3.md) and [open-items-9](open-items-9.md) record the same, where the comment then landed wrong. Here `technical-writer-1` rejected instead, which fixed it at the price of a round trip.
- **The rewritten note's list of what moves is short.** Review 1 found that `notes/flake.nix.md` says a rust-overlay update moves "only the Linux rustc, cargo, clippy and rustfmt". The `rustToolchain` override at `flake.nix:35` also carries `rust-src` and `rust-analyzer`, and the note landed as written.

## Recommendations

1. **Give the architect `regina on linux`.** This came up again: it is recommendation 1 of [round-3-2](round-3-2.md) and of [round-3-3](round-3-3.md). Here `architect-1` could have run clippy 1.99.1 in a snapshot with `--override-input`, putting the lint into T1, and run `CARGO_LOG=trace` instead of 23 fetches. That would have saved plan 2's round, about $1.20, and much of `architect-1`'s $4.89.
2. **Have the architect check claims about what it changes, not keywords.** Add to item 5 of `.claude/skills/regina/agents/architect.md` that a task moving a pinned input or version reads the comment beside that pin and the mirror section it links, and lists any claim of what the pin supplies. Plan 1's `*.md` keyword search missed `flake.nix:7` and "Single nixpkgs input", which cost plan 3's round.
3. **Correct the list in "Single nixpkgs input".** In `notes/flake.nix.md`, say that `nix flake update rust-overlay` moves the Linux Rust toolchain and its `rust-src` and `rust-analyzer` extensions, as review 1 suggested. A later agent bumping the toolchain would otherwise not expect `rust-analyzer` to move.
