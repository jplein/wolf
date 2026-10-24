#!/bin/sh
# Asserts the bundled rg is executable and reports `ripgrep 15.2.0` (bazel/rg/README.md).
# Every runfiles root is probed, then `find`.

set -eu

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# --- Locate the runfiles root.
RUNFILES_ROOTS=""
[ "${TEST_SRCDIR:-}" != "" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $TEST_SRCDIR"
[ "${RUNFILES_DIR:-}" != "" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $RUNFILES_DIR"
[ -d "$0.runfiles" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $0.runfiles"

# --- Locate the rg binary.
RG=""
for root in $RUNFILES_ROOTS; do
  [ -d "$root" ] || continue
  # Direct layout: the runfiles `rg` entry is a symlink to the TreeArtifact of the repo
  # `rg_macos_<arch>`, so `-e`, which follows symlinks, resolves the binary.
  for candidate in \
    "$root"/*rg_macos_*/rg/rg \
    "$root"/*/rg/rg \
    "$root"/bazel/rg/rg/rg
  do
    if [ -e "$candidate" ]; then
      RG="$candidate"
      break
    fi
  done
  [ -n "$RG" ] && break
  # Fallback: search anywhere under this runfiles root for */rg/rg.
  found=$(find -L "$root" -type f -path '*/rg/rg' 2>/dev/null | head -n 1 || true)
  if [ -n "$found" ]; then
    RG="$found"
    break
  fi
done

[ -n "$RG" ] || fail "could not locate bundled rg binary in runfiles. Searched roots:${RUNFILES_ROOTS:- (none)}"

echo "Located rg: $RG"

# --- (a) binary exists and is executable.
[ -f "$RG" ] || fail "expected a regular file at $RG, but it is not"
[ -x "$RG" ] || fail "rg binary is not executable (exec bit lost?): $RG"
echo "PASS check (a): rg binary exists and is executable"

# --- (b) version pin: ripgrep 15.2.0
EXPECTED_VERSION="ripgrep 15.2.0"
VERSION_OUT=$("$RG" --version 2>&1 || true)
if ! printf '%s\n' "$VERSION_OUT" | grep -q 'ripgrep 15\.2\.0'; then
  echo "---- rg --version output ----" >&2
  printf '%s\n' "$VERSION_OUT" >&2
  echo "-----------------------------" >&2
  fail "version mismatch: expected output to contain '$EXPECTED_VERSION', got the above"
fi
echo "PASS check (b): rg reports $EXPECTED_VERSION"

echo "PASS: bundled rg is 15.2.0 and executable"
