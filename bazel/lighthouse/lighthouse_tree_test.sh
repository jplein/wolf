#!/bin/sh
#
# lighthouse_tree_test.sh
#
# sh_test that inspects the assembled lighthouse colorscheme TreeArtifact (target
# //bazel/lighthouse:lighthouse_tree, declared as `data` on the sh_test) and asserts
# the vendored colorscheme file landed at the expected path inside the tree.
#
# lighthouse is a COLORSCHEME, not a plugin: its only runtime file is
# `colors/lighthouse.lua` (plus a `LICENSE` for compliance). There is NO `plugin/`
# or `lua/` dir, so this test makes a single HARD assertion:
#
# It asserts, relative to the tree's inner `lighthouse/` dir:
#   a. colors/lighthouse.lua   exists & is a regular file
#
# (Optional SOFT note: echo when a LICENSE* file is present, but never fail on it.)
#
# The tree comes from an http_archive (external repo `@lighthouse`) assembled by
# `copy_to_directory` with `replace_prefixes` remapping the archive root to an
# inner `lighthouse/` dir. The copy_to_directory output TreeArtifact is
# `lighthouse_tree/`, whose inner dir is `lighthouse/`, so the file lands at
# `<tree>/lighthouse/colors/lighthouse.lua`.
#
# Robust runfiles resolution: the exact runfiles layout for a TreeArtifact under
# aspect_bazel_lib can vary, and the alias routes through the `@lighthouse` external
# repo, so we try $TEST_SRCDIR, $RUNFILES_DIR, and $0.runfiles, and finally fall
# back to a `find` for `*/lighthouse_tree/lighthouse`.
#
# NOTE (coder): once the workspace/repo name is known, PIN the direct runfiles
# path (e.g. "$TEST_SRCDIR/_main/bazel/lighthouse/lighthouse_tree/lighthouse") as the
# first LIGHTHOUSE candidate below and keep the `find` fallback only as a safety net.

set -eu

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# ---------------------------------------------------------------------------
# Locate the runfiles roots.
# ---------------------------------------------------------------------------
RUNFILES_ROOTS=""
[ "${TEST_SRCDIR:-}" != "" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $TEST_SRCDIR"
[ "${RUNFILES_DIR:-}" != "" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $RUNFILES_DIR"
[ -d "$0.runfiles" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $0.runfiles"

# ---------------------------------------------------------------------------
# Locate the assembled lighthouse tree (its inner `lighthouse/` dir).
# ---------------------------------------------------------------------------
LIGHTHOUSE=""
for root in $RUNFILES_ROOTS; do
  [ -d "$root" ] || continue
  # Direct, expected layouts; `-d` follows symlinks (runfiles entries are
  # symlinks into the output tree / external repo).
  for candidate in \
    "$root"/*/bazel/lighthouse/lighthouse_tree/lighthouse \
    "$root"/bazel/lighthouse/lighthouse_tree/lighthouse \
    "$root"/*/lighthouse_tree/lighthouse \
    "$root"/*lighthouse*/lighthouse_tree/lighthouse
  do
    if [ -d "$candidate" ]; then
      LIGHTHOUSE="$candidate"
      break
    fi
  done
  [ -n "$LIGHTHOUSE" ] && break
  # Fallback: search anywhere under this runfiles root, following symlinks (-L)
  # so `find` descends into the symlinked TreeArtifact directory.
  found=$(find -L "$root" -type d -path '*/lighthouse_tree/lighthouse' 2>/dev/null | head -n 1 || true)
  if [ -n "$found" ]; then
    LIGHTHOUSE="$found"
    break
  fi
done

[ -n "$LIGHTHOUSE" ] || fail "could not locate the lighthouse_tree/lighthouse dir in runfiles. Searched roots:${RUNFILES_ROOTS:- (none)}"

echo "Located lighthouse tree: $LIGHTHOUSE"

# ---------------------------------------------------------------------------
# (a) colors/lighthouse.lua is a regular file (HARD assertion).
# ---------------------------------------------------------------------------
COLORS="$LIGHTHOUSE/colors/lighthouse.lua"
[ -f "$COLORS" ] || fail "expected vendored colorscheme file at $COLORS, but it is missing"
echo "PASS check (a): $COLORS exists"

# ---------------------------------------------------------------------------
# Optional SOFT note: LICENSE* is nice for compliance but not required to pass.
# ---------------------------------------------------------------------------
for lic in "$LIGHTHOUSE"/LICENSE*; do
  if [ -f "$lic" ]; then
    echo "NOTE: found bundled license file: $lic"
    break
  fi
done

echo "PASS: lighthouse tree bundles colors/lighthouse.lua"
