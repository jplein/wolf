#!/bin/sh
# Asserts `plugin/orlando.lua` and `lua/orlando/init.lua` are in the tree's inner `orlando/`
# dir (bazel/orlando/README.md). Every runfiles root is probed, then `find`.

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

# --- Locate the assembled orlando tree (its inner `orlando/` dir).
ORLANDO=""
for root in $RUNFILES_ROOTS; do
  [ -d "$root" ] || continue
  # Direct, expected layouts; `-d` follows symlinks (runfiles entries are
  # symlinks into the output tree / external repo).
  for candidate in \
    "$root"/*/bazel/orlando/orlando_tree/orlando \
    "$root"/bazel/orlando/orlando_tree/orlando \
    "$root"/*/orlando_tree/orlando \
    "$root"/*orlando*/orlando_tree/orlando
  do
    if [ -d "$candidate" ]; then
      ORLANDO="$candidate"
      break
    fi
  done
  [ -n "$ORLANDO" ] && break
  # Fallback: search anywhere under this runfiles root, following symlinks (-L)
  # so `find` descends into the symlinked TreeArtifact directory.
  found=$(find -L "$root" -type d -path '*/orlando_tree/orlando' 2>/dev/null | head -n 1 || true)
  if [ -n "$found" ]; then
    ORLANDO="$found"
    break
  fi
done

[ -n "$ORLANDO" ] || fail "could not locate the orlando_tree/orlando dir in runfiles. Searched roots:${RUNFILES_ROOTS:- (none)}"

echo "Located orlando tree: $ORLANDO"

# --- (a) plugin/orlando.lua is a regular file.
PLUGIN="$ORLANDO/plugin/orlando.lua"
[ -f "$PLUGIN" ] || fail "expected vendored plugin file at $PLUGIN, but it is missing"
echo "PASS check (a): $PLUGIN exists"

# --- (b) lua/orlando/init.lua is a regular file.
INIT="$ORLANDO/lua/orlando/init.lua"
[ -f "$INIT" ] || fail "expected vendored orlando module at $INIT, but it is missing"
echo "PASS check (b): $INIT exists"

echo "PASS: orlando tree bundles plugin/orlando.lua and lua/orlando/init.lua"
