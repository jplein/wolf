{ pkgs }:
# A plain Nix comment that a comment-only edit may rewrite.
pkgs.writeShellScript "x" ''
  # an edited embedded shell comment
  echo hi
''
