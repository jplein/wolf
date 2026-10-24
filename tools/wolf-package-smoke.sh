#!/usr/bin/env bash
# The packaged-artifact evidence run: build the Nix package, then drive the
# `#[ignore]`d tests that read and launch it.
#
# WHY THIS SCRIPT EXISTS AT ALL. A green `cargo test` says nothing about
# `$out/bin/wolf-gnome`: the checkout binary and the store binary are two
# different artifacts making two different claims. The tests that assert about
# the store artifact need a built store path, which no ordinary `cargo test` can
# produce, so they are `#[ignore]`d. An ignored test is honest only when exactly
# ONE command runs it and that command always establishes its precondition.
# This is that command.
#
# The precondition is `WOLF_PACKAGE_OUT`, the derivation's `$out` ROOT (never the
# binary inside it). Both suites derive `bin/wolf-gnome` from it themselves, so
# the path and the binary cannot name different builds.
#
# The resolved store path is printed before anything runs, because that path is
# the SUBJECT of the evidence: a recorded run that does not name it is a run
# about an unknown artifact.
#
# RUN IT FROM ANYWHERE: it re-roots itself on the repository root below, which is
# what its `nix develop` calls need. The shellHook generates `FONTCONFIG_FILE` by
# substituting store paths into `$PWD/crates/wolf-gnome/testdata/fonts.conf`, so a
# shell entered from another directory has no template to read and none of the
# font pinning the smokes rest on.
#
# Do NOT run it while another `cargo test` is in flight. Each run sizes its
# cage slots for the whole host, so two at once oversubscribe it and stretch the
# scenarios' fixed settles, which reads as scattered flakes rather than as the
# overload it is.
set -euo pipefail

readonly PACKAGE_ATTR=".#wolf-gnome"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "==> building ${PACKAGE_ATTR}"
# `--no-link` keeps a `result` symlink out of the working tree; the store path
# comes back on stdout instead, which is what makes it quotable in a report.
# A cold run here is a full release build of the workspace with no `target/`
# reuse, so it is minutes rather than seconds.
WOLF_PACKAGE_OUT="$(nix build "$PACKAGE_ATTR" --no-link --print-out-paths)"
export WOLF_PACKAGE_OUT

if [ -z "$WOLF_PACKAGE_OUT" ] || [ ! -d "$WOLF_PACKAGE_OUT" ]; then
	echo "wolf-package-smoke: nix build produced no usable store path (got '${WOLF_PACKAGE_OUT}')." >&2
	exit 1
fi

echo "==> WOLF_PACKAGE_OUT=${WOLF_PACKAGE_OUT}"

# The layout suite first: it needs no compositor, runs in milliseconds, and its
# failures name the `postInstall` line at fault. Running it before the smokes
# means a mis-installed tree is reported as a missing file rather than as a
# window that never drew.
echo "==> packaged layout assertions"
nix develop --command cargo test -p wolf-gnome --test packaged_layout -- --ignored --nocapture

# Then the compositor smokes. The `packaged_` filter selects exactly the
# `#[ignore]`d packaged tests in `smoke_headless.rs` and leaves that file's
# other ignored tests alone.
echo "==> packaged compositor smokes"
nix develop --command cargo test -p wolf-gnome --test smoke_headless -- --ignored --nocapture packaged_

echo "==> packaged-artifact evidence complete for ${WOLF_PACKAGE_OUT}"
