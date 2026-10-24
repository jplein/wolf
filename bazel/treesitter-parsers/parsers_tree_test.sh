#!/usr/bin/env bash
# Fails unless the go, rust, typescript, tsx and json parsers are non-empty; swift, kotlin, java
# and Mach-O status are only noted. See bazel/treesitter-parsers/README.md.

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

# --- Locate the assembled parsers tree (its inner `parsers/` dir).
TREE=""
for root in $RUNFILES_ROOTS; do
  [ -d "$root" ] || continue
  for candidate in \
    "$root"/*/bazel/treesitter-parsers/parsers_tree/parsers \
    "$root"/bazel/treesitter-parsers/parsers_tree/parsers \
    "$root"/*/parsers_tree/parsers \
    "$root"/*parsers*/parsers_tree/parsers
  do
    if [ -d "$candidate" ]; then
      TREE="$candidate"
      break
    fi
  done
  [ -n "$TREE" ] && break
  found=$(find -L "$root" -type d -path '*/parsers_tree/parsers' 2>/dev/null | head -n 1 || true)
  if [ -n "$found" ]; then
    TREE="$found"
    break
  fi
done

[ -n "$TREE" ] || fail "could not locate the parsers_tree/parsers dir in runfiles. Searched roots:${RUNFILES_ROOTS:- (none)}"

echo "Located parsers tree: $TREE"

# --- Helper: does `file` report this .so as Mach-O? soft note only, never fails.
note_macho() {
  so="$1"
  command -v file >/dev/null 2>&1 || return 0
  desc=$(file -b "$so" 2>/dev/null || true)
  case "$desc" in
    *Mach-O*) echo "NOTE: $so reports Mach-O ($desc)" ;;
    *) echo "NOTE: $so does NOT report Mach-O ($desc) — bundle requires signable host-compiled Mach-O" ;;
  esac
}

# --- Hard assertions: the low-risk core set must exist and be non-empty (-s).
check_letter="a"
for lang in go rust typescript tsx json; do
  so="$TREE/parser/$lang.so"
  [ -f "$so" ] || fail "expected compiled parser at $so, but it is missing"
  [ -s "$so" ] || fail "compiled parser $so exists but is EMPTY (zero bytes) — build produced no code"
  echo "PASS check ($check_letter): $so exists and is non-empty"
  note_macho "$so"
  check_letter=$(printf '%s' "$check_letter" | tr 'a-y' 'b-z')
done

# --- Soft notes: higher-risk grammars per drop+LOG policy — never fail on these.
for lang in swift kotlin java; do
  so="$TREE/parser/$lang.so"
  if [ -f "$so" ] && [ -s "$so" ]; then
    echo "NOTE: optional parser $so is present and non-empty"
    note_macho "$so"
  else
    echo "NOTE: optional parser parser/$lang.so is MISSING or empty (drop+LOG policy — acceptable; see bazel/treesitter-parsers/README.md status table)"
  fi
done

echo "PASS: parsers tree bundles the low-risk core parsers (go, rust, typescript, tsx, json) as non-empty files"
