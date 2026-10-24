#!/bin/sh
# Asserts the bundled nvim is executable, reports `NVIM v0.12.3`, self-locates VIMRUNTIME inside
# the tree and ships `markdown.so`; see bazel/nvim/README.md. Runfiles roots are probed.

set -eu

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# --- Locate the runfiles roots; any may be unset, depending on how Bazel runs the test.
RUNFILES_ROOTS=""
[ "${TEST_SRCDIR:-}" != "" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $TEST_SRCDIR"
[ "${RUNFILES_DIR:-}" != "" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $RUNFILES_DIR"
[ -d "$0.runfiles" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $0.runfiles"

# --- Locate the nvim binary.
NVIM=""
for root in $RUNFILES_ROOTS; do
  [ -d "$root" ] || continue
  # Direct layout: the runfiles `nvim` entry is a symlink to the TreeArtifact of the repo
  # `nvim_macos_<arch>`, so `-e`, which follows symlinks, resolves the binary.
  for candidate in \
    "$root"/*nvim_macos_*/nvim/bin/nvim \
    "$root"/*/nvim/bin/nvim \
    "$root"/bazel/nvim/nvim/bin/nvim
  do
    if [ -e "$candidate" ]; then
      NVIM="$candidate"
      break
    fi
  done
  [ -n "$NVIM" ] && break
  # Fallback: search anywhere under this runfiles root for */nvim/bin/nvim.
  # `-L` so `find` descends into the symlinked TreeArtifact directory.
  found=$(find -L "$root" -type f -path '*/nvim/bin/nvim' 2>/dev/null | head -n 1 || true)
  if [ -n "$found" ]; then
    NVIM="$found"
    break
  fi
done

[ -n "$NVIM" ] || fail "could not locate bundled nvim binary in runfiles. Searched roots:${RUNFILES_ROOTS:- (none)}"

# The runfiles `nvim` is a symlink, and nvim locates VIMRUNTIME from its realpath, so
# canonicalize to compare like with like.
canonicalize() {
  cp="$1"
  while [ -L "$cp" ]; do
    ct=$(readlink "$cp")
    case "$ct" in
      /*) cp="$ct" ;;
      *) cp="$(dirname "$cp")/$ct" ;;
    esac
  done
  printf '%s\n' "$(CDPATH= cd -- "$(dirname "$cp")" && pwd -P)/$(basename "$cp")"
}
NVIM="$(canonicalize "$NVIM")"

echo "Located nvim: $NVIM"

# The bundle prefix is the directory two levels up from bin/nvim
# (.../nvim/bin/nvim -> .../nvim).
NVIM_BIN_DIR=$(dirname "$NVIM")
NVIM_PREFIX=$(dirname "$NVIM_BIN_DIR")
echo "Bundle prefix: $NVIM_PREFIX"

# --- (a) binary exists and is executable.
[ -f "$NVIM" ] || fail "expected a regular file at $NVIM, but it is not"
[ -x "$NVIM" ] || fail "nvim binary is not executable (exec bit lost?): $NVIM"
echo "PASS check (a): nvim binary exists and is executable"

# --- (b) version pin: NVIM v0.12.3
EXPECTED_VERSION="NVIM v0.12.3"
VERSION_OUT=$("$NVIM" --version 2>&1 || true)
if ! printf '%s\n' "$VERSION_OUT" | grep -q 'NVIM v0\.12\.3'; then
  echo "---- nvim --version output ----" >&2
  printf '%s\n' "$VERSION_OUT" >&2
  echo "-------------------------------" >&2
  fail "version mismatch: expected output to contain '$EXPECTED_VERSION', got the above"
fi
echo "PASS check (b): nvim reports $EXPECTED_VERSION"

# --- (c) Self-location: run with no user config, VIMRUNTIME resolves inside the bundled tree.
RUNTIME_CAP=$(mktemp 2>/dev/null || echo "${TMPDIR:-/tmp}/nvim_runtime_cap.$$")
"$NVIM" --headless -u NONE -i NONE -n \
  --cmd 'lua io.stderr:write(vim.env.VIMRUNTIME or "")' \
  -c 'qa!' 2>"$RUNTIME_CAP" || true

VIMRUNTIME=$(cat "$RUNTIME_CAP" 2>/dev/null || true)
rm -f "$RUNTIME_CAP" 2>/dev/null || true

[ -n "$VIMRUNTIME" ] || fail "VIMRUNTIME was empty; nvim did not report its runtime path (self-location check)"
echo "Reported VIMRUNTIME: $VIMRUNTIME"

# It must point inside the bundled tree, NOT a system path.
case "$VIMRUNTIME" in
  "$NVIM_PREFIX"/*)
    : # good: inside the bundled prefix
    ;;
  /usr/* | /opt/homebrew/* | /opt/local/* | /Applications/*)
    fail "VIMRUNTIME points to a SYSTEM path, not the bundled tree: $VIMRUNTIME (expected under $NVIM_PREFIX)"
    ;;
  *)
    fail "VIMRUNTIME does not resolve inside the bundled tree: got '$VIMRUNTIME', expected a path under '$NVIM_PREFIX'"
    ;;
esac
echo "PASS check (c): VIMRUNTIME resolves inside the bundled tree"

# --- (d) bonus: a shipped treesitter parser exists relative to the prefix.
MARKDOWN_SO="$NVIM_PREFIX/lib/nvim/parser/markdown.so"
[ -f "$MARKDOWN_SO" ] || fail "expected shipped treesitter parser missing: $MARKDOWN_SO"
echo "PASS check (d): bundled treesitter parser present ($MARKDOWN_SO)"

echo "PASS: bundled nvim is v0.12.3, executable, self-locates its runtime inside the tree, and ships the markdown parser"
