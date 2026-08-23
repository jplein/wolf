#!/bin/bash
#
# install_wolf.sh
#
# The runner behind `bazel run //crates/wolf-macos:InstallWolf`: installs the
# optimized bundle to ~/Applications/Wolf.app, replacing whatever is there.
#
# $1 is the rootpath of the `:WolfRelease` archive, staged in this target's
# runfiles. Bazel builds it before the run, so by the time this script executes
# the optimized bundle already exists.
#
# The ZIP is the input, not the `<name>_archive-root/Wolf.app` directory beside
# it: the directory is an undeclared side effect of the bundling action that
# nothing tracks and that runfiles therefore cannot carry. `ditto -x -k` is the
# extractor because it preserves the symlinks, exec bits and code signature that
# make the bundle launchable; `unzip` does not.
#
# Extraction goes to a staging directory FIRST, and the existing app is removed
# only once a complete new bundle exists. A failed or interrupted extraction
# then leaves the installed app untouched rather than destroying it.

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
