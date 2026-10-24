#!/usr/bin/env bash
# `swift-format format -i` over the wolf-macos Swift sources and tests in the developer's
# checkout ($BUILD_WORKSPACE_DIRECTORY), so it runs only under `bazel run`.
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
