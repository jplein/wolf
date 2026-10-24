#!/usr/bin/env bash
# Asserts the tree holds a markdown query (`injections.scm`, else `highlights.scm`) and
# `lua/nvim-treesitter/`; see bazel/nvim-treesitter/README.md. Runfiles roots are probed.

set -euo pipefail

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# --- Locate the runfiles roots.
RUNFILES_ROOTS=""
[ "${TEST_SRCDIR:-}" != "" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $TEST_SRCDIR"
[ "${RUNFILES_DIR:-}" != "" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $RUNFILES_DIR"
[ -d "$0.runfiles" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $0.runfiles"

# --- Locate the assembled nvim-treesitter tree (its inner `nvim-treesitter/` dir).
TREE=""
for root in $RUNFILES_ROOTS; do
  [ -d "$root" ] || continue
  # Direct, expected layouts; `-d` follows symlinks (runfiles entries are
  # symlinks into the output tree / external repo).
  for candidate in \
    "$root"/*/bazel/nvim-treesitter/nvim_treesitter_tree/nvim-treesitter \
    "$root"/bazel/nvim-treesitter/nvim_treesitter_tree/nvim-treesitter \
    "$root"/*/nvim_treesitter_tree/nvim-treesitter \
    "$root"/*nvim*treesitter*/nvim_treesitter_tree/nvim-treesitter
  do
    if [ -d "$candidate" ]; then
      TREE="$candidate"
      break
    fi
  done
  [ -n "$TREE" ] && break
  # Fallback: search anywhere under this runfiles root, following symlinks (-L)
  # so `find` descends into the symlinked TreeArtifact directory.
  found=$(find -L "$root" -type d -path '*/nvim_treesitter_tree/nvim-treesitter' 2>/dev/null | head -n 1 || true)
  if [ -n "$found" ]; then
    TREE="$found"
    break
  fi
done

[ -n "$TREE" ] || fail "could not locate the nvim_treesitter_tree/nvim-treesitter dir in runfiles. Searched roots:${RUNFILES_ROOTS:- (none)}"

echo "Located nvim-treesitter tree: $TREE"

# --- (a) queries/markdown/injections.scm, else highlights.scm: one markdown query must exist.
INJECTIONS="$TREE/queries/markdown/injections.scm"
HIGHLIGHTS="$TREE/queries/markdown/highlights.scm"
if [ -f "$INJECTIONS" ]; then
  echo "PASS check (a): $INJECTIONS exists"
elif [ -f "$HIGHLIGHTS" ]; then
  echo "NOTE: queries/markdown/injections.scm not present at this revision; fell back to highlights.scm"
  echo "PASS check (a): $HIGHLIGHTS exists"
else
  fail "expected a markdown query file (queries/markdown/injections.scm or queries/markdown/highlights.scm) under $TREE, but none is present"
fi

# --- (b) lua/nvim-treesitter is a directory.
LUA_DIR="$TREE/lua/nvim-treesitter"
[ -d "$LUA_DIR" ] || fail "expected Lua module directory at $LUA_DIR, but it is missing or not a directory"
echo "PASS check (b): $LUA_DIR exists and is a directory"

echo "PASS: nvim-treesitter tree bundles markdown queries and lua/nvim-treesitter"
