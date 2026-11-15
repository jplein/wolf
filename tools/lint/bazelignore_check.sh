#!/usr/bin/env bash
# Fails when `//...` lists a probe package under .worktrees/, i.e. .bazelignore lost the entry.
# Writes and removes the probe in the checkout; macOS only. See tools/lint/README.md#files
set -uo pipefail

if [[ "$#" -ne 0 ]]; then
  echo "ERROR: usage: bazelignore_check.sh (takes no arguments)" >&2
  exit 2
fi

here="$(cd "$(dirname "$0")" && pwd)" || exit 2
cd "${here}/../.." || exit 2

made_dir=0
probe=""

cleanup() {
  if [[ -n "${probe}" ]]; then
    rm -rf "${probe}"
  fi
  if [[ "${made_dir}" -eq 1 ]]; then
    rmdir .worktrees 2>/dev/null
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if [[ ! -d .worktrees ]]; then
  if ! mkdir .worktrees; then
    echo "ERROR: cannot create .worktrees" >&2
    exit 2
  fi
  made_dir=1
fi

if ! probe=$(mktemp -d .worktrees/bazelignore-probe.XXXXXX); then
  probe=""
  echo "ERROR: cannot create a probe directory under .worktrees" >&2
  exit 2
fi
if ! printf '%s\n' 'filegroup(name = "probe")' >"${probe}/BUILD.bazel"; then
  echo "ERROR: cannot write ${probe}/BUILD.bazel" >&2
  exit 2
fi

packages=$(bazel query --keep_going --color=no --curses=no --output=package //...)
rc=$?

leaked=$(printf '%s\n' "${packages}" | grep -E '^\.worktrees(/|$)')
if [[ -n "${leaked}" ]]; then
  printf '%s\n' "${leaked}" | while IFS= read -r pkg; do
    echo "FAIL: ${pkg}"
  done
  exit 1
fi

count=$(printf '%s\n' "${packages}" | grep -c .)
if [[ "${rc}" -ne 0 || "${count}" -eq 0 ]]; then
  echo "ERROR: bazel query //... exited ${rc} and listed ${count} packages" >&2
  exit 2
fi

echo "bazelignore_check.sh: ${count} packages, none under .worktrees/"
