#!/usr/bin/env bash
# Runs swift_format_check.sh over testdata/swift_format staged as runfiles symlinks.
# macOS only: it needs the host Xcode swift-format, which is why the sh_test is `no-sandbox`.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
checker="${here}/swift_format_check.sh"
data="${here}/testdata/swift_format"
formatted="${data}/formatted.swift"
misindented="${data}/misindented.swift"

if [[ ! -f "${checker}" ]]; then
  echo "FAIL: ${checker} not found (under Bazel it must be in the sh_test data)" >&2
  exit 1
fi
for fixture in "${formatted}" "${misindented}"; do
  if [[ ! -f "${fixture}" ]]; then
    echo "FAIL: ${fixture} not found (under Bazel, glob testdata/swift_format into data)" >&2
    exit 1
  fi
  # The real inputs of :swift_format are runfiles symlinks; a regular file here tests another path.
  if [[ ! -L "${fixture}" ]]; then
    echo "FAIL: ${fixture} is not a symlink, so no case covers Bazel's runfiles inputs" >&2
    exit 1
  fi
done

failures=0
cases=0

fail() {
  echo "FAIL: $*" >&2
  failures=$((failures + 1))
}

# expect WANT "BASENAME:LINE:COL:RULE ..." "CLEAN_BASENAME ..." FILE...
# WANT is 0 or nonzero; findings are matched on the basename, as the resolved path is printed.
expect() {
  local want="$1" located="$2" clean="$3"
  shift 3
  local label="swift_format_check.sh $*"
  local before="${failures}"
  local output status=0
  cases=$((cases + 1))

  output="$(bash "${checker}" "$@" 2>&1)" || status=$?
  # Every line gains a leading / so that "/NAME:" matches a relative or an absolute path alike.
  local rooted
  rooted="$(sed 's|^|/|' <<<"${output}")"

  if [[ "${want}" == "0" && "${status}" -ne 0 ]]; then
    fail "${label}: exit status ${status}, want 0 (a formatted file failed the check)"
  fi
  if [[ "${want}" == "nonzero" && "${status}" -eq 0 ]]; then
    fail "${label}: exit status 0, want non-zero (the misindented file was not linted)"
  fi

  local total
  total="$(grep -cE ':[0-9]+:[0-9]+: (error|warning): \[' <<<"${output}" || true)"
  if [[ -z "${located}" && "${total}" -ne 0 ]]; then
    fail "${label}: ${total} finding(s), want none"
  fi

  local loc file rest line col rule
  for loc in ${located}; do
    file="${loc%%:*}"
    rest="${loc#*:}"
    line="${rest%%:*}"
    rest="${rest#*:}"
    col="${rest%%:*}"
    rule="${rest#*:}"
    # A finding line tells a lint failure apart from the checker's own exit-1 errors.
    if ! grep -qF "/${file}:${line}:${col}: error: [${rule}] " <<<"${rooted}"; then
      fail "${label}: no [${rule}] finding at ${file}:${line}:${col}"
    fi
  done

  for file in ${clean}; do
    if grep -qF "/${file}:" <<<"${rooted}"; then
      fail "${label}: a finding is blamed on ${file}, which is formatted"
    fi
  done

  if [[ "${failures}" -ne "${before}" ]]; then
    echo "---- output of ${label} ----" >&2
    echo "${output}" >&2
    echo "----" >&2
  fi
}

expect 0 "" "formatted.swift" "${formatted}"
expect nonzero \
  "misindented.swift:2:1:Indentation misindented.swift:3:1:Indentation" "" "${misindented}"
# Several files in one call: every argument is linted, not only the first.
expect nonzero \
  "misindented.swift:2:1:Indentation misindented.swift:3:1:Indentation" "formatted.swift" \
  "${formatted}" "${misindented}"

if [[ "${failures}" -gt 0 ]]; then
  echo "swift_format_check_test: ${failures} failure(s) across ${cases} case(s)" >&2
  exit 1
fi
echo "swift_format_check_test: all ${cases} case(s) passed"
