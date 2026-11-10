#!/usr/bin/env bash
# Runs rustfmt_files.sh over a scratch checkout of testdata/rustfmt_files copies, under bash 3.2.
# Bazel passes the rustfmt toolchain's binary as $1; without it, the PATH's rustfmt is used.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
fixer="${here}/rustfmt_files.sh"
data="${here}/testdata/rustfmt_files"
formatted="${data}/formatted.rs"
misformatted="${data}/misformatted.rs"

# Made absolute before any cd: Bazel's rootpath is relative to the runfiles root the test starts in.
if [[ "$#" -gt 0 ]]; then
  if [[ ! -f "$1" ]]; then
    echo "FAIL: rustfmt '$1' not found (under Bazel it must be in the sh_test data)" >&2
    exit 1
  fi
  rustfmt="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
else
  rustfmt="$(command -v rustfmt || true)"
  if [[ -z "${rustfmt}" ]]; then
    echo "FAIL: no rustfmt argument, and no rustfmt on PATH (run it in the devShell)" >&2
    exit 1
  fi
fi
if [[ ! -x "${rustfmt}" ]]; then
  echo "FAIL: rustfmt '${rustfmt}' is not executable" >&2
  exit 1
fi

if [[ ! -f "${fixer}" ]]; then
  echo "FAIL: ${fixer} not found (under Bazel it must be in the sh_test data)" >&2
  exit 1
fi
for fixture in "${formatted}" "${misformatted}"; do
  if [[ ! -f "${fixture}" ]]; then
    echo "FAIL: ${fixture} not found (under Bazel, glob testdata/rustfmt_files into data)" >&2
    exit 1
  fi
done
# Equal fixtures would make every "formatted" and "untouched" check below pass at once.
if cmp -s "${formatted}" "${misformatted}"; then
  echo "FAIL: misformatted.rs equals formatted.rs, so no case can tell a change apart" >&2
  exit 1
fi

scratch="$(mktemp -d "${TEST_TMPDIR:-${TMPDIR:-/tmp}}/rustfmt_files_test.XXXXXX")"
trap 'rm -rf "${scratch}"' EXIT
ws="${scratch}/ws"
# The fixer runs from here, so a path it resolves against the cwd names nothing.
cwd="${scratch}/cwd"
mkdir -p "${cwd}"
out="${scratch}/stdout"
err="${scratch}/stderr"

a_rel="crates/wolf-gnome/src/a.rs"
b_rel="crates/wolf-gnome/src/b.rs"
f_rel="crates/wolf-gnome/src/formatted.rs"
# Misformatted Rust that rustfmt would rewrite, so only the name check refuses it.
t_rel="crates/wolf-gnome/src/a.txt"
a="${ws}/${a_rel}"
b="${ws}/${b_rel}"
f="${ws}/${f_rel}"
t="${ws}/${t_rel}"
d="${scratch}/outside/d.rs"
# Its path starts with "${ws}", so only a check for "${ws}/" as a prefix refuses it.
e="${scratch}/ws-sibling/e.rs"

# Regular copies, never runfiles symlinks, which rustfmt would write through.
reset_tree() {
  rm -rf "${ws}" "${scratch}/outside" "${scratch}/ws-sibling"
  local file
  for file in "${a}" "${b}" "${t}" "${d}" "${e}" "${f}"; do
    mkdir -p "$(dirname "${file}")"
    if [[ "${file}" == "${f}" ]]; then
      cp "${formatted}" "${file}"
    else
      cp "${misformatted}" "${file}"
    fi
    if [[ -L "${file}" || ! -f "${file}" ]]; then
      echo "FAIL: ${file} is not a regular file, so the cases would not test a checkout" >&2
      exit 1
    fi
  done
}

failures=0
cases=0
label=""
before=0
status=0

fail() {
  echo "FAIL: ${label}: $*" >&2
  failures=$((failures + 1))
}

# run_in DIR BWD RUSTFMT LABEL [ARG...]: rebuilds the tree, then runs the fixer from DIR with
# BUILD_WORKSPACE_DIRECTORY set to BWD, or unset when BWD is "<unset>", and stdin empty.
run_in() {
  local dir="$1" bwd="$2" fmt="$3"
  label="rustfmt_files.sh $4"
  shift 4
  cases=$((cases + 1))
  before="${failures}"
  reset_tree
  status=0
  (
    cd "${dir}"
    unset BUILD_WORKSPACE_DIRECTORY
    if [[ "${bwd}" != "<unset>" ]]; then export BUILD_WORKSPACE_DIRECTORY="${bwd}"; fi
    exec bash "${fixer}" "${fmt}" ${1+"$@"}
  ) </dev/null >"${out}" 2>"${err}" || status=$?
}

# run LABEL [ARG...]: run_in from the empty cwd, with the scratch checkout and the absolute rustfmt.
run() {
  run_in "${cwd}" "${ws}" "${rustfmt}" "$@"
}

done_case() {
  if [[ "${failures}" -ne "${before}" ]]; then
    echo "---- ${label} (exit ${status}) ----" >&2
    echo "-- stdout:" >&2
    cat "${out}" >&2
    echo "-- stderr:" >&2
    cat "${err}" >&2
    echo "----" >&2
  fi
}

expect_status() {
  if [[ "${status}" -ne "$1" ]]; then
    fail "exit status ${status}, want $1 ($2)"
  fi
}

expect_formatted() {
  if ! cmp -s "$1" "${formatted}"; then
    fail "${1#"${scratch}"/} is not formatted.rs (the fixer skipped a file it should format," \
      "or rustfmt ran without --edition 2021 and rejected its async fn)"
  fi
}

expect_untouched() {
  if ! cmp -s "$1" "${misformatted}"; then
    fail "${1#"${scratch}"/} changed (the fixer formatted a file it was not asked to, or was" \
      "refused)"
  fi
}

# expect_only_formatted [FILE...]: the misformatted copies named are formatted, the rest untouched.
expect_only_formatted() {
  local file named want
  for file in "${a}" "${b}" "${t}" "${d}" "${e}"; do
    named=0
    for want in ${1+"$@"}; do
      if [[ "${want}" == "${file}" ]]; then named=1; fi
    done
    if [[ "${named}" -eq 1 ]]; then
      expect_formatted "${file}"
    else
      expect_untouched "${file}"
    fi
  done
  if ! cmp -s "${f}" "${formatted}"; then
    fail "ws/${f_rel} changed although it was already formatted"
  fi
}

# R1. A checkout-relative argument from another cwd is formatted in place, and only it.
run "${a_rel}" "${a_rel}"
expect_status 0 "rustfmt's own status on a file it formats"
expect_only_formatted "${a}"
done_case

# R1. So is an absolute argument under the checkout.
run "<abs>/ws/${b_rel}" "${b}"
expect_status 0 "rustfmt's own status on a file it formats"
expect_only_formatted "${b}"
done_case

# R1. RUSTFMT relative to the starting cwd, as `bazel run` passes it from the runfiles tree.
run_in "$(dirname "${rustfmt}")" "${ws}" "./$(basename "${rustfmt}")" \
  "./$(basename "${rustfmt}") ${a_rel}" "${a_rel}"
expect_status 0 "a relative RUSTFMT is resolved against the cwd the fixer starts in"
expect_only_formatted "${a}"
done_case

# R2. --check is passed on: rustfmt reports the misformatted file and leaves it as it is...
run "--check ${a_rel}" --check "${a_rel}"
expect_status 1 "rustfmt --check's status for a file it would change"
expect_only_formatted
if ! grep -qF "${a_rel}" "${out}"; then
  fail "stdout never names ${a_rel}, so rustfmt did not check it (a refusal also exits 1)"
fi
if grep -q '^ERROR:' "${err}"; then
  fail "the fixer refused an argument, so --check was taken for a file"
fi
done_case

# ...and passes a formatted one.
run "--check ${f_rel}" --check "${f_rel}"
expect_status 0 "1 means --check was refused as a file, or the formatted copy failed the check"
expect_only_formatted
done_case

# R3. Each bad argument follows a good one: one refusal formats nothing and exits 1.
for bad in \
  "${t_rel}" \
  "crates/wolf-gnome/src/missing.rs" \
  "../outside/d.rs" \
  "${d}" \
  "../ws-sibling/e.rs" \
  "${e}"; do
  run "${a_rel} ${bad#"${scratch}"/}" "${a_rel}" "${bad}"
  expect_status 1 "0 means the bad argument was accepted or the refusal did not stop the run"
  if ! grep -qF "'${bad}'" "${err}"; then
    fail "stderr never names '${bad}' as given (no refusal message, or a resolved path)"
  fi
  expect_only_formatted
  done_case
done

# R4. No FILE: refused, where rustfmt itself would read the empty stdin and exit 0.
run "(no file)"
expect_status 2 "0 means rustfmt ran with no file and read stdin"
expect_only_formatted
done_case

run "--check (no file)" --check
expect_status 2 "0 means rustfmt ran with no file and read stdin"
expect_only_formatted
done_case

# R5. Without a checkout there is nothing to resolve against, even for an absolute file.
run_in "${cwd}" "<unset>" "${rustfmt}" "${a} (BUILD_WORKSPACE_DIRECTORY unset)" "${a}"
expect_status 2 "the fixer runs only under bazel run, which sets BUILD_WORKSPACE_DIRECTORY"
expect_only_formatted
done_case

run_in "${cwd}" "" "${rustfmt}" "${a} (BUILD_WORKSPACE_DIRECTORY empty)" "${a}"
expect_status 2 "an empty BUILD_WORKSPACE_DIRECTORY names no checkout"
expect_only_formatted
done_case

# A RUSTFMT that is not there is refused before any file is looked at.
run_in "${cwd}" "${ws}" "${scratch}/no-such-rustfmt" "(missing RUSTFMT) ${a_rel}" "${a_rel}"
expect_status 2 "the fixer must check RUSTFMT before it execs it"
if ! grep -q '^ERROR:' "${err}"; then
  fail "no ERROR: line for the missing RUSTFMT"
fi
expect_only_formatted
done_case

if [[ "${failures}" -gt 0 ]]; then
  echo "rustfmt_files_test: ${failures} failure(s) across ${cases} case(s)" >&2
  exit 1
fi
echo "rustfmt_files_test: all ${cases} case(s) passed"
