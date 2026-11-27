#!/usr/bin/env bash
# The Linux scope of markdown_check.sh: every .md git knows of, but CLAUDE.md, fixtures and .claude/
# outside the regina skill and .claude/agents/. Needs host git. See tools/lint/README.md#files
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
cd "${here}/../.."
mapfile -t listed < <(git ls-files --cached --others --exclude-standard -- "*.md" \
  ":!:CLAUDE.md" ":!:.claude/*" ":!:tools/lint/testdata/*" ":!:tools/doc-lint/tests/fixtures/*")
mapfile -t -O "${#listed[@]}" listed < <(git ls-files --cached --others --exclude-standard -- \
  ".claude/skills/regina/*.md" ".claude/agents/*.md")
files=()
for f in "${listed[@]}"; do
  if [[ -e "${f}" ]]; then
    files+=("${f}")
  fi
done
exec "${here}/markdown_check.sh" "${files[@]}"
