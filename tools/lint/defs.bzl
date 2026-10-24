"""The per-package `comment_srcs` filegroup that feeds `//tools/lint:comments`."""

# Must match doc-lint's language table; root-only names are listed in the root BUILD.bazel.
COMMENT_LINT_PATTERNS = [
    "**/*.rs",
    "**/*.swift",
    "**/*.c",
    "**/*.h",
    "**/*.lua",
    "**/*.sh",
    "**/*.sh.tpl",
    "**/*.bzl",
    "**/BUILD.bazel",
    "**/BUILD",
    "**/*.nix",
    "**/*.toml",
    "**/*.css",
]

def comment_srcs(name = "comment_srcs", include = COMMENT_LINT_PATTERNS, exclude = None):
    """Declares the package's source files whose comments `//tools/lint:comments` checks.

    Args:
        name: the filegroup name; `//tools/lint` lists `//<package>:comment_srcs`.
        include: glob patterns, by default every file type doc-lint lexes.
        exclude: glob patterns to leave out, such as test fixtures.
    """
    native.filegroup(
        name = name,
        srcs = native.glob(include, exclude = exclude or [], allow_empty = True),
        visibility = ["//tools/lint:__pkg__"],
    )
