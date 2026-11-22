#!/usr/bin/env bash
# Builds :Wolf and runs the app-launch smoke on it in the logged-in GUI session, never in a gate.
# See notes/tools/wolf-macos-app-smoke.sh.md#why-these-refusals
set -euo pipefail

readonly APP_LABEL="//crates/wolf-macos:Wolf"

refuse() {
	echo "wolf-macos-app-smoke: $*" >&2
	exit 2
}

if [ "$#" -ne 0 ]; then
	echo "usage: ./tools/wolf-macos-app-smoke.sh (it takes no arguments)" >&2
	exit 2
fi

# macOS base tools by absolute path: the user's PATH may put GNU coreutils first.
os="$(/usr/bin/uname -s)"
[ "$os" = "Darwin" ] || refuse "macOS only: it launches Wolf.app (uname -s says ${os})."
[ -z "${REGINA_AGENT+set}" ] || refuse "refusing inside a regina worker (REGINA_AGENT is set)."
[ -z "${TEST_TMPDIR+set}" ] || refuse "refusing inside a Bazel test (TEST_TMPDIR is set)."

console="$(/usr/bin/stat -f %Su /dev/console 2>/dev/null || true)"
me="$(/usr/bin/id -un)"
if [ "$console" != "$me" ]; then
	refuse "needs this user's logged-in GUI session: /dev/console is owned by '${console}'," \
		"not '${me}'."
fi

root_props="$(/usr/sbin/ioreg -n Root -d1 2>/dev/null || true)"
if /usr/bin/grep -Eq '"(CGSSessionScreenIsLocked|IOConsoleLocked)" *= *Yes' <<<"$root_props"; then
	refuse "unlock the screen; no run on a locked screen has been measured."
fi

for tool in bazel python3; do
	if ! command -v "$tool" >/dev/null 2>&1; then
		refuse "no ${tool} on PATH: run inside the dev shell," \
			"or \`nix develop --command ./tools/wolf-macos-app-smoke.sh\`."
	fi
done

repo_root="$(cd "$(/usr/bin/dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "==> building ${APP_LABEL}"
bazel build "$APP_LABEL" || refuse "bazel build ${APP_LABEL} failed."

# The declared output is Wolf.zip; its `_archive-root/` sibling is untracked and may be stale.
files="$(bazel cquery --output=files "$APP_LABEL")" || refuse "bazel cquery ${APP_LABEL} failed."
zip=""
while IFS= read -r line; do
	case "$line" in
	*.zip)
		zip="$line"
		break
		;;
	esac
done <<<"$files"
[ -n "$zip" ] || refuse "bazel cquery listed no .zip for ${APP_LABEL}: got '${files}'."

case "$zip" in
/*) ;;
*) zip="${repo_root}/${zip}" ;;
esac
[ -f "$zip" ] || refuse "the built archive ${zip} is not a file."

exec python3 -I "${repo_root}/tools/wolf-macos-app-smoke.py" --app-zip "$zip"
