# Plan: Vendor treesitter plugins + curated compiled parsers into Wolf.app (macOS-only)

## Status: COMPLETE

### Technical-writer (done)
Fixed all 4 textobjects `main`→`master` doc/comment mismatches (build.md ×3, BUILD.bazel comment ×1). Grep confirms no other layout/branch stragglers. Orchestrator ran `prettier --write build.md` (clean) and `prettier --check` on all touched/new READMEs (all clean). Doc-only changes since last green test run; no re-test needed.

### Reviewer verdict: implementation APPROVED (functional). Doc-only fixes required (CLAUDE.md: doc/impl mismatch = bug):
- `build.md:162` (MEDIUM): bump procedure says textobjects `main` branch — must say `master` (would reintroduce the corrected incompatibility on next bump).
- `build.md:131` (low): says textobjects tree bundles `lua/nvim-treesitter-textobjects/**` — actual is `lua/nvim-treesitter/textobjects/**`.
- `build.md:190` (low): describes tree test asserting `lua/nvim-treesitter-textobjects/` — actual `lua/nvim-treesitter/textobjects/`.
- `bazel/nvim-treesitter-textobjects/BUILD.bazel:11-12` (low): stale comment `lua/nvim-treesitter-textobjects/**` → `lua/nvim-treesitter/textobjects/**`.

### Coder output (done)
- All 16 grammars compiled, NONE dropped: json, go, c, java, css, javascript, rust, bash, html, toml, cpp, typescript, tsx, yaml, kotlin, swift. All scanners are C at pinned revisions. Special cases: typescript/tsx (one repo, two cc_binaries, `common/**` staged), yaml (scanner textually includes `schema.core.c` via textual_hdrs cc_library), swift (pinned `with-generated-files` branch for committed parser.c), html (globs `src/*.h`). `.so`s confirmed Mach-O arm64. ABI test PASSED with plain `cc_binary(linkshared)` — NO linkopts needed.
- nvim-treesitter pinned master `cf12346a`. nvim-treesitter-textobjects RE-PINNED from main→master (`5ca4aaa6`) for API-generation compat with master nvim-treesitter; master layout is `lua/nvim-treesitter/textobjects/` (not `lua/nvim-treesitter-textobjects/`). Tree test + bundle_layout check (i) updated to master layout.
- MODULE.bazel.lock regenerated, stable under --lockfile_mode=error.
- wolf-core graph clean (no treesitter/parser repos). init.lua byte-for-byte unchanged.

### All tests PASS
`//bazel/nvim-treesitter:nvim_treesitter_tree_test`, `//bazel/nvim-treesitter-textobjects:nvim_treesitter_textobjects_tree_test`, `//bazel/treesitter-parsers:parsers_tree_test`, `//crates/wolf-macos:bundle_layout_test`, `//crates/wolf-ffi:treesitter_parser_abi`, `bazel build //crates/wolf-macos:Wolf`.

### Test-writer output (done)
Created: `bazel/nvim-treesitter/nvim_treesitter_tree_test.sh`, `bazel/nvim-treesitter-textobjects/nvim_treesitter_textobjects_tree_test.sh`, `bazel/treesitter-parsers/parsers_tree_test.sh`, `crates/wolf-ffi/tests/treesitter_parser_abi.rs`. Modified: `crates/wolf-ffi/tests/common/mod.rs` (added `parsers_dir_or_none()`).
Coder must: `chmod +x` the 3 `.sh` files (test-writer had no shell). Tests use bash shebang (`set -euo pipefail`). Sentinel written via `vim.fn.writefile` (matches orlando/lighthouse). Confirm query path names + inner-dir names match the `copy_to_directory` outputs the resolvers expect (`nvim-treesitter/`, `nvim-treesitter-textobjects/`, `parsers/parser/<lang>.so`).

## Problem statement

Vendor Neovim treesitter plugins and a curated set of compiled treesitter parsers
into the macOS `Wolf.app` bundle at build time. **macOS/Bazel ONLY** for this
change — no Nix/NixOS/flake.nix changes (Linux path deferred, mirroring how
orlando/lighthouse were vendored Bazel-only first).

**Scope: vendor + bundle + tests + docs ONLY.** Do NOT enable highlighting — do
NOT lift the treesitter deferral in `runtime/nvim/init.lua` (no `vim.treesitter.start`,
no runtimepath wiring for highlighting, no `init.lua` behavior change). Enablement
is a separate future change.

## Work items

1. **Two pure-Lua plugin TreeArtifacts** (vendored EXACTLY like `bazel/orlando` and
   `bazel/lighthouse`: sha256-pinned `http_archive` -> `copy_to_directory` with
   `replace_prefixes` into an inner dir -> TreeArtifact -> bundled into
   `Contents/Resources/` via rules_apple):
   - `bazel/nvim-treesitter` : `nvim-treesitter/nvim-treesitter`, **master** branch
     (frozen/legacy, full bundled queries). Must carry `queries/` (highlights.scm +
     markdown injections.scm) and `lua/`.
   - `bazel/nvim-treesitter-textobjects` : `nvim-treesitter/nvim-treesitter-textobjects`.
2. **One compiled-parser TreeArtifact** `bazel/treesitter-parsers`: reusable
   `ts_grammar` Starlark macro using `rules_cc` (dep 0.2.17) — `cc_binary` linkshared
   from each grammar's `src/parser.c` + optional `src/scanner.c`/`.cc`, `-Isrc` —
   producing `parser/<lang>.so`, assembled via `copy_to_directory` into a TreeArtifact
   bundled at `Contents/Resources/parsers/`. Grammar repos + revisions from
   nvim-treesitter master's `lockfile.json` (version-matched to the vendored queries).
   - Curated set: html, javascript, typescript, tsx, css, json, go, rust, c, cpp,
     yaml, toml, bash, swift, kotlin, java. markdown/markdown_inline EXCLUDED (ship
     with pinned nvim 0.12.3). typescript+tsx from one repo (subdirs). Handle C and
     C++ scanners. swift = rough edge (committed generated parser.c). Drop+LOG any
     grammar that can't compile hermetically; don't let one block the rest.
3. **MODULE.bazel**: wire new extensions, regenerate `MODULE.bazel.lock`.
4. **crates/wolf-macos BUILD.bazel**: add the three TreeArtifacts to Wolf.app resources.

## Tests (no mocks; real pinned nvim)

- Per-tree `sh_test` asserting TreeArtifact contents (mirror orlando/lighthouse tree tests).
- Extend `crates/wolf-macos:bundle_layout_test` for the new resources.
- New `crates/wolf-ffi` real-nvim test (model after orlando_load/lighthouse_load):
  load a compiled parser in pinned nvim 0.12.3, prove ABI compat via
  `vim.treesitter.language.add` + a query. Must NOT modify init.lua. Skip loudly off-macOS.

## Docs

- New READMEs: `bazel/nvim-treesitter`, `bazel/nvim-treesitter-textobjects`,
  `bazel/treesitter-parsers`.
- Update `build.md` (resources, test targets, bump procedures).
- Follow-up note in `treesitter-decision-report.md` / `runtime/nvim/README.md`:
  bundle now SHIPS parsers + plugins; ENABLEMENT and Linux/Nix path remain deferred.

## Invariants

- wolf-core stays GUI-free; these are BUILD-ONLY extensions, never in wolf-core's graph.
- Hermetic: parsers ship signed in the bundle, nothing fetched/compiled at runtime.
- `.so`s must be host-compiled, valid signable Mach-O (NOT nix-built — /nix/store install names aren't relocatable).

---

## Architect's plan

### New files

**Plugin trees (clone orlando/lighthouse pattern):**
- `bazel/nvim-treesitter/{extensions.bzl,BUILD.bazel,nvim_treesitter_tree_test.sh,README.md}` — ext `nvim_treesitter`, repo `nvim_treesitter`, pinned commit on frozen **master** branch. Globs `queries/**`, `lua/**`, `plugin/**`, `LICENSE*`; `replace_prefixes {archive_root: "nvim-treesitter"}`; out `nvim_treesitter_tree`. Tree test asserts `queries/markdown/injections.scm` + `lua/nvim-treesitter/`.
- `bazel/nvim-treesitter-textobjects/{extensions.bzl,BUILD.bazel,nvim_treesitter_textobjects_tree_test.sh,README.md}` — ext/repo `nvim_treesitter_textobjects`; globs `queries/**`,`lua/**`,`LICENSE*`; inner dir `nvim-treesitter-textobjects`. Tree test asserts `lua/nvim-treesitter-textobjects/` + a `textobjects.scm`.

**Parser tree:**
- `bazel/treesitter-parsers/{extensions.bzl,BUILD.bazel,parsers_tree_test.sh,README.md}` — ext `treesitter_parsers` declaring ONE `http_archive` per grammar repo (ts+tsx share one). Each repo gets an inline overlay `build_file_content` (the reusable `ts_grammar` template, NOT a loaded macro — matches orlando/lighthouse, keeps `-Isrc` resolving in-repo): `cc_binary(name="<lang>.so", srcs=[parser.c(+scanner.c/.cc)], includes=["<root>/<subdir>src"], linkshared=True, linkstatic=True, copts C:-std=c11 / C++:-std=c++14)` → `copy_file` to `parser/<lang>.so` → `filegroup <lang>_parser`. Main BUILD: one `copy_to_directory(out="parsers_tree", replace_prefixes {"parser":"parsers/parser"})` → inner `parsers/parser/<lang>.so`. Tree test HARD-asserts json/go/rust/typescript/tsx `.so` non-empty; SOFT-notes swift/kotlin/java.

**FFI ABI test:**
- `crates/wolf-ffi/tests/treesitter_parser_abi.rs` — models orlando_load but uses the REAL captive init.lua UNCHANGED and points nvim at the staged parser by abs path. Asserts `pcall(vim.treesitter.language.add,'go',{path=<abs>/parser/go.so})` truthy AND `vim.treesitter.get_string_parser('package main\nfunc f() {}','go'):parse()[1]:root()` type `source_file` & `not has_error()`. Sentinel-file readback. Skips loudly off-macOS / tree unstaged. MUST NOT touch init.lua.

### Edits to existing files
- `MODULE.bazel`: add 3 `use_extension`/`use_repo` blocks after lighthouse; `use_repo(treesitter_parsers, "ts_html","ts_javascript","ts_typescript","ts_css","ts_json","ts_go","ts_rust","ts_c","ts_cpp","ts_yaml","ts_toml","ts_bash","ts_swift","ts_kotlin","ts_java")` (keep in sync with final compiled set). Regenerate `MODULE.bazel.lock`.
- `crates/wolf-macos/BUILD.bazel`: add the 3 trees to `macos_application` resources; update comments.
- `crates/wolf-macos/bundle_layout_test.sh`: add checks (h) nvim-treesitter query file, (i) textobjects lua dir, (j) `parsers/parser/{go,rust,typescript,tsx,json}.so` non-empty; SOFT-note swift/kotlin/java.
- `crates/wolf-ffi/BUILD.bazel`: add `rust_test treesitter_parser_abi` (data: nvim_tree, parsers_tree, init.lua; env WOLF_TEST_INIT_LUA/WOLF_TEST_NVIM_DIR/WOLF_TEST_PARSERS_DIR; tags no-sandbox).
- `crates/wolf-ffi/tests/common/mod.rs`: add `parsers_dir_or_none()` → `resolve_runfile("WOLF_TEST_PARSERS_DIR")`.
- Docs: `build.md` (3 resource paragraphs + bump procs + new test targets + layout checks h/i/j; parsers SHIP but highlighting NOT enabled, Linux/Nix deferred), `runtime/nvim/README.md` Deferred note, `treesitter-decision-report.md` follow-up note. `prettier --write` all touched md.

### Curated grammar table (revisions from nvim-treesitter master lockfile.json; repos from lua/nvim-treesitter/parsers.lua; RE-VERIFY scanner ext at pinned rev)
| lang | repo | subdir | scanner | risk |
|---|---|---|---|---|
| json | tree-sitter/tree-sitter-json | — | none | low |
| go | tree-sitter/tree-sitter-go | — | none | low |
| c | tree-sitter/tree-sitter-c | — | none | low |
| java | tree-sitter/tree-sitter-java | — | none | low |
| css | tree-sitter/tree-sitter-css | — | .c | low |
| javascript | tree-sitter/tree-sitter-javascript | — | .c | low |
| rust | tree-sitter/tree-sitter-rust | — | .c | low |
| bash | tree-sitter/tree-sitter-bash | — | .c | low-med |
| html | tree-sitter/tree-sitter-html | — | .c (was .cc) | low-med |
| toml | tree-sitter-grammars/tree-sitter-toml | — | .c | low-med |
| typescript | tree-sitter/tree-sitter-typescript | typescript/ | .c | med (shared common/ header) |
| tsx | tree-sitter/tree-sitter-typescript | tsx/ | .c | med |
| cpp | tree-sitter/tree-sitter-cpp | — | .c (was .cc) | med (verify) |
| yaml | tree-sitter-grammars/tree-sitter-yaml | — | .c (ikatyang was .cc) | med (verify) |
| kotlin | fwcd/tree-sitter-kotlin | — | .c | HIGH |
| swift | alex-pinkus/tree-sitter-swift | — | .c | HIGHEST — parser.c GENERATED, NOT committed on default branch; pin a source carrying committed src/parser.c (with-generated-parser branch / release asset) or DROP+LOG |

**Drop+LOG policy:** a grammar that won't compile hermetically is removed from ext + use_repo + assembly and recorded in `bazel/treesitter-parsers/README.md` status table. One grammar never blocks the rest; hard asserts only cover the low-risk set.

### Implementation order
1. Both plugin trees + wire + lock + tree tests. 2. Parser scaffold with safe subset (json,go,c,java,javascript,css,rust,bash,html,toml). 3. Add typescript+tsx. 4. Add C++-scanner cpp,yaml. 5. Add kotlin then swift last, drop+LOG on failure. 6. Bundle wiring + bundle_layout_test. 7. FFI ABI test. 8. Docs + prettier.

### Key uncertainties for coder
- **cc_binary linkshared → loadable Mach-O**: apple_support emits `-dynamiclib` (MH_DYLIB); dlopen accepts dylibs on macOS so should work — the ABI test is the gate. Fallback: `linkopts=["-Wl,-undefined,dynamic_lookup"]` or cc_shared_library.
- **sha256**: `curl -L <url>/archive/<rev>.tar.gz && shasum -a 256`, OR empty-then-Bazel-reports (must pin before commit).
- **swift** parser.c not committed on default branch — pin generated-source branch/release or drop+LOG.
- **typescript/tsx** shared `common/` header — may need `includes=["<root>/common"]`.
- **copy_to_directory** merge of N `parser/<lang>.so` → `parsers/parser/` — verify prefix mechanics empirically.
- No new bazel_dep needed (rules_cc, aspect_bazel_lib, apple_support all present). Build-only; never in wolf-core graph.
