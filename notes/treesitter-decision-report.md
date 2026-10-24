# Treesitter Decision Report

Last verified at commit `220883e48cd87b6ca30e69bfb9641de7845e4340`.

This report pins how Wolf supplies Treesitter grammars (parsers) for syntax highlighting, both for Markdown itself and for the languages embedded in fenced code blocks. It records a decision; it is not a task list. Implementation is deferred until other in-flight changes land.

## Context

Wolf is a Markdown editor that embeds a pinned, hermetic Neovim. The captive editor is spawned `--embed --clean` with `NVIM_APPNAME` and the `XDG_*` directories pinned to a private temp dir, so it never reads or writes the user's Neovim config or state. The macOS app ships its own pinned Neovim inside `Wolf.app`, so double-clicking works with no shell environment and no separately-installed Neovim. On NixOS the Rust core and GTK front-end build under Nix, which also provides the pinned `nvim`.

To highlight fenced code blocks, Wolf needs Treesitter parsers for the embedded languages in addition to the two Markdown parsers (`markdown` and `markdown_inline`). The Markdown parser's injection queries are what route a fenced block's contents to the embedded language's parser, so both the parsers and their queries/injections must be present together.

At the time of this report, `runtime/nvim/init.lua` relied on built-in `syntax on` and explicitly deferred Treesitter; that deferral has since been lifted on macOS (see the follow-up below). This report decides the supply strategy that let the deferral be lifted cleanly.

## Decision

1. **Curated set, installed at build time by Nix.** Wolf bundles a curated set of Treesitter parsers built reproducibly by Nix at app-build time, not the full upstream grammar set and not a runtime-assembled set. The set is `markdown` and `markdown_inline` plus a curated list of the languages commonly fenced in Markdown. The curation list is the size dial: it is expanded by editing the list, rebuilding, and shipping.

2. **No external cache interaction.** Wolf does not read from or write to any external or shared parser location — not the user's `~/.local/share/nvim`, not a system parser directory, not a network mirror, and not a Wolf-private on-disk cache populated at runtime. All parsers come from the bundle.

3. **Neovim uses only the curated parsers.** The captive Neovim resolves parsers and their queries exclusively from the bundled, curated set on its `runtimepath`. It does not fall back to system-installed parsers or the user's parsers. This preserves the existing hermetic guarantee: the editor's behavior is fully determined by what Wolf ships.

4. **Avoid lazy install for now.** Wolf does not fetch, download, or compile parsers at runtime. A fenced block whose language is not in the curated set renders as plain text — the same graceful fallback as today — so an incomplete set is a missing nicety, never a correctness bug.

## Rationale

A build-time, bundled, curated set is the only approach that holds every invariant Wolf already enforces, on both targets.

- **Hermetic and deterministic.** Parsers ship with the app, so the captive editor's highlighting is fully reproducible. This matters directly for the render/highlight tests, which start a real Neovim and assert on the grid; a runtime-assembled set would inject network and version nondeterminism into exactly those tests.
- **macOS signing.** The bundled `nvim` loads each parser via `dlopen`. A parser baked into the bundle is signed as part of `Wolf.app`. Anything fetched at runtime is unsigned and fails Library Validation under the hardened runtime unless the app weakens its posture with `com.apple.security.cs.disable-library-validation`. Build-time bundling avoids that entirely.
- **No environment dependency.** "Works with no shell environment, double-click works" is a stated property of the macOS app. Lazy install would need network access plus prebuilt per-architecture binaries (a compiler cannot be assumed on an end-user Mac), reintroducing the dependency the spawn design removed.
- **Parsers and queries stay paired.** Nix's `nvim-treesitter` packaging provides parsers together with their highlight queries and injections at matched versions. Hand-rolled fetching would have to pair and version-match each `.so` with its queries by hand; the injection queries are also precisely the piece that lights up embedded code in fences.
- **Uniform across platforms.** On NixOS, Nix is already present, so bundling is trivial. On macOS, signing makes runtime install genuinely painful. Build-time Nix bundling is clean on both.

The full upstream grammar set was rejected because it is far larger than needed: only languages actually fenced in Markdown matter, and the graceful plain-text fallback means a curated subset carries no correctness cost. Reading the user's or the system's parsers was rejected because it re-couples Wolf to state it deliberately isolates, is incomplete (users install only the parsers for languages they personally edit), and risks ABI skew against the pinned Neovim.

## Consequences and follow-up (not part of this decision)

- The curated parser list must be defined and maintained; growing language coverage is a rebuild-and-ship operation, not a runtime one.
- Lifting the Treesitter deferral in `runtime/nvim/init.lua` — **done on macOS.** The captive `init.lua` now enables `vim.treesitter.start` for Markdown (a `FileType=markdown` autocmd, pcall-wrapped) and the Markdown injection queries are active, so fenced blocks route to the embedded-language parsers. The nvim-treesitter plugin is vendored from the **`main`** branch, whose `queries/markdown/injections.scm` is pure core-native capture-based injection; the original `master` pin was incompatible with pinned nvim 0.12.3 (its `#set-lang-from-info-string!` predicate targets a query-match API 0.12.3 removed, raising `E5108` on every fenced redraw).

  This is gated on the vendored `nvim-treesitter/` + `parsers/` siblings being present, so it is enabled in the macOS `.app` and a strict no-op on Linux/cargo.

- The Nix change will wrap the pinned Neovim so the curated parsers and their queries are on the captive editor's `runtimepath` — **still deferred** (the Linux/Nix packaging path for the vendored parsers + plugins), mirroring how orlando/lighthouse were vendored Bazel-only first.

The Linux/Nix packaging step above remains deferred to a later change; the macOS enablement has landed.

## Follow-up: implementation landed (macOS/Bazel — vendor, bundle, and enable)

This strategy has now been implemented **macOS/Bazel-only** — it ships the curated parsers and the treesitter plugins in `Wolf.app` **and enables** markdown highlighting + fenced-code injection. Specifics and deviations from the strategy above:

- **Host-compiled, not Nix-built.** On macOS the parsers are compiled from pinned grammar source with the apple_support / Xcode host cc toolchain (`//bazel/treesitter-parsers`), producing signable, relocatable `MH_DYLIB` Mach-O shared objects bundled at `Contents/Resources/parsers/parser/<lang>.so`. This deviates from the report's "built by Nix" wording but preserves every invariant it cares about (hermetic, signed-in-bundle, no runtime fetch/compile, ABI-matched to the pinned Neovim). A nix-built `.so` was explicitly avoided because its `/nix/store` install names are not relocatable inside a signed `.app`. The **Linux/Nix** vendoring path remains deferred (mirroring how orlando/lighthouse were vendored Bazel-only first).
- **Curated set.** html, javascript, typescript, tsx, css, json, go, rust, c, cpp, yaml, toml, bash, swift, kotlin, java. markdown / markdown_inline are excluded because pinned Neovim 0.12.3 already ships them. Revisions/repos come from the nvim-treesitter `lockfile.json` / `parsers.lua` as pinned during the original vendoring — when the plugin was on `master` (master-era) — and were **not** re-synced when the plugin later moved to `main`.

  Re-deriving the parser revisions from `main`'s `lockfile.json` for an exact queries⇄parsers version-match is a future nicety; the current pairing is proven working (`treesitter_highlight` + `treesitter_parser_abi` pass) and tree-sitter grammar ABIs are stable, so queries-from-`main` with parsers-from-master-era is fine. See [`bazel/treesitter-parsers/README.md`](../bazel/treesitter-parsers/README.md).

- **Plugins vendored on `main`.** The nvim-treesitter and nvim-treesitter-textobjects plugins are bundled at `Contents/Resources/nvim-treesitter/` and `…/nvim-treesitter-textobjects/` (`//bazel/nvim-treesitter`, `//bazel/nvim-treesitter-textobjects`), both pinned to the **`main`** branch. nvim-treesitter `main` ships the bundled queries under `runtime/queries/` (Wolf's overlay lifts them back to `queries/`) with **core-native, capture-based** markdown injections — the reason for the `master → main` reversal (master's `#set-lang-from-info-string!` predicate is incompatible with pinned nvim 0.12.3, raising `E5108`). textobjects tracks `main` for branch-consistency and stays bundled-but-inert.
- **Highlighting + injection enabled (macOS).** `runtime/nvim/init.lua` now self-locates the sibling `nvim-treesitter/` + `parsers/` trees and, when present, prepends them to `runtimepath` and runs a pcall-wrapped `vim.treesitter.start` on each markdown buffer (`FileType=markdown` autocmd + emit-once), so markdown highlighting and fenced-code injection are live. The block is strictly gated on the vendored siblings, so it is a no-op on Linux/cargo (where `syntax on` remains). textobjects is prepended but never sourced (no keymaps). Enabling textobjects keymaps and a Markdown LSP/formatter are still deferred.
- **ABI + live highlighting proven.** `//crates/wolf-ffi:treesitter_parser_abi` drives a real pinned Neovim 0.12.3, `add`s a compiled parser by path, and parses with it — proving the host-compiled `.so`s are ABI-compatible. `//crates/wolf-ffi:treesitter_highlight` drives a real pinned Neovim with the enablement active and asserts the markdown highlighter is on and an injected `go` langtree is present inside a fenced block — proving injection works end-to-end.
