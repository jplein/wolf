#!/bin/sh
# Compiles the xterm-ghostty terminfo entry: build_terminfo.sh SRC.tar.gz GEN.zig ZIG OUT_DIR
# See bazel/ghostty-vt/README.md
set -eu

# The action env carries the Xcode keys only, so the script names its own tools' directories.
PATH=/usr/bin:/bin
export PATH

abs() {
  case "$1" in
  /*) printf '%s\n' "$1" ;;
  *) printf '%s\n' "$PWD/$1" ;;
  esac
}

src=$(abs "$1")
generator=$(abs "$2")
zig=$(abs "$3")
out=$(abs "$4")

# Scratch beside the output, never a literal /tmp, which a sandboxed build may not write.
work="${out}.build"
rm -rf "$work"
mkdir -p "$work/zig-global" "$work/zig-local"
trap 'rm -rf "$work"' EXIT

tar -xzf "$src" -C "$work" '*/src/terminfo'
source_zig=$(find "$work" -path '*/src/terminfo/ghostty.zig' -type f)

ZIG_GLOBAL_CACHE_DIR="$work/zig-global" \
  ZIG_LOCAL_CACHE_DIR="$work/zig-local" \
  "$zig" run --dep ghostty -Mroot="$generator" -Mghostty="$source_zig" >"$work/ghostty.terminfo"

# The system tic writes the hex layout (78/xterm-ghostty) the host's libncurses reads. It warns
# on every build that the last name reads as a description, so its stderr shows only on failure.
if ! /usr/bin/tic -x -o "$work/db" "$work/ghostty.terminfo" 2>"$work/tic.log"; then
  cat "$work/tic.log" >&2
  exit 1
fi

rm -rf "$out"
mkdir -p "$out/terminfo"
cp -RL "$work/db/." "$out/terminfo/"
test -f "$out/terminfo/78/xterm-ghostty"
