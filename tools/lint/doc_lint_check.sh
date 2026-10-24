#!/usr/bin/env bash
# The `:comments` test body: $1 is the doc-lint binary, the rest are the files to check.
exec "$1" --files "${@:2}"
