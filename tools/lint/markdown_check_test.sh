#!/usr/bin/env bash
# Runs markdown_check.sh over testdata/markdown: exit status, rule finding counts, --entry-words.
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

# ---- --entry-words ----------------------------------------------------------

entries="./other/entries.md"
if [[ ! -f "${data}/${entries}" ]]; then
  echo "FAIL: ${data}/${entries} not found (under Bazel, glob testdata/** into data)" >&2
  exit 1
fi

# entries.md's #### entries as LINE:WORDS:HEADING, counted by hand: the sum of awk's NF over each
# entry's lines after its heading, fence lines and fenced lines excluded. Gamma alone is largest.
ENTRIES=(
  "5:11:\`alpha.rs\`"
  "11:13:Beta entry"
  "15:30:Gamma with a fence"
  "26:5:Delta, with %s and %d in it"
  "34:8:Epsilon"
)
largest_line=15
largest=30
largest_heading="Gamma with a fence"

scratch="$(mktemp -d "${TEST_TMPDIR:-${TMPDIR:-/tmp}}/markdown_check_test.XXXXXX")"
trap 'rm -rf "${scratch}"' EXIT
out="${scratch}/stdout"
err="${scratch}/stderr"
want="${scratch}/want"
label=""
before=0
status=0

# want_words BUDGET: writes to ${want} the whole stdout --entry-words owes for entries.md.
want_words() {
  local budget="$1" e line rest words heading
  : >"${want}"
  for e in "${ENTRIES[@]}"; do
    line="${e%%:*}"
    rest="${e#*:}"
    words="${rest%%:*}"
    heading="${rest#*:}"
    printf '%s:%s: %s/%s words: %s\n' "${entries}" "${line}" "${words}" "${budget}" "${heading}" \
      >>"${want}"
  done
}

# run_checker BUDGET [ARG...]: runs the checker from the fixture root into ${out} and ${err}, with
# WOLF_MD_MAX_ENTRY_WORDS set to BUDGET, or unset when BUDGET is empty.
run_checker() {
  local budget="$1"
  shift
  label="markdown_check.sh $*${budget:+ (WOLF_MD_MAX_ENTRY_WORDS=${budget})}"
  cases=$((cases + 1))
  before="${failures}"
  status=0
  (
    cd "${data}"
    unset WOLF_MD_MAX_ENTRY_WORDS
    if [[ -n "${budget}" ]]; then export WOLF_MD_MAX_ENTRY_WORDS="${budget}"; fi
    exec bash "${checker}" ${1+"$@"}
  ) </dev/null >"${out}" 2>"${err}" || status=$?
}

expect_status() {
  if [[ "${status}" -ne "$1" ]]; then
    fail "${label}: exit status ${status}, want $1 ($2)"
  fi
}

expect_words_stdout() {
  if ! cmp -s "${want}" "${out}"; then
    fail "${label}: stdout is not exactly one count line per entry. A missing line means only" \
      "over-budget entries print; an extra one a header, a rule's finding or a summary; a" \
      "different count that fenced lines were counted; a different file or line that the entry" \
      "was blamed where it closed rather than where it began"
  fi
}

done_case() {
  if [[ "${failures}" -ne "${before}" ]]; then
    echo "---- ${label} (exit ${status}) ----" >&2
    echo "-- stdout:" >&2
    cat "${out}" >&2
    echo "-- stderr:" >&2
    cat "${err}" >&2
    if [[ -s "${want}" ]]; then
      echo "-- expected stdout:" >&2
      cat "${want}" >&2
    fi
    echo "----" >&2
  fi
  : >"${want}"
}

# W1. One line per entry with the default budget, and nothing else: no header, no [changelog]
# for line 13's "now", no line for the fenced "#### ", and no "No findings.".
run_checker "" --entry-words "${entries}"
expect_status 0 "a report mode never fails; 1 means a rule ran or an entry failed its budget"
want_words 250
expect_words_stdout
done_case

# W2. The count is the one entry-budget applies: one word under the largest fires on it alone...
run_checker "$((largest - 1))" "${entries}"
expect_status 1 "the [changelog] on line 13 and the [entry-budget] on the largest entry"
want_line="${entries}:${largest_line}: [entry-budget] entry ${largest_heading} is ${largest} words"
want_line="${want_line} (budget $((largest - 1)))"
got="$(grep -F ': [entry-budget] ' "${out}" || true)"
if [[ "${got}" != "${want_line}" ]]; then
  fail "${label}: the [entry-budget] findings are not exactly '${want_line}': none means the" \
    "default mode counts fewer words than --entry-words, another count or entry that the two" \
    "modes disagree"
fi
done_case

# ...and the largest itself as the budget fires on nothing.
run_checker "${largest}" "${entries}"
expect_status 1 "the [changelog] on line 13, which shows the file was checked"
if ! grep -qF "${entries}:13: [changelog] " "${out}"; then
  fail "${label}: no [changelog] at line 13, so the checker never reached the file"
fi
if grep -qF ': [entry-budget] ' "${out}"; then
  fail "${label}: an [entry-budget] finding at a budget equal to the largest count, so the" \
    "default mode counts more words than --entry-words"
fi
done_case

# An over-budget entry is reported, not failed.
run_checker "$((largest - 1))" --entry-words "${entries}"
expect_status 0 "an entry over budget fails only the default mode"
want_words "$((largest - 1))"
expect_words_stdout
done_case

# W3. The last entry is closed by the next file's first line, and is still blamed on its own.
run_checker "" --entry-words "${entries}" ./other/no-sha.md
expect_status 0 "a report mode never fails"
want_words 250
expect_words_stdout
done_case

# W4. The option alone names no file, which the checker refuses rather than reading stdin.
run_checker "" --entry-words
expect_status 1 "0 means the checker read the empty stdin as a file"
if ! grep -q '^ERROR:' "${out}" "${err}"; then
  fail "${label}: no ERROR: line saying no files were passed"
fi
done_case

if [[ "${failures}" -gt 0 ]]; then
  echo "markdown_check_test: ${failures} failure(s) across ${cases} case(s)" >&2
  exit 1
fi
echo "markdown_check_test: all ${cases} case(s) passed"
