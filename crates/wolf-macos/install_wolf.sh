#!/bin/bash
# Runner for `bazel run :InstallWolf`: installs the `:WolfRelease` zip to ~/Applications/Wolf.app.
# See notes/crates/wolf-macos/install_wolf.sh.md#archive-and-staging

set -euo pipefail

fail() {
  echo "install_wolf: $1" >&2
  exit 1
}

[ "$#" -eq 1 ] || fail "usage: install_wolf.sh <path-to-Wolf.zip>"
ARCHIVE="$1"

# `bazel run` sets CWD to the runfiles root, so the rootpath resolves as given.
[ -e "$ARCHIVE" ] || fail "release archive not found at '$ARCHIVE' (cwd: $PWD)"

DEST_DIR="$HOME/Applications"
DEST="$DEST_DIR/Wolf.app"

command -v ditto >/dev/null 2>&1 || fail "'ditto' is required but not on PATH"

mkdir -p "$DEST_DIR"

STAGING=$(mktemp -d "$DEST_DIR/.wolf-install.XXXXXX")
trap 'rm -rf "$STAGING"' EXIT

echo "install_wolf: extracting $ARCHIVE"
ditto -x -k "$ARCHIVE" "$STAGING" || fail "failed to extract $ARCHIVE"

NEW_APP="$STAGING/Wolf.app"
[ -d "$NEW_APP" ] || fail "extracted archive contains no Wolf.app at its root"
[ -x "$NEW_APP/Contents/MacOS/Wolf" ] || fail "extracted bundle has no executable at Contents/MacOS/Wolf"

if [ -e "$DEST" ]; then
  echo "install_wolf: removing existing $DEST"
  rm -rf "$DEST" || fail "could not remove $DEST"
fi

mv "$NEW_APP" "$DEST" || fail "could not move the new bundle into $DEST"

echo "install_wolf: installed $DEST"
