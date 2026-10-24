#!/usr/bin/env bash
# `swift-format lint --strict` over the Swift files passed as "$@", with the host Xcode binary,
# which is why the test is `no-sandbox`.
set -euo pipefail

# Bazel tests run with a scrubbed environment, so try xcrun, then xcode-select, then the
# canonical Xcode path.
find_swift_format() {
  local sf
  sf="$(/usr/bin/xcrun --find swift-format 2>/dev/null || true)"
  if [[ -n "${sf}" && -x "${sf}" ]]; then echo "${sf}"; return 0; fi

  local dev
  dev="$(/usr/bin/xcode-select -p 2>/dev/null || true)"
  if [[ -n "${dev}" ]]; then
    sf="${dev}/Toolchains/XcodeDefault.xctoolchain/usr/bin/swift-format"
    if [[ -x "${sf}" ]]; then echo "${sf}"; return 0; fi
  fi

  sf="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin/swift-format"
  if [[ -x "${sf}" ]]; then echo "${sf}"; return 0; fi

  return 1
}

SF="$(find_swift_format || true)"
if [[ -z "${SF}" || ! -x "${SF}" ]]; then
  echo "ERROR: swift-format not found (tried xcrun, xcode-select, default path)." >&2
  echo "       PATH=${PATH}" >&2
  echo "       DEVELOPER_DIR=${DEVELOPER_DIR:-<unset>}" >&2
  exit 1
fi

if [[ "$#" -eq 0 ]]; then
  echo "ERROR: no Swift source files were passed to the checker." >&2
  exit 1
fi

echo "Using swift-format: ${SF}"
"${SF}" --version
echo "Linting $# Swift file(s)..."
exec "${SF}" lint --strict "$@"
