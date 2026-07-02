# bazel/ia-writer-mono

Hermetic, pinned **iA Writer Mono S** font for bundling into `Wolf.app`.

This package downloads the pinned GitHub _source_ tarball for the iA Writer fonts (sha256-pinned, fetched by Bazel — no system or PATH dependency), assembles the four static iA Writer Mono S desktop TTF faces (Regular / Italic / Bold / BoldItalic) plus the per-font-dir SIL OFL 1.1 license into a structure-preserving directory artifact, and exposes it for bundling. `//crates/wolf-macos:Wolf` bundles the result at `Contents/Resources/fonts/`, where `FontRegistration.swift` registers the TTFs with CoreText (`CTFontManagerRegisterFontURLs`) so `"iA Writer Mono S"` resolves, and `CellMetrics.swift` measures it as the config-driven **default** font-family. iA Writer Mono S **shares** `Contents/Resources/fonts/` with JetBrains Mono: both `//bazel/jetbrains-mono:jetbrains_mono_tree` and `//bazel/ia-writer-mono:ia_writer_mono_tree` are in the Wolf `resources` list and MERGE into the one `fonts/` directory (their file sets are disjoint — `JetBrainsMono-*.ttf` + `OFL.txt` vs `iAWriterMonoS-*.ttf` + `LICENSE.md`, no path collision).

## The pin

- Commit: `f32c04c3058a75d7ce28919ce70fe8800817491b`
- Tarball: https://github.com/iaolo/iA-Fonts/archive/f32c04c3058a75d7ce28919ce70fe8800817491b.tar.gz
- sha256: `826767eaea4ce43ccb7ca71327a5c63ac31206b99df400699e6744df6104e1b5`
- Archive root dir: `iA-Fonts-f32c04c3058a75d7ce28919ce70fe8800817491b` (used as `http_archive`'s `strip_prefix`, so paths resolve against the archive root). The `iaolo/iA-Fonts` repo publishes **no release assets**, so — like `//bazel/orlando` and unlike `//bazel/jetbrains-mono`'s flat release zip — this pins a **COMMIT source tarball** wrapped in an `iA-Fonts-<commit>/` top dir.

## The spaced source dir

The four static faces live under **`iA Writer Mono/Static/`** — note the **SPACES** in the `iA Writer Mono` directory name — and the license is PER-FONT-DIR at **`iA Writer Mono/LICENSE.md`** (SIL OFL 1.1, named `LICENSE.md`, NOT `OFL.txt`). To de-risk the spaces, the `runtime_files` filegroup lists the five source files **EXPLICITLY** (not a glob): the four `iA Writer Mono/Static/iAWriterMonoS-{Regular,Italic,Bold,BoldItalic}.ttf` faces + `iA Writer Mono/LICENSE.md`. The Duo/Quattro families, the variable font, and any dev/README files are deliberately excluded.

## Targets

- **`//bazel/ia-writer-mono:ia_writer_mono_tree`** — an `alias` to `@ia_writer_mono//:ia_writer_mono_tree`. Like `//bazel/jetbrains-mono` and `//bazel/orlando` and unlike `//bazel/nvim` there is **no** `select()`: iA Writer Mono S is DATA (arch-independent TTFs + a text license), so there is a single repo. The actual target is a `copy_to_directory` TreeArtifact whose contents are `fonts/iAWriterMonoS-Regular.ttf`, `fonts/iAWriterMonoS-Italic.ttf`, `fonts/iAWriterMonoS-Bold.ttf`, `fonts/iAWriterMonoS-BoldItalic.ttf`, and `fonts/LICENSE.md`. This is the value bundled into the app.
- **`//bazel/ia-writer-mono:ia_writer_mono_tree_test`** — an `sh_test` over `:ia_writer_mono_tree`: asserts the four TTF faces landed non-empty and `LICENSE.md` is present at the expected paths inside the tree's inner `fonts/` dir.

## How it works

`extensions.bzl` is a bzlmod module extension that declares a single `http_archive` repo (`ia_writer_mono`) for the pinned commit source tarball — no `select()`, pure data. It gets an overlay `BUILD` file (`build_file_content`) that:

1. lists EXACTLY the four static desktop TTF faces (under `iA Writer Mono/Static/`) plus `iA Writer Mono/LICENSE.md`, explicitly (spaces and all), and
2. runs `@aspect_bazel_lib//lib:copy_to_directory` to assemble them into a single TreeArtifact, **remapping** the spaced source path into an inner `fonts/` dir via `replace_prefixes` (NOT `http_archive`'s `strip_prefix`):

   ```starlark
   replace_prefixes = {
       "iA Writer Mono/Static": "fonts",
       "iA Writer Mono/LICENSE.md": "fonts/LICENSE.md",
   }
   ```

   This flattens the spaced `iA Writer Mono/Static/` dir away (so the four TTFs land directly under `fonts/`) and moves the license to `fonts/LICENSE.md`.

The inner `fonts/` directory is deliberate — and is the SAME inner dir JetBrains Mono uses: `rules_apple` strips the bundled TreeArtifact's _own_ top-level directory name when copying it into `Contents/Resources/`. By nesting everything under `fonts/`, the app reliably gets `Contents/Resources/fonts/...` (where `FontRegistration` looks and the bundle-layout test expects them), and because both font trees use `fonts/` they merge into the one bundled directory.

`aspect_bazel_lib` is a build-time-only ruleset (already used by `//bazel/nvim`, `//bazel/orlando`, `//bazel/lighthouse`, and `//bazel/jetbrains-mono`); it never enters `wolf-core`'s dependency graph. `MODULE.bazel` wires the extension up:

```starlark
ia_writer_mono = use_extension("//bazel/ia-writer-mono:extensions.bzl", "ia_writer_mono")
use_repo(ia_writer_mono, "ia_writer_mono")
```

## Bumping the pin

1. Pick the new commit SHA and update `_COMMIT` in `extensions.bzl`. The archive root name (`iA-Fonts-<commit>`) auto-follows.
2. Recompute the checksum: `curl -L https://github.com/iaolo/iA-Fonts/archive/<commit>.tar.gz -o f.tgz && shasum -a 256 f.tgz`. Update `_SHA256`.
3. Confirm the (spaced) TTF paths in the new archive: `tar -tzf f.tgz | grep 'iA Writer Mono/Static'` (verify the exact `iA Writer Mono/Static/iAWriterMonoS-<Face>.ttf` filenames and `iA Writer Mono/LICENSE.md` — a wrong path makes the explicit `runtime_files` list fail to resolve). If a face is renamed or relocated, update the `runtime_files` filegroup and/or the `replace_prefixes` map accordingly.
4. Regenerate the lockfile (a plain build does this with `--lockfile_mode=update`) and commit `MODULE.bazel.lock`.
5. Re-run `bazel test //bazel/ia-writer-mono:ia_writer_mono_tree_test`.
