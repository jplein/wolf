#!/usr/bin/env bash
# Runs `regina-run.sh clean` against throwaway repositories, with Bazel stubbed through PATH.
# Each case asserts the exit status, the stdout lines and what is left on disk; bash 3.2 safe.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
script="${here}/../../.claude/skills/regina/regina-run.sh"
if [[ ! -f "${script}" ]]; then
  echo "FAIL: ${script} not found (under Bazel it must be in the sh_test data)" >&2
  exit 1
fi

unset REGINA_AGENT GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE
root="$(cd "$(mktemp -d "${TEST_TMPDIR:-${TMPDIR:-/tmp}}/regina-clean.XXXXXX")" && pwd -P)"
holder=""

cleanup() {
  if [[ -n "${holder}" ]]; then
    kill "${holder}" 2>/dev/null || true
    wait "${holder}" 2>/dev/null || true
  fi
  chmod -R u+w "${root}" 2>/dev/null || true
  rm -rf "${root}"
}
trap cleanup EXIT

export HOME="${root}/home"
mkdir -p "${HOME}"
export GIT_CONFIG_NOSYSTEM=1
export GIT_CONFIG_GLOBAL=/dev/null
export GIT_AUTHOR_NAME="regina clean test"
export GIT_AUTHOR_EMAIL="regina-clean-test@example.invalid"
export GIT_COMMITTER_NAME="${GIT_AUTHOR_NAME}"
export GIT_COMMITTER_EMAIL="${GIT_AUTHOR_EMAIL}"
base_path="${PATH}"

# Line prefixes as regina-run.sh's `printf '%-10s%s\n'` writes them.
removed_p="$(printf '%-10s' removed)"
skipped_p="$(printf '%-10s' skipped)"
failed_p="$(printf '%-10s' failed)"
kept_p="$(printf '%-10s' kept)"
summary_p="$(printf '%-10s' summary)"

checks=0
failures=0
label=""
before=0
status=0
run_path="${base_path}"
run_agent=""
case_dir=""
repo=""
bin=""
log=""
exitf=""
out=""
err=""
base=""
main_seen=""

fail() {
  echo "FAIL: $*" >&2
  failures=$((failures + 1))
}

digest() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256
  else
    sha256sum
  fi
}

# The state-root key regina-run.sh derives for BRANCH in the current case's repository.
key() {
  printf '%s\n%s\n' "${main_seen}" "$1" | digest | cut -c1-8
}

# write_stub NAME DIR: a fake Bazel that logs "<cwd> <name> <args>" and exits with stub.exit.
write_stub() {
  cat >"$2/$1" <<EOF
#!/bin/sh
printf '%s %s %s\n' "\$(pwd -P)" "\${0##*/}" "\$*" >>'${log}'
code=0
if [ -f '${exitf}' ]; then read -r code <'${exitf}'; fi
exit "\$code"
EOF
  chmod +x "$2/$1"
}

# new_case NAME [BASE]: a fresh repository whose default branch is BASE, main by default.
new_case() {
  case_dir="${root}/$1"
  base="${2:-main}"
  repo="${case_dir}/repo"
  bin="${case_dir}/bin"
  log="${case_dir}/stub.log"
  exitf="${case_dir}/stub.exit"
  out="${case_dir}/stdout.txt"
  err="${case_dir}/stderr.txt"
  mkdir -p "${repo}" "${bin}"
  : >"${log}"
  write_stub bazelisk "${bin}"
  write_stub bazel "${bin}"
  git init -q -b "${base}" "${repo}"
  printf 'fixture\n' >"${repo}/README"
  : >"${repo}/MODULE.bazel"
  printf '/bazel-*\n.worktrees\n/.workspace/\n' >"${repo}/.gitignore"
  git -C "${repo}" add README MODULE.bazel .gitignore
  git -C "${repo}" commit -q -m initial
  main_seen="$(dirname "$(git -C "${repo}" rev-parse --path-format=absolute --git-common-dir)")"
  run_path="${bin}:${base_path}"
  run_agent=""
}

# A worktree for BRANCH at .worktrees/BRANCH, on a new branch at the tip of the base branch.
at_main() {
  mkdir -p "$(dirname "${repo}/.worktrees/$1")"
  git -C "${repo}" worktree add -q -b "$1" "${repo}/.worktrees/$1" "${base}"
}

commit_in() {
  printf '%s\n' "$1" >"${repo}/.worktrees/$1/change-${1//\//-}.txt"
  git -C "${repo}/.worktrees/$1" add -A
  git -C "${repo}/.worktrees/$1" commit -q -m "work on $1"
}

# BRANCH has a commit of its own, and the base branch is fast-forwarded to it.
merged() {
  at_main "$1"
  commit_in "$1"
  git -C "${repo}" merge -q --ff-only "$1"
}

unmerged() {
  at_main "$1"
  commit_in "$1"
}

wt_path() {
  (cd "${repo}/.worktrees/$1" && pwd -P)
}

state_of() {
  printf '%s' "${HOME}/.local/state/wolf-regina/$(key "$1")"
}

# write_wrapper BRANCH EXIT: a fake run wrapper that prints status-out.txt for `status --open`.
write_wrapper() {
  mkdir -p "${repo}/.workspace/$1"
  cat >"${repo}/.workspace/$1/regina" <<EOF
#!/bin/sh
if [ "\$#" -ne 2 ] || [ "\$1" != status ] || [ "\$2" != --open ]; then
  echo "fake regina: unexpected arguments: \$*" >&2
  exit 2
fi
cat '${case_dir}/status-out.txt'
exit $2
EOF
  chmod +x "${repo}/.workspace/$1/regina"
}

# PATH without any directory that holds a bazel or bazelisk, real or stub.
path_without_bazel() {
  local IFS=: d kept=""
  for d in ${base_path}; do
    if [[ -z "${d}" || -e "${d}/bazel" || -e "${d}/bazelisk" ]]; then
      continue
    fi
    kept="${kept:+${kept}:}${d}"
  done
  printf '%s' "${kept}"
}

# Exits 0 when LOCKFILE is free, 3 when another process holds it: a non-blocking LOCK_SH.
probe_lock() {
  python3 -I -c '
import errno, fcntl, sys
f = open(sys.argv[1], "r")
try:
    fcntl.flock(f.fileno(), fcntl.LOCK_SH | fcntl.LOCK_NB)
except OSError as e:
    sys.exit(3 if e.errno in (errno.EAGAIN, errno.EWOULDBLOCK) else 4)
' "$1"
}

# start_holder LOCKFILE: a background process holding LOCK_EX on it, as a live regina run does.
start_holder() {
  local ready="${case_dir}/holder.ready" i=0
  python3 -I -c '
import fcntl, sys, time
f = open(sys.argv[1], "a")
fcntl.flock(f.fileno(), fcntl.LOCK_EX)
open(sys.argv[2], "w").close()
time.sleep(600)
' "$1" "${ready}" </dev/null >"${case_dir}/holder.log" 2>&1 &
  holder=$!
  while [[ ! -e "${ready}" && "${i}" -lt 100 ]]; do
    sleep 0.1
    i=$((i + 1))
  done
  [[ -e "${ready}" ]]
}

stop_holder() {
  kill "${holder}" 2>/dev/null || true
  wait "${holder}" 2>/dev/null || true
  holder=""
}

# run_clean LABEL CWD [BRANCH...]: runs `regina-run.sh clean` from CWD with run_path as PATH.
run_clean() {
  label="$1"
  local cwd="$2"
  shift 2
  before="${failures}"
  status=0
  : >"${log}"
  (
    cd "${cwd}" || exit 99
    export PATH="${run_path}"
    if [[ -n "${run_agent}" ]]; then
      export REGINA_AGENT="${run_agent}"
    fi
    exec sh "${script}" clean "$@"
  ) </dev/null >"${out}" 2>"${err}" || status=$?
}

done_run() {
  if [[ "${failures}" -ne "${before}" ]]; then
    echo "---- ${label}: exit ${status} ----" >&2
    echo "-- stdout --" >&2
    cat "${out}" >&2
    echo "-- stderr --" >&2
    cat "${err}" >&2
    echo "-- Bazel stub log --" >&2
    cat "${log}" >&2
    echo "----" >&2
  fi
}

want_status() {
  checks=$((checks + 1))
  if [[ "${status}" -ne "$2" ]]; then
    fail "$1: exit status ${status}, want $2 ($3)"
  fi
}

want_line() {
  checks=$((checks + 1))
  if ! grep -qxF -e "$2" "${out}"; then
    fail "$1: no stdout line '$2' ($3)"
  fi
}

# want_line_like ID WHY PREFIX SUBSTRING...: a stdout line starts with PREFIX and holds each one.
want_line_like() {
  local id="$1" why="$2" prefix="$3" line sub ok found=0
  shift 3
  checks=$((checks + 1))
  while IFS= read -r line; do
    if [[ "${line}" != "${prefix}"* ]]; then
      continue
    fi
    ok=1
    for sub in "$@"; do
      if [[ "${line}" != *"${sub}"* ]]; then
        ok=0
      fi
    done
    if [[ "${ok}" -eq 1 ]]; then
      found=1
    fi
  done <"${out}"
  if [[ "${found}" -eq 0 ]]; then
    fail "${id}: no stdout line starting '${prefix}' and containing '$*' (${why})"
  fi
}

want_stderr() {
  checks=$((checks + 1))
  if ! grep -qF -e "$2" "${err}"; then
    fail "$1: stderr does not mention '$2' ($3)"
  fi
}

want_path() {
  checks=$((checks + 1))
  if [[ ! -e "$2" && ! -L "$2" ]]; then
    fail "$1: $2 is gone ($3)"
  fi
}

want_no_path() {
  checks=$((checks + 1))
  if [[ -e "$2" || -L "$2" ]]; then
    fail "$1: $2 still exists ($3)"
  fi
}

want_branch() {
  checks=$((checks + 1))
  if ! git -C "${repo}" show-ref --verify --quiet "refs/heads/$2"; then
    fail "$1: refs/heads/$2 is gone ($3)"
  fi
}

want_no_branch() {
  checks=$((checks + 1))
  if git -C "${repo}" show-ref --verify --quiet "refs/heads/$2"; then
    fail "$1: refs/heads/$2 still exists ($3)"
  fi
}

# want_unlisted ID FRAGMENT WHY: no line of `git worktree list --porcelain` contains FRAGMENT.
want_unlisted() {
  local listing
  checks=$((checks + 1))
  listing="$(git -C "${repo}" worktree list --porcelain)"
  if grep -qF -e "$2" <<<"${listing}"; then
    fail "$1: git worktree list still has '$2' ($3)"
  fi
}

want_no_stub_calls() {
  checks=$((checks + 1))
  if [[ -s "${log}" ]]; then
    fail "$1: the Bazel stub ran: $(tr '\n' '|' <"${log}") ($2)"
  fi
}

want_no_stub_call_at() {
  checks=$((checks + 1))
  if grep -qF -e "$2 " "${log}"; then
    fail "$1: the Bazel stub ran in $2 ($3)"
  fi
}

# want_stub_call ID LINE WHY: the stub log is exactly the one line LINE.
want_stub_call() {
  local logged
  checks=$((checks + 1))
  logged="$(cat "${log}")"
  if [[ "${logged}" != "$2" ]]; then
    fail "$1: Bazel stub log is '$(tr '\n' '|' <"${log}")', want exactly '$2' ($3)"
  fi
}

# ---- S: a sweep over every kind of worktree ----

new_case S
merged solo/a
solo="$(wt_path solo/a)"
mkdir -p "${case_dir}/outbase"
ln -s "${case_dir}/outbase" "${solo}/bazel-out"
mkdir -p "${repo}/.workspace/solo/a/tasks"
printf 'task\n' >"${repo}/.workspace/solo/a/tasks/t.md"
solo_state="$(state_of solo/a)"
mkdir -p "${solo_state}"
: >"${solo_state}/regina.db"
: >"${solo_state}/regina.lock"
unmerged wip/unmerged
at_main wip/tracked
printf 'edited\n' >>"${repo}/.worktrees/wip/tracked/README"
at_main wip/untracked
printf 'notes\n' >"${repo}/.worktrees/wip/untracked/scratch"
git -C "${repo}" worktree add -q --detach "${repo}/.worktrees/wip/detached" main
at_main wip/locked
git -C "${repo}" worktree lock "${repo}/.worktrees/wip/locked"
at_main wip/gone
rm -rf "${repo}/.worktrees/wip/gone"
git -C "${repo}" worktree add -q -b odd/name "${repo}/.worktrees/elsewhere-name" main
outside="${case_dir}/outside-x"
git -C "${repo}" worktree add -q -b outside/x "${outside}" main

# Fixture premises: the ignored bazel-out link leaves solo/a clean, and scratch is only untracked.
checks=$((checks + 1))
if [[ -n "$(git -C "${solo}" status --porcelain)" ]]; then
  fail "S0a: solo/a is dirty before the run, so S2a cannot tell a dirty check from a working one"
fi
checks=$((checks + 1))
tracked="$(git -C "${repo}/.worktrees/wip/untracked" status --porcelain --untracked-files=no)"
if [[ -n "${tracked}" ]]; then
  fail "S0b: wip/untracked has tracked changes, so S9 cannot tell untracked files apart"
fi

run_clean "S sweep" "${repo}"
want_status S1 0 "1 means a sweep skip counted toward the exit, or the solo/a removal failed"
want_line S2a "${removed_p}solo/a" \
  "merged and clean, so it is removed; an ignored bazel-out link must not count as dirty"
want_no_path S2a "${solo}" "a removed line without the worktree directory going is a lie"
want_unlisted S2a "${solo}" "git still records solo/a as a worktree"
want_no_branch S2b solo/a "git branch -d did not run after the worktree was removed"
want_stub_call S3 "${solo} bazelisk --noblock_for_lock clean --expunge" \
  "one expunge, in solo/a, by bazelisk; a line elsewhere means it ran before a skip check"
want_no_path S4 "${repo}/.worktrees/solo" "the emptied parent of solo/a was not removed"
want_path S4 "${repo}/.worktrees" "the rmdir chain went past the parent and removed .worktrees"
want_path S4 "${repo}/.worktrees/wip" "a parent still holding worktrees was removed"
want_path S5a "${repo}/.workspace/solo/a/tasks/t.md" "the run directory .workspace/solo/a was deleted"
want_path S5a "${solo_state}" "the regina state root was deleted"
want_path S5a "${solo_state}/regina.db" "the state root's regina.db was deleted"
want_path S5a "${solo_state}/regina.lock" "the state root's regina.lock was deleted"
want_line S5b "${kept_p}${main_seen}/.workspace/solo/a" "the kept run directory went unreported"
want_line S5b "${kept_p}${solo_state}" \
  "the kept state root went unreported, or its key differs from regina-run.sh's"
want_line S6 "${removed_p}solo/a" \
  "solo/a's state root has an unheld regina.lock; a lock file alone must not refuse"
want_line S7 "${skipped_p}wip/unmerged: not merged into main" "an unmerged branch was not skipped"
want_path S7 "${repo}/.worktrees/wip/unmerged" "the unmerged worktree was removed"
want_branch S7 wip/unmerged "the unmerged branch was deleted"
want_line S8 "${skipped_p}wip/tracked: uncommitted or untracked changes" \
  "a worktree with an edited tracked file was not skipped"
want_path S8 "${repo}/.worktrees/wip/tracked" "a worktree with uncommitted changes was removed"
want_no_stub_call_at S8 "${repo}/.worktrees/wip/tracked" "Bazel expunged a dirty worktree"
want_line S9 "${skipped_p}wip/untracked: uncommitted or untracked changes" \
  "a worktree whose only change is an untracked file was not skipped"
want_path S9 "${repo}/.worktrees/wip/untracked" "a worktree with an untracked file was removed"
want_no_stub_call_at S9 "${repo}/.worktrees/wip/untracked" "Bazel expunged a dirty worktree"
want_line_like S10 "a detached worktree has no branch to delete, so it must be skipped" \
  "${skipped_p}" "wip/detached" ": detached HEAD"
want_path S10 "${repo}/.worktrees/wip/detached" "the detached worktree was removed"
want_line S11 "${skipped_p}wip/locked: locked" "a locked worktree was not skipped"
want_path S11 "${repo}/.worktrees/wip/locked" "the locked worktree was removed"
want_line S12a "${skipped_p}wip/gone: directory missing" \
  "a worktree whose directory is gone was not reported as missing"
want_line_like S13 "a worktree not at .worktrees/<its branch> must be skipped" \
  "${skipped_p}odd/name: " "not at"
want_path S13 "${repo}/.worktrees/elsewhere-name" "the misplaced worktree was removed"

checks=$((checks + 1))
bad=""
while IFS= read -r line; do
  rest="${line:10}"
  subject="${rest%%: *}"
  case "${subject}" in
    outside/x | "${outside}" | main | "${repo}" | "${main_seen}") bad="${bad}[${line}]" ;;
  esac
  case "${line}" in
    *outside-x* | *outside/x*) bad="${bad}[${line}]" ;;
  esac
done <"${out}"
if [[ -n "${bad}" ]]; then
  fail "S14: the sweep reported the main checkout or a worktree outside .worktrees: ${bad}"
fi
want_path S14 "${outside}/.git" "the worktree outside .worktrees was removed"
want_branch S14 outside/x "the branch of the worktree outside .worktrees was deleted"
want_branch S14 main "the main branch was deleted"
want_path S14 "${repo}/README" "the main checkout was touched"
want_line S15 "${summary_p}removed=1 skipped=7 failed=0" \
  "7 skips: unmerged, tracked, untracked, detached, locked, gone, odd/name; none for outside/x"
done_run

# ---- N: named branches ----

new_case N
merged done/b
unmerged wip/unmerged
merged done/c
mkdir -p "${case_dir}/escape" "${repo}/.worktrees"
ln -s "${case_dir}/escape" "${repo}/.worktrees/sym"
git -C "${repo}" worktree add -q -b sym/x "${repo}/.worktrees/sym/x" main
checks=$((checks + 1))
if [[ ! -e "${case_dir}/escape/x/.git" ]]; then
  fail "N0: sym/x did not land under escape/ through the .worktrees/sym link, so N6 tests nothing"
fi

run_clean "N1 clean done/b" "${repo}" done/b
want_status N1 0 "a named merged clean branch that is removed exits 0"
want_line N1 "${removed_p}done/b" "a named merged clean branch was not removed"
want_no_path N1 "${repo}/.worktrees/done/b" "the done/b worktree directory remains"
done_run

run_clean "N2 clean wip/unmerged" "${repo}" wip/unmerged
want_status N2 1 "a named branch that is skipped must make the exit 1"
want_line N2 "${skipped_p}wip/unmerged: not merged into main" "the unmerged branch was not skipped"
want_path N2 "${repo}/.worktrees/wip/unmerged" "the unmerged worktree was removed"
done_run

run_clean "N3 clean no/such" "${repo}" no/such
want_status N3 1 "a named branch with no worktree must make the exit 1, not a silent no-op"
want_line N3 "${skipped_p}no/such: not checked out in any worktree" \
  "a named branch with no worktree went unreported"
done_run

run_clean "N4 clean done/c bad..name" "${repo}" done/c 'bad..name'
want_status N4 1 "an invalid branch name anywhere in the arguments refuses the whole command"
want_stderr N4 "bad..name" "the refusal does not name the invalid argument"
want_path N4 "${repo}/.worktrees/done/c" \
  "done/c was removed before bad..name was validated; names must all be checked up front"
want_no_stub_calls N4 "Bazel ran although the command was refused"
done_run

run_clean "N5 clean main" "${repo}" main
want_status N5 1 "naming main must make the exit 1"
want_line N5 "${skipped_p}main: is main" "the main branch was not refused as main"
want_path N5 "${repo}/README" "the main checkout was touched"
done_run

run_clean "N6 clean sym/x" "${repo}" sym/x
want_status N6 1 "a worktree reached through a symlink out of .worktrees must make the exit 1"
want_line_like N6 "the escaping worktree was not skipped by the layout check" "${skipped_p}sym/x: "
want_path N6 "${case_dir}/escape/x/.git" "the worktree outside .worktrees was removed"
want_branch N6 sym/x "the escaping worktree's branch was deleted"
want_no_stub_calls N6 "Bazel expunged a worktree outside .worktrees"
done_run

# ---- C: the command runs from inside a worktree ----

new_case C
merged done/self
self="$(wt_path done/self)"
run_clean "C1 sweep from done/self" "${self}"
want_status C1 0 "a sweep skip does not change the exit"
want_line C1 "${skipped_p}done/self: this command runs from it" \
  "the worktree the command runs from was not skipped"
want_path C1 "${self}" "the worktree the command runs from was removed"
want_no_stub_calls C1 "Bazel expunged the worktree the command runs from"
done_run

# ---- L: a regina command holds the state root's lock ----

new_case L
merged done/run
run_dir="$(wt_path done/run)"
run_state="$(state_of done/run)"
mkdir -p "${run_state}"
: >"${run_state}/regina.lock"
checks=$((checks + 1))
if ! start_holder "${run_state}/regina.lock"; then
  fail "L0: the lock holder never became ready: $(cat "${case_dir}/holder.log")"
else
  checks=$((checks + 1))
  lock_status=0
  probe_lock "${run_state}/regina.lock" || lock_status=$?
  if [[ "${lock_status}" -ne 3 ]]; then
    fail "L0: a LOCK_SH|LOCK_NB probe exited ${lock_status} under a held LOCK_EX, not 3 (held)"
  fi

  run_clean "L1 sweep with the lock held" "${repo}"
  want_status L1 0 "a sweep skip does not change the exit"
  want_line_like L1 "a held regina.lock means a live regina run, so the worktree is skipped" \
    "${skipped_p}done/run: a regina command holds"
  want_path L1 "${run_dir}" "the worktree of a live run was removed"
  want_no_stub_calls L1 "Bazel expunged the worktree of a live run"
  done_run

  stop_holder
  checks=$((checks + 1))
  lock_status=0
  probe_lock "${run_state}/regina.lock" || lock_status=$?
  if [[ "${lock_status}" -ne 0 ]]; then
    fail "L0: the probe exited ${lock_status} after the holder died, not 0 (free)"
  fi

  run_clean "L2 sweep with the lock released" "${repo}"
  want_status L2 0 "the released lock still blocks, or the removal failed"
  want_line L2 "${removed_p}done/run" \
    "the lock file exists but nobody holds it; its presence alone must not refuse"
  done_run
fi

# ---- T: the run wrapper's open tasks ----

new_case T
merged done/q
q_dir="$(wt_path done/q)"

printf '%s\n' "4 queued wolf t1 done/q attempts=0 last=none now=none" "stuck: none" \
  >"${case_dir}/status-out.txt"
write_wrapper done/q 0
run_clean "T1 sweep with an open task" "${repo}"
want_status T1 0 "a sweep skip does not change the exit"
want_line_like T1 "a run with an open task is active, so its worktree is skipped" \
  "${skipped_p}done/q: open tasks in"
want_path T1 "${q_dir}" "the worktree of a run with open tasks was removed"
want_no_stub_calls T1 "Bazel expunged the worktree of a run with open tasks"
done_run

# It prints the no-tasks line yet fails, so only its exit status can make the skip.
printf '%s\n' "no open tasks" >"${case_dir}/status-out.txt"
write_wrapper done/q 1
run_clean "T2 sweep with a failing status" "${repo}"
want_status T2 0 "a sweep skip does not change the exit"
want_line_like T2 "a failing status --open must fail closed, whatever it printed" \
  "${skipped_p}done/q: " "status --open exited 1"
want_path T2 "${q_dir}" "the worktree was removed although status --open failed"
want_no_stub_calls T2 "Bazel expunged a worktree whose run status is unknown"
done_run

printf '%s\n' "no open tasks" "stuck: none" >"${case_dir}/status-out.txt"
write_wrapper done/q 0
run_clean "T3 sweep with no open tasks" "${repo}"
want_status T3 0 "a run with no open tasks blocks nothing"
want_line T3 "${removed_p}done/q" "a wrapper reporting no open tasks must not refuse"
want_path T3 "${repo}/.workspace/done/q/regina" "the run wrapper was deleted"
done_run

# ---- B: the Bazel binary and its output ----

new_case B
merged done/z
z_dir="$(wt_path done/z)"

printf '1\n' >"${exitf}"
run_clean "B1 sweep with a failing expunge" "${repo}"
want_status B1 1 "a failed line must make the exit 1"
want_line B1 "${failed_p}done/z: bazelisk clean --expunge exited 1; worktree kept" \
  "a failed expunge must keep the worktree, whose output base would be stranded"
want_path B1 "${z_dir}" "the worktree was removed after its expunge failed"
want_branch B1 done/z "the branch was deleted after its expunge failed"
done_run
rm -f "${exitf}"

filtered="$(path_without_bazel)"
premise_ok=1
checks=$((checks + 1))
for tool in git python3 sh awk sed cut dirname basename rmdir cat tr; do
  if ! (PATH="${filtered}" && command -v "${tool}" >/dev/null 2>&1); then
    fail "B0: ${tool} does not resolve once bazel's directories leave PATH: ${filtered}"
    premise_ok=0
  fi
done
if ! (PATH="${filtered}" && { command -v shasum || command -v sha256sum; } >/dev/null 2>&1); then
  fail "B0: neither shasum nor sha256sum resolves once bazel's directories leave PATH"
  premise_ok=0
fi
if (PATH="${filtered}" && { command -v bazel || command -v bazelisk; } >/dev/null 2>&1); then
  fail "B0: bazel or bazelisk still resolves on the filtered PATH: ${filtered}"
  premise_ok=0
fi

if [[ "${premise_ok}" -eq 1 ]]; then
  mkdir -p "${case_dir}/outbase"
  run_path="${filtered}"
  # B2 and B3 reuse done/z; if a broken run removed it, skip them instead of aborting under set -e.
  z_ok=1
  checks=$((checks + 1))
  if [[ ! -d "${z_dir}" ]]; then
    fail "B1: fixture: done/z is gone, so B2 and B3 cannot set up"
    z_ok=0
  fi

  if [[ "${z_ok}" -eq 1 ]]; then
    ln -s "${case_dir}/outbase" "${z_dir}/bazel-out"
    run_clean "B2 sweep with Bazel output and no binary" "${repo}"
    want_status B2 0 "a sweep skip does not change the exit"
    want_line B2 "${skipped_p}done/z: Bazel output but no bazelisk or bazel on PATH" \
      "an output base that cannot be expunged would be stranded by the removal"
    want_path B2 "${z_dir}" "the worktree with unexpungeable Bazel output was removed"
    done_run

    checks=$((checks + 1))
    if [[ ! -d "${z_dir}" ]]; then
      fail "B2: fixture: done/z is gone, so B3 cannot set up"
      z_ok=0
    fi
  fi

  if [[ "${z_ok}" -eq 1 ]]; then
    # -f: a link the run deleted still leaves B3's premise, a worktree with no Bazel output.
    rm -f "${z_dir}/bazel-out"
    run_clean "B3 sweep with no Bazel output and no binary" "${repo}"
    want_status B3a 0 "a worktree with no Bazel output needs no binary"
    want_line B3a "${removed_p}done/z" "a worktree that never ran Bazel was refused for lack of one"
    want_no_path B3a "${z_dir}" "the removed worktree's directory remains"
    want_path B3b "${repo}/.worktrees" "the rmdir chain removed .worktrees itself"
    checks=$((checks + 1))
    if [[ -d "${repo}/.worktrees" && -n "$(ls -A "${repo}/.worktrees")" ]]; then
      fail "B3b: .worktrees is not empty: $(ls -A "${repo}/.worktrees" | tr '\n' ' ')"
    fi
    done_run
  fi

  merged done/y
  y_dir="$(wt_path done/y)"
  mkdir -p "${case_dir}/bazel-only"
  write_stub bazel "${case_dir}/bazel-only"
  run_path="${case_dir}/bazel-only:${filtered}"
  run_clean "B4 sweep with bazel and no bazelisk" "${repo}"
  want_status B4 0 "bazel alone is a binary that can expunge"
  want_line B4 "${removed_p}done/y" "bazel without bazelisk was not used"
  want_stub_call B4 "${y_dir} bazel --noblock_for_lock clean --expunge" \
    "bazel is the fallback when bazelisk is absent; an empty log means no expunge ran"
  done_run
fi

# ---- D: git branch -d refuses after the worktree is gone ----

new_case D
git -C "${repo}" branch other
merged done/h
h_dir="$(wt_path done/h)"
# Merged into main but not into HEAD, so `git branch -d` refuses it from the main checkout.
git -C "${repo}" checkout -q other
run_clean "D1 sweep with HEAD off main" "${repo}"
want_status D1 1 "a failed line must make the exit 1"
want_line D1 "${failed_p}done/h: worktree removed; git branch -d refused, branch kept" \
  "a refused git branch -d went unreported"
want_no_path D1 "${h_dir}" "the worktree should be gone before git branch -d runs"
want_branch D1 done/h "the branch was force-deleted; only git branch -d is allowed"
done_run

# ---- G: inside a regina worker ----

new_case G
merged done/g
g_dir="$(wt_path done/g)"
run_agent=1
run_clean "G1 sweep with REGINA_AGENT set" "${repo}"
run_agent=""
want_status G1 1 "a regina worker must be refused outright"
want_stderr G1 "REGINA_AGENT" "the refusal does not say REGINA_AGENT is why"
want_path G1 "${g_dir}" "a regina worker removed a worktree"
want_no_stub_calls G1 "Bazel ran although the command was refused"
done_run

# ---- M: no local main branch ----

new_case M trunk
at_main done/m
m_dir="$(wt_path done/m)"
run_clean "M1 sweep with no main branch" "${repo}"
want_status M1 1 "with no main there is nothing to be merged into, so the command refuses"
want_stderr M1 "no local main branch" "the refusal does not say main is missing"
want_path M1 "${m_dir}" "a worktree was removed with no main to check against"
want_no_stub_calls M1 "Bazel ran although the command was refused"
done_run

if [[ "${failures}" -gt 0 ]]; then
  echo "regina_run_clean_test: ${failures} failure(s) across ${checks} assertion(s)" >&2
  exit 1
fi
echo "regina_run_clean_test: all ${checks} assertion(s) passed"
