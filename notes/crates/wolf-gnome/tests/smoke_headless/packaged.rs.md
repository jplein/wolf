# `crates/wolf-gnome/tests/smoke_headless/packaged.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Packaged smokes share this binary

The packaged smokes exercise the binary `nix build .#wolf-gnome` installs, which is built by a different command from `CARGO_BIN_EXE_wolf-gnome` and is the one a user launches, so the other smokes say nothing about it. The store-layout assertions need no compositor and live in `crates/wolf-gnome/tests/packaged_layout.rs`.

They live in the `smoke_headless` binary because they launch through `cage_command` and hold a `CAGE_SLOTS` slot like every other smoke. A binary of their own would need a compositor spawn of its own, which `cage_is_only_spawned_through_the_isolating_helper` forbids: it counts the spawn needle across every test source of the crate. Its cages would also take slots from a `CAGE_SLOTS` of their own, outside this binary's bound. For the same reason `CAGE_SPAWN_NEEDLE` is assembled from fragments: a contiguous spelling anywhere among those sources counts as a second spawn site and turns the guard red.
