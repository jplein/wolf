#!/bin/bash
# Runner for `bazel run :InstallWolf`: installs the `:WolfRelease` zip to ~/Applications/Wolf.app.
# See notes/crates/wolf-macos/install_wolf.sh.md#archive-and-staging

set -euo pipefail

fail() {
  echo "install_wolf: $1" >&2
  exit 1
}

warn() { echo "install_wolf: warning: $1" >&2; }

[ "$#" -eq 1 ] || fail "usage: install_wolf.sh <path-to-Wolf.zip>"
ARCHIVE="$1"

# `bazel run` sets CWD to the runfiles root, so the rootpath resolves as given.
[ -e "$ARCHIVE" ] || fail "release archive not found at '$ARCHIVE' (cwd: $PWD)"

DEST_DIR="$HOME/Applications"
DEST="$DEST_DIR/Wolf.app"
# WOLF_LSREGISTER replaces the tool, so a test never registers with the real database.
LSREGISTER="${WOLF_LSREGISTER:-/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister}"

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

# Force Launch Services to re-read the bundle's declarations; a failure only warns.
# See notes/crates/wolf-macos/install_wolf.sh.md#launch-services-registration
if [ ! -x "$LSREGISTER" ]; then
  warn "lsregister not found at '$LSREGISTER'; Finder may not offer Wolf until it is registered"
elif ! "$LSREGISTER" -f "$DEST"; then
  warn "lsregister could not register $DEST; Finder may not offer Wolf until it is registered"
fi
