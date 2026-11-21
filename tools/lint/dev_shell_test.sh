#!/usr/bin/env bash
# Checks that the flake's dev shell puts its own python3 on PATH, not the host's, under bash 3.2.
# Bazel hands it the client PATH through env_inherit; on Linux gates.sh runs it in the shell.
set -euo pipefail

py="$(command -v python3 || true)"
if [[ -z "${py}" ]]; then
  echo "FAIL: no python3 on PATH; the dev shell provides one, so this is not its PATH or it lacks" \
    "python3. PATH=${PATH}" >&2
  exit 1
fi

# The host's /usr/bin/python3 also runs on macOS: only the store prefix tells the shell's apart.
case "${py}" in
  /nix/store/*) ;;
  *)
    echo "FAIL: python3 on PATH is ${py}, not a /nix/store path; run inside the flake's dev" \
      "shell, which provides it" >&2
    exit 1
    ;;
esac

# -I keeps the working directory and the PYTHON* variables out of the interpreter's start.
if ! major="$("${py}" -I -c 'import sys; print(sys.version_info[0])' 2>&1)"; then
  echo "FAIL: ${py} -I -c exited non-zero, so the shell's python3 does not run: ${major}" >&2
  exit 1
fi
if [[ "${major}" != "3" ]]; then
  echo "FAIL: ${py} reports major version '${major}', not 3" >&2
  exit 1
fi

echo "dev_shell_test: python3 is ${py}"
