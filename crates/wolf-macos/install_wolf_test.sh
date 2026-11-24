#!/usr/bin/env bash
# Runs install_wolf.sh on a synthetic Wolf.zip with a temporary HOME and a fake lsregister.
# Each case asserts the exit status, the warning on stderr and the installed bundle; bash 3.2 safe.
set -euo pipefail

script="$(dirname "$0")/install_wolf.sh"
if [[ ! -f "${script}" ]]; then
  echo "FAIL: ${script} not found (under Bazel it must be in the sh_test data)" >&2
  exit 1
fi

root="$(cd "$(mktemp -d "${TEST_TMPDIR:-${TMPDIR:-/tmp}}/install-wolf.XXXXXX")" && pwd -P)"
trap 'rm -rf "${root}"' EXIT

fail() {
  echo "FAIL: ($1) $2" >&2
  exit 1
}

# The synthetic bundle has no Info.plist, so even a real registration could never record Wolf.
mkdir -p "${root}/src/Wolf.app/Contents/MacOS"
printf '#!/bin/sh\nexit 0\n' >"${root}/src/Wolf.app/Contents/MacOS/Wolf"
chmod +x "${root}/src/Wolf.app/Contents/MacOS/Wolf"
(cd "${root}/src" && ditto -c -k --keepParent Wolf.app "${root}/Wolf.zip") ||
  fail setup "ditto could not build the synthetic archive"

# The fake logs its arguments, then whether the bundle at its second argument is in place.
fake="${root}/lsregister"
cat >"${fake}" <<'EOF'
#!/bin/sh
echo "$*" >>"$FAKE_LSREGISTER_LOG"
if [ -x "$2/Contents/MacOS/Wolf" ]; then
  echo present >>"$FAKE_LSREGISTER_LOG"
else
  echo absent >>"$FAKE_LSREGISTER_LOG"
fi
exit "$FAKE_LSREGISTER_STATUS"
EOF
chmod +x "${fake}"

case_home=""
case_log=""
case_out=""
case_err=""
case_status=0

# run_case LABEL LSREGISTER STATUS: runs the installer with a fresh HOME and WOLF_LSREGISTER set.
run_case() {
  case_home="${root}/$1/home"
  case_log="${root}/$1/lsregister.log"
  case_out="${root}/$1/stdout"
  case_err="${root}/$1/stderr"
  mkdir -p "${case_home}"
  case_status=0
  HOME="${case_home}" WOLF_LSREGISTER="$2" \
    FAKE_LSREGISTER_LOG="${case_log}" FAKE_LSREGISTER_STATUS="$3" \
    bash "${script}" "${root}/Wolf.zip" >"${case_out}" 2>"${case_err}" || case_status=$?
}

# assert_installed LABEL: the installer exited 0 and left an executable bundle in HOME.
assert_installed() {
  [[ "${case_status}" -eq 0 ]] ||
    fail "$1" "install_wolf.sh exited ${case_status}; registration trouble must not fail the install. stderr: $(cat "${case_err}")"
  [[ -x "${case_home}/Applications/Wolf.app/Contents/MacOS/Wolf" ]] ||
    fail "$1" "no executable bundle at ${case_home}/Applications/Wolf.app: the install did not complete"
}

# assert_warned LABEL: stderr carries the installer's warning prefix.
assert_warned() {
  grep -q 'install_wolf: warning:' "${case_err}" ||
    fail "$1" "stderr has no 'install_wolf: warning:' line: the registration problem went unreported. stderr: $(cat "${case_err}")"
}

# (a) A working lsregister is run once, with -f on the installed path, after the move.
run_case a "${fake}" 0
assert_installed a
[[ -f "${case_log}" ]] ||
  fail a "the fake lsregister was never run: the installer does not register the bundle, or ignores WOLF_LSREGISTER"
expected="$(printf -- '-f %s\npresent' "${case_home}/Applications/Wolf.app")"
actual="$(cat "${case_log}")"
[[ "${actual}" == "${expected}" ]] ||
  fail a "lsregister log is not '-f <installed path>' then 'present'. 'absent' means it ran before the move; a missing -f or another path means the wrong arguments; extra lines mean more than one run. Expected:
${expected}
Got:
${actual}"
if grep -q 'install_wolf: warning:' "${case_err}"; then
  fail a "a successful registration still warned: $(cat "${case_err}")"
fi
echo "PASS check (a): lsregister -f runs on the installed bundle after the move, without a warning"

# (b) A failing lsregister warns, and the install still succeeds.
run_case b "${fake}" 1
assert_installed b
[[ -f "${case_log}" ]] || fail b "the fake lsregister was never run, so its failure was never exercised"
assert_warned b
echo "PASS check (b): a failed registration warns and the install succeeds"

# (c) A missing lsregister warns, naming its path, and the install still succeeds.
missing="${root}/no-such-dir/lsregister"
run_case c "${missing}" 0
assert_installed c
assert_warned c
grep -qF "${missing}" "${case_err}" ||
  fail c "the warning does not name the missing lsregister path ${missing}. stderr: $(cat "${case_err}")"
echo "PASS check (c): a missing lsregister warns with its path and the install succeeds"

echo "PASS: install_wolf.sh registers the installed bundle with lsregister -f, and only warns when it is missing or fails"
