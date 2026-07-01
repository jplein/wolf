#!/usr/bin/env bash
#
# parsers_tree_test.sh
#
# sh_test that inspects the assembled compiled-parser TreeArtifact (target
# //bazel/treesitter-parsers:parsers_tree, declared as `data` on the sh_test) and
# asserts the curated set of host-compiled treesitter parser shared objects
# landed at the expected paths inside the tree.
#
# It asserts, relative to the tree's inner `parsers/` dir:
#   HARD (must pass — the low-risk core set):
#     a. parser/go.so           regular, NON-EMPTY file
#     b. parser/rust.so         regular, NON-EMPTY file
#     c. parser/typescript.so   regular, NON-EMPTY file
#     d. parser/tsx.so          regular, NON-EMPTY file
#     e. parser/json.so         regular, NON-EMPTY file
#   SOFT (echo a NOTE, NEVER fail — drop+LOG policy per the plan):
#     - parser/swift.so, parser/kotlin.so, parser/java.so — these are the
#       higher-risk grammars that may be dropped if they cannot compile
#       hermetically. java is low-risk so likely present, but kept soft.
#     - if `file` is available, SOFT-note whether each present `.so` reports as
#       Mach-O (the bundle requires signable host-compiled Mach-O, not nix-built).
#
# NOTE: this change VENDORS compiled parsers only; it does NOT enable treesitter
# highlighting. The parsers are shipped for a future enablement change.
#
# The tree comes from `copy_to_directory` with `replace_prefixes` remapping each
# grammar's `parser/<lang>.so` under an inner `parsers/` dir. The copy_to_directory
# output TreeArtifact is `parsers_tree/`, whose inner dir is `parsers/`, so the
# files land at `<tree>/parsers/parser/go.so` etc.
#
# Robust runfiles resolution: mirror the orlando/lighthouse tree tests — try
# $TEST_SRCDIR, $RUNFILES_DIR, and $0.runfiles, then fall back to a `find`.
#
# NOTE (coder): once the workspace/repo name is known, PIN the direct runfiles
# path (e.g. "$TEST_SRCDIR/_main/bazel/treesitter-parsers/parsers_tree/parsers")
# as the first TREE candidate below and keep the `find` fallback only as a safety
# net. Also confirm the HARD set matches the final compiled set (drop+LOG any
# grammar that cannot build; hard asserts only cover the low-risk core).

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
# Locate the assembled parsers tree (its inner `parsers/` dir).
# ---------------------------------------------------------------------------
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

# ---------------------------------------------------------------------------
# Helper: does `file` report this .so as Mach-O? SOFT note only, never fails.
# ---------------------------------------------------------------------------
note_macho() {
  so="$1"
  command -v file >/dev/null 2>&1 || return 0
  desc=$(file -b "$so" 2>/dev/null || true)
  case "$desc" in
    *Mach-O*) echo "NOTE: $so reports Mach-O ($desc)" ;;
    *) echo "NOTE: $so does NOT report Mach-O ($desc) — bundle requires signable host-compiled Mach-O" ;;
  esac
}

# ---------------------------------------------------------------------------
# HARD assertions: the low-risk core set must exist and be NON-EMPTY (-s).
# ---------------------------------------------------------------------------
check_letter="a"
for lang in go rust typescript tsx json; do
  so="$TREE/parser/$lang.so"
  [ -f "$so" ] || fail "expected compiled parser at $so, but it is missing"
  [ -s "$so" ] || fail "compiled parser $so exists but is EMPTY (zero bytes) — build produced no code"
  echo "PASS check ($check_letter): $so exists and is non-empty"
  note_macho "$so"
  check_letter=$(printf '%s' "$check_letter" | tr 'a-y' 'b-z')
done

# ---------------------------------------------------------------------------
# SOFT notes: higher-risk grammars per drop+LOG policy — never fail on these.
# ---------------------------------------------------------------------------
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
