#!/usr/bin/env bash
# Fails on a wolf-gnome source line naming macOS, Swift, AppKit or an NS type outside the allowlist.
# Usage: [ROOT [ALLOWLIST]]; exits 0 clean, 1 on findings, 2 on an error; runs under /bin/bash 3.2.
set -euo pipefail
export LC_ALL=C

here="$(cd "$(dirname "$0")" && pwd)"

if [[ "$#" -gt 2 ]]; then
  echo "ERROR: usage: wolf_gnome_wording_check.sh [ROOT [ALLOWLIST]]" >&2
  exit 2
fi
root="${1:-${here}/../..}"
allowlist="${2:-${here}/wolf_gnome_wording_allowlist.txt}"
src="crates/wolf-gnome/src"

if [[ ! -d "${root}/${src}" ]]; then
  echo "ERROR: no ${src} directory under ${root}" >&2
  exit 2
fi
if [[ ! -f "${allowlist}" || ! -r "${allowlist}" ]]; then
  echo "ERROR: cannot read the allowlist ${allowlist}" >&2
  exit 2
fi

# Runfiles are symlinks, so -L: a plain -type f would list nothing under Bazel.
rc=0
files="$(cd "${root}" && find -L "${src}" -type f -name '*.rs' | sort)" || rc=$?
if [[ "${rc}" -ne 0 ]]; then
  echo "ERROR: cannot list ${root}/${src}" >&2
  exit 2
fi
if [[ -z "${files}" ]]; then
  echo "ERROR: no *.rs file under ${root}/${src}; is the Bazel data wiring broken?" >&2
  exit 2
fi

entries=""
count=0
while IFS= read -r line || [[ -n "${line}" ]]; do
  line="${line#"${line%%[![:space:]]*}"}"
  line="${line%"${line##*[![:space:]]}"}"
  if [[ -z "${line}" || "${line}" == \#* ]]; then
    continue
  fi
  entries="${entries}${line}"$'\n'
  count=$((count + 1))
done <"${allowlist}"

# grep_lines PATTERN-ARGS... FILE: prints matching lines as N:text, exits 2 on a grep error.
grep_lines() {
  local out rc=0
  out="$(grep -n "$@")" || rc=$?
  if [[ "${rc}" -gt 1 ]]; then
    echo "ERROR: grep failed on ${*: -1}" >&2
    exit 2
  fi
  if [[ -n "${out}" ]]; then
    printf '%s\n' "${out}"
  fi
}

keys=""
findings=0
nfiles=0
while IFS= read -r path; do
  nfiles=$((nfiles + 1))
  file="${root}/${path}"
  word="$(grep_lines -i -E -e 'macos|swift|appkit' -- "${file}")"
  ns="$(grep_lines -E -e '(^|[^[:alnum:]_])NS[A-Z]' -- "${file}")"
  hits="$(printf '%s\n%s\n' "${word}" "${ns}" | grep -v '^$' | sort -t: -k1,1n -u || true)"
  if [[ -z "${hits}" ]]; then
    continue
  fi
  while IFS= read -r hit; do
    lineno="${hit%%:*}"
    text="${hit#*:}"
    text="${text#"${text%%[![:space:]]*}"}"
    text="${text%"${text##*[![:space:]]}"}"
    key="${path}:${text}"
    keys="${keys}${key}"$'\n'
    if ! grep -qxF -e "${key}" <<<"${entries}"; then
      echo "FAIL: ${path}:${lineno}: ${text}"
      findings=$((findings + 1))
    fi
  done <<<"${hits}"
done <<<"${files}"

if [[ -n "${entries}" ]]; then
  while IFS= read -r entry; do
    if [[ -n "${entry}" ]] && ! grep -qxF -e "${entry}" <<<"${keys}"; then
      echo "FAIL: stale allowlist entry: ${entry}"
      findings=$((findings + 1))
    fi
  done <<<"${entries}"
fi

if [[ "${findings}" -gt 0 ]]; then
  echo "wolf_gnome_wording_check: ${findings} finding(s)"
  exit 1
fi
echo "wolf_gnome_wording_check: ${nfiles} file(s) clean, ${count} allowlisted line(s)"
