# bazel/nerd-fonts-symbols

Hermetic, pinned **Symbols Nerd Font Mono** icon face for bundling into `Wolf.app`.

This package downloads the Nerd Fonts project's symbols-only release asset (sha256-pinned, fetched by Bazel — no system or `PATH` dependency), assembles its Mono face and the release's two notices into a `fonts/` directory artifact, and exposes it for bundling. The face carries the Private Use Area icons that shell prompts, `ls` replacements and Neovim plugins print, and no text, so it is never a configured family.

`//crates/wolf-macos:Wolf` bundles the result at `Contents/Resources/fonts/`, where `FontRegistration.swift` registers it with CoreText; [the bundle inventory](../../crates/wolf-macos/README.md#bundle-contents) owns that shared directory. On Linux, `flake.nix`'s `wolfNerdFontsSymbols` fetches the same archive at the same pin, the package installs the face under `share/fonts/truetype/`, and the devShell's [test fontconfig](../../crates/wolf-gnome/tests/README.md#testdatafontsconf) exposes it. On macOS, `CellMetrics.swift` cascades the face behind every grid face ([why](../../notes/wolf-macos-decisions.md#nerd-font-icons)); the GNOME painters reach it only through fontconfig's fallback.

## The pin

- Version: `3.5.1`
- Archive: https://github.com/ryanoasis/nerd-fonts/releases/download/v3.5.1/NerdFontsSymbolsOnly.tar.xz
- sha256: `01172f37db8543edb102e5cb5c64101c9f4686630804d49b419aa07b23a69996`, which matches upstream's `SHA-256.txt` release asset
- Archive layout: **flat, with no top directory.** It holds `LICENSE`, `README.md`, `SymbolsNerdFont-Regular.ttf`, `SymbolsNerdFontMono-Regular.ttf` and `10-nerd-font-symbols.conf`.

v3.5.1 is the newest stable release: v3.5.0 plus a bugfix release that adds the Devicons v3.5.0 promised and left out. The `.tar.xz` is pinned rather than the `.zip` upstream offers beside it because it is the smaller download and Nix's stdenv unpacks it without `unzip`.

## Why the Mono face

Wolf's painters draw one glyph per cell, with no per-glyph scaling. Measured on the v3.5.1 files, in a 2048-unit em:

- Both faces map the same 10,624 code points, so the Mono face lacks nothing the proportional face has.
- Every Mono glyph advances 2048 units, one em. The proportional face has 852 distinct advances, up to 2729 units.
- 850 Mono outlines reach outside their advance on one side or the other, 162 of them by more than 20 units. The widest, 123 units, are the Powerline separators from U+E0B0, which bleed on purpose so adjacent separators join.

So one scale would fit the whole Mono face to a cell, since its cell is a square em, where the proportional face would need a scale per glyph. The macOS painter applies no scale at all, a [present limitation](../../notes/wolf-macos-decisions.md#present-limitations).

The overlay leaves out the proportional face and the upstream `10-nerd-font-symbols.conf`. With both faces present, fallback could pick the wider proportional face for the same code points, and the `.conf`'s aliases prefer the proportional family for every monospace family. `flake.nix` installs the Mono face alone for the same reasons.

## Licence

The release ships two notices: `LICENSE`, the project's MIT licence, and `README.md`, whose "Icon sets" table names each bundled glyph set's upstream, version and licence — CC BY 4.0, Apache 2.0, OFL 1.1, MIT, and one set marked unlicensed. Both packages ship both files whole: `Contents/Resources/fonts/nerd-fonts-symbols-LICENSE` and `nerd-fonts-symbols-README.md` in the bundle, and the same two names under `share/licenses/` in the Nix package.

The text the About surfaces show is `crates/wolf-core/src/licenses/nerd-fonts-symbols-LICENSE.txt`, which [`licenses.rs`](../../crates/wolf-core/src/README.md#licensesrs) compiles in.

## Targets

- **`//bazel/nerd-fonts-symbols:nerd_fonts_symbols_tree`** — an `alias` to `@nerd_fonts_symbols//:nerd_fonts_symbols_tree`, with no `select()`: the face is arch-independent data. The actual target is a `copy_to_directory` TreeArtifact holding `fonts/SymbolsNerdFontMono-Regular.ttf`, `fonts/nerd-fonts-symbols-LICENSE` and `fonts/nerd-fonts-symbols-README.md`. This is the value bundled into the app.
- **`//bazel/nerd-fonts-symbols:nerd_fonts_symbols_tree_test`** — an `sh_test` over `:nerd_fonts_symbols_tree`: asserts the three files are non-empty inside the tree's inner `fonts/` dir, and that neither the proportional face nor the `.conf` is there.

## How it works

`extensions.bzl` is a bzlmod module extension that declares a single `http_archive` repo (`nerd_fonts_symbols`), with `_URL` built from `_VERSION`. The archive is flat, so there is no `strip_prefix`. Its overlay `BUILD` file lists the three files explicitly in a `runtime_files` filegroup and runs `@aspect_bazel_lib//lib:copy_to_directory`, whose `replace_prefixes` nests them under an inner `fonts/` dir:

```starlark
replace_prefixes = {
    "SymbolsNerdFontMono-Regular.ttf": "fonts/SymbolsNerdFontMono-Regular.ttf",
    "LICENSE": "fonts/nerd-fonts-symbols-LICENSE",
    "README.md": "fonts/nerd-fonts-symbols-README.md",
}
```

The inner `fonts/` is the one every font tree uses: `rules_apple` strips a bundled TreeArtifact's own directory name, so the files land in `Contents/Resources/fonts/` beside the editor faces. The notices are renamed because that directory is shared, and a bare `LICENSE` or `README.md` names no font.

`MODULE.bazel` wires the extension up:

```starlark
nerd_fonts_symbols = use_extension("//bazel/nerd-fonts-symbols:extensions.bzl", "nerd_fonts_symbols")
use_repo(nerd_fonts_symbols, "nerd_fonts_symbols")
```

## Bumping the pin

**This asset has one pin and two consumers.** `flake.nix`'s `wolfNerdFontsSymbols` fetches the same archive at the same sha256, and `crates/wolf-core/src/licenses/nerd-fonts-symbols-LICENSE.txt` is derived from it. All three move together. [`crates/wolf-gnome/tests/vendored_pins.rs`](../../crates/wolf-gnome/tests/README.md#vendored_pinsrs) fails under `cargo test` on Linux when the version or sha256 differs between `extensions.bzl` and `flake.nix`, and it reads each from a line of its own, so keep `_VERSION = "…"` and `_SHA256 = "…"` the only lines beginning with those names. Nothing checks the committed licence copy.

1. Update `_VERSION` in `extensions.bzl`; `_URL` follows it.
2. Recompute the checksum: `curl -L https://github.com/ryanoasis/nerd-fonts/releases/download/v<version>/NerdFontsSymbolsOnly.tar.xz -o f.tar.xz && shasum -a 256 f.tar.xz`, and check it against the release's `SHA-256.txt`. Update `_SHA256`.
3. Confirm the flat layout and the three file names with `tar -tJf f.tar.xz`. If one moved, update the `runtime_files` filegroup and the `replace_prefixes` map.
4. Set the same `version` and `sha256` on the `nerd-fonts-symbols` entry of `flake.nix`'s `wolfVendoredPins` in the same change, and re-enter the devShell so the fontconfig template is regenerated.
5. Regenerate the committed licence text with the commands below, from the repository root, and commit it if it moved. The row's `provenance` in `licenses.rs` names the version and the archive sha256, so it moves too.
6. Re-check the measurements under [Why the Mono face](#why-the-mono-face) against the new faces.
7. Regenerate the lockfile (a plain build does this with `--lockfile_mode=update`) and commit `MODULE.bazel.lock`.
8. Re-run `bazel test //bazel/nerd-fonts-symbols:nerd_fonts_symbols_tree_test`.

```sh
D=$(mktemp -d "${TMPDIR:-/tmp}/nf.XXXXXX")
curl -fsSL -o "$D/a.tar.xz" https://github.com/ryanoasis/nerd-fonts/releases/download/v<version>/NerdFontsSymbolsOnly.tar.xz
tar -xJf "$D/a.tar.xz" -C "$D" LICENSE README.md
{ cat "$D/LICENSE"; printf '\n'; awk '/^## Icon sets$/{p=1} /^## Which font/{exit} p' "$D/README.md" | sed '$d'; } > crates/wolf-core/src/licenses/nerd-fonts-symbols-LICENSE.txt
```
