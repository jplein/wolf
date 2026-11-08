#!/usr/bin/env bash
# Matches a dependency listing, file "$1" or stdin, against wolf_core_forbidden_deps.txt beside it.
# Exits 0 when clean, 1 on any forbidden match, 2 on an error; must run under /bin/bash 3.2.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
list="${here}/wolf_core_forbidden_deps.txt"

if [[ "$#" -gt 1 ]]; then
  echo "ERROR: usage: wolf_core_deps_check.sh [LISTING]" >&2
  exit 2
fi

if [[ "$#" -eq 1 ]]; then
  if [[ ! -f "$1" || ! -r "$1" ]]; then
    echo "ERROR: cannot read the listing $1" >&2
    exit 2
  fi
  listing="$(cat -- "$1")"
else
  listing="$(cat)"
fi

lines="$(grep -c '[^[:space:]]' <<<"${listing}" || true)"
if [[ "${lines}" -eq 0 ]]; then
  echo "ERROR: the dependency listing is empty; did its producer fail?" >&2
  exit 2
fi

if [[ ! -f "${list}" || ! -r "${list}" ]]; then
  echo "ERROR: cannot read the forbidden list ${list}" >&2
  exit 2
fi

# An empty pattern would match every listing line, so blank lines are skipped with the comments.
patterns=()
count=0
while IFS= read -r line || [[ -n "${line}" ]]; do
  line="${line#"${line%%[![:space:]]*}"}"
  line="${line%"${line##*[![:space:]]}"}"
  if [[ -z "${line}" || "${line}" == \#* ]]; then
    continue
  fi
  patterns+=("${line}")
  count=$((count + 1))
done <"${list}"

if [[ "${count}" -eq 0 ]]; then
  echo "ERROR: the forbidden list ${list} holds no pattern" >&2
  exit 2
fi

matches=0
for pattern in "${patterns[@]}"; do
  rc=0
  hits="$(grep -i -F -e "${pattern}" <<<"${listing}")" || rc=$?
  if [[ "${rc}" -gt 1 ]]; then
    echo "ERROR: grep failed on pattern ${pattern}" >&2
    exit 2
  fi
  if [[ "${rc}" -eq 0 ]]; then
    while IFS= read -r hit; do
      echo "FAIL: ${hit} matches forbidden pattern ${pattern}"
      matches=$((matches + 1))
    done <<<"${hits}"
  fi
done

if [[ "${matches}" -gt 0 ]]; then
  echo "wolf_core_deps_check: ${matches} forbidden match(es)"
  exit 1
fi
echo "wolf_core_deps_check: ${lines} line(s) clean against ${count} pattern(s)"
