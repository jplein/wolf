#!/usr/bin/env bash
# `swift-format format -i` over the .swift files given, relative to $BUILD_WORKSPACE_DIRECTORY or
# absolute, or with none over the wolf-macos sources and tests. Must run under macOS bash 3.2.
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

if [[ -z "${BUILD_WORKSPACE_DIRECTORY:-}" ]]; then
  echo "ERROR: BUILD_WORKSPACE_DIRECTORY is unset; run this via 'bazel run'." >&2
  exit 1
fi

# Both sides are resolved, because a checkout can sit behind a symlink such as /var.
ROOT="$(realpath "${BUILD_WORKSPACE_DIRECTORY}")"
FILES=()

if [[ "$#" -gt 0 ]]; then
  refused=0
  for arg in "$@"; do
    # Never the cwd, which under `bazel run` is the runfiles tree.
    if [[ "${arg}" == /* ]]; then
      path="${arg}"
    else
      path="${BUILD_WORKSPACE_DIRECTORY}/${arg}"
    fi

    reason=""
    real=""
    if [[ ! -f "${path}" ]]; then
      reason="not a regular file"
    else
      real="$(realpath "${path}")"
      if [[ "${real}" != *.swift ]]; then
        reason="not a .swift file"
      elif [[ "${real}" != "${ROOT}"/* ]]; then
        reason="not under the checkout ${ROOT}"
      fi
    fi
    if [[ -n "${reason}" ]]; then
      echo "ERROR: refusing '${arg}': ${reason}." >&2
      refused=1
      continue
    fi

    seen=0
    for f in ${FILES[@]+"${FILES[@]}"}; do
      if [[ "${f}" == "${real}" ]]; then seen=1; fi
    done
    if [[ "${seen}" -eq 0 ]]; then FILES+=("${real}"); fi
  done

  if [[ "${refused}" -ne 0 ]]; then
    echo "ERROR: nothing was formatted." >&2
    exit 1
  fi
else
  DIRS=(
    "${ROOT}/crates/wolf-macos/Sources"
    "${ROOT}/crates/wolf-macos/Tests"
  )
  while IFS= read -r f; do
    FILES+=("${f}")
  done < <(find "${DIRS[@]}" -type f -name '*.swift' 2>/dev/null | sort)

  if [[ "${#FILES[@]}" -eq 0 ]]; then
    echo "No Swift files found to format under ${DIRS[*]}." >&2
    exit 0
  fi
fi

SF="$(find_swift_format || true)"
if [[ -z "${SF}" || ! -x "${SF}" ]]; then
  echo "ERROR: swift-format not found (tried xcrun, xcode-select, default path)." >&2
  exit 1
fi

echo "Using swift-format: ${SF}"
echo "Formatting ${#FILES[@]} Swift file(s) in place..."
exec "${SF}" format -i --parallel "${FILES[@]}"
