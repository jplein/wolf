#!/usr/bin/env bash
#
# nvim_treesitter_textobjects_tree_test.sh
#
# sh_test that inspects the assembled nvim-treesitter-textobjects plugin
# TreeArtifact (target
# //bazel/nvim-treesitter-textobjects:nvim_treesitter_textobjects_tree, declared
# as `data` on the sh_test) and asserts the vendored plugin files landed at the
# expected paths inside the tree.
#
# nvim-treesitter-textobjects is a pure-Lua plugin carrying textobject queries
# and Lua modules. It asserts, relative to the tree's inner
# `nvim-treesitter-textobjects/` dir:
#   a. lua/nvim-treesitter/textobjects        exists & is a directory
#   b. a representative queries/**/textobjects.scm exists (found via `find` so
#      the check is robust across pinned revisions / per-language layouts)
#
# NOTE: this change VENDORS the plugin only; it does NOT enable treesitter
# textobjects. The queries are shipped for a future enablement change.
#
# The tree comes from an http_archive (external repo
# `@nvim_treesitter_textobjects`) assembled by `copy_to_directory` with
# `replace_prefixes` remapping the archive root to an inner
# `nvim-treesitter-textobjects/` dir. The copy_to_directory output TreeArtifact
# is `nvim_treesitter_textobjects_tree/`, whose inner dir is
# `nvim-treesitter-textobjects/`.
#
# Robust runfiles resolution: mirror the orlando/lighthouse tree tests — try
# $TEST_SRCDIR, $RUNFILES_DIR, and $0.runfiles, then fall back to a `find`.
#
# NOTE (coder): once the workspace/repo name is known, PIN the direct runfiles
# path as the first TREE candidate below and keep the `find` fallback only as a
# safety net.

set -euo pipefail

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
# Locate the assembled tree (its inner `nvim-treesitter-textobjects/` dir).
# ---------------------------------------------------------------------------
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

# ---------------------------------------------------------------------------
# (a) lua/nvim-treesitter/textobjects is a directory.
# ---------------------------------------------------------------------------
LUA_DIR="$TREE/lua/nvim-treesitter/textobjects"
[ -d "$LUA_DIR" ] || fail "expected Lua module directory at $LUA_DIR, but it is missing or not a directory"
echo "PASS check (a): $LUA_DIR exists and is a directory"

# ---------------------------------------------------------------------------
# (b) a representative queries/**/textobjects.scm exists. Use `find` (following
#     symlinks) so the check is robust to the exact per-language layout at the
#     pinned revision.
# ---------------------------------------------------------------------------
TEXTOBJECTS=$(find -L "$TREE/queries" -type f -name 'textobjects.scm' 2>/dev/null | head -n 1 || true)
[ -n "$TEXTOBJECTS" ] || fail "expected at least one queries/**/textobjects.scm under $TREE/queries, but none is present"
echo "PASS check (b): found a textobjects query file: $TEXTOBJECTS"

echo "PASS: nvim-treesitter-textobjects tree bundles lua/nvim-treesitter/textobjects and a textobjects query"
