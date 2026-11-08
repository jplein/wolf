#!/usr/bin/env bash
# Runs swift_format_fix.sh over a scratch checkout of testdata/swift_format copies, under bash 3.2.
# macOS only: it needs the host Xcode swift-format, which is why the sh_test is `no-sandbox`.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
fixer="${here}/swift_format_fix.sh"
data="${here}/testdata/swift_format"
formatted="${data}/formatted.swift"
misindented="${data}/misindented.swift"

if [[ ! -f "${fixer}" ]]; then
  echo "FAIL: ${fixer} not found (under Bazel it must be in the sh_test data)" >&2
  exit 1
fi
for fixture in "${formatted}" "${misindented}"; do
  if [[ ! -f "${fixture}" ]]; then
    echo "FAIL: ${fixture} not found (under Bazel, glob testdata/swift_format into data)" >&2
    exit 1
  fi
done
# Equal fixtures would make every "formatted" and "untouched" check below pass at once.
if cmp -s "${formatted}" "${misindented}"; then
  echo "FAIL: misindented.swift equals formatted.swift, so no case can tell a change apart" >&2
  exit 1
fi

scratch="$(mktemp -d "${TEST_TMPDIR:-${TMPDIR:-/tmp}}/swift_format_fix_test.XXXXXX")"
ws="${scratch}/ws"
# The fixer runs from here, so a path it resolves against the cwd names nothing.
cwd="${scratch}/cwd"
mkdir -p "${cwd}"

a_rel="crates/wolf-macos/Sources/WolfKeyMap/A.swift"
b_rel="crates/wolf-macos/Tests/WolfKeyMapTests/B.swift"
c_rel="tools/C.swift"
a="${ws}/${a_rel}"
b="${ws}/${b_rel}"
c="${ws}/${c_rel}"
readme="${ws}/README.md"
d="${scratch}/outside/D.swift"
# Its path starts with "${ws}", so only a check for "${ws}/" as a prefix refuses it.
e="${scratch}/ws-sibling/E.swift"
readme_text="# Not Swift"

# Regular copies, never runfiles symlinks: under no-sandbox those resolve into the real checkout.
reset_tree() {
  rm -rf "${ws}" "${scratch}/outside" "${scratch}/ws-sibling"
  local f
  for f in "${a}" "${b}" "${c}" "${d}" "${e}"; do
    mkdir -p "$(dirname "${f}")"
    cp "${misindented}" "${f}"
    if [[ -L "${f}" || ! -f "${f}" ]]; then
      echo "FAIL: ${f} is not a regular file, so the cases would not test a checkout" >&2
      exit 1
    fi
  done
  printf '%s\n' "${readme_text}" >"${readme}"
}

failures=0
cases=0
label=""
before=0
output=""
status=0

fail() {
  echo "FAIL: ${label}: $*" >&2
  failures=$((failures + 1))
}

# run LABEL [ARG...]: rebuilds the tree, then runs the fixer with ARG... and records the result.
run() {
  label="swift_format_fix.sh $1"
  shift
  cases=$((cases + 1))
  before="${failures}"
  reset_tree
  status=0
  output="$(cd "${cwd}" && env -u DEVELOPER_DIR BUILD_WORKSPACE_DIRECTORY="${ws}" \
    bash "${fixer}" ${1+"$@"} 2>&1)" || status=$?
}

done_case() {
  if [[ "${failures}" -ne "${before}" ]]; then
    echo "---- output of ${label} (exit ${status}) ----" >&2
    echo "${output}" >&2
    echo "----" >&2
  fi
}

expect_exit_zero() {
  if [[ "${status}" -ne 0 ]]; then
    fail "exit status ${status}, want 0 (the fixer refused a valid argument or crashed)"
  fi
}

expect_formatted() {
  if ! cmp -s "$1" "${formatted}"; then
    fail "${1#"${scratch}"/} is not formatted.swift (the fixer skipped a file it should format)"
  fi
}

expect_untouched() {
  if ! cmp -s "$1" "${misindented}"; then
    fail "${1#"${scratch}"/} changed (the fixer formatted a file it was not asked to)"
  fi
}

expect_readme_untouched() {
  if [[ "$(cat "${readme}")" != "${readme_text}" ]]; then
    fail "ws/README.md changed (the fixer rewrote a file that is not Swift)"
  fi
}

# 1. A relative argument is joined to $BUILD_WORKSPACE_DIRECTORY, and only it is formatted.
run "${a_rel}" "${a_rel}"
expect_exit_zero
expect_formatted "${a}"
expect_untouched "${b}"
expect_untouched "${c}"
expect_readme_untouched
done_case

# 2. An absolute argument under the checkout is accepted, even outside crates/wolf-macos.
run "<abs>/ws/${c_rel}" "${c}"
expect_exit_zero
expect_formatted "${c}"
expect_untouched "${a}"
expect_untouched "${b}"
expect_readme_untouched
done_case

# 3. No arguments: the wolf-macos sources and tests, and nothing else in the checkout.
run "(no arguments)"
expect_exit_zero
expect_formatted "${a}"
expect_formatted "${b}"
expect_untouched "${c}"
expect_untouched "${d}"
expect_readme_untouched
done_case

# 4. Each bad argument follows a valid one, so formatting A shows validation did not come first.
for bad in \
  "README.md" \
  "crates/wolf-macos/Sources" \
  "crates/wolf-macos/Sources/WolfKeyMap/Missing.swift" \
  "../outside/D.swift" \
  "${d}" \
  "${e}"; do
  run "${a_rel} ${bad#"${scratch}"/}" "${a_rel}" "${bad}"
  if [[ "${status}" -eq 0 ]]; then
    fail "exit status 0, want non-zero (the bad argument was accepted)"
  fi
  # A's own path contains "crates/wolf-macos/Sources", so it must not count as naming the bad one.
  if [[ "${output//"${a_rel}"/<A>}" != *"${bad}"* ]]; then
    fail "the output never names '${bad}' as given (no refusal message, or a resolved path)"
  fi
  expect_untouched "${a}"
  expect_untouched "${b}"
  expect_untouched "${c}"
  expect_untouched "${d}"
  expect_untouched "${e}"
  expect_readme_untouched
  done_case
done

if [[ "${failures}" -gt 0 ]]; then
  echo "swift_format_fix_test: ${failures} failure(s) across ${cases} case(s)" >&2
  exit 1
fi
echo "swift_format_fix_test: all ${cases} case(s) passed"
