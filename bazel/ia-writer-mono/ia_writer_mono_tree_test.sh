#!/usr/bin/env bash
# Asserts the four iA Writer Mono S faces and LICENSE.md are non-empty in the tree's inner
# `fonts/` dir (bazel/ia-writer-mono/README.md). Every runfiles root is probed, then `find`.

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

# --- Locate the assembled ia_writer_mono tree (its inner `fonts/` dir).
FONTS=""
for root in $RUNFILES_ROOTS; do
  [ -d "$root" ] || continue
  # Direct, expected layouts; `-d` follows symlinks (runfiles entries are
  # symlinks into the output tree / external repo).
  for candidate in \
    "$root"/*/bazel/ia-writer-mono/ia_writer_mono_tree/fonts \
    "$root"/bazel/ia-writer-mono/ia_writer_mono_tree/fonts \
    "$root"/*/ia_writer_mono_tree/fonts \
    "$root"/*ia_writer_mono*/ia_writer_mono_tree/fonts
  do
    if [ -d "$candidate" ]; then
      FONTS="$candidate"
      break
    fi
  done
  [ -n "$FONTS" ] && break
  # Fallback: search anywhere under this runfiles root, following symlinks (-L)
  # so `find` descends into the symlinked TreeArtifact directory.
  found=$(find -L "$root" -type d -path '*/ia_writer_mono_tree/fonts' 2>/dev/null | head -n 1 || true)
  if [ -n "$found" ]; then
    FONTS="$found"
    break
  fi
done

[ -n "$FONTS" ] || fail "could not locate the ia_writer_mono_tree/fonts dir in runfiles. Searched roots:${RUNFILES_ROOTS:- (none)}"

echo "Located ia_writer_mono tree: $FONTS"

# --- (a)-(d) The four TTF faces are non-empty regular files.
for entry in \
  "a:iAWriterMonoS-Regular.ttf" \
  "b:iAWriterMonoS-Italic.ttf" \
  "c:iAWriterMonoS-Bold.ttf" \
  "d:iAWriterMonoS-BoldItalic.ttf"
do
  label="${entry%%:*}"
  ttf="${entry#*:}"
  path="$FONTS/$ttf"
  [ -f "$path" ] || fail "expected bundled font at $path, but it is missing"
  [ -s "$path" ] || fail "bundled font $path exists but is EMPTY (zero bytes)"
  echo "PASS check ($label): $path exists and is non-empty"
done

# --- (e) LICENSE.md (the iA SIL Open Font License) is a non-empty regular file.
LICENSE="$FONTS/LICENSE.md"
[ -f "$LICENSE" ] || fail "expected bundled iA license at $LICENSE, but it is missing"
[ -s "$LICENSE" ] || fail "bundled iA license $LICENSE exists but is EMPTY (zero bytes)"
echo "PASS check (e): $LICENSE exists and is non-empty"

echo "PASS: ia_writer_mono tree bundles the 4 iA Writer Mono S TTF faces and LICENSE.md"
