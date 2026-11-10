#!/usr/bin/env bash
# Usage: rustfmt_files.sh RUSTFMT [--check] FILE..., with each .rs FILE absolute or relative to
# $BUILD_WORKSPACE_DIRECTORY. Runs RUSTFMT --edition 2021 outside any Bazel target; bash 3.2.
set -euo pipefail

if [[ "$#" -eq 0 ]]; then
  echo "ERROR: usage: rustfmt_files.sh RUSTFMT [--check] FILE..." >&2
  exit 2
fi

# Absolute before anything else, since under `bazel run` it is relative to the runfiles tree.
RUSTFMT="$1"
shift
if [[ "${RUSTFMT}" != /* ]]; then
  RUSTFMT="${PWD}/${RUSTFMT}"
fi
if [[ ! -f "${RUSTFMT}" || ! -x "${RUSTFMT}" ]]; then
  echo "ERROR: rustfmt '${RUSTFMT}' is not an executable file." >&2
  exit 2
fi

if [[ -z "${BUILD_WORKSPACE_DIRECTORY:-}" ]]; then
  echo "ERROR: BUILD_WORKSPACE_DIRECTORY is unset; run this via 'bazel run'." >&2
  exit 2
fi

CHECK=()
if [[ "${1:-}" == "--check" ]]; then
  CHECK=(--check)
  shift
fi

if [[ "$#" -eq 0 ]]; then
  echo "ERROR: no .rs files were given; rustfmt would read stdin instead." >&2
  exit 2
fi

# Both sides are resolved, because a checkout can sit behind a symlink such as /var.
ROOT="$(realpath "${BUILD_WORKSPACE_DIRECTORY}")"
FILES=()
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
    if [[ "${real}" != *.rs ]]; then
      reason="not a .rs file"
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

exec "${RUSTFMT}" --edition 2021 ${CHECK[@]+"${CHECK[@]}"} "${FILES[@]}"
