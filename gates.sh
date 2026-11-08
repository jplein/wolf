#!/usr/bin/env bash
# Runs every deterministic quality gate for this platform and reports each one's exit status.
# The output and the exit codes are documented in build.md, under "The verification gate".
set -uo pipefail

trap 'exit 130' INT
trap 'exit 143' TERM

cd "$(dirname "${BASH_SOURCE[0]}")" || exit 2

if [ "$#" -ne 0 ]; then
  echo "gates.sh: takes no arguments" >&2
  exit 2
fi

case "$(uname -s)" in
  Linux) platform=linux ;;
  Darwin) platform=darwin ;;
  *)
    echo "gates.sh: unsupported platform $(uname -s)" >&2
    exit 2
    ;;
esac

logs="$PWD/target/gates"

# Linux only: a short base under /tmp, never nested in nix develop's TMPDIR, for the cage sockets.
# See notes/gates.sh.md#temporary-directory
make_tmpdir() {
  tmp=$(mktemp -d /tmp/wolf-gates.XXXXXX) || exit 2
  trap 'rm -rf "$tmp"' EXIT
  export TMPDIR="$tmp" WOLF_GATES_TMPDIR="$tmp"
}

# Outside the dev shell, enter it once as a child and report what the inner run reports.
# See notes/gates.sh.md#dev-shell-entry
if [ "${WOLF_DEV_SHELL:-}" != 1 ]; then
  if [ -n "${WOLF_GATES_REENTERED:-}" ]; then
    echo "gates.sh: nix develop did not export WOLF_DEV_SHELL; check flake.nix's shellHook" >&2
    exit 2
  fi
  if ! command -v nix >/dev/null 2>&1; then
    echo "gates.sh: nix not found; outside the dev shell gates.sh needs nix to enter it" >&2
    exit 2
  fi
  rm -rf "$logs" || exit 2
  if [ "$platform" = linux ]; then
    make_tmpdir
  fi
  echo "gates.sh: entering the dev shell" >&2
  WOLF_GATES_REENTERED=1 nix --extra-experimental-features 'nix-command flakes' \
    develop --command ./gates.sh
  rc=$?
  if [ ! -d "$logs" ]; then
    echo "gates.sh: nix develop or the inner run failed (exit $rc) before any gate ran" >&2
    exit 2
  fi
  exit "$rc"
fi

rm -rf "$logs" || exit 2
mkdir -p "$logs" || exit 2

if [ "$platform" = linux ]; then
  if [ -n "${WOLF_GATES_TMPDIR:-}" ]; then
    export TMPDIR="$WOLF_GATES_TMPDIR"
  else
    make_tmpdir
  fi
fi

if [ "$platform" = darwin ] && ! xcrun --find swift-format >/dev/null 2>&1; then
  echo "gates.sh: xcrun cannot find swift-format under DEVELOPER_DIR=${DEVELOPER_DIR:-<unset>};" \
    "the gates need the Xcode at /Applications/Xcode.app (under regina, its \`xcode\` key)" >&2
  exit 2
fi

status=0
failed=""

run() {
  local name=$1
  shift
  local log="$logs/$name.log"
  local start=$SECONDS
  local rc
  echo "gates.sh: $name: $*" >"$log"
  "$@" </dev/null >>"$log" 2>&1
  rc=$?
  echo "gates.sh: $name exited $rc after $((SECONDS - start))s" >>"$log"
  echo "$name $rc"
  if [ "$rc" -ne 0 ]; then
    status=1
    failed="$failed $name:$rc"
  fi
}

# A remote run is diagnosed from stdout alone: a short log whole, a long one as excerpt plus tail.
# See notes/gates.sh.md#failure-output
excerpt_patterns='^---- .* stdout ----$|^error(\[E[0-9]+\])?: |^(ERROR|FAIL): '
excerpt_patterns="$excerpt_patterns"'|^==================== Test output for |^Diff in |^\[warn\] '

print_failure() {
  local name=$1
  local rc=$2
  local log="$logs/$name.log"
  local lines
  lines=$(($(wc -l <"$log")))
  echo "--- $name (exit $rc): $log ---"
  if [ "$lines" -le 150 ]; then
    cat "$log"
  else
    echo "gates.sh: $lines lines; matching lines with context, then the last 150 lines"
    grep -n -E -A 40 "$excerpt_patterns" "$log" | head -n 400
    echo "gates.sh: last 150 lines"
    tail -n 150 "$log"
  fi
  echo "--- end $name ---"
}

# Crate names only, across every target, so macOS-only deps count. See notes/gates.sh.md#linux-gates
core_deps_cargo() {
  cargo tree -e no-dev -p wolf-core --target all --prefix none --format '{p}' \
    | cut -d ' ' -f 1 | sort -u | ./tools/lint/wolf_core_deps_check.sh
}

# kind(rule, …) keeps source paths such as libc's glibc/ out. See notes/gates.sh.md#macos-gates
core_deps_bazel() {
  bazel query --color=no --curses=no 'kind(rule, deps(//crates/wolf-core:wolf_core))' \
    | ./tools/lint/wolf_core_deps_check.sh
}

# Fast to slow, so a formatting slip fails in seconds. See notes/gates.sh.md#gate-order
if [ "$platform" = linux ]; then
  # See notes/gates.sh.md#linux-gates
  run fmt cargo fmt --all -- --check
  run fmt-doc-lint cargo fmt --manifest-path tools/doc-lint/Cargo.toml -- --check
  run prettier prettier --check "**/*.md"
  run markdown ./tools/lint/markdown_all.sh
  run markdown-check-test ./tools/lint/markdown_check_test.sh
  run wolf-core-deps-check-test ./tools/lint/wolf_core_deps_check_test.sh
  run clippy-doc-lint cargo clippy --manifest-path tools/doc-lint/Cargo.toml --all-targets \
    -- -D warnings
  run test-doc-lint cargo test --manifest-path tools/doc-lint/Cargo.toml
  run doc-lint cargo run --quiet --manifest-path tools/doc-lint/Cargo.toml -- --root .
  run wolf-core-deps core_deps_cargo
  run clippy cargo clippy --workspace --all-targets -- -D warnings
  run build cargo build --workspace --all-targets
  run test cargo test --workspace --no-fail-fast
else
  # See notes/gates.sh.md#macos-gates
  bazel_flags=(--keep_going --color=no --curses=no --show_progress_rate_limit=30)
  test_flags=(--test_output=errors --test_summary=short)
  run prettier prettier --check "**/*.md"
  run doc-lint bazel run --color=no --curses=no //tools/doc-lint:doc_lint -- --root "$PWD"
  run wolf-core-deps core_deps_bazel
  run lint bazel test "${bazel_flags[@]}" "${test_flags[@]}" //tools/lint:check
  run build bazel build "${bazel_flags[@]}" //...
  run test bazel test "${bazel_flags[@]}" "${test_flags[@]}" //...
fi

for entry in $failed; do
  print_failure "${entry%:*}" "${entry##*:}"
done

echo "logs: $logs"
exit "$status"
