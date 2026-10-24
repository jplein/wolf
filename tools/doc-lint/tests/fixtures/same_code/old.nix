{ pkgs }:
# A plain Nix comment that a comment-only edit may rewrite.
pkgs.writeShellScript "x" ''
  # an embedded shell comment, which is string content to Nix
  echo hi
''
