#!/bin/sh
# Asserts the Zig at the rootpath in $1 is executable, reports `0.17.0` and finds its lib/.
# See specs/terminal-panes.md#building-libghostty-vt

set -eu

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

[ "$#" -eq 1 ] || fail "expected one argument, the Zig binary's rootpath; got $#"
ZIG="$1"
echo "Zig under test: $ZIG"

# --- (a) binary exists and is executable.
[ -f "$ZIG" ] || fail "expected a regular file at $ZIG, so the runfiles do not carry the binary"
[ -x "$ZIG" ] || fail "Zig binary is not executable (exec bit lost?): $ZIG"
echo "PASS check (a): Zig binary exists and is executable"

# A test has no usable HOME cache, and Zig 0.17 can crash on a cache it did not create.
: "${TEST_TMPDIR:?TEST_TMPDIR is unset, so there is nowhere to put a fresh Zig cache}"
ZIG_GLOBAL_CACHE_DIR=$(mktemp -d "$TEST_TMPDIR/zig-global.XXXXXX")
ZIG_LOCAL_CACHE_DIR=$(mktemp -d "$TEST_TMPDIR/zig-local.XXXXXX")
export ZIG_GLOBAL_CACHE_DIR ZIG_LOCAL_CACHE_DIR

# --- (b) version pin: exactly 0.17.0, the minor libghostty-vt 1.0.0's build.zig.zon requires.
EXPECTED_VERSION="0.17.0"
VERSION_STATUS=0
VERSION_OUT=$("$ZIG" version 2>&1) || VERSION_STATUS=$?
[ "$VERSION_STATUS" -eq 0 ] ||
  fail "'zig version' exited $VERSION_STATUS, so the binary does not run here. Output: $VERSION_OUT"
[ "$VERSION_OUT" = "$EXPECTED_VERSION" ] ||
  fail "'zig version' printed '$VERSION_OUT', not exactly '$EXPECTED_VERSION': another pin"
echo "PASS check (b): Zig reports $EXPECTED_VERSION"

# --- (c) `zig env` resolves the lib/ directory beside the binary, or exits non-zero.
ENV_STATUS=0
ENV_OUT=$("$ZIG" env 2>&1) || ENV_STATUS=$?
if [ "$ENV_STATUS" -ne 0 ]; then
  echo "---- zig env output ----" >&2
  printf '%s\n' "$ENV_OUT" >&2
  echo "------------------------" >&2
  fail "'zig env' exited $ENV_STATUS: Zig cannot find its lib/ (the :zig_files runfiles)"
fi
echo "PASS check (c): zig env exits 0"

echo "PASS: bundled Zig is $EXPECTED_VERSION, executable, and finds its lib/"
