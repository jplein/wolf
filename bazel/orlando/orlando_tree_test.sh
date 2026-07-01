#!/bin/sh
#
# orlando_tree_test.sh
#
# sh_test that inspects the assembled orlando plugin TreeArtifact (target
# //bazel/orlando:orlando_tree, declared as `data` on the sh_test) and asserts the
# vendored plugin files landed at the expected paths inside the tree.
#
# It asserts, relative to the tree's inner `orlando/` dir:
#   a. plugin/orlando.lua     exists & is a regular file
#   b. lua/orlando/init.lua   exists & is a regular file
#
# The tree comes from an http_archive (external repo `@orlando`) assembled by
# `copy_to_directory` with `replace_prefixes` remapping the archive root to an
# inner `orlando/` dir. The copy_to_directory output TreeArtifact is
# `orlando_tree/`, whose inner dir is `orlando/`, so the files land at
# `<tree>/orlando/plugin/orlando.lua` etc.
#
# Robust runfiles resolution: the exact runfiles layout for a TreeArtifact under
# aspect_bazel_lib can vary, and the alias routes through the `@orlando` external
# repo, so we try $TEST_SRCDIR, $RUNFILES_DIR, and $0.runfiles, and finally fall
# back to a `find` for `*/orlando_tree/orlando`.
#
# NOTE (coder): once the workspace/repo name is known, PIN the direct runfiles
# path (e.g. "$TEST_SRCDIR/_main/bazel/orlando/orlando_tree/orlando") as the first
# ORLANDO candidate below and keep the `find` fallback only as a safety net.

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
# Locate the assembled orlando tree (its inner `orlando/` dir).
# ---------------------------------------------------------------------------
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

# ---------------------------------------------------------------------------
# (a) plugin/orlando.lua is a regular file.
# ---------------------------------------------------------------------------
PLUGIN="$ORLANDO/plugin/orlando.lua"
[ -f "$PLUGIN" ] || fail "expected vendored plugin file at $PLUGIN, but it is missing"
echo "PASS check (a): $PLUGIN exists"

# ---------------------------------------------------------------------------
# (b) lua/orlando/init.lua is a regular file.
# ---------------------------------------------------------------------------
INIT="$ORLANDO/lua/orlando/init.lua"
[ -f "$INIT" ] || fail "expected vendored orlando module at $INIT, but it is missing"
echo "PASS check (b): $INIT exists"

echo "PASS: orlando tree bundles plugin/orlando.lua and lua/orlando/init.lua"
