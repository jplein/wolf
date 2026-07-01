#!/usr/bin/env bash
#
# nvim_treesitter_tree_test.sh
#
# sh_test that inspects the assembled nvim-treesitter plugin TreeArtifact (target
# //bazel/nvim-treesitter:nvim_treesitter_tree, declared as `data` on the sh_test)
# and asserts the vendored plugin files landed at the expected paths inside the
# tree.
#
# nvim-treesitter is a pure-Lua plugin carrying bundled queries and Lua modules.
# It asserts, relative to the tree's inner `nvim-treesitter/` dir:
#   a. queries/markdown/injections.scm  exists & is a regular file
#      (fall back to queries/markdown/highlights.scm if injections.scm is not
#       present at the pinned revision — at least one markdown query file must
#       exist)
#   b. lua/nvim-treesitter               exists & is a directory
#
# NOTE: this change VENDORS the plugin only; it does NOT enable treesitter
# highlighting. The queries are shipped for a future enablement change.
#
# The tree comes from an http_archive (external repo `@nvim_treesitter`)
# assembled by `copy_to_directory` with `replace_prefixes` remapping the archive
# root to an inner `nvim-treesitter/` dir. The copy_to_directory output
# TreeArtifact is `nvim_treesitter_tree/`, whose inner dir is `nvim-treesitter/`,
# so the files land at `<tree>/nvim-treesitter/queries/markdown/injections.scm`
# etc.
#
# Robust runfiles resolution: the exact runfiles layout for a TreeArtifact under
# aspect_bazel_lib can vary, and the alias routes through the `@nvim_treesitter`
# external repo, so we try $TEST_SRCDIR, $RUNFILES_DIR, and $0.runfiles, and
# finally fall back to a `find` for `*/nvim_treesitter_tree/nvim-treesitter`.
#
# NOTE (coder): once the workspace/repo name is known, PIN the direct runfiles
# path (e.g. "$TEST_SRCDIR/_main/bazel/nvim-treesitter/nvim_treesitter_tree/nvim-treesitter")
# as the first TREE candidate below and keep the `find` fallback only as a
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
# Locate the assembled nvim-treesitter tree (its inner `nvim-treesitter/` dir).
# ---------------------------------------------------------------------------
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

# ---------------------------------------------------------------------------
# (a) queries/markdown/injections.scm is a regular file (HARD assertion), with a
#     fallback to queries/markdown/highlights.scm — at least one markdown query
#     file must be present.
# ---------------------------------------------------------------------------
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

# ---------------------------------------------------------------------------
# (b) lua/nvim-treesitter is a directory.
# ---------------------------------------------------------------------------
LUA_DIR="$TREE/lua/nvim-treesitter"
[ -d "$LUA_DIR" ] || fail "expected Lua module directory at $LUA_DIR, but it is missing or not a directory"
echo "PASS check (b): $LUA_DIR exists and is a directory"

echo "PASS: nvim-treesitter tree bundles markdown queries and lua/nvim-treesitter"
