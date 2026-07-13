#!/usr/bin/env bash
# swift-format CHECK: lint Swift sources with the host Xcode swift-format binary.
#
# This is a `no-sandbox` sh_test: the swift-format binary lives in the host
# Xcode toolchain (resolved via `xcrun --find swift-format`), not a hermetic
# Bazel input, so it must run outside the sandbox. The Swift source files are
# staged as runfiles (passed as "$@" by the BUILD rule via `$(rootpaths ...)`).
#
# ADVISORY: `lint --strict` may report style violations on existing code. That
# is expected; the target's job is to RUN the linter, not to guarantee zero
# findings.
set -euo pipefail

# Resolve the host swift-format binary. Prefer `xcrun --find`, then derive it
# from `xcode-select -p`, then fall back to the canonical Xcode path. Bazel test
# actions run with a scrubbed environment, so we try several routes and print
# diagnostics if all fail.
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
