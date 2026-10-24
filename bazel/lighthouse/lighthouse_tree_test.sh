#!/bin/sh
# Asserts `colors/lighthouse.lua` is in the tree's inner `lighthouse/` dir and only notes LICENSE*
# (bazel/lighthouse/README.md). Every runfiles root is probed, then `find`.

set -eu

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# --- Locate the runfiles roots.
RUNFILES_ROOTS=""
[ "${TEST_SRCDIR:-}" != "" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $TEST_SRCDIR"
[ "${RUNFILES_DIR:-}" != "" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $RUNFILES_DIR"
[ -d "$0.runfiles" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $0.runfiles"

# --- Locate the assembled lighthouse tree (its inner `lighthouse/` dir).
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

# --- (a) colors/lighthouse.lua is a regular file (hard assertion).
COLORS="$LIGHTHOUSE/colors/lighthouse.lua"
[ -f "$COLORS" ] || fail "expected vendored colorscheme file at $COLORS, but it is missing"
echo "PASS check (a): $COLORS exists"

# --- Optional soft note: LICENSE* is nice for compliance but not required to pass.
for lic in "$LIGHTHOUSE"/LICENSE*; do
  if [ -f "$lic" ]; then
    echo "NOTE: found bundled license file: $lic"
    break
  fi
done

echo "PASS: lighthouse tree bundles colors/lighthouse.lua"
