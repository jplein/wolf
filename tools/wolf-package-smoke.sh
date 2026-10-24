#!/usr/bin/env bash
# Builds the Nix package and runs the ignored tests on it. Never beside another `cargo test`.
# See notes/tools/wolf-package-smoke.sh.md#why-this-script-exists
set -euo pipefail

readonly PACKAGE_ATTR=".#wolf-gnome"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "==> building ${PACKAGE_ATTR}"
# `--no-link` keeps `result` out of the tree and prints the store path for the report.
# A cold run is a full release build with no `target/` reuse: minutes, not seconds.
WOLF_PACKAGE_OUT="$(nix build "$PACKAGE_ATTR" --no-link --print-out-paths)"
export WOLF_PACKAGE_OUT

if [ -z "$WOLF_PACKAGE_OUT" ] || [ ! -d "$WOLF_PACKAGE_OUT" ]; then
	echo "wolf-package-smoke: nix build produced no usable store path (got '${WOLF_PACKAGE_OUT}')." >&2
	exit 1
fi

echo "==> WOLF_PACKAGE_OUT=${WOLF_PACKAGE_OUT}"

# Layout suite first: no compositor, milliseconds, and a mis-installed tree reports as a missing
# file rather than as a window that never drew.
echo "==> packaged layout assertions"
nix develop --command cargo test -p wolf-gnome --test packaged_layout -- --ignored --nocapture

# The `packaged_` filter selects exactly the ignored packaged tests in `smoke_headless.rs`.
echo "==> packaged compositor smokes"
nix develop --command cargo test -p wolf-gnome --test smoke_headless -- --ignored --nocapture packaged_

echo "==> packaged-artifact evidence complete for ${WOLF_PACKAGE_OUT}"
