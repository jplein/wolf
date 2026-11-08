# Sourced, never run, by swift_format_check.sh and swift_format_fix.sh. It must run under macOS
# /bin/bash 3.2, which Bazel's test PATH gives: no mapfile, no associative arrays.

# Bazel tests run with a scrubbed environment, so try xcrun, then xcode-select, then the
# canonical Xcode path.
find_swift_format() {
  local sf
  sf="$(/usr/bin/xcrun --find swift-format 2>/dev/null || true)"
  if [[ -n "${sf}" && -x "${sf}" ]]; then echo "${sf}"; return 0; fi

  local dev
  dev="$(/usr/bin/xcode-select -p 2>/dev/null || true)"
  if [[ -n "${dev}" ]]; then
    sf="${dev}/Toolchains/XcodeDefault.xctoolchain/usr/bin/swift-format"
    if [[ -x "${sf}" ]]; then echo "${sf}"; return 0; fi
  fi

  sf="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin/swift-format"
  if [[ -x "${sf}" ]]; then echo "${sf}"; return 0; fi

  return 1
}
