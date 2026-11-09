#!/bin/sh
# Builds libghostty-vt.a with the pinned Zig: build_archive.sh SRC.tar.gz DEPS.tar.gz ZIG OUT.a
# See specs/terminal-panes.md#building-libghostty-vt
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
deps=$(abs "$2")
zig=$(abs "$3")
out=$(abs "$4")

# Scratch beside the output, never a literal /tmp, which a sandboxed build may not write.
work="${out}.build"
rm -rf "$work"
mkdir -p "$work/bin" "$work/tmp" "$work/zig-global" "$work/zig-local"
trap 'rm -rf "$work"' EXIT

tar -xzf "$src" -C "$work"
tar -xzf "$deps" -C "$work"
srcdir=$(find "$work" -maxdepth 1 -type d -name 'libghostty-vt-*' ! -name '*-deps')
depsdir=$(find "$work" -maxdepth 1 -type d -name 'libghostty-vt-*-deps')

# macOS `mktemp -d` ignores TMPDIR, and the library's libsystem_override.sh calls it bare: the
# shim keeps its scratch in TMPDIR, since a sandboxed build may not write the user temp dir.
cat >"$work/bin/mktemp" <<'EOF'
#!/bin/sh
if [ "$#" -eq 1 ] && [ "$1" = "-d" ]; then exec /usr/bin/mktemp -d "$TMPDIR/tmp.XXXXXX"; fi
exec /usr/bin/mktemp "$@"
EOF
chmod +x "$work/bin/mktemp"

# Zig 0.17 can crash on a global cache it did not create, so each build gets an empty one.
cd "$srcdir"
PATH="$work/bin:$PATH" \
  TMPDIR="$work/tmp" \
  ZIG_GLOBAL_CACHE_DIR="$work/zig-global" \
  "$zig" build \
  --prefix "$work/out" \
  --system "$depsdir" \
  --cache-dir "$work/zig-local" \
  -Doptimize=ReleaseFast \
  -Dcpu=baseline

cp "$work/out/lib/libghostty-vt.a" "$out"
