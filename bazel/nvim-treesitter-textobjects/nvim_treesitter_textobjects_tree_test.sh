#!/usr/bin/env bash
# Asserts the tree holds `lua/nvim-treesitter-textobjects/` and a `queries/**/textobjects.scm`
# (bazel/nvim-treesitter-textobjects/README.md). Every runfiles root is probed, then `find`.

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

# --- Locate the assembled tree (its inner `nvim-treesitter-textobjects/` dir).
TREE=""
for root in $RUNFILES_ROOTS; do
  [ -d "$root" ] || continue
  for candidate in \
    "$root"/*/bazel/nvim-treesitter-textobjects/nvim_treesitter_textobjects_tree/nvim-treesitter-textobjects \
    "$root"/bazel/nvim-treesitter-textobjects/nvim_treesitter_textobjects_tree/nvim-treesitter-textobjects \
    "$root"/*/nvim_treesitter_textobjects_tree/nvim-treesitter-textobjects \
    "$root"/*textobjects*/nvim_treesitter_textobjects_tree/nvim-treesitter-textobjects
  do
    if [ -d "$candidate" ]; then
      TREE="$candidate"
      break
    fi
  done
  [ -n "$TREE" ] && break
  found=$(find -L "$root" -type d -path '*/nvim_treesitter_textobjects_tree/nvim-treesitter-textobjects' 2>/dev/null | head -n 1 || true)
  if [ -n "$found" ]; then
    TREE="$found"
    break
  fi
done

[ -n "$TREE" ] || fail "could not locate the nvim_treesitter_textobjects_tree/nvim-treesitter-textobjects dir in runfiles. Searched roots:${RUNFILES_ROOTS:- (none)}"

echo "Located nvim-treesitter-textobjects tree: $TREE"

# --- (a) lua/nvim-treesitter-textobjects is a directory.
LUA_DIR="$TREE/lua/nvim-treesitter-textobjects"
[ -d "$LUA_DIR" ] || fail "expected Lua module directory at $LUA_DIR, but it is missing or not a directory"
echo "PASS check (a): $LUA_DIR exists and is a directory"

# --- (b) A queries/**/textobjects.scm exists; `find -L` tolerates the per-language layout.
TEXTOBJECTS=$(find -L "$TREE/queries" -type f -name 'textobjects.scm' 2>/dev/null | head -n 1 || true)
[ -n "$TEXTOBJECTS" ] || fail "expected at least one queries/**/textobjects.scm under $TREE/queries, but none is present"
echo "PASS check (b): found a textobjects query file: $TEXTOBJECTS"

echo "PASS: nvim-treesitter-textobjects tree bundles lua/nvim-treesitter-textobjects and a textobjects query"
