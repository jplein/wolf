#!/usr/bin/env bash
# Asserts the Mono face and its two upstream notices are non-empty in the tree's inner `fonts/`
# dir, and the proportional face and upstream .conf are not (this package's README.md).

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

# --- Locate the assembled nerd_fonts_symbols tree (its inner `fonts/` dir).
FONTS=""
for root in $RUNFILES_ROOTS; do
  [ -d "$root" ] || continue
  # Direct, expected layouts; `-d` follows symlinks (runfiles entries are
  # symlinks into the output tree / external repo).
  for candidate in \
    "$root"/*/bazel/nerd-fonts-symbols/nerd_fonts_symbols_tree/fonts \
    "$root"/bazel/nerd-fonts-symbols/nerd_fonts_symbols_tree/fonts \
    "$root"/*/nerd_fonts_symbols_tree/fonts \
    "$root"/*nerd_fonts_symbols*/nerd_fonts_symbols_tree/fonts
  do
    if [ -d "$candidate" ]; then
      FONTS="$candidate"
      break
    fi
  done
  [ -n "$FONTS" ] && break
  # Fallback: search anywhere under this runfiles root, following symlinks (-L)
  # so `find` descends into the symlinked TreeArtifact directory.
  found=$(find -L "$root" -type d -path '*/nerd_fonts_symbols_tree/fonts' 2>/dev/null | head -n 1 || true)
  if [ -n "$found" ]; then
    FONTS="$found"
    break
  fi
done

[ -n "$FONTS" ] || fail "could not locate the nerd_fonts_symbols_tree/fonts dir in runfiles. Searched roots:${RUNFILES_ROOTS:- (none)}"

echo "Located nerd_fonts_symbols tree: $FONTS"

# --- (a)-(c) The Mono face, the MIT licence and the README carrying the icon-set licence table.
for entry in \
  "a:SymbolsNerdFontMono-Regular.ttf:the Symbols Nerd Font Mono face" \
  "b:nerd-fonts-symbols-LICENSE:the Nerd Fonts MIT licence (the archive's LICENSE)" \
  "c:nerd-fonts-symbols-README.md:the icon-set licence table (the archive's README.md)"
do
  label="${entry%%:*}"
  rest="${entry#*:}"
  name="${rest%%:*}"
  what="${rest#*:}"
  path="$FONTS/$name"
  [ -f "$path" ] || fail "expected $what at $path, but it is missing: the overlay's runtime_files or its replace_prefixes no longer maps it there"
  [ -s "$path" ] || fail "$what at $path exists but is EMPTY (zero bytes)"
  echo "PASS check ($label): $path exists and is non-empty"
done

# --- (d) The proportional face and the upstream fontconfig file stay out of the tree.
for name in SymbolsNerdFont-Regular.ttf 10-nerd-font-symbols.conf; do
  path="$FONTS/$name"
  if [ -e "$path" ]; then
    case "$name" in
      *.ttf) why="it maps the same code points as the Mono face at up to ~1.3 em advances, so CoreText fallback may pick the wider face and draw icons that overflow their cell" ;;
      *) why="its aliases prefer the proportional family for every monospace family" ;;
    esac
    fail "the tree carries $path, which the overlay must leave out: $why"
  fi
  echo "PASS check (d): $name is absent"
done

echo "PASS: nerd_fonts_symbols tree bundles SymbolsNerdFontMono-Regular.ttf, nerd-fonts-symbols-LICENSE and nerd-fonts-symbols-README.md, and leaves out the proportional face and 10-nerd-font-symbols.conf"
