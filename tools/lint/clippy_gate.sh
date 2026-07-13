#!/usr/bin/env bash
# Trivial gate body for the `:clippy` sh_test. The real work is done by Bazel
# BUILDING the `rust_clippy` targets listed in this test's `data` attribute:
# staging them as runfiles forces clippy to run, and a clippy failure fails the
# build (hence the test). If we get here, all clippy data deps built cleanly.
set -euo pipefail
exit 0
