#!/usr/bin/env bash
# Runs markdown_check.sh over testdata/markdown, asserting exit status and per-rule finding counts.
# The fixtures and what each covers are listed in tools/lint/README.md.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
checker="${here}/markdown_check.sh"
data="${here}/testdata/markdown"

if [[ ! -f "${checker}" ]]; then
  echo "FAIL: ${checker} not found (under Bazel it must be in the sh_test data)" >&2
  exit 1
fi
if [[ ! -d "${data}/notes" || ! -d "${data}/other" ]]; then
  echo "FAIL: fixtures not found under ${data} (under Bazel, glob testdata/** into data)" >&2
  exit 1
fi

failures=0
cases=0

fail() {
  echo "FAIL: $*" >&2
  failures=$((failures + 1))
}

# expect WANT_EXIT "RULE=COUNT ..." "FILE:LINE:RULE ..." FILE...
# Rules not named in the counts must not fire; files are passed as ./x so ^notes/ cannot match.
expect() {
  local want_exit="$1" counts="$2" located="$3"
  shift 3
  local label="markdown_check.sh $*"
  local before="${failures}"
  local output status=0
  cases=$((cases + 1))

  output="$(cd "${data}" && bash "${checker}" "$@" 2>&1)" || status=$?

  if [[ "${status}" -ne "${want_exit}" ]]; then
    fail "${label}: exit status ${status}, want ${want_exit}"
  fi

  local pair rule count got want_total=0 total
  for pair in ${counts}; do
    rule="${pair%%=*}"
    count="${pair#*=}"
    want_total=$((want_total + count))
    got="$(grep -cF ": [${rule}] " <<<"${output}" || true)"
    if [[ "${got}" -ne "${count}" ]]; then
      fail "${label}: ${got} [${rule}] finding(s), want ${count}"
    fi
  done
  total="$(grep -cE '^[^ ]+:[0-9]+: \[[a-z-]+\] ' <<<"${output}" || true)"
  if [[ "${total}" -ne "${want_total}" ]]; then
    fail "${label}: ${total} finding(s) of any rule, want ${want_total}"
  fi

  local loc file rest line
  for loc in ${located}; do
    file="${loc%%:*}"
    rest="${loc#*:}"
    line="${rest%%:*}"
    rule="${rest#*:}"
    if ! grep -qF "${file}:${line}: [${rule}] " <<<"${output}"; then
      fail "${label}: no [${rule}] finding at ${file}:${line}"
    fi
  done

  if [[ "${failures}" -ne "${before}" ]]; then
    echo "---- output of ${label} ----" >&2
    echo "${output}" >&2
    echo "----" >&2
  fi
}

# A valid header, and "now" in prose, which notes/ is exempt from.
expect 0 "" "" ./notes/good.md
expect 1 "notes-header=1" "./notes/missing.md:3:notes-header" ./notes/missing.md
expect 1 "notes-header=1" "./notes/short-sha.md:3:notes-header" ./notes/short-sha.md
expect 0 "" "" ./notes/no-h1.md
expect 0 "" "" ./notes/nested/deep.md
# No line after the H1: reported at line 1 by the end-of-file flush.
expect 1 "notes-header=1" "./notes/empty-after-h1.md:1:notes-header" ./notes/empty-after-h1.md
expect 1 "changelog=1" "./other/changelog.md:3:changelog" ./other/changelog.md
expect 0 "" "" ./other/no-sha.md

# One awk pass over several files: each header finding is blamed on its own file.
expect 1 "notes-header=1" "./notes/empty-after-h1.md:1:notes-header" \
  ./notes/empty-after-h1.md ./notes/good.md
expect 1 "notes-header=2" "./notes/missing.md:3:notes-header ./notes/short-sha.md:3:notes-header" \
  ./notes/missing.md ./notes/no-h1.md ./notes/short-sha.md ./other/no-sha.md

if [[ "${failures}" -gt 0 ]]; then
  echo "markdown_check_test: ${failures} failure(s) across ${cases} case(s)" >&2
  exit 1
fi
echo "markdown_check_test: all ${cases} case(s) passed"
