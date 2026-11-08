#!/usr/bin/env bash
# Runs wolf_gnome_wording_check.sh over the fake checkouts in testdata/wolf_gnome_wording.
# Each case asserts the exit status and the exact FAIL: lines; must run under /bin/bash 3.2.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
checker="${here}/wolf_gnome_wording_check.sh"
data="${here}/testdata/wolf_gnome_wording"
src="crates/wolf-gnome/src"

if [[ ! -f "${checker}" ]]; then
  echo "FAIL: ${checker} not found (under Bazel it must be in the sh_test data)" >&2
  exit 1
fi
for root in clean unlisted line-start allowlisted wrong-file; do
  if [[ ! -d "${data}/${root}/${src}" ]]; then
    echo "FAIL: ${data}/${root} not found (under Bazel, glob testdata/wolf_gnome_wording)" >&2
    exit 1
  fi
done

scratch="$(mktemp -d "${TEST_TMPDIR:-${TMPDIR:-/tmp}}/wolf_gnome_wording_check_test.XXXXXX")"
trap 'rm -rf "${scratch}"' EXIT

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

# check LABEL ARG...: runs the checker with the given arguments, stdout and stderr together.
check() {
  label="$1"
  shift
  before="${failures}"
  cases=$((cases + 1))
  status=0
  output="$(bash "${checker}" "$@" 2>&1)" || status=$?
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

# want_fail_lines WANT WHY: the FAIL: lines, in order, are exactly WANT's lines.
want_fail_lines() {
  local got
  got="$(grep '^FAIL: ' <<<"${output}" || true)"
  if [[ "${got}" != "$1" ]]; then
    fail "${label}: FAIL: lines differ from the expected ones ($2)"
    echo "---- want ----" >&2
    echo "$1" >&2
  fi
}

no_fail_lines() {
  if grep -q '^FAIL: ' <<<"${output}"; then
    fail "${label}: printed a FAIL: line ($1)"
  fi
}

want_error_line() {
  if ! grep -q '^ERROR: ' <<<"${output}"; then
    fail "${label}: no ERROR: line ($1)"
  fi
}

check "clean tree" "${data}/clean" "${data}/clean/allowlist.txt"
want_status 0 "1 means a clean line was reported: DNSServer or _NSFoo matched NS[A-Z] with no \
word boundary, or README.md was scanned though only *.rs files are; 2 an error"
no_fail_lines "DNSServer, _NSFoo and GTK are clean, and src/README.md is not a Rust source"
want_line "wolf_gnome_wording_check: 1 file(s) clean, 0 allowlisted line(s)" \
  "a different file count means README.md was counted or lib.rs missed; a nonzero entry count \
that the # line was read as an entry"
done_case

check "unlisted mentions" "${data}/unlisted" "${data}/unlisted/allowlist.txt"
want_status 1 "0 means no alternative of the patterns matched; 2 an error"
want_fail_lines "$(printf '%s\n' \
  "FAIL: ${src}/drawer.rs:4: /// the \`min_drawer_height\` floor \`DrawerLayout.swift\` shares." \
  "FAIL: ${src}/tree_view.rs:3: //! GTK has no \`NSOutlineView\` equivalent." \
  "FAIL: ${src}/window.rs:3: /// Scrolls the window as on macOS." \
  "FAIL: ${src}/window.rs:6: /// Builds an Appkit-style toolbar.")" \
  "a missing drawer.rs line means swift is not matched; tree_view.rs, that the NS pattern is \
missing or its boundary is broken, e.g. a literal backspace from echo; window.rs:3, that macos is \
not matched case-insensitively; window.rs:6, the same for appkit. A wrong number or untrimmed \
text means the line is misreported, and a reordering that files are not sorted"
want_line "wolf_gnome_wording_check: 4 finding(s)" "the summary miscounts the unexcused hits"
done_case

check "NS at the start of a line" "${data}/line-start" "${data}/line-start/allowlist.txt"
want_status 1 "0 means the NS pattern needs a character before NS, so a line-initial NSView passes"
want_fail_lines "FAIL: ${src}/view.rs:2: NSView has no counterpart in this tree." \
  "the boundary must accept the start of a line as well as a non-word character"
want_line "wolf_gnome_wording_check: 1 finding(s)" "the summary miscounts the unexcused hits"
done_case

check "allowlisted site" "${data}/allowlisted" "${data}/allowlisted/allowlist.txt"
want_status 0 "1 means the allowlist was ignored, or the indented line was compared untrimmed, or \
the blank allowlist line became an entry that is stale; 2 an error"
no_fail_lines "the only hit is the allowlisted scrollbar.rs line"
want_line "wolf_gnome_wording_check: 1 file(s) clean, 1 allowlisted line(s)" \
  "the entry count must skip the # line and the blank line and count the one entry"
done_case

check "allowlisted text in another file" "${data}/wrong-file" "${data}/allowlisted/allowlist.txt"
want_status 1 "0 means the key is the line text alone, without the file it is in"
want_fail_lines "FAIL: ${src}/other.rs:4: /// Unlike the macOS bar, this one never fades." \
  "other.rs must be reported, scrollbar.rs excused by its entry, and the entry must not be \
stale, since scrollbar.rs still holds it"
want_line "wolf_gnome_wording_check: 1 finding(s)" "the summary miscounts the unexcused hits"
done_case

check "stale allowlist entry" "${data}/clean" "${data}/allowlisted/allowlist.txt"
want_status 1 "0 means an entry that matches no line passes, so the allowlist can go stale"
want_fail_lines \
  "FAIL: stale allowlist entry: ${src}/scrollbar.rs:/// Unlike the macOS bar, this one never fades." \
  "the one entry names a file the clean tree does not have, and no other line may be reported"
want_line "wolf_gnome_wording_check: 1 finding(s)" "a stale entry must count as a finding"
done_case

# Runfiles are symlinks, so a listing that keeps only regular files finds nothing under Bazel.
mkdir -p "${scratch}/linked/${src}"
ln -s "${data}/unlisted/${src}/tree_view.rs" "${scratch}/linked/${src}/tree_view.rs"
check "symlinked source" "${scratch}/linked" "${data}/clean/allowlist.txt"
want_status 1 "2 means the symlink was not listed (find -type f without -L); 0 that it went unread"
want_fail_lines "FAIL: ${src}/tree_view.rs:3: //! GTK has no \`NSOutlineView\` equivalent." \
  "the symlinked file must be scanned like a regular one, its path relative to ROOT"
done_case

# Bazel stages no empty directory, so this tree is made here; README.md is there but no *.rs.
mkdir -p "${scratch}/empty/${src}"
cp "${data}/clean/${src}/README.md" "${scratch}/empty/${src}/README.md"
check "no Rust source" "${scratch}/empty" "${data}/clean/allowlist.txt"
want_status 2 "0 means an empty tree, as from a broken Bazel data list, passes vacuously; 1 that \
README.md was scanned"
no_fail_lines "an error must not be reported as a finding"
want_error_line "the empty tree must be explained on stderr"
done_case

mkdir -p "${scratch}/bare"
check "no wolf-gnome src directory" "${scratch}/bare" "${data}/clean/allowlist.txt"
want_status 2 "0 means a wrong ROOT passes vacuously; 1 that it was reported as a finding"
no_fail_lines "an error must not be reported as a finding"
want_error_line "the missing directory must be explained on stderr"
done_case

check "missing allowlist" "${data}/clean" "${data}/no-such-allowlist.txt"
want_status 2 "0 or 1 means an unreadable allowlist was read as an empty one"
no_fail_lines "an error must not be reported as a finding"
want_error_line "the missing allowlist must be named on stderr"
done_case

check "too many arguments" "${data}/clean" "${data}/clean/allowlist.txt" extra
want_status 2 "0 means a third argument was silently ignored"
no_fail_lines "a usage error must not be reported as a finding"
want_error_line "the usage error must be explained on stderr"
done_case

if [[ "${failures}" -gt 0 ]]; then
  echo "wolf_gnome_wording_check_test: ${failures} failure(s) across ${cases} case(s)" >&2
  exit 1
fi
echo "wolf_gnome_wording_check_test: all ${cases} case(s) passed"
