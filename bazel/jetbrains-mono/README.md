# bazel/jetbrains-mono

Hermetic, pinned **JetBrains Mono** font for bundling into `Wolf.app`.

This package downloads the pinned GitHub release-asset zip for JetBrains Mono (sha256-pinned, fetched by Bazel — no system or PATH dependency), assembles the four static desktop TTF faces (Regular / Italic / Bold / BoldItalic) plus the OFL license into a structure-preserving directory artifact, and exposes it for bundling. `//crates/wolf-macos:Wolf` bundles the result at `Contents/Resources/fonts/`, where `FontRegistration.swift` registers the TTFs with CoreText (`CTFontManagerRegisterFontURLs`) so `"JetBrains Mono"` resolves, and `CellMetrics.swift` measures it as the config-driven default font.

## The pin

- Version: `2.304`
- Zip: https://github.com/JetBrains/JetBrainsMono/releases/download/v2.304/JetBrainsMono-2.304.zip
- sha256: `6f6376c6ed2960ea8a963cd7387ec9d76e3f629125bc33d1fdcd7eb7012f7bbf`
- Archive layout: **FLAT** — the zip has NO wrapping top directory. Its contents are `fonts/ttf/JetBrainsMono-<Face>.ttf`, `fonts/webfonts/...`, `fonts/variable/...`, `OFL.txt`, `AUTHORS.txt`. (This differs from `//bazel/orlando`, whose GitHub _source_ tarball is wrapped in an `orlando-<commit>/` top dir.)

## Targets

- **`//bazel/jetbrains-mono:jetbrains_mono_tree`** — an `alias` to `@jetbrains_mono//:jetbrains_mono_tree`. Like `//bazel/orlando` and unlike `//bazel/nvim` there is **no** `select()`: JetBrains Mono is DATA (arch-independent TTFs + a text license), so there is a single repo. The actual target is a `copy_to_directory` TreeArtifact whose contents are `fonts/JetBrainsMono-Regular.ttf`, `fonts/JetBrainsMono-Italic.ttf`, `fonts/JetBrainsMono-Bold.ttf`, `fonts/JetBrainsMono-BoldItalic.ttf`, and `fonts/OFL.txt`. This is the value bundled into the app.
- **`//bazel/jetbrains-mono:jetbrains_mono_tree_test`** — an `sh_test` over `:jetbrains_mono_tree`: asserts the four TTF faces landed non-empty and `OFL.txt` is present at the expected paths inside the tree's inner `fonts/` dir.

## How it works

`extensions.bzl` is a bzlmod module extension that declares a single `http_archive` repo (`jetbrains_mono`) for the pinned release-asset zip — no `select()`, pure data. It gets an overlay `BUILD` file (`build_file_content`) that:

1. globs EXACTLY the four static desktop TTF faces (under `fonts/ttf/`) plus the root `OFL.txt` (deliberately excluding the webfonts, the variable fonts, the "NL" no-ligature faces, the other weights, and `AUTHORS.txt`), and
2. runs `@aspect_bazel_lib//lib:copy_to_directory` to assemble them into a single TreeArtifact, **remapping** the archive layout into an inner `fonts/` dir via `replace_prefixes` (NOT `http_archive`'s `strip_prefix`):

   ```starlark
   replace_prefixes = {
       "fonts/ttf": "fonts",
       "OFL.txt": "fonts/OFL.txt",
   }
   ```

   This flattens the `ttf/` subdir away (so the four TTFs land directly under `fonts/`) and moves the license to `fonts/OFL.txt`.

The inner `fonts/` directory is deliberate: `rules_apple` strips the bundled TreeArtifact's _own_ top-level directory name when copying it into `Contents/Resources/`. By nesting everything under `fonts/`, the app reliably gets `Contents/Resources/fonts/...` (where `FontRegistration` looks and the bundle-layout test expects them), robustly even if a future release adds a new top-level entry.

`aspect_bazel_lib` is a build-time-only ruleset (already used by `//bazel/nvim`, `//bazel/orlando`, and `//bazel/lighthouse`); it never enters `wolf-core`'s dependency graph. `MODULE.bazel` wires the extension up:

```starlark
jetbrains_mono = use_extension("//bazel/jetbrains-mono:extensions.bzl", "jetbrains_mono")
use_repo(jetbrains_mono, "jetbrains_mono")
```

## Bumping the pinned version

1. Pick the new version and update `_VERSION` in `extensions.bzl`. The `_URL` (and the archive's inner filenames) auto-follow.
2. Recompute the checksum: `curl -L https://github.com/JetBrains/JetBrainsMono/releases/download/v<version>/JetBrainsMono-<version>.zip -o f.zip && shasum -a 256 f.zip`. Update `_SHA256`.
3. Confirm the TTF paths in the new archive with `unzip -l f.zip` (verify the flat layout and the exact `fonts/ttf/JetBrainsMono-<Face>.ttf` filenames — a wrong path makes the glob match nothing). If a face is renamed or relocated, update the `runtime_files` filegroup and/or the `replace_prefixes` map accordingly.
4. Regenerate the lockfile (a plain build does this with `--lockfile_mode=update`) and commit `MODULE.bazel.lock`.
5. Re-run `bazel test //bazel/jetbrains-mono:jetbrains_mono_tree_test`.
