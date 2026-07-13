#!/usr/bin/env bash
# Aggregate FIX launcher (template for the `fix_all` rule in clippy_fix.bzl).
# Run via `bazel run //tools/lint:fix`. Applies all three fixers in place in the
# developer's real checkout ($BUILD_WORKSPACE_DIRECTORY), using the rules_rust
# downloaded toolchain (cargo/rustfmt/clippy staged as runfiles) and the host
# Xcode swift-format (resolved via xcrun).
#
# NON-HERMETIC dev convenience: cargo fmt / cargo clippy --fix reach crates.io
# (network) and the host cc toolchain.
set -euo pipefail

# --- Resolve the staged cargo from the runfiles tree -------------------------
RUNFILES_DIR="${RUNFILES_DIR:-}"
if [[ -z "${RUNFILES_DIR}" && -d "${0}.runfiles" ]]; then
  RUNFILES_DIR="${0}.runfiles"
fi

CARGO_SHORT_PATH="@@CARGO_SHORT_PATH@@"
CARGO=""
if [[ -n "${RUNFILES_DIR}" && -x "${RUNFILES_DIR}/_main/${CARGO_SHORT_PATH}" ]]; then
  CARGO="${RUNFILES_DIR}/_main/${CARGO_SHORT_PATH}"
elif [[ -n "${RUNFILES_DIR}" ]]; then
  CARGO="$(find "${RUNFILES_DIR}" -type f -name cargo -perm -u+x 2>/dev/null | head -n1 || true)"
fi

if [[ -z "${CARGO}" || ! -x "${CARGO}" ]]; then
  echo "ERROR: could not locate the staged cargo binary in the runfiles tree." >&2
  echo "       RUNFILES_DIR=${RUNFILES_DIR}" >&2
  exit 1
fi
CARGO_BIN_DIR="$(cd "$(dirname "${CARGO}")" && pwd)"
export PATH="${CARGO_BIN_DIR}:${PATH}"

if [[ -z "${BUILD_WORKSPACE_DIRECTORY:-}" ]]; then
  echo "ERROR: BUILD_WORKSPACE_DIRECTORY is unset; run this via 'bazel run'." >&2
  exit 1
fi
cd "${BUILD_WORKSPACE_DIRECTORY}"

# rustfmt comes from the SEPARATE rules_rust rustfmt toolchain (staged as
# runfiles): its binary links the matching librustc_driver, unlike the rust
# toolchain's bare `rustfmt`. We invoke it directly over the crate sources
# (there is no `cargo fmt` subcommand in the downloaded toolchain). Scope to the
# Bazel-built crates only: wolf-gnome (GTK) is a cargo-workspace member that does
# NOT build on macOS, so `cargo clippy` must not touch it.
RUSTFMT_SHORT_PATH="@@RUSTFMT_SHORT_PATH@@"
RUSTFMT=""
if [[ -n "${RUNFILES_DIR}" && -x "${RUNFILES_DIR}/_main/${RUSTFMT_SHORT_PATH}" ]]; then
  RUSTFMT="${RUNFILES_DIR}/_main/${RUSTFMT_SHORT_PATH}"
elif [[ -n "${RUNFILES_DIR}" ]]; then
  RUSTFMT="$(find "${RUNFILES_DIR}" -type f -name rustfmt -perm -u+x 2>/dev/null | head -n1 || true)"
fi
if [[ -z "${RUSTFMT}" || ! -x "${RUSTFMT}" ]]; then
  echo "ERROR: could not locate the staged rustfmt binary in the runfiles tree." >&2
  exit 1
fi

echo "==> [1/3] rustfmt"
mapfile -t RS_FILES < <(find \
  "${BUILD_WORKSPACE_DIRECTORY}/crates/wolf-core" \
  "${BUILD_WORKSPACE_DIRECTORY}/crates/wolf-ffi" \
  -type f -name '*.rs' 2>/dev/null | sort)
if [[ "${#RS_FILES[@]}" -gt 0 ]]; then
  "${RUSTFMT}" --edition 2021 "${RS_FILES[@]}"
fi

echo "==> [2/3] clippy --fix (cargo clippy --fix)"
"${CARGO}" clippy --fix --allow-dirty --allow-staged -p wolf-core -p wolf-ffi

echo "==> [3/3] swift-format (host Xcode swift-format)"
SF="$(xcrun --find swift-format 2>/dev/null || true)"
if [[ -z "${SF}" || ! -x "${SF}" ]]; then
  echo "ERROR: swift-format not found via 'xcrun --find swift-format'." >&2
  exit 1
fi
mapfile -t SWIFT_FILES < <(find \
  "${BUILD_WORKSPACE_DIRECTORY}/crates/wolf-macos/Sources" \
  "${BUILD_WORKSPACE_DIRECTORY}/crates/wolf-macos/Tests" \
  -type f -name '*.swift' 2>/dev/null | sort)
if [[ "${#SWIFT_FILES[@]}" -gt 0 ]]; then
  "${SF}" format -i --parallel "${SWIFT_FILES[@]}"
else
  echo "No Swift files found to format."
fi

echo "All fixers completed."
