#!/usr/bin/env bash
# Launcher template for the `clippy_fix` rule: `cargo clippy --fix` from the staged rules_rust
# toolchain, run in the developer's checkout ($BUILD_WORKSPACE_DIRECTORY).
set -euo pipefail

# Resolve the runfiles tree. `bazel run` sets RUNFILES_DIR; fall back to the
# conventional <argv0>.runfiles layout otherwise.
RUNFILES_DIR="${RUNFILES_DIR:-}"
if [[ -z "${RUNFILES_DIR}" ]]; then
  if [[ -d "${0}.runfiles" ]]; then
    RUNFILES_DIR="${0}.runfiles"
  fi
fi

CARGO_SHORT_PATH="@@CARGO_SHORT_PATH@@"
CARGO=""
if [[ -n "${RUNFILES_DIR}" && -x "${RUNFILES_DIR}/_main/${CARGO_SHORT_PATH}" ]]; then
  CARGO="${RUNFILES_DIR}/_main/${CARGO_SHORT_PATH}"
elif [[ -n "${RUNFILES_DIR}" ]]; then
  # Fall back to searching the runfiles tree for the staged cargo binary.
  CARGO="$(find "${RUNFILES_DIR}" -type f -name cargo -perm -u+x 2>/dev/null | head -n1 || true)"
fi

if [[ -z "${CARGO}" || ! -x "${CARGO}" ]]; then
  echo "ERROR: could not locate the staged cargo binary in the runfiles tree." >&2
  echo "       RUNFILES_DIR=${RUNFILES_DIR}" >&2
  echo "       CARGO_SHORT_PATH=${CARGO_SHORT_PATH}" >&2
  exit 1
fi

# Put the toolchain bin/ (cargo, cargo-clippy, clippy-driver, rustc, rustdoc)
# on PATH so `cargo clippy` can find its subcommand + driver + compiler.
CARGO_BIN_DIR="$(cd "$(dirname "${CARGO}")" && pwd)"
export PATH="${CARGO_BIN_DIR}:${PATH}"

if [[ -z "${BUILD_WORKSPACE_DIRECTORY:-}" ]]; then
  echo "ERROR: BUILD_WORKSPACE_DIRECTORY is unset; run this via 'bazel run'." >&2
  exit 1
fi

cd "${BUILD_WORKSPACE_DIRECTORY}"
echo "Using cargo: ${CARGO}"
# Not --workspace: wolf-gnome's GTK stack does not build on macOS, so only the crates Bazel
# builds are fixed.
echo "Running: cargo clippy --fix --allow-dirty --allow-staged -p wolf-core -p wolf-ffi $*"
exec "${CARGO}" clippy --fix --allow-dirty --allow-staged -p wolf-core -p wolf-ffi "$@"
