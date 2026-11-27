#!/usr/bin/env bash
# Runs markdown_all.sh in a scratch repository with markdown_check.sh stubbed to echo its arguments,
# and asserts the exact file list: the .claude/ scope, exclusions, ignores. bash 3.2 safe.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
script="${here}/markdown_all.sh"
if [[ ! -f "${script}" ]]; then
  echo "FAIL: ${script} not found (under Bazel it must be in the sh_test data)" >&2
  exit 1
fi
if ! command -v git >/dev/null 2>&1; then
  echo "FAIL: no git on PATH; markdown_all.sh lists files with host git. PATH=${PATH}" >&2
  exit 1
fi
if ! bash -c 'mapfile -t x </dev/null' 2>/dev/null; then
  echo "FAIL: markdown_all.sh needs bash 4 or later on PATH for mapfile; run inside the dev" \
    "shell. bash on PATH: $(command -v bash || echo none)" >&2
  exit 1
fi

unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE
root="$(cd "$(mktemp -d "${TEST_TMPDIR:-${TMPDIR:-/tmp}}/markdown-all.XXXXXX")" && pwd -P)"
cleanup() {
  rm -rf "${root}"
}
trap cleanup EXIT

# A scratch HOME and XDG_CONFIG_HOME keep a user's global excludes file out of the listing.
export HOME="${root}/home"
export XDG_CONFIG_HOME="${root}/home/.config"
mkdir -p "${XDG_CONFIG_HOME}"
export GIT_CONFIG_NOSYSTEM=1
export GIT_CONFIG_GLOBAL=/dev/null
export GIT_AUTHOR_NAME="markdown all test"
export GIT_AUTHOR_EMAIL="markdown-all-test@example.invalid"
export GIT_COMMITTER_NAME="${GIT_AUTHOR_NAME}"
export GIT_COMMITTER_EMAIL="${GIT_AUTHOR_EMAIL}"

repo="${root}/repo"
put() {
  mkdir -p "$(dirname "${repo}/$1")"
  printf '# %s\n' "$1" >"${repo}/$1"
}

mkdir -p "${repo}/tools/lint"
cp "${script}" "${repo}/tools/lint/markdown_all.sh"
cat >"${repo}/tools/lint/markdown_check.sh" <<'EOF'
#!/bin/sh
for f in "$@"; do
  printf '%s\n' "$f"
done
EOF
chmod +x "${repo}/tools/lint/markdown_all.sh" "${repo}/tools/lint/markdown_check.sh"

committed="README.md notes/a.md CLAUDE.md
  .claude/skills/regina/SKILL.md .claude/skills/regina/agents/coder.md
  .claude/skills/regina/flow.yaml .claude/agents/coder.md .claude/agents/gone.md
  .claude/commands/orchestrate.md .claude/skills/other/SKILL.md
  tools/lint/testdata/x.md tools/doc-lint/tests/fixtures/x.md"
for f in ${committed}; do
  put "${f}"
done
printf '/.claude/agents/ignored.md\n' >"${repo}/.gitignore"
git init -q -b main "${repo}"
git -C "${repo}" add -A
git -C "${repo}" commit -q -m fixture
# Tracked but gone from the worktree: git ls-files --cached still lists it.
rm "${repo}/.claude/agents/gone.md"
for f in .claude/skills/regina/new.md .claude/current-plan.md .claude/agents/ignored.md; do
  put "${f}"
done
ignored="$(git -C "${repo}" ls-files --others --exclude-standard -- .claude/agents/ignored.md)"
if [[ -n "${ignored}" ]]; then
  echo "FAIL: fixture: .gitignore does not ignore .claude/agents/ignored.md, so its absence" \
    "below would prove nothing" >&2
  exit 1
fi

out="${root}/stdout.txt"
err="${root}/stderr.txt"
got="${root}/got.txt"
want="${root}/want.txt"
status=0
# From the scratch dir's parent, not the repository root: the script must cd to its own root.
(cd "${root}/.." && bash "${repo}/tools/lint/markdown_all.sh") >"${out}" 2>"${err}" || status=$?

failures=0
fail() {
  echo "FAIL: $*" >&2
  failures=$((failures + 1))
}

if [[ "${status}" -ne 0 ]]; then
  fail "markdown_all.sh exited ${status}, want 0: the stub checker exits 0, so the script itself" \
    "failed before or at its exec"
fi

LC_ALL=C sort "${out}" >"${got}"
LC_ALL=C sort >"${want}" <<'EOF'
.claude/agents/coder.md
.claude/skills/regina/SKILL.md
.claude/skills/regina/agents/coder.md
.claude/skills/regina/new.md
README.md
notes/a.md
EOF

# comm compares multisets, so a path listed twice shows up as an extra copy.
while IFS= read -r path; do
  fail "missing ${path}: markdown_all.sh does not hand it to the checker, so a finding in it" \
    "would pass the Linux gate"
done < <(LC_ALL=C comm -23 "${want}" "${got}")
while IFS= read -r path; do
  case "${path}" in
    .claude/agents/gone.md) why="a tracked file deleted from the worktree was not filtered out" ;;
    .claude/agents/ignored.md) why="a gitignored untracked file was listed" ;;
    .claude/*) why="the rest of .claude/ is scratch and must stay out" ;;
    CLAUDE.md) why="CLAUDE.md is excluded" ;;
    tools/*) why="test fixtures are excluded" ;;
    *.yaml) why="a file that is not Markdown was listed" ;;
    *) why="the listing reached a file outside its scope, or listed one twice" ;;
  esac
  fail "extra ${path}: ${why}"
done < <(LC_ALL=C comm -13 "${want}" "${got}")

if [[ "${failures}" -gt 0 ]]; then
  echo "---- stdout of markdown_all.sh ----" >&2
  cat "${out}" >&2
  echo "---- stderr of markdown_all.sh ----" >&2
  cat "${err}" >&2
  echo "----" >&2
  echo "markdown_all_test: ${failures} failure(s)" >&2
  exit 1
fi
echo "markdown_all_test: markdown_all.sh listed exactly the $(wc -l <"${want}" | tr -d ' ') files"
