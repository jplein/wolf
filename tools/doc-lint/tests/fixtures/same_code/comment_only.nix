{ pkgs }:
# A rewritten Nix comment.
pkgs.writeShellScript "x" ''
  # an embedded shell comment, which is string content to Nix
  echo hi
''
