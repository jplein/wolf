#!/bin/sh
# Runs the pinned nvim on queries_compile_probe.lua ($1) to compile every vendored nvim-treesitter
# highlights/injections query against its pinned parser; see bazel/treesitter-parsers/README.md.

set -eu

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

[ "$#" -ge 1 ] || fail "missing probe path argument (the sh_test passes \$(rootpath :queries_compile_probe.lua))"

# --- Locate the runfiles roots; any may be unset, depending on how Bazel runs the test.
RUNFILES_ROOTS=""
[ "${TEST_SRCDIR:-}" != "" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $TEST_SRCDIR"
[ "${RUNFILES_DIR:-}" != "" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $RUNFILES_DIR"
[ -d "$0.runfiles" ] && RUNFILES_ROOTS="$RUNFILES_ROOTS $0.runfiles"

# The runfiles entries are symlinks; nvim locates VIMRUNTIME from its realpath, so resolve them.
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

canonical_dir() {
  (CDPATH= cd -- "$1" && pwd -P)
}

# --- Locate the probe. `rootpath` is relative to the main repo, the test's usual cwd.
PROBE=""
if [ -f "$1" ]; then
  PROBE="$1"
else
  for root in $RUNFILES_ROOTS; do
    for candidate in "$root/_main/$1" "$root/$1"; do
      if [ -f "$candidate" ]; then
        PROBE="$candidate"
        break
      fi
    done
    [ -n "$PROBE" ] && break
  done
fi
[ -n "$PROBE" ] || fail "could not locate probe '$1' from cwd $(pwd) or runfiles roots:${RUNFILES_ROOTS:- (none)}"
PROBE="$(canonicalize "$PROBE")"
echo "Located probe: $PROBE"

# --- Locate the nvim binary (same probing as bazel/nvim/nvim_version_test.sh).
NVIM=""
for root in $RUNFILES_ROOTS; do
  [ -d "$root" ] || continue
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
  # `-L` so `find` descends into the symlinked TreeArtifact directory.
  found=$(find -L "$root" -type f -path '*/nvim/bin/nvim' 2>/dev/null | head -n 1 || true)
  if [ -n "$found" ]; then
    NVIM="$found"
    break
  fi
done
[ -n "$NVIM" ] || fail "could not locate the pinned nvim binary in runfiles. Searched roots:${RUNFILES_ROOTS:- (none)}"
NVIM="$(canonicalize "$NVIM")"
[ -x "$NVIM" ] || fail "nvim binary is not executable: $NVIM"
echo "Located nvim: $NVIM"

# --- Locate the nvim-treesitter tree's inner `nvim-treesitter/` dir.
TS_DIR=""
for root in $RUNFILES_ROOTS; do
  [ -d "$root" ] || continue
  for candidate in \
    "$root"/*/bazel/nvim-treesitter/nvim_treesitter_tree/nvim-treesitter \
    "$root"/bazel/nvim-treesitter/nvim_treesitter_tree/nvim-treesitter \
    "$root"/*/nvim_treesitter_tree/nvim-treesitter \
    "$root"/*nvim*treesitter*/nvim_treesitter_tree/nvim-treesitter
  do
    if [ -d "$candidate" ]; then
      TS_DIR="$candidate"
      break
    fi
  done
  [ -n "$TS_DIR" ] && break
  found=$(find -L "$root" -type d -path '*/nvim_treesitter_tree/nvim-treesitter' 2>/dev/null | head -n 1 || true)
  if [ -n "$found" ]; then
    TS_DIR="$found"
    break
  fi
done
[ -n "$TS_DIR" ] || fail "could not locate the nvim_treesitter_tree/nvim-treesitter dir in runfiles. Searched roots:${RUNFILES_ROOTS:- (none)}"
TS_DIR="$(canonical_dir "$TS_DIR")"
[ -d "$TS_DIR/queries" ] || fail "nvim-treesitter tree has no queries/ dir: $TS_DIR (overlay glob dropped runtime/queries?)"
echo "Located nvim-treesitter tree: $TS_DIR"

# --- Locate the parsers tree's `parsers/parser` dir.
PARSERS=""
for root in $RUNFILES_ROOTS; do
  [ -d "$root" ] || continue
  for candidate in \
    "$root"/*/bazel/treesitter-parsers/parsers_tree/parsers \
    "$root"/bazel/treesitter-parsers/parsers_tree/parsers \
    "$root"/*/parsers_tree/parsers \
    "$root"/*parsers*/parsers_tree/parsers
  do
    if [ -d "$candidate" ]; then
      PARSERS="$candidate"
      break
    fi
  done
  [ -n "$PARSERS" ] && break
  found=$(find -L "$root" -type d -path '*/parsers_tree/parsers' 2>/dev/null | head -n 1 || true)
  if [ -n "$found" ]; then
    PARSERS="$found"
    break
  fi
done
[ -n "$PARSERS" ] || fail "could not locate the parsers_tree/parsers dir in runfiles. Searched roots:${RUNFILES_ROOTS:- (none)}"
[ -d "$PARSERS/parser" ] || fail "parsers tree has no parser/ dir: $PARSERS"
PARSER_DIR="$(canonical_dir "$PARSERS/parser")"
echo "Located parser dir: $PARSER_DIR"

# --- Enumerate the compiled languages: every parser/<lang>.so.
LANGS=""
for so in "$PARSER_DIR"/*.so; do
  [ -f "$so" ] || continue
  lang=$(basename "$so" .so)
  LANGS="$LANGS $lang"
done
[ -n "$LANGS" ] || fail "no parser/*.so under $PARSER_DIR; nothing to compile queries against"
echo "Languages:$LANGS"

# --- Isolate nvim from the host's config, data, state and cache.
SCRATCH="${TEST_TMPDIR:-$(mktemp -d)}/queries_compile"
for d in config data state cache; do
  mkdir -p "$SCRATCH/$d"
done
export XDG_CONFIG_HOME="$SCRATCH/config"
export XDG_DATA_HOME="$SCRATCH/data"
export XDG_STATE_HOME="$SCRATCH/state"
export XDG_CACHE_HOME="$SCRATCH/cache"

# --- Run the probe. Output is captured so the per-language verdicts can be cross-checked.
OUT="$SCRATCH/probe.out"
status=0
"$NVIM" --clean -n -i NONE --headless -l "$PROBE" "$TS_DIR" "$PARSER_DIR" $LANGS \
  >"$OUT" 2>&1 </dev/null || status=$?
echo "---- probe output (nvim exit status $status) ----"
cat "$OUT"
echo "-------------------------------------------------"

if [ "$status" -ne 0 ]; then
  if grep -q '^FAIL ' "$OUT"; then
    fail "nvim exited $status: the FAIL lines above name each language whose query did not compile"
  fi
  fail "nvim exited $status with no FAIL verdict: nvim or the probe itself errored before reporting"
fi

# Exit 0 alone could mean the probe never saw the languages; require each verdict explicitly.
if grep -q '^FAIL ' "$OUT"; then
  fail "probe printed a FAIL verdict but nvim exited 0 (probe exit code lost)"
fi
for lang in $LANGS; do
  grep -qxF "PASS $lang" "$OUT" || fail "nvim exited 0 but printed no 'PASS $lang' (argument passing to -l broken?)"
done

echo "PASS: every vendored highlights/injections query compiles against its pinned parser:$LANGS"
