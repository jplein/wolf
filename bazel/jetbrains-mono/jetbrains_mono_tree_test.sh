#!/usr/bin/env bash
#
# jetbrains_mono_tree_test.sh
#
# sh_test that inspects the assembled JetBrains Mono font TreeArtifact (target
# //bazel/jetbrains-mono:jetbrains_mono_tree, declared as `data` on the sh_test)
# and asserts the vendored font files landed at the expected paths inside the
# tree.
#
# JetBrains Mono is DATA (arch-independent TTFs + an OFL license), so — like
# //bazel/orlando and //bazel/lighthouse — there is NO `select()`. The macOS app
# bundles this tree at `Contents/Resources/fonts/`, where CoreText registers the
# TTFs so "JetBrains Mono" resolves.
#
# It asserts, relative to the tree's inner `fonts/` dir (non-empty regular files):
#   a. JetBrainsMono-Regular.ttf
#   b. JetBrainsMono-Italic.ttf
#   c. JetBrainsMono-Bold.ttf
#   d. JetBrainsMono-BoldItalic.ttf
#   e. OFL.txt
#
# The tree comes from an http_archive (external repo `@jetbrains_mono`) assembled
# by `copy_to_directory` with `replace_prefixes` remapping the archive contents
# into an inner `fonts/` dir. The copy_to_directory output TreeArtifact is
# `jetbrains_mono_tree/`, whose inner dir is `fonts/`, so a file lands at
# `<tree>/fonts/JetBrainsMono-Regular.ttf`.
#
# Robust runfiles resolution: the exact runfiles layout for a TreeArtifact under
# aspect_bazel_lib can vary, and the alias routes through the `@jetbrains_mono`
# external repo, so we try $TEST_SRCDIR, $RUNFILES_DIR, and $0.runfiles, and
# finally fall back to a `find` for `*/jetbrains_mono_tree/fonts`.
#
# NOTE (coder): once the workspace/repo name is known, PIN the direct runfiles
# path (e.g. "$TEST_SRCDIR/_main/bazel/jetbrains-mono/jetbrains_mono_tree/fonts")
# as the first FONTS candidate below and keep the `find` fallback as a safety net.

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
# Locate the assembled jetbrains_mono tree (its inner `fonts/` dir).
# ---------------------------------------------------------------------------
FONTS=""
for root in $RUNFILES_ROOTS; do
  [ -d "$root" ] || continue
  # Direct, expected layouts; `-d` follows symlinks (runfiles entries are
  # symlinks into the output tree / external repo).
  for candidate in \
    "$root"/*/bazel/jetbrains-mono/jetbrains_mono_tree/fonts \
    "$root"/bazel/jetbrains-mono/jetbrains_mono_tree/fonts \
    "$root"/*/jetbrains_mono_tree/fonts \
    "$root"/*jetbrains_mono*/jetbrains_mono_tree/fonts
  do
    if [ -d "$candidate" ]; then
      FONTS="$candidate"
      break
    fi
  done
  [ -n "$FONTS" ] && break
  # Fallback: search anywhere under this runfiles root, following symlinks (-L)
  # so `find` descends into the symlinked TreeArtifact directory.
  found=$(find -L "$root" -type d -path '*/jetbrains_mono_tree/fonts' 2>/dev/null | head -n 1 || true)
  if [ -n "$found" ]; then
    FONTS="$found"
    break
  fi
done

[ -n "$FONTS" ] || fail "could not locate the jetbrains_mono_tree/fonts dir in runfiles. Searched roots:${RUNFILES_ROOTS:- (none)}"

echo "Located jetbrains_mono tree: $FONTS"

# ---------------------------------------------------------------------------
# (a)-(d) The four TTF faces are non-empty regular files.
# ---------------------------------------------------------------------------
for entry in \
  "a:JetBrainsMono-Regular.ttf" \
  "b:JetBrainsMono-Italic.ttf" \
  "c:JetBrainsMono-Bold.ttf" \
  "d:JetBrainsMono-BoldItalic.ttf"
do
  label="${entry%%:*}"
  ttf="${entry#*:}"
  path="$FONTS/$ttf"
  [ -f "$path" ] || fail "expected bundled font at $path, but it is missing"
  [ -s "$path" ] || fail "bundled font $path exists but is EMPTY (zero bytes)"
  echo "PASS check ($label): $path exists and is non-empty"
done

# ---------------------------------------------------------------------------
# (e) OFL.txt (the SIL Open Font License) is a non-empty regular file.
# ---------------------------------------------------------------------------
OFL="$FONTS/OFL.txt"
[ -f "$OFL" ] || fail "expected bundled OFL license at $OFL, but it is missing"
[ -s "$OFL" ] || fail "bundled OFL license $OFL exists but is EMPTY (zero bytes)"
echo "PASS check (e): $OFL exists and is non-empty"

echo "PASS: jetbrains_mono tree bundles the 4 JetBrains Mono TTF faces and OFL.txt"
