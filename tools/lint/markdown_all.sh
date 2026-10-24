#!/usr/bin/env bash
# The Linux scope of markdown_check.sh: every .md on disk that git knows of, tracked or not,
# except CLAUDE.md, .claude/ and test fixtures. Needs host git. See tools/lint/README.md#files
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
cd "${here}/../.."
mapfile -t listed < <(git ls-files --cached --others --exclude-standard -- "*.md" \
  ":!:CLAUDE.md" ":!:.claude/*" ":!:tools/lint/testdata/*" ":!:tools/doc-lint/tests/fixtures/*")
files=()
for f in "${listed[@]}"; do
  if [[ -e "${f}" ]]; then
    files+=("${f}")
  fi
done
exec "${here}/markdown_check.sh" "${files[@]}"
