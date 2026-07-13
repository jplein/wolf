#!/usr/bin/env bash
# swift-format FIX: format Swift sources in place with the host Xcode
# swift-format binary. Run via `bazel run //tools/lint:swift_format.fix`.
#
# Rewrites files in the developer's real workspace (NOT the sandbox), so it uses
# `$BUILD_WORKSPACE_DIRECTORY` (set by `bazel run`). It formats the wolf-macos
# Swift sources and tests, and EXCLUDES the machine-generated swift-bridge
# bindings under crates/wolf-ffi/generated/.
set -euo pipefail

SF="$(xcrun --find swift-format 2>/dev/null || true)"
if [[ -z "${SF}" || ! -x "${SF}" ]]; then
  echo "ERROR: swift-format not found via 'xcrun --find swift-format'." >&2
  exit 1
fi

if [[ -z "${BUILD_WORKSPACE_DIRECTORY:-}" ]]; then
  echo "ERROR: BUILD_WORKSPACE_DIRECTORY is unset; run this via 'bazel run'." >&2
  exit 1
fi

ROOT="${BUILD_WORKSPACE_DIRECTORY}"
DIRS=(
  "${ROOT}/crates/wolf-macos/Sources"
  "${ROOT}/crates/wolf-macos/Tests"
)

mapfile -t FILES < <(find "${DIRS[@]}" -type f -name '*.swift' 2>/dev/null | sort)
if [[ "${#FILES[@]}" -eq 0 ]]; then
  echo "No Swift files found to format under ${DIRS[*]}." >&2
  exit 0
fi

echo "Using swift-format: ${SF}"
echo "Formatting ${#FILES[@]} Swift file(s) in place..."
exec "${SF}" format -i --parallel "${FILES[@]}"
