# `tools/wolf-package-smoke.sh`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Why this script exists

A green `cargo test` says nothing about `$out/bin/wolf-gnome`: the checkout binary and the store binary are different artifacts making different claims. The tests that assert about the store artifact need a built store path, which no ordinary `cargo test` can produce, so they are ignored. An ignored test is honest only when exactly one command runs it and that command always establishes its precondition, and this script is that command.

The precondition is `WOLF_PACKAGE_OUT`, the derivation's `$out` root and never the binary inside it. Both suites derive `bin/wolf-gnome` from it themselves, so the path and the binary cannot name different builds. The resolved store path is printed before anything runs, because it is the subject of the evidence: a recorded run that does not name it is a run about an unknown artifact.

The script re-roots itself on the repository root, so it runs from anywhere. Its `nix develop` calls need that: the shellHook generates `FONTCONFIG_FILE` from `$PWD/crates/wolf-gnome/testdata/fonts.conf`, so a shell entered elsewhere has no template and none of the font pinning the smokes rest on.

Do not run it while another `cargo test` is in flight. Each run sizes its cage slots for the whole host, so two at once oversubscribe it and stretch the scenarios' fixed settles, which reads as scattered flakes rather than as the overload it is.
