#!/bin/sh
#
# bundle_layout_test.sh
#
# sh_test that inspects the built macOS application bundle (target
# //crates/wolf-macos:Wolf, declared as `data` on the sh_test) and asserts the
# pinned Neovim tree + captive init.lua + vendored orlando plugin landed at the
# expected resource paths.
#
# It asserts, relative to `Wolf.app/`:
#   a. Contents/Resources/nvim/bin/nvim                       exists & executable
#   b. Contents/Resources/nvim/lib/nvim/parser/markdown.so   exists
#   c. Contents/Resources/nvim/share/nvim/runtime            exists & is a dir
#   d. Contents/Resources/init.lua                           exists & is a file
#   e. Contents/Resources/orlando/plugin/orlando.lua         exists & is a file
#   f. Contents/Resources/orlando/lua/orlando/init.lua       exists & is a file
#
# This test only inspects files; it does NOT execute nvim and needs no network.
#
# rules_apple may deliver the application as either a `Wolf.app` directory OR a
# zipped archive in runfiles, so we handle BOTH forms.
#
# NOTE to coder: confirm which form rules_apple 4.5.3 delivers `:Wolf` as in the
# test runfiles (an unzipped `Wolf.app` directory, or a `Wolf.zip`/ipa-like
# archive). Once known, pin the direct runfiles path and simplify this resolver
# to the single form that actually occurs.

set -eu

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
# Locate the bundle: prefer an unzipped Wolf.app directory; else a Wolf archive.
# ---------------------------------------------------------------------------
# NOTE to coder: once the workspace/repo name is known, prefer the direct path
# (e.g. "$TEST_SRCDIR/_main/crates/wolf-macos/Wolf.app").
APP_DIR=""
APP_ARCHIVE=""
for root in $RUNFILES_ROOTS; do
  [ -d "$root" ] || continue

  # Direct, expected layout. rules_apple 4.5.3 delivers `:Wolf` as a `Wolf.zip`
  # archive in runfiles (a SYMLINK), so probe the direct path first; `-e`/`-d`
  # follow symlinks.
  for cand in \
    "$root"/*/crates/wolf-macos/Wolf.app \
    "$root"/crates/wolf-macos/Wolf.app
  do
    if [ -d "$cand" ]; then
      APP_DIR="$cand"
      break
    fi
  done
  [ -n "$APP_DIR" ] && break

  for cand in \
    "$root"/*/crates/wolf-macos/Wolf.zip \
    "$root"/crates/wolf-macos/Wolf.zip
  do
    if [ -e "$cand" ]; then
      APP_ARCHIVE="$cand"
      break
    fi
  done
  [ -n "$APP_ARCHIVE" ] && break

  # Fallbacks: search this runfiles root, following symlinks (-L) since runfiles
  # entries are symlinks into the output tree.
  found_dir=$(find -L "$root" -type d -name 'Wolf.app' 2>/dev/null | head -n 1 || true)
  if [ -n "$found_dir" ]; then
    APP_DIR="$found_dir"
    break
  fi

  # Otherwise an archive (zip/ipa-like) that we unzip below.
  found_zip=$(find -L "$root" -type f \
    \( -name 'Wolf.zip' -o -name 'Wolf.ipa' -o -name 'Wolf.app.zip' \) \
    2>/dev/null | head -n 1 || true)
  if [ -n "$found_zip" ]; then
    APP_ARCHIVE="$found_zip"
    break
  fi
done

if [ -z "$APP_DIR" ] && [ -n "$APP_ARCHIVE" ]; then
  echo "Found app archive: $APP_ARCHIVE"
  UNPACK_DIR=$(mktemp -d 2>/dev/null || echo "${TMPDIR:-/tmp}/wolf_app_unpack.$$")
  mkdir -p "$UNPACK_DIR"
  command -v unzip >/dev/null 2>&1 || fail "need 'unzip' to extract $APP_ARCHIVE but it is not available"
  unzip -q -o "$APP_ARCHIVE" -d "$UNPACK_DIR" || fail "failed to unzip $APP_ARCHIVE"
  APP_DIR=$(find "$UNPACK_DIR" -type d -name 'Wolf.app' 2>/dev/null | head -n 1 || true)
  [ -n "$APP_DIR" ] || fail "unzipped $APP_ARCHIVE but found no Wolf.app inside $UNPACK_DIR"
fi

[ -n "$APP_DIR" ] || fail "could not locate Wolf.app (directory or archive) in runfiles. Searched roots:${RUNFILES_ROOTS:- (none)}"

echo "Located bundle: $APP_DIR"

RES="$APP_DIR/Contents/Resources"
[ -d "$RES" ] || fail "bundle has no Contents/Resources directory: $RES"

# ---------------------------------------------------------------------------
# (a) Contents/Resources/nvim/bin/nvim exists and is executable.
# ---------------------------------------------------------------------------
NVIM="$RES/nvim/bin/nvim"
[ -f "$NVIM" ] || fail "expected bundled nvim binary at $NVIM, but it is missing"
[ -x "$NVIM" ] || fail "bundled nvim binary is not executable (exec bit lost?): $NVIM"
echo "PASS check (a): $NVIM exists and is executable"

# ---------------------------------------------------------------------------
# (b) Contents/Resources/nvim/lib/nvim/parser/markdown.so exists.
# ---------------------------------------------------------------------------
MARKDOWN_SO="$RES/nvim/lib/nvim/parser/markdown.so"
[ -f "$MARKDOWN_SO" ] || fail "expected treesitter parser at $MARKDOWN_SO, but it is missing"
echo "PASS check (b): $MARKDOWN_SO exists"

# ---------------------------------------------------------------------------
# (c) Contents/Resources/nvim/share/nvim/runtime is a directory.
# ---------------------------------------------------------------------------
RUNTIME_DIR="$RES/nvim/share/nvim/runtime"
[ -d "$RUNTIME_DIR" ] || fail "expected runtime directory at $RUNTIME_DIR, but it is missing or not a directory"
echo "PASS check (c): $RUNTIME_DIR exists and is a directory"

# ---------------------------------------------------------------------------
# (d) Contents/Resources/init.lua is a regular file.
# ---------------------------------------------------------------------------
INIT_LUA="$RES/init.lua"
[ -f "$INIT_LUA" ] || fail "expected captive config at $INIT_LUA, but it is missing or not a regular file"
echo "PASS check (d): $INIT_LUA exists and is a regular file"

# ---------------------------------------------------------------------------
# (e) Contents/Resources/orlando/plugin/orlando.lua is a regular file.
# ---------------------------------------------------------------------------
ORLANDO_PLUGIN="$RES/orlando/plugin/orlando.lua"
[ -f "$ORLANDO_PLUGIN" ] || fail "expected vendored orlando plugin at $ORLANDO_PLUGIN, but it is missing"
echo "PASS check (e): $ORLANDO_PLUGIN exists"

# ---------------------------------------------------------------------------
# (f) Contents/Resources/orlando/lua/orlando/init.lua is a regular file.
# ---------------------------------------------------------------------------
ORLANDO_INIT="$RES/orlando/lua/orlando/init.lua"
[ -f "$ORLANDO_INIT" ] || fail "expected vendored orlando module at $ORLANDO_INIT, but it is missing"
echo "PASS check (f): $ORLANDO_INIT exists"

echo "PASS: Wolf.app bundles nvim/bin/nvim (executable), the markdown parser, the runtime tree, init.lua, and the vendored orlando plugin"
