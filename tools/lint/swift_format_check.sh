#!/usr/bin/env bash
# `swift-format lint --strict` over the Swift files passed as "$@", with the host Xcode binary,
# which is why the test is `external` and `no-sandbox`.
set -euo pipefail

# Resolved, since under `bazel run` $0 is a bazel-bin symlink with no library beside it.
source "$(dirname "$(realpath "${BASH_SOURCE[0]}")")/swift_format_lib.sh"

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
# Bazel passes runfiles symlinks, which swift-format skips without --follow-symlinks.
exec "${SF}" lint --strict --follow-symlinks "$@"
