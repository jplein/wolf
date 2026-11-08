#!/usr/bin/env bash
# Runs wolf_core_deps_check.sh, with the real forbidden list, over testdata/wolf_core_deps.
# Each case asserts the exit status and which FAIL: lines appear; must run under /bin/bash 3.2.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
checker="${here}/wolf_core_deps_check.sh"
list="${here}/wolf_core_forbidden_deps.txt"
data="${here}/testdata/wolf_core_deps"
clean="${data}/clean.txt"
forbidden="${data}/forbidden.txt"

if [[ ! -f "${checker}" ]]; then
  echo "FAIL: ${checker} not found (under Bazel it must be in the sh_test data)" >&2
  exit 1
fi
if [[ ! -f "${list}" ]]; then
  echo "FAIL: ${list} not found (under Bazel it must be in the sh_test data)" >&2
  exit 1
fi
for fixture in "${clean}" "${forbidden}"; do
  if [[ ! -f "${fixture}" ]]; then
    echo "FAIL: ${fixture} not found (under Bazel, glob testdata/wolf_core_deps into data)" >&2
    exit 1
  fi
done

failures=0
cases=0
label=""
output=""
status=0
before=0

fail() {
  echo "FAIL: $*" >&2
  failures=$((failures + 1))
}

# check LABEL file|stdin PATH: runs the checker on PATH, as its argument or on its stdin.
check() {
  label="$1"
  before="${failures}"
  cases=$((cases + 1))
  status=0
  if [[ "$2" == "file" ]]; then
    output="$(bash "${checker}" "$3" 2>&1)" || status=$?
  else
    output="$(bash "${checker}" <"$3" 2>&1)" || status=$?
  fi
}

done_case() {
  if [[ "${failures}" -ne "${before}" ]]; then
    echo "---- output of ${label} (exit ${status}) ----" >&2
    echo "${output}" >&2
    echo "----" >&2
  fi
}

want_status() {
  if [[ "${status}" -ne "$1" ]]; then
    fail "${label}: exit status ${status}, want $1 ($2)"
  fi
}

want_line() {
  if ! grep -qxF -e "$1" <<<"${output}"; then
    fail "${label}: no line '$1' ($2)"
  fi
}

no_fail_lines() {
  if grep -q '^FAIL: ' <<<"${output}"; then
    fail "${label}: printed a FAIL: line ($1)"
  fi
}

# Every name in clean.txt is also in forbidden.txt, and none of them may be reported.
no_clean_name_reported() {
  local name
  while IFS= read -r name; do
    if grep -qF -e "FAIL: ${name} " <<<"${output}"; then
      fail "${label}: reported clean name ${name} (a pattern over-matches, or blank patterns match)"
    fi
  done <"${clean}"
}

want_clean_summary() {
  if ! grep -qE "^wolf_core_deps_check: $1 line\(s\) clean against [1-9][0-9]* pattern\(s\)$" \
    <<<"${output}"; then
    fail "${label}: no summary '$1 line(s) clean against <n> pattern(s)' (listing not read whole?)"
  fi
}

check "clean.txt as argument" file "${clean}"
want_status 0 "1 means a clean name matched, e.g. a blank list line used as a pattern; 2 an error"
no_fail_lines "a clean crate name or label was reported as forbidden"
want_clean_summary 10
done_case

check "clean.txt on stdin" stdin "${clean}"
want_status 0 "2 means stdin was not read as the listing; 1 that a clean name matched"
no_fail_lines "a clean crate name or label was reported as forbidden"
want_clean_summary 10
done_case

check "forbidden.txt as argument" file "${forbidden}"
want_status 1 "0 means gtk4 and objc2-core-foundation went unnoticed; 2 an error"
want_line "FAIL: gtk4 matches forbidden pattern gtk" \
  "gtk is missing from the list, or patterns match whole lines rather than substrings"
want_line "FAIL: objc2-core-foundation matches forbidden pattern objc" \
  "the notify 8.2.1 case: objc is missing, or patterns match whole lines rather than substrings"
no_clean_name_reported
done_case

check "forbidden.txt on stdin" stdin "${forbidden}"
want_status 1 "0 means stdin went unchecked or gtk4 unnoticed; 2 that stdin was not read"
want_line "FAIL: gtk4 matches forbidden pattern gtk" "gtk4 on stdin was not reported"
done_case

check "empty stdin" stdin /dev/null
want_status 2 "0 means an empty listing, as from a failed producer, passes as clean"
no_fail_lines "an empty listing has nothing to report"
if ! grep -q '^ERROR: ' <<<"${output}"; then
  fail "${label}: no ERROR: line explaining the empty listing"
fi
done_case

check "missing listing file" file "${data}/no-such-listing.txt"
want_status 2 "0 or 1 means an unreadable listing was not treated as an error"
no_fail_lines "a missing file has nothing to report"
if ! grep -q '^ERROR: ' <<<"${output}"; then
  fail "${label}: no ERROR: line naming the missing listing"
fi
done_case

if [[ "${failures}" -gt 0 ]]; then
  echo "wolf_core_deps_check_test: ${failures} failure(s) across ${cases} case(s)" >&2
  exit 1
fi
echo "wolf_core_deps_check_test: all ${cases} case(s) passed"
