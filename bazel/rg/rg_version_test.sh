#!/bin/sh
#
# rg_version_test.sh
#
# sh_test that exercises the pinned, bundled ripgrep 15.2.0 TreeArtifact
# (target //bazel/rg:rg_tree, declared as `data` on the sh_test).
#
# It asserts:
#   a. the bundled `rg` binary exists and is executable (catches exec-bit loss
#      through http_archive -> copy_to_directory);
#   b. `rg --version` reports exactly `ripgrep 15.2.0` (asserts the pin).
#
# Robust runfiles resolution: the exact runfiles layout for a TreeArtifact under
# rules_apple / aspect_bazel_lib can vary, so we try $TEST_SRCDIR, $RUNFILES_DIR,
# and $0.runfiles, and finally fall back to a `find` for `*/rg/rg`. The TreeArtifact
# directory is named `rg/`, so the binary lands at
# `<runfiles>/<repo>/rg/rg`.

set -eu

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# ---------------------------------------------------------------------------
# Locate the runfiles root.
# ---------------------------------------------------------------------------
RUNFILES_ROOTS=""
[ "${TEST_SRCDIR:-}" != "" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $TEST_SRCDIR"
[ "${RUNFILES_DIR:-}" != "" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $RUNFILES_DIR"
[ -d "$0.runfiles" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $0.runfiles"

# ---------------------------------------------------------------------------
# Locate the rg binary.
# ---------------------------------------------------------------------------
RG=""
for root in $RUNFILES_ROOTS; do
  [ -d "$root" ] || continue
  # Direct, expected layout. The `rg/` TreeArtifact comes from the http_archive
  # repo `rg_macos_<arch>` (bzlmod canonical name like `+rg+rg_macos_arm64`), and
  # the runfiles `rg` entry is a SYMLINK to the TreeArtifact directory, so `-e`
  # (which follows symlinks) resolves the binary.
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

# ---------------------------------------------------------------------------
# (a) binary exists and is executable.
# ---------------------------------------------------------------------------
[ -f "$RG" ] || fail "expected a regular file at $RG, but it is not"
[ -x "$RG" ] || fail "rg binary is not executable (exec bit lost?): $RG"
echo "PASS check (a): rg binary exists and is executable"

# ---------------------------------------------------------------------------
# (b) version pin: ripgrep 15.2.0
# ---------------------------------------------------------------------------
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
