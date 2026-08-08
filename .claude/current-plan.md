# Work unit 20 — treesitter vendoring, highlighted code blocks, and the baseline-grid fix

Branch: `linux-feat/treesitter-vendoring` off `linux` (`c6f18d8`, clean and green:
963 tests / 94 suites, 52 cage smokes).

## Scope

**Row A — "Syntax-highlighted code blocks"** (`feat-list.md:151`, Markdown preview
table). **Row B — "Vendored treesitter plugins and parsers"** (`feat-list.md:184`,
Appearance / typography / theming table). **Row C — an approved one-line CSS fix**
in shared `wolf-core` Markdown CSS, not a feat-list row of its own.

Out of scope, do not touch: vendored `orlando`, vendored `lighthouse`,
preview-in-browser, multi-window/tabs.

### Established, do not re-derive

- `wolf_highlight_code_blocks` sits at **module level** (`runtime/nvim/init.lua`
  :605-760), deliberately outside the vendored-tree gate, and gates **per
  language**.
- The pinned Neovim ships seven parsers with `highlights` queries — `c`, `lua`,
  `markdown`, `markdown_inline`, `query`, `vim`, `vimdoc`.
- The gap is **parser coverage**. `bazel/treesitter-parsers` emits Mach-O and buys
  Linux nothing.
- `render_source_html` degrades per block to the plain render on any failure, so
  added coverage is additive and cannot break existing rendering.
- Rows A and B are **siblings, not a chain**.

## Reconnaissance — measured on this host, take as fact

### R1. The gate is a sibling-directory probe, not an env var

`runtime/nvim/init.lua:233-268`. `this_dir` is `init.lua`'s own directory
(`:176-177`). Three independent gates: `orlando/` (:180-183), `lighthouse/`
(:204-208), `nvim-treesitter/` (:233-268). The treesitter gate's **outer** probe is
`nvim-treesitter/`; `parsers/` and `nvim-treesitter-textobjects/` are probed
**inside** it, so staging `parsers/` alone is inert. Inside the gate it also
installs a `FileType=markdown` autocmd calling `pcall(vim.treesitter.start, buf)`
— that is the **editor-appearance half of row B**, and it is lost by any delivery
route that puts the trees on `rtp` without firing this gate.

`WOLF_TEST_NVIM_TREESITTER_DIR` / `WOLF_TEST_PARSERS_DIR` are read only by
`crates/wolf-ffi/tests/common/mod.rs:175-193`, set only by
`crates/wolf-ffi/BUILD.bazel:797, 835-836, 868-869`. `flake.nix` contains the
string "treesitter" zero times.

### R2. Baseline, measured with `--clean` on the pinned nvim

Pinned nvim is **0.12.2** (`WOLF_NVIM_BIN`, `flake.nix:295`); core spawns it
`--embed --clean -u <init.lua> -i NONE` (`crates/wolf-core/src/nvim.rs:1592-1607`).
Probe: 7 of 34 candidate languages have both a parser and a `highlights` query —
exactly the seven builtins. Every miss is `No parser for language "<lang>"`;
**parsers are the missing half, not queries.**

This host has stray `~/.local/share/nvim/site/parser/*.so`. Any probe without
`--clean` reports false positives. Every measurement must use `--clean`.

### R3. What nixpkgs offers at the flake's pinned rev

nixpkgs rev `9ae611a455b90cf061d8f332b977e387bda8e1ca`.

- `vimPlugins.nvim-treesitter` is version `0.10.0-unstable-2026-04-03`, rev
  **`4916d6592ede8c07973490d9322f187e07dfefac`** — **byte-identical to the rev
  `bazel/nvim-treesitter/extensions.bzl:47-51` pins for macOS.** Queries live at
  `<plugin>/runtime/queries/<lang>/`, 329 language dirs, no `parser/` dir.
- `nvim-treesitter.parsers.<lang>` → a dir containing exactly `parser/<lang>.so`
  (ELF). `nvim-treesitter.queries.<lang>` → a dir containing exactly
  `queries/<lang>/`. Together they are precisely the rtp layout nvim wants.
- `withPlugins` / `withAllGrammars` build to the **same store path as the bare
  plugin** — they only set `passthru.dependencies`, materialized by
  `neovim.override`. `withAllGrammars` costs 249.9 MiB and 313 local builds.
- `tree-sitter-grammars.tree-sitter-<lang>` is **unsuitable**: the shared object is
  named `parser` not `<lang>.so`, its queries are flat and in the upstream
  tree-sitter dialect rather than nvim-treesitter's, and its revs do not match the
  queries. Do not use it.

### R4. ABI — not a blocker

nvim 0.12.2 accepts ABI ≥ 13. Measured grammars are 14 or 15.

### R5. The end-to-end result, measured

A `symlinkJoin` of `nvim-treesitter.parsers.<lang>` + the plugin's `runtime`,
staged as `nvim-treesitter/` and `parsers/` siblings, makes
`_G.wolf_highlight_code_blocks` return real spans:

```
### BASELINE ###                 ### VENDORED ###
[1] rust   -> false              [1] rust   -> 19 spans
[2] python -> false              [2] python -> 12 spans
[3] go     -> false              [3] go     -> 15 spans
[4] lua    -> 14 spans           [4] lua    -> 14 spans
[5] sql    -> false              [5] sql    -> false   (not in the set)
[6] (none) -> false              [6] (none) -> false   (correct)
```

Closure cost of the 16-language set: **22.6 MiB** added, 18 paths.

### R6. The macOS reference set — 16 languages

`json, go, c, java, css, javascript, rust, bash, html, toml, cpp, typescript, tsx,
yaml, kotlin, swift` (`bazel/treesitter-parsers/BUILD.bazel:26-52`,
`MODULE.bazel:189-207`, `bazel/treesitter-parsers/README.md:58-75`). macOS
deliberately excludes `markdown`/`markdown_inline` because nvim ships them.
Queries come from a **separate** package, `bazel/nvim-treesitter`, globbed with **no
per-language filter** — so on macOS queries are language-complete and only parsers
are curated.

### R7. Row C's blast radius

`body.wolf-markdown code` is `base.css:393-399` and declares no `line-height`.
`body.wolf-markdown pre` (:401-408) declares `line-height: 24px`;
`body.wolf-markdown pre code` (:410-414) exists. The highlighter emits **no CSS
class** — only inline `style="color:#rrggbb;…"` spans (`markdown.rs:1039-1096`),
which declare colour and the three emphases and never `line-height` or
`font-family`. So the new rule computes onto `pre code` and onto every highlight
span, and nothing in the span markup fights it. `pre`'s own 24px strut is what
still governs the block, which is why the measured `pre` case is 123 → 120.

`.wolf-src`'s `line-height: 0` (`base.css:264-273`) is a **different claim** —
zeroing an anchor's strut, not an inline-box-union term — and
`markdown_render.rs:2215-2245` pins it. Do not merge the two.

`GRID_SELECTORS` (`markdown_render.rs:556-581`) deliberately omits `code`; the doc
comment at `:535-542` argues that allowlisting `code`'s 2px padding would rubber-
stamp the audit. That argument survives the fix and `code` stays out of the sweep,
but the comment becomes stale the moment `code` grows a vertical property.

`embedded_stylesheets_carry_no_markup_like_text` (`markdown.rs:3271`) forbids an
element name in angle brackets anywhere in the CSS — binding on any new comment.

### R8. Unit 18's "12 of 16 off-grid" is prose, not a test

`git show --stat fca3397` touched only `markdown/README.md` and `markdown/base.css`
— "Comment and prose only; no declaration changed." There is **no laid-out-box
harness** to extend. `crates/wolf-gnome/tests/README.md:217` says so explicitly.

### R9. Three FFI tests skip on Linux today, and the vendoring can unskip them

`crates/wolf-ffi/tests/{treesitter_parser_abi,treesitter_highlight,code_block_highlight}.rs`
each loud-no-op-pass when `WOLF_TEST_*` is unset. `code_block_highlight.rs` uses a
**`go`** fence, and `go` is in the 16-language set.

### R10. Two pre-existing doc defects found in passing

- `bazel/treesitter-parsers/README.md` says "Pinned Neovim 0.12.3 ships only the
  `markdown` and `markdown_inline` parsers." Measured false — 0.12.2 ships seven
  with queries. This is an eighth site of the same family units 18 and 19 corrected.
- `bazel/nvim/extensions.bzl:28` pins nvim `0.12.3` for macOS; the flake's
  `pkgs.neovim` is `0.12.2`. Not an ABI problem; the two platforms are not on the
  same nvim. Record, do not fix.

## Orchestrator decisions — binding

**O1. Vendor exactly the macOS 16, no more and no fewer.** The row's whole point is
cross-platform parity, so the set is *defined by* `bazel/treesitter-parsers`'s list
and a divergence needs its own reason. `python` is tempting and is deliberately
excluded — adding it here would make macOS the laggard on a row macOS already owns,
and the honest way to get `python` is to add it to both sides in one unit. `c` is
kept even though nvim ships it, because taking the parser and the query from the
same nvim-treesitter rev is more consistent than mixing sources, and because macOS
does exactly that. Record this reasoning where the set is written down, with the
"adding a language" recipe naming **both** sides.

**O2. Queries come from `${vimPlugins.nvim-treesitter}/runtime`, whole, unfiltered.**
It is the same upstream rev macOS pins (R3), so the two platforms get byte-identical
queries; and macOS already globs queries with no per-language filter (R6). Parsers
stay curated. Do not use `tree-sitter-grammars.*` (R3).

**O3. Delivery is an env-var fallback inside the existing gate, not a wrapper and
not a repo-staged symlink.** Three routes were measured and all three work
mechanically. The wrapped-`neovim.override` route needs no repo change but never
fires the gate at `init.lua:233`, so it silently drops the `FileType=markdown`
`vim.treesitter.start` autocmd — **half of row B**. Staging real siblings next to
`runtime/nvim/init.lua` writes `/nix/store` symlinks into a git-tracked directory.
So: `init.lua`'s two probes fall back to environment variables when the sibling is
absent, and `flake.nix` exports them.

- macOS behaviour is unchanged by construction: the siblings exist there, so the
  fallback never fires.
- Names must not carry `TEST`. `WOLF_TEST_NVIM_TREESITTER_DIR` /
  `WOLF_TEST_PARSERS_DIR` keep their present meaning and their present readers, and
  **no Bazel file changes** — that is deliberate, because there is no Bazel on this
  host to verify a change with.
- The `wolf-ffi` harness gains a fallback to the new variables so R9's three tests
  run on Linux from the devShell.

**O4. Row A's evidence must come from the running GTK application, not only from
`cargo test`.** R9's unskipped FFI tests are necessary and not sufficient: they
prove core produces styled spans, not that the preview shows colour. The pixel
observable is that a highlighted `pre` block contains **more than one ink colour**,
where an unhighlighted one is a single `#e6e6e6` on `#161616`. That is falsifiable
by pointing the fence at a language with no parser.

**O5. Row C is pinned twice, and the second pin is measured before it is asserted.**
The declared-value test is the floor. Above it, the coder must **measure first**
whether a laid-out observation is affordable under `cage` — a fixture carrying
inline `code` above a high-contrast full-width `hr`, whose ink row shifts by 2px per
preceding code-carrying block. **No assertion may be written before its measurement
exists** (unit 19's O6). If the measurement is unstable, record it and ship the
declared-value pin alone.

The coder must also eyeball one live capture for the risk nobody has stated: an
inline box with `line-height: 0` contributes no height, so monospace glyphs can
overflow the line box. Unit 18 measured the heights, not the legibility.

**O6. The macOS half of row C is unverifiable here and the docs must say so
plainly.** The rule changes the document macOS renders. There is no macOS host in
this unit.

**O7. `Cargo.lock` must not move.** No new Rust dependency. If it moves, find out
what happened before proceeding.

**O8. `base.css:120-153` is the primary Row-C doc target and it currently asserts
the opposite of what will ship** — "Neither override ships — they are measurements,
not a fix, and the fix is an open question because this document renders on two
platforms." Rewrite in place. Companion sites, all to be rewritten and not appended
beside: `markdown/README.md:61, 62, 68, 69, 70`,
`crates/wolf-gnome/tests/README.md:217`, `feat-list.md:190`, and
`markdown_render.rs:535-542`'s stale premise.

---

## Orchestrator decisions on the architect's plan — binding, read before the plan

The plan below is **approved in full** except where these override it.

**O9. O3 is narrowed: comment-only edits to `.bazel` / `.bzl` files are permitted
and required. Structural edits remain forbidden.** The architect's caveat 2 asks
whether to leave four comments that this unit makes false. O3's stated criterion
was verifiability — "there is no Bazel on this host to verify a change with." A
comment cannot change build semantics, so it is verifiable by inspection and the
criterion does not reach it. CLAUDE.md's rule that a false comment is a bug does
reach it. So:

- **Correct** `crates/wolf-ffi/BUILD.bazel:775, 812, 846` ("SKIPS when … not staged
  (the cargo / Linux path)"), which this unit falsifies.
- **Read `bazel/treesitter-parsers/extensions.bzl:14` before touching it.** If its
  claim is about a nix-built `.so` being non-relocatable **inside a signed
  macOS bundle**, it stays true and must be left alone; the Linux devShell is not a
  signed bundle. Correct it only if it makes a claim about Linux.
- Change **no** target, `srcs`, `deps`, `data`, `env`, tag or attribute. Keep every
  edited comment inside the surrounding file's line-length style.

**O10. The Row A smoke's readback must also carry a treesitter-active flag, and
that flag is Row B's running-application evidence.** The plan gives Row B its tick
on two `cargo test` suites, both of which stage siblings into a temp directory and
therefore never exercise the devShell env fallback (V11) or the running GTK app.
The editor-appearance half of Row B — the `FileType=markdown` autocmd firing inside
the real application — would then have no evidence at all.

The fix is cheap and decisive: extend the scenario's existing `writefile` readback
with `vim.treesitter.highlighter.active[<source bufnr>] ~= nil`, produced by
Neovim, unsatisfiable by any pixel or OCR term. `treesitter_highlight.rs` already
establishes that exact expression as this repo's idiom.

This earns a **second named falsifier, distinct from T2 and isolating the outer
gate**: delete the environment fallback from the **queries** probe (leaving the
parsers one). Predicted red: the gate does not fire, the flag reads false, and the
chroma assertion reddens too, because `parsers/` is probed only inside the outer
gate (R1). Both must be recorded.

**O11. Passes are consolidated to cut full-suite runs; the agent boundaries below
replace §11's table.** A `cargo test --workspace` with 52 cage smokes is the
expensive step, and §11 spends five of them. The regression gate that matters is
"everything that makes the vendoring reachable, then one full suite."

| Pass | Actor | Files (exclusive) |
| --- | --- | --- |
| 1 | coder | `flake.nix`, `runtime/nvim/init.lua`, `crates/wolf-ffi/tests/common/mod.rs`. Capture the FFI skip transcript **before** editing. Then `nix develop` timing, then one full `cargo test --workspace` against the 52-smoke baseline — this is §3.6's regression gate. |
| 2 | test-writer | `crates/wolf-core/tests/markdown_render.rs` — T1 **and** the `GRID_SELECTORS` comment rewrite at `:535-542`. Written before the CSS lands, so it is red by construction. |
| 3 | coder | `crates/wolf-core/src/markdown/base.css`, `crates/wolf-core/src/nvim.rs` doc comment. Run `markdown_render`; record T1's red-before and green-after — that pairing **is** T1's red run, obtained for free. |
| 4 | coder | Measurements only. A temporary scenario in `smoke_headless.rs`, run, recorded, then **reverted to a clean tree** (unit 19's Phase A procedure). Covers §5.7's chroma calibration, §6.3's `hr` A/B, O10's readback shape, and §6.4's legibility eyeball. |
| 5 | test-writer | `crates/wolf-gnome/tests/smoke_headless.rs` — the Row A smoke with the measured constants, the T3 guard, O10's readback assertion, and the Row C smoke **only if** §6.3's (A)(B)(C) all held. |
| 6 | coder | Run the smokes; apply every falsifier one at a time, each **alone**; record the verbatim failure; restore; re-run green; verify the restore. |
| 7 | reviewer | read-only |
| 8 | technical-writer | every document in §10, plus O9's Bazel comments, plus O12. |
| 9 | gates, commit, merge |

**O12. Two documents §10 misses; add them to the writer's list.**
`crates/wolf-core/src/README.md` — check whether `#### nvim.rs` or `#### markdown.rs`
restates the "seven parsers" or "Linux renders code blocks plain" claim, and rewrite
in place only if it does. And `crates/wolf-core/README.md` — verify it names no
parser set or platform claim.

**O13. Every prediction in §3.6 stays a prediction until pass 1's suite run.** Any
smoke that moves is a **finding to record**, never a threshold to retune. If a
mover cannot be explained, stop and report rather than adjusting a constant.

---

# Work unit 20 — implementation plan

## 0. Preamble: scope of my own authority, and two caveats

I have no Bash in this session. Everything below that is not cited to a file I read is either (a) taken as fact from R1–R10, or (b) labelled a **prediction** with the pass that converts it into a measurement. I contradict none of R1–R10.

**Caveat 1 — working tree.** `git status` at session start showed `flake.nix`, `Cargo.lock`, and four `crates/wolf-gnome/src/*.rs` files modified on a different branch. The `flake.nix` I read has `WOLF_NVIM_BIN` at exactly line 295, matching the plan's citation, so my reading matches the orchestrator's. The coder must still re-verify every line number after checking out `linux-feat/treesitter-vendoring` from `c6f18d8`.

**Caveat 2 — one place O3 and CLAUDE.md pull against each other.** This unit makes three comments in `crates/wolf-ffi/BUILD.bazel` (lines 775, 812, 846: "SKIPS when … not staged (the cargo / Linux path)") and one in `bazel/treesitter-parsers/extensions.bzl:14` factually wrong. O3 forbids Bazel file changes and there is no Bazel here to verify with. **My ruling: obey O3 — leave every `.bazel` and `.bzl` file byte-identical — and carry the four stale comments as a named, deliberate follow-up for a host with Bazel.** Flagging this to you rather than deciding it silently.

---

## 1. Established facts I verified myself (beyond R1–R10)

Cite these; do not re-derive.

| # | Fact | Source |
|---|---|---|
| V1 | `nvim::spawn` builds the child `Command` with six `.env(...)` overrides and **no `env_clear()`**, so the captive nvim inherits the spawning process's whole environment. | `crates/wolf-core/src/nvim.rs:1592-1608` |
| V2 | `cage_command` sets five variables and `env_remove`s `DBUS_SESSION_BUS_ADDRESS`; it does **not** clear. The devShell environment therefore flows cage → `sh` → the app → nvim. | `crates/wolf-gnome/tests/smoke_headless.rs:10016-10034` |
| V3 | `resolve_runfile` returns an **absolute** path unchanged when it exists, before any runfiles-base probing. A `/nix/store/…` value works with no helper change. | `crates/wolf-ffi/tests/common/mod.rs:36-38` |
| V4 | `vim.env` is already the idiom in this init.lua (`vim.env.WOLF_SWAP_DIR`), and `--clean` does not touch the process environment. | `runtime/nvim/init.lua:33` |
| V5 | Nothing in `runtime/` sets `conceallevel`, so nvim-treesitter's `(#set! conceal "")` directives are inert. | grep `conceal` over `runtime/` → no matches |
| V6 | Dark palette: `body` bg `#1b1b1b`, `body` color `#d4d4d4`, `code` color `#e6e6e6` on `#2b2b2b`, **`pre` bg `#161616`**, `hr` `border-top-color: #3a3a3a`. All five are **neutral** (r = g = b). | `crates/wolf-core/src/markdown/themes/default.css:21-64` |
| V7 | `hr { height: 24px; margin: 24px 0 0; border: 0; border-top: 1px solid }` with `box-sizing: border-box` inherited — the hairline is the first pixel row of the 24px box. | `base.css:275-277, 443-449` |
| V8 | `rule_decls(css, "body.wolf-markdown code")` matches **only** the exact rule: `selector_entry_matches("body.wolf-markdown pre code", "body.wolf-markdown code")` fails `strip_suffix`. A bare `"code"` query would merge both rules. | `markdown_render.rs:674-682, 693-730` |
| V9 | The 16-language macOS set is alphabetical in `srcs` and matches R6 exactly. | `bazel/treesitter-parsers/BUILD.bazel:29-44` |
| V10 | Every smoke helper the new smoke needs already exists: `require_smoke_env` (:2302), `make_temp_cwd` (:9422), `smoke_state_dir_at` (:9538), `smoke_config_dir_at` (:9617), `assert_png_present` (:10423), `clear_evidence` (:10444), `exact_f64` (:10682), `pixel_target` (:10700), `read_window_buffer_names` (:10976), `write_relaunch_fixtures` (:18647), `APPEARANCE_FORCED_DARK` (:19654). | `smoke_headless.rs` |
| V11 | `crates/wolf-ffi/tests/treesitter_highlight.rs` and `code_block_highlight.rs` **stage real siblings** in a temp dir (`copy_dir_recursive` → `T/nvim-treesitter`, `T/parsers`), and `treesitter_parser_abi.rs` loads the `.so` **by absolute path**. None of the three exercises `init.lua`'s env fallback. | the three test files |

**V11 is the single most important fact in this plan.** It is what makes the cage smoke's named falsifier *isolated*: deleting the env fallback from `init.lua` reddens the smoke and leaves all three FFI tests green, because none of them reaches that code path.

---

## 2. Area 1 — `flake.nix`

### 2.1 The language list: one owner

Add to the `let` block **between `iaWriterMono`'s closing `};` (:183) and `darwinShell` (:185)**:

```
wolfTreesitterLanguages = [ ... 16 strings ... ];
```

Exactly O1's set, **alphabetical**, matching `bazel/treesitter-parsers/BUILD.bazel:29-44`'s `srcs` order so a divergence is eyeball-visible in a side-by-side diff:

`bash, c, cpp, css, go, html, java, javascript, json, kotlin, rust, swift, toml, tsx, typescript, yaml`

The comment above it must carry, in this order:

1. **This list is defined by `bazel/treesitter-parsers/BUILD.bazel`'s `srcs`, and the two must not diverge.** The row's subject is cross-platform parity.
2. **`c` is kept even though nvim ships a `c` parser**, because taking the parser and the query from the same nvim-treesitter revision is more consistent than mixing sources, and because macOS does exactly that.
3. **`markdown` / `markdown_inline` are deliberately absent** — nvim ships both, macOS excludes both, and re-vendoring them would shadow nvim's own with a second copy.
4. **`python` is deliberately absent.** Adding it here alone would make macOS the laggard on a row macOS owns. The honest route is to add it to both sides in one unit.
5. **Adding a language means editing BOTH sides**: this list, and `bazel/treesitter-parsers/BUILD.bazel`'s `srcs` plus its `MODULE.bazel` / `extensions.bzl` pin.
6. **Do not reach for `pkgs.tree-sitter-grammars.tree-sitter-<lang>`** — its shared object is named `parser`, not `<lang>.so`; its queries are flat and in the upstream tree-sitter dialect rather than nvim-treesitter's; and its revisions do not match the queries. (R3.)

### 2.2 The two derivations

Immediately after the list, in the same `let`:

```
wolfTreesitterParsers = pkgs.symlinkJoin {
  name = "wolf-treesitter-parsers";
  paths = map (lang: pkgs.vimPlugins.nvim-treesitter.parsers.${lang}) wolfTreesitterLanguages;
};

wolfTreesitterQueries = "${pkgs.vimPlugins.nvim-treesitter}/runtime";
```

Comment contents:

- Each `nvim-treesitter.parsers.<lang>` contributes exactly `parser/<lang>.so`, so the join is `$out/parser/<lang>.so` — precisely the `runtimepath` layout nvim resolves `<rtp>/parser/<lang>.so` from. No two inputs collide, because each contributes a distinct filename.
- **Queries are the plugin's `runtime` directory, whole and unfiltered** (O2). It already contains `queries/<lang>/`, so it *is* an rtp entry. It is the same upstream revision `bazel/nvim-treesitter/extensions.bzl:47-51` pins for macOS (`4916d6592ede8c07973490d9322f187e07dfefac`), so the two platforms get byte-identical queries; and macOS already globs queries with no per-language filter. Parsers stay curated; queries do not.
- **Do not reach for `withPlugins` / `withAllGrammars`.** They build to the same store path as the bare plugin and only set `passthru.dependencies`, which nothing here materializes; `withAllGrammars` costs 249.9 MiB and 313 local builds for a set we do not want. (R3.)

### 2.3 The exports

In `linuxShell.shellHook`, **immediately after the `WOLF_RG_BIN` export (:298) and before `RUST_SRC_PATH` (:299)**, so the three "where a pinned external artifact lives" exports sit together:

```
export WOLF_NVIM_TREESITTER_DIR="${wolfTreesitterQueries}"
export WOLF_PARSERS_DIR="${wolfTreesitterParsers}"
```

Exact names, exact values. Names carry no `TEST` (O3): `WOLF_TEST_*` keeps its present meaning and its present readers, and no Bazel file changes.

The comment must state: these are the two variables `runtime/nvim/init.lua`'s treesitter gate falls back to when the macOS `.app`'s sibling directories are absent; without them the gate is a strict no-op on Linux and the editor loses markdown treesitter highlighting *and* the preview loses every fenced language nvim does not ship.

### 2.4 `packages` vs `shellHook`

**Nothing goes into `packages` or `buildInputs`, and this is deliberate.**

Neither tree carries a `bin/`, a `lib/` or a `.pc` file, so `packages` membership would only add them to `PATH` and `PKG_CONFIG_PATH` for nothing. A store path interpolated into `shellHook` acquires a Nix string context, which makes it an input of the shell derivation and therefore built or substituted before the shell is entered — exactly how `${pkgs.gnused}/bin/sed` (`:308`) works today, and `gnused` is in no `packages` list. Cite that precedent in the comment; it is the thing a later agent will "fix".

### 2.5 The laziness NOTE

**Rewrite `flake.nix:48-59` in place** to name `wolfTreesitterLanguages`, `wolfTreesitterParsers` and `wolfTreesitterQueries` alongside `rustToolchain`, `wolfPointerInject` and `iaWriterMono`. The grammars *could* build on Darwin, so the argument is `iaWriterMono`'s ("safer there than here") rather than `wolfPointerInject`'s. Keep the two prohibitions verbatim: no top-level reference, and no `lib.optional` guard — a guard would imply the laziness argument is unsound.

### 2.6 What a developer sees on first `nix develop`

R3 measured **313 local builds** for `withAllGrammars`, so the honest expectation is that the 16 grammars **build locally rather than substitute**. R5 measured the added closure at **22.6 MiB across 18 paths**. Each grammar is a single small C/C++ compile.

The coder must **record the actual wall clock** of the first `nix develop --command true` after the edit and put the observed number in `build.md`. Do not write "substitutes" into any document without having watched it.

---

## 3. Area 2 — `runtime/nvim/init.lua`

### 3.1 The edit

Immediately before the gate, add one local helper:

```lua
local function resolve_vendored_dir(sibling, env_name)
  -- sibling first, environment second
end
```

Semantics, precisely:
1. If `uv` is truthy and `uv.fs_stat(sibling)` is truthy → return `sibling`.
2. Else read `vim.env[env_name]`; if it is a non-`nil`, non-empty string **and** `uv.fs_stat` of it is truthy → return it.
3. Else return `nil`.

Then rewrite the gate at `:233-243`:

- `local nvim_treesitter_dir = resolve_vendored_dir(this_dir .. "/nvim-treesitter", "WOLF_NVIM_TREESITTER_DIR")`, and `if nvim_treesitter_dir then`.
- Inside it, `local parsers_dir = resolve_vendored_dir(this_dir .. "/parsers", "WOLF_PARSERS_DIR")`, and `if parsers_dir then`.
- **`nvim-treesitter-textobjects` keeps its bare sibling probe with no fallback.** Nothing on Linux produces such a tree, so an env var for it would be a variable with no producer — dead surface that a later agent wires up wrongly. The tree is documented as bundled-but-inert (no source, no setup, no keymaps), so its absence changes nothing observable and no test could discriminate a fallback for it. Say this in the comment, or someone adds it "for symmetry".

The step-2 `fs_stat` check is not belt-and-braces: it is what makes a stale store path (a `nix store gc` under a long-lived shell) degrade to the bare-checkout no-op instead of prepending a dead rtp entry.

### 3.2 Ordering: sibling first, environment second

**Sibling first.** Two reasons, both to be written down:

1. **Bundle self-containment.** A developer who has `WOLF_PARSERS_DIR` exported (the devShell does it unconditionally) and then runs the macOS `.app` would, under env-first, silently swap the bundle's own Mach-O parsers for a Linux ELF tree — a cross-platform mis-load with no symptom until a `dlopen` fails inside a `pcall`.
2. **It is what makes "macOS behaviour is unchanged" a construction rather than a hope.** On macOS both siblings exist, so `resolve_vendored_dir` returns at branch 1 and `vim.env` is never read at all. There is no macOS host in this unit (O6-adjacent), so the argument has to be structural.

### 3.3 `vim.env`, `--clean`, and the two consumer paths

- `--clean` suppresses user config, plugin and shada discovery. It does not touch the process environment, and `vim.env` is a table view over `getenv`/`setenv`. The file already relies on this at `:33` (V4).
- **App path** (`crates/wolf-gnome/src/app.rs` → `WolfCore::start` → `nvim::spawn`): V1 — six `.env` overrides, no `env_clear()`. The GTK process is launched from inside `nix develop`, so both variables reach nvim. Under `cage`, V2 — `cage_command` clears nothing, so the chain holds through the compositor too.
- **cargo-test path** (`crates/wolf-core/tests/common/mod.rs`): `config_in` sets `WOLF_NVIM_BIN` and `WOLF_STATE_DIR` and nothing else; the test process inherits the devShell environment and `nvim::spawn` passes it through. Same for `crates/wolf-ffi`'s `wolf_ffi_start`.

### 3.4 What the comment at the gate must say

**Rewrite `:210-232` in place.** Five points, each aimed at a specific "simplification":

1. The gate resolves **sibling first, environment second**, and why (§3.2). Do not reorder.
2. **The environment fallback is the only thing that fires this gate on Linux.** Deleting it does not merely change `runtimepath`: it removes the `FileType=markdown` autocmd, so the Linux editor silently loses markdown treesitter highlighting while `wolf_highlight_code_blocks` (module level, outside this gate) keeps working — the symptom is invisible to every preview test. That is half of feat-list row "Vendored treesitter plugins and parsers".
3. **`nvim-treesitter-textobjects` deliberately has no fallback**, and why (§3.1).
4. **`vim.env` is unaffected by `--clean`**, and the child inherits the spawning process's environment (`nvim.rs` sets six variables and clears none).
5. A resolved path that has vanished falls through to `nil`, so a garbage-collected store path degrades to the bare-checkout no-op.

### 3.5 `syntax on` at `:158-162` — this comment is now wrong and must be rewritten

`vim.treesitter.start` **disables** regex syntax for the buffer it attaches to (`:h vim.treesitter.start`). The current comment says `syntax on` "is the highlight source on Linux/cargo" and that the treesitter block "layers over it (via its own namespace)". Both clauses become false on a Linux devShell. Rewrite to: `syntax on` is the highlight source for every buffer treesitter does not attach to, and the documented fallback when the gate's `pcall` fails; where the gate fires, treesitter replaces it per markdown buffer.

### 3.6 The regression risk, enumerated

**This is the highest-risk part of the unit.** Every smoke fixture is markdown (`vim.filetype.add({ extension = { md = "markdown" } })`, `:156`), so the `FileType=markdown` autocmd fires in every one of them.

The channel is **cell foreground colour and cell attributes**, never position: the grid is cell-addressed and monospace, so no glyph moves and no OCR term changes column.

| Group | Members | Mechanism | Prediction |
|---|---|---|---|
| **1. Heading lines** (largest) | `PICKER_HEADING` `# PICKERTARGET`, `RESTART_HEADING` `# BOUNCEBACK`, `CLOSE_FILE_GONE` `# CLOSEGONE`, `NEWBUF_FILE` `# NEWBUFBASE`, `PREVIEW_SOURCE_HEADING` `# ZZBODYOSCAR`, `STATUS_FIXTURE_BODY` `# ZZCHROMEBODY`, `LINK_TARGET_BODY` `# ZZTARGETQUEBEC`, `SIDEBAR_FILLER_BODY` `# FILLER` | `syntax on` gives these `markdownH1 → htmlH1 → Title`. Treesitter gives them `@markup.heading.1.markdown`, which nvim's default colorscheme links to `Title`. **Same group ⇒ same pixels.** The `#` marker carries a `conceal` directive that is inert at `conceallevel = 0` (V5). | **No movement.** |
| **2. Non-heading markdown syntax** | `LINK_SOURCE_LINE` `[LLLL…](lnkinto.md)`, in `preview_link_click_repoints_the_pane_under_cage` | `@markup.link.label` / `@markup.link.url` differ from `markdownLinkText` / `markdownUrl` in colour and possibly underline. But that smoke's evidence is a `tabpagebuflist()` readback plus link ink located inside `link_search_rect`, which is derived as `area_x + area_w/2 + inset …` — inside the **preview** half by construction, never the editor. | **No movement.** Check first anyway. |
| **3. Pixel-fraction smokes** | `preview_document_follows_the_configured_appearance_under_cage` assertion 4 (control band, measured 0.0000 of 76,680); `configured_appearance_reaches_neovims_background_under_cage` | Treesitter changes glyph *foregrounds*, not the editor's background. Neither `#1b1b1b` nor `#f4f4f4` is a plausible new foreground; assertion 4's ceiling is 0.02 against a measured 0.0000. | **No movement.** The one to read numerically, because it is a fraction rather than an OCR word. |
| **4. Timing** | all smokes | One `vim.treesitter.start` parse per markdown buffer; fixtures are ≤ 20 lines. Sub-millisecond against a 1.0 s `SETTLE`. | **No movement.** |
| **5. Non-smoke** | `crates/wolf-core/tests/save_open.rs::open_existing_markdown` (`:57-73`, heading `hl_id` ≠ body `hl_id`); `redraw_grid.rs:74-90` (the `H` cell's `hl_id` resolves to a non-`None` fg) | Treesitter decorations compose into the `hl_id` the external UI receives, so a heading still differs from Normal; plain typed text is Normal under both regimes. | **No movement.** |

All five are **predictions**. Pass 3 of the agent order exists to convert them into measurements *before anything else is built on top*. Any smoke that moves is a finding to be recorded, not a threshold to be retuned.

---

## 4. Area 3 — `crates/wolf-ffi/tests/common/mod.rs`

### 4.1 The edit

Two functions, one line each:

- `parsers_dir_or_none()` (`:175-177`) → `resolve_runfile("WOLF_TEST_PARSERS_DIR").or_else(|| resolve_runfile("WOLF_PARSERS_DIR"))`
- `nvim_treesitter_dir_or_none()` (`:191-193`) → `resolve_runfile("WOLF_TEST_NVIM_TREESITTER_DIR").or_else(|| resolve_runfile("WOLF_NVIM_TREESITTER_DIR"))`

**Precedence: the Bazel `WOLF_TEST_*` variable first.** Under Bazel the staged runfiles are what the target's `data` guarantees and what its assertions were written against, so a stray devShell variable in a Bazel invocation must not shadow them. On Linux/cargo the `WOLF_TEST_*` variables are never set, so the fallback is reached unconditionally.

**What changes for macOS/Bazel: nothing.** No `BUILD.bazel` edit, no target edit, no runfile edit. The added `or_else` arm is only reachable when the first arm is `None`, which under Bazel it never is.

`resolve_runfile` needs **no change**: V3 — it returns an absolute existing path unchanged, before any base probing. `/nix/store/…` works as-is.

Both doc comments must be rewritten in place (they currently say "the cargo / Linux path, where the compiled parsers are not built at all" — false after this unit).

### 4.2 Per-test prediction on Linux, with reasons

**`treesitter_parser_abi.rs` — PASSES.**
- `parsers_dir_or_none()` resolves to `/nix/store/…-wolf-treesitter-parsers`. The test probes `<dir>/parsers/parser/go.so` (nested) then `<dir>/parser/go.so` (flat). **`symlinkJoin` gives the flat layout, and the flat branch already exists** (`:136-150`) — no test change needed.
- Its assertions are `language.add('go', {path=…})` succeeding and a `source_file` root with no error. **Both are platform-neutral**; nothing in the Rust inspects Mach-O. "Mach-O", "Bazel/macOS-only" and "pinned Neovim 0.12.3" appear only in prose.
- The one real constraint is the CONTRACT at `:152-156`: the interpolated `.so` path must contain **no spaces and no double quotes**, because it is pasted verbatim into a double-quoted Lua string. A `/nix/store/<hash>-wolf-treesitter-parsers/parser/go.so` path has neither.
- ABI: R4 — nvim 0.12.2 accepts ≥ 13, measured grammars are 14/15.

**`treesitter_highlight.rs` — PASSES.**
- `nvim_treesitter_dir_or_none()` resolves to `…/runtime`. The test probes `<dir>/nvim-treesitter` (absent) and falls back to `<dir>` itself (`:152-159`) — correct.
- `copy_dir_recursive` classifies with `std::fs::metadata`, which follows symlinks, and copies leaves by content — exactly right for a `symlinkJoin` tree, for the same reason it was right for Bazel runfiles.
- **Pre-flight the coder must run once before trusting this**: confirm `${vimPlugins.nvim-treesitter}/runtime/queries/markdown/injections.scm` exists. The test hard-asserts it at `:188-196` before launching nvim.
- Evidence it will pass: R5 measured `_G.wolf_highlight_code_blocks` returning real spans with this exact layout, which requires `vim.treesitter.query.get(lang, "highlights")` to succeed against these queries under the pinned nvim.
- **Cost note**: it recursively copies 329 query directories per run, and `code_block_highlight.rs` does it again. Expect a few seconds each. Do **not** "optimize" this into a symlink — `copy_dir_recursive`'s contract is a copy, and the gate probes real directories.

**`code_block_highlight.rs` — PASSES.**
- Same staging. R5 measured `go → 15 spans` with this layout, which is exactly what the positive half asserts. The negative half (`notalang` → no `<span style="color:#`) is independent of vendoring.

### 4.3 No new wolf-ffi test file

I propose **none**. The three existing files carry the coverage, they are already wired into `rust_test` targets, and adding a file would require a target this host cannot verify.

---

## 5. Area 4 — Row A's evidence from the running application (O4)

### 5.1 A new smoke, not an extension

`preview_document_follows_the_configured_appearance_under_cage` **cannot** carry this. Three reasons:

1. `PREVIEW_THEME_SOURCE_BODY`'s doc comment (`:20063-20096`) makes its fixture load-bearing three ways: it must be a **paragraph** (so assertion 6's only remaining term is the body's padding), it must **wrap**, and it must stay **short** (so assertion 5's lower-half band is background). A `pre` block violates all three — and `pre`'s own `#161616` is a *different* colour from `#1b1b1b`, so a taller document would push assertion 5's own-fraction below 0.90 and fire a message naming three causes, none of which is "someone added a code block".
2. Its exit-code scheme is eight codes for eight causes, four per launch, and its doc comment says so twice. A code-block leg would add a ninth cause to a scheme whose design is the symmetry.
3. The two subjects are different feat-list rows. A shared failure would name the wrong one — which is precisely what "exit codes carry meaning, and folding two causes into one code destroys evidence" (`tests/README.md:108`) exists to prevent.

### 5.2 Name and shape

`preview_highlights_a_fenced_code_block_under_cage`. **One launch**, forced `APPEARANCE_FORCED_DARK`, one `grim` capture, one `tabpagebuflist()` readback.

Exit codes: **11** (app died before its window mapped) and **14** (readback never appeared) — `PREVIEW_SCENARIO`'s own scheme minus its `16`. Codes 12 and 13 are deliberately absent and the comment must say why: nothing here depends on a clean exit — there is no second launch and no persisted state to read — so the scenario ends with `PREVIEW_SCENARIO`'s best-effort `kill` + `wait`.

One forced appearance is correct, and the comment must say so: unlike the appearance smoke, the appearance is not this smoke's subject, so the "the pair is what discriminates" argument does not apply and a second launch buys nothing.

### 5.3 The fixture — the load-bearing design choice

`CODEHL_SOURCE_FILE = "cbhl.md"`. Body, in this exact order:

1. **One short paragraph** of NATO words, no markdown syntax.
2. A fence in **`go`**, containing two lines of Go.
3. A fence in **`zzznotalang`**, containing **byte-identical text to block 2**.

Three things about this fixture are load-bearing and must be written down:

- **The two blocks carry identical text.** The only difference between them is the fence's language, so glyph coverage, antialiasing and every rendering variable except the highlighter's answer are held constant. Block 2 is therefore an *in-capture control*, not a second measurement.
- **The leading paragraph is mandatory.** `web_top` is located as the first row entirely `PREVIEW_DARK_BG` (`#1b1b1b`). If the first block were the `pre`, the first such row would be the 24px *gap between the two blocks*, and every derived rectangle would be wrong. The paragraph guarantees a run of pure body background at the document's top — the same mechanism the appearance smoke measured at row 159.
- **The fence language must not be one of nvim's seven builtins.** `go` is in O1's sixteen and is *not* one of `c, lua, markdown, markdown_inline, query, vim, vimdoc`. **A `c` or `lua` fence would highlight with nothing vendored at all, and the named falsifier below would leave the smoke green.** This is exactly unit 19's non-falsifier shape and it is the single most likely way this smoke gets silently disarmed by a later edit.

### 5.4 Locating the two rectangles — no magic constant

Reuse `preview_theme_preview_columns()` verbatim, for the reason `link_search_rect`'s doc comment already gives (`:20445-20450`): the two smokes put their preview in the same half by the same `split_right` placement, and duplicating that arithmetic would be two things to keep in step.

1. `web_top = capture.first_uniform_row(preview_columns, preview_theme_scan_rows(), PREVIEW_DARK_BG)` — the existing helper, the existing rectangles, the mechanism measured at row 159 on both launches of the appearance smoke. `None` is a precondition failure with a message naming the two causes (no document, or a first block that starts with full-width ink).
2. A new free function that walks rows from `web_top` to `preview_theme_band_rows().1` and returns **maximal runs of rows in which the count of pixels exactly `CODEHL_PRE_BG` is a strict majority of the sampled width**, discarding runs shorter than `CODEHL_MIN_BLOCK_ROWS` (48, two baselines) so a stray row cannot be a block. The 24px inter-block gap is 100% `#1b1b1b` and 0% `#161616`, so it breaks the run cleanly.
3. Assert **exactly two runs** — a precondition that both fences rendered as `pre` at all.
4. `hl_rect` = preview columns × run 1; `plain_rect` = preview columns × run 2.

### 5.5 The assertion — chroma, not "distinct colours"

"More than one ink colour" (O4) cannot be counted as distinct RGB values: antialiasing produces hundreds of intermediates and the count would be large for a plain block too. The rigorous form of the same claim uses V6:

**Every colour in an unhighlighted block is neutral** (`#e6e6e6` on `#161616`, both r = g = b), and blending two neutral colours channel-independently yields a neutral colour. **Treesitter token colours are chromatic.** So:

- `chroma(pixel) = max(r,g,b) - min(r,g,b)`, computed in `u16` (mirroring `is_link_blue`'s stated reason at `:20351-20353`), never `u8` arithmetic.
- `count_chromatic(rect, min_chroma)` counts pixels whose chroma exceeds `CODEHL_CHROMA_MIN`.

Assertions:

- **Assertion (discriminator):** `count_chromatic(hl_rect) >= CODEHL_MIN_CHROMA_PX`.
- **Assertion (in-capture control):** `count_chromatic(plain_rect) <= CODEHL_MAX_CHROMA_PX`.

All three constants are **MEASURED, not chosen**, in the same run that produces the first green — see §5.7.

### 5.6 Where the defect could hide, and what closes each hole

| Hiding place | Closed by |
|---|---|
| The fence language is a builtin, so the block highlights with nothing vendored | Fixture uses `go`; the fixture comment states the rule and names the seven builtins |
| `CODEHL_CHROMA_MIN` too low → antialiasing alone clears the ceiling | Fires in the **green** run on the plain block. Self-announcing. |
| `CODEHL_CHROMA_MIN` too high → both counts ≈ 0 | Fires in the **green** run on the highlighted block. Self-announcing. |
| **`CODEHL_MIN_CHROMA_PX` set below the plain block's own count**, so a de-highlighted block still clears the floor | A **new compositor-free guard** `codehl_chroma_thresholds_are_separated` asserting `CODEHL_MIN_CHROMA_PX >= CODEHL_MAX_CHROMA_PX * 5`. Label it in `tests/README.md` as a tripwire on a pin with no `src/` falsifier by construction, the shape `the_shared_smoke_config_pins_the_appearance_every_ocr_smoke_assumes` already has (`tests/README.md:226`). |
| Chroma from subpixel antialiasing rather than from the highlighter | Blocks 1 and 2 carry **identical text**, so any such chroma appears in both. If the measured plain count is not at least 5× below the highlighted one, **the pixel assertion is not shippable** — record the negative, do not tune. |
| `render_preview` returned an empty string / the preview never opened | The `tabpagebuflist()` readback precondition (`[Preview: cbhl.md, cbhl.md]` sorted), plus `blocks.len() == 2` |
| The two blocks in the wrong document order | Fires in the green run. Fixture order is fixed and documented. |

### 5.7 The measurement that must precede the constants

Write the smoke **without thresholds first**: `eprintln!` `web_top`, both run extents, both chroma counts at several candidate `CODEHL_CHROMA_MIN` values. Run with `--nocapture`. Only then fill in the three constants, and record every reading in the smoke's assertion table.

### 5.8 The named falsifier

**Delete the environment fallback from the PARSERS probe only** in `runtime/nvim/init.lua` — i.e. restore `local parsers_dir = this_dir .. "/parsers"` with a bare `uv.fs_stat` guard, leaving the queries fallback in place.

Predicted red: `parser/go.so` is not on the runtimepath, `wolf_highlight_code_blocks` answers `false` for the `go` block, both blocks render plain, and the highlighted-block assertion fires with a count at the plain block's level.

**Isolation: yes, and this is not obvious.** All three wolf-ffi treesitter tests stay green under this falsifier, because none of them reaches `init.lua`'s env fallback (V11): two stage real siblings in a temp directory, and the third loads the `.so` by absolute path. State this in the assertion table, or the next reader applies the revert, sees three green FFI tests, and concludes the smoke is measuring something else.

### 5.9 Housekeeping

- `CAGE_LOCK` is taken as the **first statement** of the body, with the poisoning recovery (`unwrap_or_else(|e| e.into_inner())`).
- `clear_evidence` on the PNG and the readback path before spawning; `assert_png_present` then `decode_capture`.
- `cage` is spawned **only** through `cage_command` — `cage_is_only_spawned_through_the_isolating_helper` scans this file's own source.
- **Smoke count bookkeeping**: `crates/wolf-gnome/tests/README.md:66-69` currently says forty-six smokes, thirty-five by OCR, six across two launches, six compositor-free guards, `52 passed`. This unit adds one cage smoke and one guard. **Recount every one of those five numbers from the source — never by arithmetic.** The new smoke is neither OCR nor two-launch, so the "thirty-five" and "six across two launches" figures are expected unchanged; confirm by counting.
- **`ASSERTED_OCR_TERMS` / `RELAUNCH_FIXTURE_NAMES`**: follow the appearance smoke's stated precedent (`:21034-21041`) — it asserts no OCR term and its fixture is read as text by nothing, so it contributes no row to either. Before relying on that, grep `RELAUNCH_FIXTURE_NAMES` for `pvtheme.md`: if it is absent, mirror it exactly; if it is present, register `cbhl.md` the same way.

---

## 6. Area 5 — Row C: the CSS fix and its two pins

### 6.1 The edit

**One declaration**, added to the existing `body.wolf-markdown code` rule at `base.css:393-399`, between `font-size: 0.875em;` (`:396`) and `padding: 2px 4px;` (`:397`):

```
  line-height: 0;
```

That is the whole change. `base.css:139-145` already records the A/B: overriding the code element's `line-height` to 0 gives **24 / 72 / 120** for the paragraph, list and `pre` cases against units of 24 / 72 / 120 — fully on grid. The `font-family: inherit` alternative gives 25 / 73 / 120 and would destroy the monospace face; it is not the fix.

**Why the chip survives** — this is the fact the whole change rests on and the one a later agent will doubt: an inline box's *content area* (what its background paints over) is derived from its font's metrics, not from `line-height`. `line-height` governs only the *line box*. So the `#2b2b2b` chip is still painted and only the union term disappears. This is engine behaviour, so O5 requires it to be **eyeballed**, not reasoned (§6.4).

### 6.2 The declared-value test (the floor)

**File**: `crates/wolf-core/tests/markdown_render.rs`, placed **immediately after `pre_sits_on_the_baseline_grid`** (ends `:1228`) — the shape precedent for a per-rule declared-value assertion.

**Name**: `inline_code_declares_zero_leading_so_it_cannot_grow_a_line_box`.

**Body**: query `rule_decls(&css, "body.wolf-markdown code")` — **not** `"code"` — and assert `decls.get("line-height").and_then(|v| px(v)) == Some(0)`.

Both choices are load-bearing:
- **The full selector is what closes the hiding place.** V8: `"code"` merges the `pre code` rule too, so a future edit that moved the declaration onto `pre code` alone would keep this test green while inline `code` in a paragraph silently regained its 24px box. `"body.wolf-markdown code"` cannot match `body.wolf-markdown pre code`.
- **`px()` rather than string equality**, mirroring `scroll_sync_anchors_stay_zero_sized` (`:2226-2244`), so `0` and `0px` both pass and `normal` fails.

The failure message must state *why* zero: the body declares `line-height` as a LENGTH, an inline box's leading is distributed as half-leading around **that box's own** font metrics, and the line box is the union over the faces present — so a monospace `code` box at any non-zero leading pushes the line past 24px and every later block's offset with it. It must also state that the zero does **not** collapse the chip's background, and that `pre`'s own 24px declaration is what still governs a code block.

**Falsifier**: delete `line-height: 0;` from `body.wolf-markdown code` in `base.css`.
**Predicted failure**: `assert_eq!(…, Some(0))` fails with `None`.
**Isolation**: the grid sweep stays green (`code` is not in `GRID_SELECTORS`), and every other test in the file stays green. Confirm by running the whole target under the falsifier and recording the count.

### 6.3 The measurement (O5) — executable, step by step

**No laid-out assertion may be written before this measurement exists.** The measurement is taken with a **temporary, uncommitted** extension of the Row A smoke: a second `:e` + palette Preview + `grim` leg inside the same `cage` launch, plus `eprintln!`s. No second compositor start.

**Fixture** (temporary file `cgrid.md`): three paragraphs, each one line, each carrying exactly one inline `code` element, followed by `---`. For example: `alfa \`bravo\` charlie.`, blank, `delta \`echo\` foxtrot.`, blank, `golf \`hotel\` india.`, blank, `---`.

**What is captured**: one `grim` PNG of the preview pane showing that document, appearance forced dark.

**What is read out**: `hr_row - web_top`, where
- `web_top` = the first row in `preview_theme_scan_rows()` entirely `PREVIEW_DARK_BG` across `preview_theme_preview_columns()` (the existing helper), and
- `hr_row` = the first row below `web_top` in which the count of pixels exactly `(0x3a, 0x3a, 0x3a)` — the `hr` colour, re-typed from `themes/default.css:63`, never imported — is a strict majority of the sampled columns. The `hr` is full content width and its hairline is the first pixel row of its 24px box (V7), so this is a 1-px-resolution locator.

**The arithmetic to check against**:
- **With the fix**: p1 box 24 at 0, margin 24, p2 at 48, p3 at 96 ending 120, then p3's 24px bottom margin collapses with `hr`'s 24px top margin → `hr` box top at **144**, which is 6 × 24.
- **Without the fix**: each paragraph is 26 (`base.css:139-141`), so p1 0..26, p2 50..76, p3 100..126, collapse 24 → `hr` at **150**, not a multiple of 24.

A 6px separation, one on-grid and one not.

**Acceptance criteria — all three must hold to commit an assertion:**
- **(A) Reproducible.** Identical value across three consecutive runs.
- **(B) On grid.** The green reading is 144 — or, if not, it is some whole multiple of 24 *and* the arithmetic above has been re-derived to explain the difference before anything is committed.
- **(C) Discriminating.** The reading with the one-line CSS edit reverted differs from the green one and is **not** a multiple of 24.

If any of (A)–(C) fails — including "no row matches the `hr` colour exactly", which a fractional device-pixel landing would produce — **record the negative and ship the declared-value pin alone** (O5). The temporary fixture and capture leg are then deleted, not left commented out.

If all three hold, the leg is **promoted to its own smoke** `preview_inline_code_keeps_the_baseline_grid_under_cage` with its own scenario and its own exit codes 11/14, for the same reason §5.1 gives. Its named falsifier is the one-line CSS revert; predicted red is `hr_row - web_top` reading 150 against an expected 144.

### 6.4 The legibility eyeball (O5)

On one retained green-run PNG, at 1:1 or magnified, the coder must look at and record two things:

1. **Is the inline `code` chip's `#2b2b2b` background still painted** around the monospace glyphs, or did `line-height: 0` collapse it to nothing? (§6.1's mechanism predicts it survives; this is the check.)
2. **Do the monospace ascenders or descenders overlap the lines above or below?** A 14px monospace box inside a 24px strut should have room, but the strut is the *body* face's and the glyphs are not.

The answer goes into `base.css`'s header as a MEASURED sentence with the WebKitGTK version, because the arithmetic cannot settle it. A negative answer to either question **blocks the whole of Row C**, declared-value pin included, and must be reported before the docs are written.

---

## 7. Area 6 — Row A / Row C interaction

**What happens to a highlighted `pre` block under the new rule.** `markdown.rs:1039-1096` emits `<span style="color:#…">` with, at most, colour, `font-weight`, `font-style` and `text-decoration` — never `line-height` and never `font-family`. Those spans sit inside `<pre><code>`, so they **inherit the computed `line-height: 0`** from the `code` element (a length inherits as the computed length). `body.wolf-markdown pre code` (`:410-414`) declares no `line-height`, so nothing intervenes. `pre` declares its own `line-height: 24px` (`:403`), and the block container's strut is what governs its line boxes — so each code line is exactly 24px and **a highlighted block is exactly the same height as a plain one**. Highlighting adds no vertical drift, which is why the measured `pre` case goes 123 → 120 with the fix and stays 120 with spans present.

**What a reader must be told, so they do not later "fix" one of the two.** Two prohibitions, both in `base.css`'s header (the one owner), with `markdown/README.md:68` cross-referencing rather than restating:

1. **Do not give the highlight spans, or `pre code`, a `line-height`.** It would reintroduce exactly the union term the `code` zero exists to remove, and it would do so only on lines that happen to carry a highlighted token — a per-token grid error, which is worse than the constant one the fix removed.
2. **Do not delete `code { line-height: 0 }` on the grounds that "`pre` already declares 24px".** The zero is not for `pre`. It is for **inline** `code` in a paragraph or list item, where the block strut is the *body* face's and the union with the monospace box is what overflows. The `pre` case is the one place where the zero is redundant; deleting it there is deleting it everywhere.

---

## 8. File-by-file change list

### Source

| File | Change | Owner-agent |
|---|---|---|
| `flake.nix` | `let`: `wolfTreesitterLanguages`, `wolfTreesitterParsers`, `wolfTreesitterQueries` after `iaWriterMono` (`:183`). `shellHook`: two exports after `:298`. Rewrite the laziness NOTE (`:48-59`) in place. Nothing added to `packages`. | A |
| `runtime/nvim/init.lua` | Add `resolve_vendored_dir`; rewrite the gate at `:233-243` for two probes; textobjects unchanged. Rewrite the block comment at `:210-232` and the `syntax on` comment at `:158-162`. | B |
| `crates/wolf-ffi/tests/common/mod.rs` | `.or_else` arm in `parsers_dir_or_none` (`:175-177`) and `nvim_treesitter_dir_or_none` (`:191-193`); rewrite both doc comments. | C |
| `crates/wolf-core/src/markdown/base.css` | One declaration into the `code` rule (`:396/397`); rewrite the header at `:120-153` (see §10). **No element name in angle brackets anywhere** — `markdown.rs:3271` forbids it. Not prettier-formatted. | D |
| `crates/wolf-core/src/nvim.rs` | Rewrite `render_source_html`'s doc comment at `:1386-1395`: the seven nvim ships is the *fallback* set, and what is on the runtimepath is decided by `init.lua`'s gate, which the macOS bundle and the Linux devShell both satisfy. The clause "a `go` one falls back to plain" is now false. | D |

### Tests

| File | Change | Owner-agent |
|---|---|---|
| `crates/wolf-core/tests/markdown_render.rs` | New `inline_code_declares_zero_leading_so_it_cannot_grow_a_line_box` after `:1228`; rewrite the `code` bullet of `GRID_SELECTORS`' "What is deliberately NOT swept" comment (`:535-542`). | D |
| `crates/wolf-gnome/tests/smoke_headless.rs` | New fixture/colour/threshold constants, `chroma`/`count_chromatic`, the pre-run locator, `CODEHL_SCENARIO`, the smoke, and the `codehl_chroma_thresholds_are_separated` guard. Optionally the Row C smoke, if §6.3 earns it. | E (then F) |

### Untouched, deliberately

`crates/wolf-ffi/BUILD.bazel`, `bazel/nvim-treesitter/*`, `bazel/treesitter-parsers/BUILD.bazel`, `bazel/treesitter-parsers/extensions.bzl`, `MODULE.bazel`, `Cargo.lock`, `architecture.md` (grep confirms `:87`'s only treesitter mention is about bundling the macOS binary tree and stays true).

---

## 9. Test plan and the red runs

### 9.1 New / changed tests, with the exact falsifying edit

| # | Test | Falsifying edit (exact) | Predicted failure | Isolated? |
|---|---|---|---|---|
| T1 | `markdown_render.rs::inline_code_declares_zero_leading_so_it_cannot_grow_a_line_box` | Delete `line-height: 0;` from `body.wolf-markdown code` in `base.css` | `assert_eq!(decls.get("line-height").and_then(px), Some(0))` fails with `None` | **Yes.** `code` is not in `GRID_SELECTORS`; run the whole target and record the count. |
| T2 | `smoke_headless.rs::preview_highlights_a_fenced_code_block_under_cage` | In `runtime/nvim/init.lua`, revert the **parsers** probe to a bare sibling `uv.fs_stat` (leave the queries fallback) | Highlighted-block chroma count collapses to the plain block's level; the discriminator assertion fires naming both counts | **Yes** — all three wolf-ffi treesitter tests stay green (V11). Say so in the table. |
| T3 | `smoke_headless.rs::codehl_chroma_thresholds_are_separated` | Lower `CODEHL_MIN_CHROMA_PX` below `5 × CODEHL_MAX_CHROMA_PX` | The separation assertion fires | Tripwire on a pin; **no `src/` falsifier by construction**, the shape `tests/README.md:226` already labels. |
| T4 | `treesitter_highlight.rs::markdown_treesitter_highlight_and_go_injection_active` (now running on Linux) | In `init.lua`, delete `vim.opt.runtimepath:prepend(parsers_dir)` (the prepend, not the fallback) | Sentinel line 1 stays `OK` (markdown ships with nvim); line 2 becomes `NO`; the injected-`go`-langtree `assert_eq!` fires | **No** — `code_block_highlight` and T2 redden too. Reading: parsers are not reaching nvim. Record the joint red. |
| T5 | `code_block_highlight.rs::code_block_go_highlighted_and_unknown_lang_plain` | Same edit as T4 | `html.contains(STYLED_SPAN_SIGNATURE)` fires on `hl.md` | Shares T4's red. |
| T6 | `preview_inline_code_keeps_the_baseline_grid_under_cage` — **only if §6.3's (A)(B)(C) all hold** | Delete `line-height: 0;` from `body.wolf-markdown code` | `hr_row - web_top` reads 150 against an expected 144 | **No** — T1 reddens too. Reading: a red laid-out smoke over a red declared-value test says the declaration went missing; a red smoke over a *green* T1 says the engine stopped honouring it, which is a very different investigation. Write that pairing down. |

### 9.2 The one behaviour with no falsifier, stated honestly

**The three wolf-ffi tests transitioning from loud-skip to real work is not falsifiable by an assertion.** Removing the `.or_else` arm returns them to a skip, which is a *pass*. Asserting "I did not skip" would require a fail-on-unset mode, which would break Bazel/macOS and is forbidden by O3.

The evidence is therefore a **captured transcript**: `cargo test -p wolf-ffi --test treesitter_parser_abi --test treesitter_highlight --test code_block_highlight -- --nocapture`, before and after the harness edit, showing the SKIP banners present then absent. That transcript is a deliverable of pass 5 and goes into the unit's evidence, exactly as `tests/README.md` records other measured evidence. Do not invent a falsifier for it.

### 9.3 Regression watch (no new tests, mandatory re-runs)

- `crates/wolf-core/tests/save_open.rs::open_existing_markdown` — heading `hl_id` ≠ body `hl_id`. Predicted pass.
- `crates/wolf-core/tests/redraw_grid.rs` — the `H` cell's `hl_id` resolves to a non-`None` fg. Predicted pass.
- The whole `smoke_headless` target, against the pre-change baseline (52 passed). §3.6's five groups predict no movement. **Any mover is a finding to be recorded in `tests/README.md`, never a threshold to retune.**
- `crates/wolf-core/tests/markdown_render.rs` in full, under T1's falsifier and green.

### 9.4 Test wiring

- `crates/wolf-gnome` has **no `BUILD.bazel`**, so the two new smoke `#[test]`s are picked up by `cargo test -p wolf-gnome --test smoke_headless` and need no wiring.
- The new `markdown_render.rs` test is a `#[test]` **added to a file already listed in an existing target's `srcs`**. CLAUDE.md's gap is at the *file* level only, so no wiring is needed. The coder must nonetheless confirm `tests/markdown_render.rs` appears in `crates/wolf-core/BUILD.bazel`'s `srcs` (it must, since the suite is green on macOS) and say so.
- **No new test file is created anywhere**, so no `rust_test` target is added and the `bazelisk`-unavailable constraint is respected.

---

## 10. Documentation ownership

One owner each. R = rewrite in place; A = addition.

| # | Document / site | R/A | What |
|---|---|---|---|
| 1 | `flake.nix` header (`:1-20`) and the laziness NOTE (`:48-59`) | R | Name the two trees among the Linux-only derivations; keep the two prohibitions. |
| 2 | `build.md:23-25` (Linux devShell inventory) | A | The two exports, what they are for, and the **recorded** first-`nix develop` cost (§2.6). One sentence each; `build.md` owns environment. |
| 3 | `runtime/nvim/README.md:15` | R | Sibling-first / environment-second, and the two variable names. |
| 4 | `runtime/nvim/README.md:243` | R | The gate fires wherever the trees resolve; `syntax on` is the fallback where they do not; the `pcall` degradation is unchanged. |
| 5 | `runtime/nvim/README.md:282` | R | **Currently false**: "depend on the vendored sibling trees, which only the macOS bundle stages. Elsewhere `syntax on` is the highlight source and every preview code block renders plain." |
| 6 | `runtime/nvim/README.md:283` | R | Textobjects: still bundled-and-inert, and deliberately has **no** environment fallback, with the reason. |
| 7 | `crates/wolf-ffi/tests/README.md:98-108, 361-371` | R | The three entries: they run on Linux from the devShell; the skip is now only for a host with neither the Bazel staging nor the devShell exports. |
| 8 | **`crates/wolf-core/src/markdown/base.css:120-153`** (O8's primary target) | R | The residual is **fixed** by `code { line-height: 0 }`. Keep the mechanism (half-leading around each box's own metrics, the line box as their union). Replace the "neither override ships … the fix is an open question" close with: the `line-height` override ships; the `font-family` one does not and why; the measured 26/74/123 → 24/72/120; the eyeball result from §6.4; **O6's macOS-unverifiable caveat, stated plainly**; and §7's two prohibitions. Keep "THE DECLARED-VALUE SWEEP STRUCTURALLY CANNOT SEE THIS" — still true — but drop the sentence that makes the residual open. **No element name in angle brackets.** |
| 9 | `crates/wolf-core/src/markdown/README.md:61` | R | The last sentence ("the invariant does not hold there: see the inline-`code` residual below"). |
| 10 | `crates/wolf-core/src/markdown/README.md:62` | R | The closing clause "the residual below is what it costs". |
| 11 | `crates/wolf-core/src/markdown/README.md:68` | R | Add §7's interaction: the spans inherit the zero and must not be given a `line-height`. |
| 12 | `crates/wolf-core/src/markdown/README.md:69` | — | **Inspect and leave.** It covers the table's 1px and intrinsic heights only, and makes no claim this unit falsifies. Say so rather than editing. |
| 13 | `crates/wolf-core/src/markdown/README.md:70` | R | The whole bullet ("The inline-`code` residual accumulates … twelve of sixteen top offsets sit off-grid"). Replace with the fix, the one-line declaration, and O6's caveat; link to `base.css`'s header for the arithmetic. **Net-negative.** |
| 14 | `crates/wolf-core/tests/markdown_render.rs:535-542` | R | O8's stale premise. `code` now carries a genuinely vertical dimension (`line-height: 0`) that is on-grid; it stays out of the sweep because of its 2px padding; the zero is pinned directly by T1. |
| 15 | `crates/wolf-core/tests/README.md:237, 242` | A | One clause in **Covers** for the inline-`code` leading pin; one clause in **Gotchas** for V8 (query the full selector, not `code`, or `pre code` merges in). |
| 16 | `crates/wolf-gnome/tests/README.md:66-69` | R | **Recount all five numbers from source.** |
| 17 | `crates/wolf-gnome/tests/README.md:217` (O8) | R | Two branches. If §6.3 earned an assertion: delete the entry from "What is still unproven" and add the smoke to the Files entry — net-negative. If it did not: rewrite to say the fix ships, the declared value is pinned, and **no laid-out box is measured**, with the recorded reason. |
| 18 | `crates/wolf-gnome/tests/README.md:138, 142` | R | **The credit that pays for #16/#17.** Both restate the counts owned at `:66-69` ("all forty-six smokes", "all thirty-five OCR pages"). This unit changes those counts, so the restatements become three drift sites. Cut both to count-free phrasing ("every smoke", "every OCR page"). Genuine one-owner violation, unit-motivated. |
| 19 | `crates/wolf-gnome/DECISIONS.md` | — | **Nothing is added.** The change is `flake.nix` + `runtime/nvim` + `wolf-core`; the only wolf-gnome fact is the new smoke, which `tests/README.md` owns. So the 49,844-byte ceiling is not pressed. **If room is ever needed**, the genuine violation is the `Present limitations` bullet at `:244`, whose clause "the configured `appearance` is resolved against libadwaita's light/dark reading, pushed into Neovim as `background=dark|light`, matched on the chrome's colour scheme, and carried by that scheme into a preview document's `prefers-color-scheme`" re-narrates the chain `## The preview's light and dark` (`:151-161`) owns in full — and links to it in the same sentence. Cut the restated chain; keep the two Linux-only facts (no OS colour read into Neovim's highlight groups, `macos-option-as-alt` ignored). |
| 20 | `bazel/treesitter-parsers/README.md:11` (R10) | R | "Pinned Neovim 0.12.3 ships only the `markdown` and `markdown_inline` parsers" is measured false — 0.12.2 ships **seven** with `highlights` queries (`c`, `lua`, `markdown`, `markdown_inline`, `query`, `vim`, `vimdoc`). Eighth site of the family units 18 and 19 corrected. |
| 21 | `bazel/treesitter-parsers/README.md` (R10, second) | A | One sentence: macOS pins nvim 0.12.3 (`bazel/nvim/extensions.bzl:28`) while the Linux flake ships 0.12.2, so the two platforms are not on the same Neovim. **Record, do not fix.** |
| 22 | `bazel/treesitter-parsers/extensions.bzl:14` and `crates/wolf-ffi/BUILD.bazel:775, 812, 846` | — | **Left byte-identical (O3).** Four stale comments carried as a named follow-up for a host with Bazel. See caveat 2. |

### `feat-list.md`

**Boxes.**
- **`:151` "Syntax-highlighted code blocks" Linux `[ ]` → `[x]`.** Evidence: `preview_highlights_a_fenced_code_block_under_cage` (raw pixels from the running GTK application, with an in-capture control) **plus** `crates/wolf-ffi/tests/code_block_highlight.rs` now running on Linux.
- **`:184` "Vendored treesitter plugins and parsers" Linux `[ ]` → `[x]`.** Evidence: `treesitter_highlight.rs` (the gate fires, highlighting is genuinely active on a markdown buffer, and the injections query routes a `go` fence) **plus** `treesitter_parser_abi.rs` (the ELF parsers load and parse under the pinned nvim). The first is the editor-appearance half; without it the row would be half-ticked.
- `:182` / `:183` (orlando, lighthouse) stay `[ ]` — out of scope.

**Prose.**
- **`:165` — R.** "the one row still blocked, and on parser coverage" is false. State what delivers parsers on each platform.
- **`:167` — R.** "Unblocking the rest is a flake-side vendoring of ELF parsers" describes future work that has happened. Present tense: what ships, from which nvim-treesitter revision, and that the sixteen are the same set on both platforms.
- **`:190` — R.** "a line carrying an inline `code` element still overflows the 24px unit and shifts every block below it, which is a residual shared with macOS rather than a Linux gap" is false after Row C. Replace with the fix and **O6's caveat, stated plainly**: the rule changes the document macOS renders and there was no macOS host in this unit.
- **`:192` — R. A site O8 does not list; flagging it.** "The three vendored plugin assets are bundled by Bazel into `Wolf.app` only — the captive `init.lua` self-locates them and is a strict no-op when absent, so on Linux they are simply not there." False for two of the three. Rewrite so orlando and lighthouse keep the macOS-only statement while treesitter does not.
- **`:235` — R.** Item 2 of "Suggested order of work": only "Preview in browser" remains; the treesitter prerequisite sentence goes.
- `:157`, `:159`, `:161`, `:163` — inspect, expected unchanged.

### Mechanics for every Markdown edit

`prettier --write` after each file, then `./tools/lint/markdown_check.sh <files>`. Banned changelog vocabulary is binding: no "now", "no longer", "previously", "the old", "this increment". Every rewrite is present tense.

---

## 11. Serialized agent order

Writers are strictly serialized. **Never two agents editing at once. Never a test run while an agent is editing.** Each agent's file set is exclusive.

| Pass | Actor | Files | Action |
|---|---|---|---|
| 1 | Agent A | `flake.nix` | The derivations + exports + NOTE rewrite. Then `nix develop --command true` from a **fresh** shell (direnv is stale after a flake edit — `build.md:27`). Record wall clock and whether the grammars built or substituted. No tests. |
| 2 | Agent B | `runtime/nvim/init.lua` | Helper, gate, both comment rewrites. |
| 3 | **verification** | none | Fresh `nix develop --command cargo test --workspace`. Record: the three FFI tests' banners; the `smoke_headless` count and result against the 52-passed baseline; `save_open`, `redraw_grid`. **This is the §3.6 regression gate; nothing is built on top until it is green or its movers are recorded.** |
| 4 | Agent C | `crates/wolf-ffi/tests/common/mod.rs` | The two `.or_else` arms + doc comments. |
| 5 | **verification** | none | `cargo test -p wolf-ffi -- --nocapture`. Capture the transcript of the three tests (§9.2). |
| 6 | Agent D | `base.css`, `nvim.rs` doc comment, `markdown_render.rs` | The one CSS declaration, T1, and the `GRID_SELECTORS` comment. |
| 7 | **verification** | none | `cargo test -p wolf-core --test markdown_render` green → apply T1's falsifier → red, record the count → restore → green. |
| 8 | Agent E | `crates/wolf-gnome/tests/smoke_headless.rs` | First the **measurement** version (§5.7, `eprintln!` only). Run `-- --nocapture`, record. Then fill in the three constants, add the guard T3. |
| 9 | **verification** | none | The smoke green → T2's falsifier → red, and **confirm the three FFI tests stay green** → restore → green. Recount the smoke totals from source. |
| 10 | Agent F | `crates/wolf-gnome/tests/smoke_headless.rs` (temporary) | §6.3's measurement leg + fixture, uncommitted. Run three times; apply/revert the CSS line; record (A)(B)(C). Do §6.4's eyeball. Then either promote to `preview_inline_code_keeps_the_baseline_grid_under_cage` or delete the leg entirely. |
| 11 | **verification** | none | If promoted: green → T6's falsifier → red → restore → green. |
| 12 | Agent G | every document in §10, one at a time, in table order | Each followed by `prettier --write` + `markdown_check.sh`. |
| 13 | **final gate** | none | `build.md`'s full verification gate: `cargo fmt --all -- --check`, prettier check, `cargo clippy --all-targets`, build, `cargo test`. Confirm `git diff --stat` shows `Cargo.lock` untouched (O7) and no `.bazel`/`.bzl` file touched (O3). |

---

## 12. Dependencies

**None.** No new crate; `Cargo.lock` must not move (O7). If it does, stop and find out why before proceeding.

New Nix inputs are `pkgs.vimPlugins.nvim-treesitter` and its `.parsers.<lang>` attributes, both already in the pinned nixpkgs (`9ae611a455b90cf061d8f332b977e387bda8e1ca`). `flake.lock` must not move either — no new flake input.

---

## 13. Lint considerations

- **No `#[allow]` should be needed.** `crates/wolf-ffi/tests/common/mod.rs` already carries `#![allow(dead_code)]` at `:13`.
- **`smoke_headless.rs` forbids bare `as` casts.** Every new numeric conversion goes through `exact_f64`, `pixel_target`, `pixel_index`, `u32::try_from` or `f64::from`. `band_fraction` is reused for any fraction rather than a fresh division.
- **Chroma arithmetic must widen to `u16`** before subtracting, mirroring `is_link_blue`'s stated reason (`:20351-20353`): a `u8` subtraction can wrap and silently classify a pixel wrongly.
- **New samplers take `SampleRect`, never four loose `usize`s** — `clippy::too_many_arguments`, and the transposition hazard the struct's doc comment names (`:20265-20272`).
- **`cargo clippy --all-targets`** lints `tests/*.rs`, so every new helper must be clean there, not only in `src/`.
- **`base.css` is not prettier-formatted** (`markdown/README.md:3`) and must contain **no element name in angle brackets anywhere, comments included** — `markdown.rs:3271`'s `embedded_stylesheets_carry_no_markup_like_text` enforces it and it is binding on every new comment. Name elements in prose: "the code element", "p", "li".
- **`flake.nix` has no formatter in the gate.** Match the file's existing two-space Nix style by hand.
- **Markdown**: `prettier --write` after every `.md` edit; `.prettierrc` sets `proseWrap: never`, so shortening a paragraph is an authorial act prettier will not perform.

## Status

Plan approved with decisions O1-O13.

### Progress log

**Pass 1 — coder — DONE AND GREEN.** Full suite `963 tests / 94 suites / 52 cage
smokes`, 0 failed — exactly the baseline. §3.6's five groups: no movement.
`save_open::open_existing_markdown` ok, `redraw_grid` 8/8 ok, `markdown_render`
45/45. `cargo fmt --check` and `cargo clippy --workspace --all-targets -D
warnings` both clean. `Cargo.lock`, `Cargo.Bazel.lock`, `flake.lock` all
untouched; no `.bazel` / `.bzl` file touched. §9.2's before/after transcripts
captured (SKIP banners present, then absent with finish times off `0.00s`).
T4/T5's falsifier applied, verbatim red recorded, restored and re-verified by
checksum.

- `flake.nix` — DONE. `wolfTreesitterLanguages` (the 16, alphabetical),
  `wolfTreesitterParsers` (`symlinkJoin`), `wolfTreesitterQueries` (the plugin's
  `runtime`) added to the `let`; `WOLF_NVIM_TREESITTER_DIR` /
  `WOLF_PARSERS_DIR` exported from `linuxShell.shellHook` after `WOLF_RG_BIN`;
  the laziness NOTE rewritten in place. Nothing added to `packages`.
- `runtime/nvim/init.lua` — NOT STARTED.
- `crates/wolf-ffi/tests/common/mod.rs` — NOT STARTED.

### F1. §2.6's first-`nix develop` cost is NOT measurable on this host

Orchestrator ran `nix develop --command true` against the edited flake:
**0.371 s total**, exit 0. That is a WARM-store reading, not a cold one — the
reconnaissance that produced R5 already built or substituted these grammars into
this host's store, so the first-build cost was spent before this unit began.

This does not invalidate the flake edit; it invalidates the *measurement*. §2.6
forbids writing "substitutes" into a document without having watched it, and the
same honesty forbids writing a cold wall clock this host cannot produce. The
coder's §2.6 deliverable is therefore redefined:

- Report the warm reading (0.371 s) as warm, labelled.
- Establish substitutability by dry run rather than by wall clock —
  `nix build --dry-run` / `nix path-info --store https://cache.nixos.org` over
  the two derivations — and report what would be FETCHED versus BUILT for a
  developer with a cold store.
- `build.md` then states the closure cost (R5's 22.6 MiB / 18 paths) and the
  fetch-vs-build answer as measured, and states plainly that no cold wall clock
  was taken. Do not invent one.

**Answered, measured, two independent methods agreeing.** A cold-store developer
**fetches everything and compiles no grammar.** `nix build --dry-run` against an
empty chroot store: `these 66 paths will be fetched (22.2 MiB download, 100.3 MiB
unpacked)`, and the only local build is the `symlinkJoin` itself (`lndir` over 16
inputs, no compiler). `nix path-info --store https://cache.nixos.org` over both
closures agrees: every grammar, the plugin and all transitive deps report FETCH.

**This contradicts §2.6**, which expected the 16 grammars to build locally. R3's
313-local-builds figure was for `withAllGrammars`, a much larger set. The 22.2 MiB
download corroborates R5's 22.6 MiB. `build.md` says **fetched, not built**.

Warm readings, labelled as warm: 1.62 s first, 0.371 s on a consecutive run.

Realised store paths (`$WOLF_PARSERS_DIR` / `$WOLF_NVIM_TREESITTER_DIR`) carry all
16 languages flat under `parser/`, and `queries/markdown/injections.scm` exists —
§4.2's mandatory pre-flight, performed.

### F2. O10's readback flag is a tautology — O10 is AMENDED (see O14)

The coder measured `vim.treesitter.highlighter.active[buf] ~= nil` under three
conditions with `--clean`:

| condition | `highlighter.active[buf] ~= nil` | injected `go` langtree |
| --- | --- | --- |
| bare nvim, `-u NONE`, no wolf `init.lua` at all | **OK** | NO |
| wolf `init.lua`, gate NOT firing (dead env, no siblings) | **OK** | NO |
| wolf `init.lua`, gate firing via devShell env | **OK** | OK |

The pinned Neovim starts treesitter on markdown **by itself**. O10 predicted the
flag would read false when the gate does not fire; it reads true in every case.
Had pass 5 written that assertion, it would have passed under its own named
falsifier — precisely the defect class this pipeline exists to catch.

The plan already contained the contradiction and nobody joined it up: §9.1's T4
row says "Sentinel line 1 stays `OK` (markdown ships with nvim)", and O10 then
proposed that same expression as the smoke's discriminator.

### F3. Two premises this unit inherited are false; pass 8 must not restate them

- **`init.lua`'s `syntax on` comment was already false before this unit** — it
  claimed to be "the highlight source on Linux/cargo" for markdown buffers, which
  the F2 table shows was never true. Corrected in pass 1.
- **§3.6's group-1 mechanism is wrong.** "`syntax on` gives these `markdownH1 →
  htmlH1 → Title`" describes a baseline never in force for markdown buffers. The
  *prediction* (no movement) held anyway, for a different reason: treesitter was
  already highlighting those headings.
- **O3's argument against the wrapper route is weakened, and O3 is NOT reopened.**
  O3 rejected `neovim.override` partly because it "silently drops the
  `FileType=markdown` autocmd — half of row B". Since nvim auto-starts treesitter
  on markdown, that autocmd is redundant for *base* markdown highlighting. What
  the gate genuinely delivers is the two rtp entries: the 16 grammars, and
  nvim-treesitter's `injections.scm` that routes a fenced block to an embedded
  parser. The chosen route is measured working and ships unchanged; only the
  *reasoning* is corrected. Pass 8 must state row B's editor half as **injected
  fenced-language highlighting**, never as "the autocmd turns treesitter on".

---

## Orchestrator decisions after pass 1 — binding, override the plan above

**O14. O10's readback expression is REPLACED. Blocking for pass 5.** The smoke's
Neovim-produced readback must use the **injected-child-langtree** expression —
`crates/wolf-ffi/tests/treesitter_highlight.rs:255-261`'s *second* sentinel
(`p:children()["go"]`, with the `child:lang() == "go"` fallback) — not
`vim.treesitter.highlighter.active[buf]`. F2 measures that expression NO/OK across
the gate, so it is a genuine discriminator. It remains Neovim-produced and
unsatisfiable by any pixel or OCR term, which is what O10 wanted it for.

O10's second named falsifier stands but is **restated against the langtree
expression**: deleting the environment fallback from the **queries** probe stops
the outer gate firing, so neither rtp entry lands, the langtree reads NO, and the
chroma assertion reddens too (R1 — `parsers/` is probed only inside the outer
gate). Both reds must be recorded.

**O15. The sibling-first ordering gets a real test. ACCEPTED from the coder's
recommendation 3.** §3.2 calls sibling-first binding and it is the entire basis of
"macOS behaviour is unchanged **by construction**". This unit introduced that
invariant and left it with no automated guard, on a host that cannot run the macOS
side at all. An unguarded load-bearing invariant that the unit itself created is a
defect the unit owns.

Scope is tight and non-negotiable: **a `#[test]` added to an existing, already-wired
test file — no new file, therefore no `rust_test` target and no Bazel edit** (§9.4,
O3, O9). It must cover both cases the coder verified by hand:

- (a) With decoy `nvim-treesitter/` and `parsers/` siblings staged next to the
  `init.lua` **and** the environment variables also pointing somewhere valid, the
  **siblings** are what land on `runtimepath`, never the environment paths.
- (b) A stale environment path (a store path that has been collected) and no
  siblings degrades to the bare-checkout **no-op** — the gate does not fire, and
  the `WolfTreesitter` augroup does not exist. This is §3.1's stated reason for the
  step-2 `fs_stat`, so far reasoned and hand-checked but unpinned.

Coder's recommendation 4 is folded into (b) rather than given a test of its own:
same helper, same fixture machinery, and splitting it would buy nothing.

**O16. Pass 2 is widened to two files, one agent.** The test-writer takes
`crates/wolf-core/tests/markdown_render.rs` (T1 + the `GRID_SELECTORS` comment) and
O15's ordering test together. Different files, one writer, so O11's "never two
agents editing at once" is respected. T1 is red by construction (the CSS has not
landed); O15's test is green on arrival, so **its failure evidence is owed
separately** — the coder captures it in pass 3 by reordering `resolve_vendored_dir`
to environment-first and recording the verbatim red.

---

## Orchestrator decisions after pass 2 — binding

**Pass 2 — test-writer — DONE.** `markdown_render.rs` gained T1
(`inline_code_declares_zero_leading_so_it_cannot_grow_a_line_box`, after
`pre_sits_on_the_baseline_grid`) and the rewritten `GRID_SELECTORS` bullet.
`crates/wolf-ffi/tests/treesitter_highlight.rs` gained O15's
`vendored_dirs_resolve_sibling_first_and_a_stale_env_path_is_a_no_op`. Wiring
confirmed by reading the targets: `//crates/wolf-core:markdown_render`
(`BUILD.bazel:450`) and `//crates/wolf-ffi:treesitter_highlight`
(`BUILD.bazel:814-839`) both already list their host file in `srcs`, so no Bazel
edit was needed and none was made.

**O17. Three test-writer deviations are ACCEPTED, and one correction stands
against this plan.**

- **The `GRID_SELECTORS` bullet's padding claim is corrected.** §10 row 14 and R7
  say `code` stays out of the sweep because of "its 2px padding"; the
  orchestrator's pass-2 brief made that "2px horizontal padding". Both are wrong
  in the same direction. `padding: 2px 4px` puts **2px on the block edges** and
  4px on the inline ones — a horizontal value would never have been swept at all.
  The shipped bullet says "the 2px block edges". Pass 8 must not re-introduce the
  inverted claim into any document.
- **The augroup observable is `nvim_get_autocmds({event, pattern})` matched on
  `group_name`, not the `E5108` raise** the coder measured. `E5108` is a
  version-dependent contract, and macOS pins nvim 0.12.3 while the flake ships
  0.12.2 (R10) — the version the premise would break on is the one that cannot be
  measured here. Same discriminator, no version premise. Correct call.
- **O15's test spawns the pinned nvim directly rather than through the FFI.**
  `wolf_ffi_start` offers no per-spawn environment, so the FFI route would need
  `std::env::set_var` on the whole test process — which races
  `markdown_treesitter_highlight_and_go_injection_active` **in the same binary**,
  whose helpers read those two variables. The losing outcome is not a flake: case
  (b)'s deliberately-nonexistent value would turn that test into a silent
  loud-skip pass. Spawning with a per-child environment is the only shape that
  does not corrupt its neighbour.

**O18. Two further stale-comment sites, assigned.** Both are the same family as
F3 and O9 — claims that Linux stages no trees and that `syntax on` is its
highlight source.

- `runtime/nvim/init.lua:13-20` (the file header) — **pass 3, coder.** It is a
  source comment in a file pass 1 falsified, and CLAUDE.md makes a false comment a
  bug. §10 lists it nowhere.
- `crates/wolf-ffi/tests/treesitter_highlight.rs`'s `//!` SKIP-when-unset
  paragraph and the two in-test skip banners at `:123` and `:133` — **pass 8,
  technical-writer**, together with §10 row 7's README entry. Pass 2 deliberately
  wrote beside them rather than rewriting; that is a temporary state pass 8 must
  resolve by rewriting in place, never by leaving both.

**O19. The Bazel `size = "small"` risk is recorded, not pre-empted.**
`//crates/wolf-ffi:treesitter_highlight` launches three nvims where it launched
one, plus its 329-directory query copy, under a 60 s cap. Bumping `size` is a
structural `.bazel` edit O3/O9 forbid, and there is no Bazel here to measure the
macOS side. **Pass 3 must record the target's actual Linux wall clock** so the
follow-up carries a number rather than a worry. The existing test measured 0.14 s
including the copy, so the expectation is two orders of magnitude of headroom —
but it stays a prediction until pass 3 prints it.

---

## Orchestrator decisions after pass 3 — binding

**Pass 3 — coder — DONE AND GREEN.** `base.css` carries the one declaration
(`line-height: 0` in `body.wolf-markdown code`); `nvim.rs`'s `render_source_html`
doc comment and `init.lua`'s file header are rewritten in place.

- **T1's red is captured**: `left: None, right: Some(0)` at
  `markdown_render.rs:1252`, target `45 passed; 1 failed`. Isolation confirmed by
  count — `every_vertical_dimension_is_on_the_baseline_grid`,
  `pre_sits_on_the_baseline_grid` and `scroll_sync_anchors_stay_zero_sized` all
  green under the falsifier. After the edit: `46 passed; 0 failed`.
- **O15's two falsifiers each produced a verbatim red on a DIFFERENT assertion** —
  the branch swap at `:719` (case a), the deleted `fs_stat` guard at `:816`
  (case b). That the two reds land on different assertions is what demonstrates
  the single test genuinely covers both halves. Both restored and verified by
  `sha256sum -c`, not by eye.
- **O19 is answered**: the target measures **0.24 s** at `--test-threads=1`,
  reproducible over three runs, with all three nvims launched for real and the
  329-directory copy performed. Against Bazel's 60 s `size = "small"` cap that is
  ~2.4 orders of magnitude of headroom, so the `size` follow-up carries a number
  and is not urgent.
- **Full gate green**: `cargo fmt --check` OK, `cargo clippy --workspace
  --all-targets -D warnings` clean, `cargo test --workspace` **965 passed / 0
  failed / 1 ignored, 94 suites, 52 cage smokes**. Delta `+2 tests, +0 suites, +0
  smokes`, accounted exactly to pass 2's two additions (`markdown_render` 45→46,
  `treesitter_highlight` 1→2). The one `ignored` is the pre-existing
  `regenerate_png_golden` dev aid. **O13: no smoke moved, nothing retuned.**
- `Cargo.lock`, `flake.lock`, `Cargo.Bazel.lock`, `MODULE.bazel.lock` all
  unmoved. No `.bazel` / `.bzl` file modified.
- `cargo fmt --all` reflowed **only** lines pass 2 had added in
  `treesitter_highlight.rs` (pure insertion, 0 deletions);
  `markdown_render.rs` was byte-unchanged.

**O20. A companion declared-value assertion is ACCEPTED — folded into pass 5, its
red into pass 6.** §7's first prohibition ("do not give `pre code`, or the
highlight spans, a `line-height`") is load-bearing and currently protected only by
prose. **T1 structurally cannot see a violation**: it queries the full
`body.wolf-markdown code` selector, so adding `body.wolf-markdown pre code {
line-height: … }` leaves T1 green while reintroducing the union term — as a
*per-token* grid error, which §7 argues is worse than the constant one this unit
removed.

The fix is one `assert!(decls.get("line-height").is_none())` on `rule_decls(&css,
"body.wolf-markdown pre code")`, in `markdown_render.rs`, same shape as T1. Its
failure message must carry §7's reason, not just the expected value. Falsifier:
add a `line-height` to that rule in `base.css`; predicted red is the new
assertion alone, with T1 and the grid sweep both staying green — and that
isolation is the point, so it must be recorded.

Rather than spend two round trips, pass 5's test-writer takes
`markdown_render.rs` alongside `smoke_headless.rs` (one agent, two files, as O16
did), and pass 6's coder captures the red.

**O21. `base.css` currently contradicts itself, and that is a known transient.**
The header at `:139-145` still reads "Neither override ships — they are
measurements, not a fix, and the fix is an open question" while the
`line-height: 0` override ships eleven lines below it. Pass 3 was correctly told
to leave it: §10 row 8 / O8 assign it to pass 8, and its rewrite depends on
measurements pass 4 has not yet taken. **This must not survive the unit.** If
pass 8 is reached without pass 4's §6.4 result, the header is still rewritten —
with the negative recorded — because a self-contradicting file is not a shippable
state.

---

## Pass 4 measurements — MEASURED FACT, the source of truth for pass 5 and pass 8

Host: Linux/NixOS, cargo inside the flake devShell. GTK 4.22.4, libadwaita 1.9.0,
**WebKitGTK 2.52.4**, cage 0.3.0, 1280x720. Tree ended clean, restore verified by
`sha256sum -c`. The macOS half of every claim here is unexercisable on this host
(O6) and stays so.

### M1. Row A geometry and chroma (§5.7) — SHIPPABLE

`preview_theme_preview_columns()` = `(846, 1200)` (354 columns);
`preview_theme_scan_rows()` = `(47, 384)`; **`web_top` = 159**. Exactly two
majority-`#161616` runs detected (§5.4 step 3's precondition holds): run 1 (`go`)
rows 231..327, run 2 (`zzznotalang`) rows 351..447, both 96 rows tall, separated
by a 24-row gap that is 0% `#161616`. Each block rect is 354 x 96 = 33,984 px.

| threshold | block 1 (`go`) | block 2 (`zzznotalang`) |
| --- | --- | --- |
| >= 1 | 984 | **0** |
| >= 8 | 494 | **0** |
| >= 12 | 126 | **0** |
| >= 32 | **95** | **0** |
| >= 64 | 78 | **0** |
| max chroma | 108 | **0** |

**The plain block's chroma is exactly zero at threshold 1 over all 33,984 pixels.**
This host's antialiasing is grayscale and contributes no chroma at all, so §5.6's
"not at least 5x apart, therefore not shippable" clause does not fire — the
separation is unbounded, not merely sufficient.

**Constants, chosen off the curve rather than guessed:** `CODEHL_CHROMA_MIN = 32`
(the knee is between 8 and 12; 32 sits on the flat plateau of 104/95/88 across
24–48 and is 3.4x below the measured max of 108, clear of both self-announcing
failure modes in §5.6). `CODEHL_MIN_CHROMA_PX = 40` (measured 95, 2.4x headroom).
`CODEHL_MAX_CHROMA_PX = 5` (measured 0; T3's `MIN >= MAX * 5` reads 40 >= 25).
`CODEHL_MIN_BLOCK_ROWS = 48` (measured height 96, gap 24).

**Three things pass 5 and pass 8 must not restate wrongly:**

1. The leading paragraph **wraps to two lines**, so block 1 starts at offset 72,
   not 48. `web_top` is still located correctly because the first line box's top 4
   rows are pure background.
2. The `zzznotalang` fence **does render as a `pre` block** — the unknown language
   degrades to the plain render, which is what makes it an in-capture control
   rather than a missing block.
3. The two blocks' **ink masses are NOT equal** (1053 vs 971) despite
   byte-identical text. **Do not write "identical glyph coverage"** anywhere.
   Chromatic ink produces more pixels differing from the background. The *chroma*
   comparison is the sound one; an *ink* comparison would not be.

### M2. O14's readback (B) — works, reads `OK src=2`

The injected-child-langtree expression is producible from the smoke's `writefile`
readback and reads OK in the running application. Pass 4's report carries the
verbatim shell line for pass 5 to copy.

**The source buffer is located through `vim.b[b].wolf_preview_source_bufnr`** (set
by `wolf_ensure_preview_buf`, `runtime/nvim/init.lua:2038`).
**`vim.fn.bufnr('cbhl.md')` is wrong and must not be substituted** — the preview
scratch is named `Preview: cbhl.md`, so that pattern matches two buffers and
`bufnr()` answers -1. Corroborating precondition from the same run:
`tabpagebuflist()` = `"Preview: cbhl.md,cbhl.md"`.

The NO side of this readback is pass 6's, under O14's second falsifier.

### M3. Row C's `hr` A/B (§6.3) — (A)(B)(C) ALL PASS, the smoke is earned

| condition | `hr_row` (abs) | `hr_row - web_top` |
| --- | --- | --- |
| `line-height: 0` present, three consecutive runs | 303 | **144** |
| declaration reverted, two consecutive runs | 309 | **150** |

**(A) Reproducible — PASS** (144 x3, 150 x2; a fourth green run with a completely
different glyph set also read 144). **(B) On grid — PASS**, 144 = 6 x 24, exactly
§6.3's predicted value, no re-derivation needed. **(C) Discriminating — PASS**,
150 / 24 = 6.25.

The feared "no row matches the `hr` colour exactly" failure did **not** occur: the
match is **354 of 354 sampled columns exact** on a single row in every run.

### M4. The legibility eyeball (§6.4) — both answers positive, O5's block does NOT fire

**Chip painted: YES.** Exact-`#2b2b2b` pixels occupy three 20-row bands
(163..182, 211..230, 259..278), each inside its own 24px line box. §6.1's
mechanism holds: the inline box's content area comes from its font metrics, and
`line-height: 0` removed only the union term.

**Overlap: NO.** Re-run with a deliberately hostile fixture (`` `gjpqy` ``,
`` `bdfhkl` ``, `` `Qjy_|[]` ``) to exercise descenders and tall ascenders. Every
line's ink stays strictly inside its own 24px unit; offsets 24..47 and 72..95
carry **zero** ink. The monospace ink is vertically *narrower* than the body
text's, and the monospace face survives visibly intact — the outcome the rejected
`font-family: inherit` alternative would have destroyed.

Row C proceeds, declared-value pin and laid-out smoke both.

---

## Orchestrator decisions after pass 4 — binding

**O22. The chip-survival claim gets a pixel pin. ACCEPTED from pass 4's
recommendation 6.** §6.1 calls the chip's survival "the fact the whole change
rests on and the one a later agent will doubt", and M4 currently rests it on one
coder's eyeball at one WebKitGTK version — an observation that will rot silently
and that pass 8 is about to assert in `base.css`'s header as a measured fact.
Asserting it in a document while leaving it unpinned in the suite is the gap this
unit should not ship.

The marginal cost is near zero: the Row C smoke already captures a document
carrying three inline code elements. Add to it an assertion that a band inside the
first paragraph carries at least a floor of exactly-`#2b2b2b` pixels (measured:
40–50 per row across 20 rows, ~700 total; pick the floor with M1's headroom
discipline, not the measured value itself).

**Its falsifier is distinct from T6's and must be recorded separately:** remove
`background-color` from `body.wolf-markdown code` in `base.css`. Predicted red is
the chip assertion alone, with T6's 144 reading and T1 both unmoved — because
removing a background changes no geometry. That isolation is the point of adding
it, so pass 6 must record it.

---

## Orchestrator decisions after pass 5 — binding

**Pass 5 — test-writer — DONE.** `smoke_headless.rs` gained
`preview_highlights_a_fenced_code_block_under_cage` (9 assertions incl. O14's
langtree readback), `codehl_chroma_thresholds_are_separated` (T3), and
`preview_inline_code_keeps_the_baseline_grid_under_cage` (incl. O22's chip pin).
`markdown_render.rs` gained `pre_code_declares_no_leading_of_its_own` (O20).
Neither file needed a Bazel edit: `crates/wolf-gnome` has **no `BUILD.bazel` at
all**, and `//crates/wolf-core:markdown_render` already lists its host file.

Byte-identity of the two fences is **structural, not coincidental** — both
interpolate one `CODEHL_FENCE_BODY` const. That is better than §5.3 asked for and
must not be "simplified" into two literals.

### Recounted smoke bookkeeping — from source, never by arithmetic (§5.9)

| figure | was | is |
| --- | --- | --- |
| `^#\[test\]` in `smoke_headless.rs` | 52 | **55** |
| `*_under_cage` smokes | forty-six | **48** |
| compositor-free guards | six | **7** |
| target summary line | `52 passed` | **`55 passed`** |
| OCR smokes | 35 | **35, unchanged** |
| two-launch smokes | 6 | **6, unchanged** |
| raw-pixel smokes | 2 | **4** |

`RELAUNCH_FIXTURE_NAMES`: `pvtheme.md` is **absent** with a documented reason, so
§5.9's precedent was mirrored — `cbhl.md` and `cgrid.md` are likewise
unregistered, each carrying that reason. Neither smoke contributes an
`ASSERTED_OCR_TERMS` row.

**O23. Four pass-5 judgement calls are ACCEPTED; three become pass-6
verification duties rather than assumptions.**

- **The O14 readback Lua was RECONSTRUCTED, not copied.** Pass 4's verbatim line
  lived only in its report, not in the repo. Pass 5 rebuilt it from
  `treesitter_highlight.rs:288-297` plus M2's two constraints. **Pass 6 must read
  the readback file's actual content on the first green run** and confirm it
  matches M2's `OK src=2` shape. The assertion tests the first token, so a wording
  drift does not invalidate it — but an unverified reconstruction would.
- **O22's measurement was internally inconsistent and pass 5 caught it.** "40–50
  per row across 20 rows" is 800–1000, not "~700 total". That inconsistency is
  mine, propagated from pass 4's report. `CODEGRID_MIN_CHIP_PX = 120` is set to
  hold under the pessimistic reading (~233 in the single sampled band) with ~5.8x
  headroom under the likely one. **Pass 6 must record the actual count.**
- **The chip band is deliberately WIDER than the `hr` band** — columns 776..1270
  via a new `CODEGRID_CHIP_INSET_X_PX = 10.0`, because
  `preview_theme_preview_columns()`'s 80px inset would bisect a chip that starts a
  few characters into the line. The `hr` row keeps the narrow 846..1200 band,
  because that is where M3's 354-of-354 was measured. Pass 5 argues a wider band
  can only add to a floor. **That argument is not self-evident** — a band reaching
  left of the pane boundary could pick up `#2b2b2b` from the editor half and mask
  a genuine collapse. **O22's falsifier settles it decisively**: pass 6 must report
  the chip count *under* the falsifier as a number, not merely that it reddened.
  A count that stays above 120 with the background removed means the band is
  contaminated and the assertion is measuring the wrong thing.
- **Assertion order** puts chroma before langtree, because §9.1's T2 row predicts
  chroma firing. Accepted as written; no move.
- **The Row A paragraph text is pass 5's, not pass 4's**, so M1's absolute rows
  (231..327, 351..447) quoted in its comments were measured against a different
  fixture. Nothing asserts those offsets, so a green run cannot fail on it — but
  **pass 6 must confirm they still reproduce and correct the comments if they
  shifted**, or the file ships a measured-looking number that was never measured.

**O24. `smoke_headless.rs`'s own module doc comment is added to pass 8's list.**
Pass 5's two smokes and one guard falsify roughly fourteen count-bearing sites in
it — `:14` "Forty-six cage smokes", `:111` "Thirty-five of the forty-six", `:128`
"Two of the forty-six read RAW PIXELS", `:139` "Six pure guards" and its bullet
list (needs a seventh entry), `:201`/`:206`, `:215` "`52 passed`, not `46`", and
others at `:92`, `:258`, `:268`, `:274`, `:291`, `:318`, `:10011`, `:22512`.
§10 lists the README at `:66-69` but not this module doc. CLAUDE.md makes a false
comment a bug, so it is in scope. The recounted table above is the source of
truth. Line numbers cited are pre-insertion above `:21390` — re-locate, do not
trust them.

---

## Pass 6 results and the decisions they force — binding

**Pass 6 — coder — TWO BLOCKERS, correctly not worked around.** Everything else
passed: O20, T6, O22, T2 and O14-2 all produced verbatim reds with their isolation
confirmed, all restored by checksum. Full suite 968 passed / 1 failed / 1 ignored,
94 suites; the delta is exactly pass 5's +4 and **no pre-existing test moved**.

### M1 IS SUPERSEDED — read M5 below instead

M1's chroma table and ink masses were measured against **pass 4's** fence body.
Pass 5 shipped a different one (`package main` / `func f() {}`, matching the FFI
tests), carrying about half the ink. M1's geometry survives; its chroma numbers do
not.

### M5. The shipped fixture, re-derived by pass 6 from the retained capture

| threshold | M1 (pass 4 fixture) | **shipped fixture** | control |
| --- | --- | --- | --- |
| >= 1 | 984 | **529** | 0 |
| >= 8 | 494 | **240** | 0 |
| >= 12 | 126 | **32** | 0 |
| >= 32 | 95 | **22** | 0 |
| >= 64 | 78 | **18** | 0 |
| max chroma | 108 | **108** | 0 |
| ink mass | 1053 | **572** | 545 (M1 said 971) |

**Geometry reproduces exactly** (O23c answered): `web_top` = 159, exactly two runs
at **231..327** and **351..447**, 96 rows each, 24-row gap at 0% `#161616`. Pass
5's geometry comments are correct and need no change.

**O23(a) answered — the O14 readback is VERIFIED.** The retained file contains
exactly `OK src=2` (`cat -A` shows one trailing newline, nothing else), matching
M2. Pass 5's reconstruction of the Lua is sound.

**O23(b) answered — O22's chip count is 748**, not ~700, in the sampled band;
2115 across all three paragraphs; 346 in the narrow band. Against
`CODEGRID_MIN_CHIP_PX = 120` that is **6.2x headroom**. Under O22's falsifier it
reads **1 pixel** — so pass 5's contested wide band is **settled affirmatively**
and is not contaminated by the editor half.

**The chip doc comment's explanation is INVERTED and pass 7 must fix it.** It says
the first and last rows carry fewer pixels because of rounded corners; the
measurement shows the *middle* rows carry fewer (20–30) because glyph ink sits on
top of the chip, while the first and last rows read 40. Per-row range is **20–50**,
not 40–50.

### O25. F-A is fixed by ENRICHING THE FENCE, not by relaxing the guard

The discriminator reads 22 against a 40 floor. T3's `MIN >= MAX * 5` rule with
`CODEHL_MAX_CHROMA_PX = 5` forces `MIN >= 25 > 22`, so the floor cannot simply be
lowered. Of pass 6's two options:

**Enrich the fence body.** Pass 5's "the same two lines the FFI tests use" is
**pass 5's own embellishment — the plan never required it.** §5.3 requires only
that the two fences carry byte-identical text *to each other* and that the
highlighted language not be one of nvim's seven builtins. Nothing is lost by
carrying more Go tokens.

Lowering `CODEHL_MAX_CHROMA_PX` to 2 is rejected. It would leave the
discriminator resting on ~22 pixels — an absolute count small enough that a
font-rendering or WebKitGTK change could move it, which is precisely the fragility
the 5x discipline exists to prevent. A richer fence restores a large margin *and*
keeps the guard intact.

**§5.7's procedure is binding and is not to be short-circuited: no constant may be
written before its measurement exists.** So the enriched fixture is measured
first (pass 6b), and only then are constants filled in (pass 7).

### O26. F-B — T3 must be restructured, not suppressed

`assert!(MIN >= MAX * 5)` on two consts trips `clippy::assertions_on_constants`
under `-D warnings`. The coder correctly refused to add a speculative `#[allow]`.
Pass 7 must reshape it so that all three hold: clippy-clean, **no `#[allow]`**, and
the failure still carries its explanatory message. The precedent guard
`the_shared_smoke_config_pins_the_appearance_every_ocr_smoke_assumes` uses
`assert_eq!` on two consts, which clippy does not fire on. A `const {}` block is
acceptable only if the message survives; a compile error that loses the named test
failure is a downgrade, not a fix.

### O27. Assertion 8 must become provable by panic, not only by retained file

As shipped, **no falsifier can ever produce assertion 8's verbatim red** — every
edit that reddens the langtree readback reddens the chroma discriminator first, and
Rust panics on the first failure. Pass 6 also measured that **T2 produces
`NO src=2` too**, so assertion 8 is not exclusive to the queries probe: an injected
`go` child langtree needs the parser as well as the injections query.

This matters because assertions 6 and 8 are **different feat-list rows** — Row A's
preview colour and Row B's editor-side injections — and §5.1's own argument is that
a shared failure names the wrong one. Row A additionally has
`code_block_highlight.rs` as corroboration; **Row B's editor half has no other
evidence anywhere**, which is the entire reason O14 exists.

Pass 7 must evaluate both readings and report **every** violated claim in one
run — accumulate, then assert once with a combined message — so each claim has
provable text of its own. If this file has no such idiom, reorder so the langtree
assertion is evaluated first and record why in the smoke's comment.

---

## Pass 6b measurements — MEASURED FACT, supersedes M1's chroma numbers entirely

### INCIDENT — `git checkout --` is NOT a safe revert on this branch

Pass 6b ran `git checkout -- crates/wolf-gnome/tests/smoke_headless.rs` to drop
scaffolding and **discarded all 1381 lines of pass 5's uncommitted work.** It was
recovered in full by replaying pass 5's recorded edits and re-running `cargo fmt`;
the result is byte-identical to the pre-run baseline by `sha256sum -c`. No work
was lost.

**Passes 1, 3 and 5 are all uncommitted.** Every later agent must revert
scaffolding by restoring a `cp` baseline taken before editing — never by
`git checkout --`, `git restore`, or `git stash`.

### M6. Go's highlight palette — why enrichment works, and what the threshold really separates

Probed directly against the pinned nvim with the vendored dirs. Go under this
colorscheme has only three chromatic colours: **chroma 108** (function names and
calls, builtin types — `string`, `int`, `len`, `Sprintf`), **chroma 89** (struct
field members), **chroma 67** (string literals). **Everything else — keywords,
numbers, operators, punctuation, user type names, package identifiers — is
`#e0e2ea`, chroma 10**; comments are chroma 9.

**So comments, keywords and numeric constants buy almost nothing.** The plan's own
suggested candidates led on comments and `const` blocks and yielded 528 and 605,
while a function-and-string-dense fence of the same height yields 1209.

This corrects a premise in the constants' documentation: the threshold is **not**
merely guarding against antialiasing. A *highlighted* block's plain tokens sit at
chroma 10, and what `CODEHL_CHROMA_MIN` actually separates is real token colour
from that near-neutral bulk. The histogram shows a **31x cliff** — bucket 10 holds
216 pixels, bucket 11 holds 7.

### M7. The chosen fixture (candidate H, 4 lines) — measured twice, bit-identical

Fence body, verbatim (line 3 begins with a literal TAB):

```
import "fmt"
func Greet(a string, b string) string {
	return fmt.Sprintf("hi %s and %s", a, b)
}
```

| quantity | value |
| --- | --- |
| `web_top` | 159 |
| run 1 (`go`) | **231..375**, height 144 |
| run 2 (`zzznotalang`) | **399..543**, height 144 |
| gap | 24, clean |
| rect | 354 x 144 = **50,976 px** |
| rows below run 2 before the 600 scan limit | **57** |
| chroma >= 32 | **1209** |
| chroma >= 64 | 658 |
| max chroma | 108 |
| ink mass | 2697 (control 2622) |
| **control block, thresholds 1 through 64** | **0 at every one, max chroma 0** |

**Constants, derived not guessed:** `CODEHL_CHROMA_MIN = 32` **unchanged** — the
legal window is [11, 67] (above the chroma-10 spike, at or below the weakest real
token colour); 32 sits 3.2x above the spike and 2.1x below it, near the window's
log-midpoint. Any threshold in [12, 66] leaves a count between 658 and 1544, so no
floor in the low hundreds can be flipped by the choice.
**`CODEHL_MIN_CHROMA_PX = 300`** — measured 1209, **4.0x headroom**, 12x above
T3's minimum of 25, still 2.2x clear at the most pessimistic legal threshold.
Losing any single token, even the 14-column string literal, still clears it;
losing the highlighter entirely reads 0. `CODEHL_MAX_CHROMA_PX = 5` and
`CODEHL_MIN_BLOCK_ROWS = 48` both **unchanged**; block height becomes 144.

**5-line candidates are rejected** despite being legal: they leave only 9 rows of
margin below the scan limit, and one more wrapped paragraph line would clip them.

### O28. `blocks.len() == 2` does not detect a CLIPPED block — a guard is required

**Measured, not theorised.** The 6-line candidate produced **exactly two runs**
with the second silently truncated at the scan limit — 153 rows against run 1's
192, and a control rect of 54,162 px instead of 67,968, so **20% of the control
went unsampled**. The in-capture control assertion would then read stronger than
it is, because a smaller control rect can only lower its chroma count.

Pass 7 must add a guard to assertion 5: the two runs have **equal height**, or
`blocks[1].y1 < block_scan.y1`. Prefer equal height — it also catches a first
block clipped by anything else. Without it, any later fixture growth silently
weakens the control.

---

## Pass 7 + pass 8 — DONE, IMPLEMENTATION COMPLETE AND GREEN

`cargo fmt --all -- --check` clean, `cargo clippy --workspace --all-targets -D
warnings` clean, **`cargo test --workspace` 969 passed / 0 failed / 1 ignored, 94
suites, `smoke_headless` 55 passed.** Delta +4 against pass 3's 965, accounted
exactly to pass 5's four additions. **O13: no pre-existing test moved.** All four
lockfiles unmoved; no `.bazel`/`.bzl`/`.md` touched.

**M7 reproduces exactly against the shipped fixture** — `web_top` 159; runs
231..375 and 399..543, 144 each; chroma >= 32 = **1209** highlighted, **0**
control; chip **748**; `hr_row - web_top` 144 at 354/354 columns; M6's histogram
cliff bucket 10 = 216 vs bucket 11 = 7. Nothing pass 7 wrote is contradicted.

**O26 CONFIRMED by a cache-cold re-lint** — `assert_eq!(shortfall, 0, …)` does not
trip `clippy::assertions_on_constants`, because `required_floor` and `shortfall`
are `let` bindings rather than a literal constant condition. **Pass 7's `fn`-wrapping
fallback must NOT be applied** — it would be a change with no lint behind it.

**The literal TAB CONFIRMED** by `cat -A` on the fixture the running application
actually wrote: `^Ireturn fmt.Sprintf(…)`, four lines byte-for-byte M7's.

### Every falsifier, re-applied alone, all verbatim reds captured

| falsifier | red | isolation |
| --- | --- | --- |
| **T2** | CLAIM 6 **and** CLAIM 8 in one combined message | **O27's payoff** — claim 8 was previously unprovable. All four wolf-ffi treesitter tests stayed green (V11) |
| **O14-2** | byte-identical message, same `NO src=2` | neither falsifier isolates which probe failed — recorded, as pass 7's comment states |
| **T6** | 150 vs 144 | T1 reddens too (the documented pairing) |
| **O22** | chip count **1**, down from 748 | panic at assertion 6 means assertion 5 passed first, so 144 unmoved; `markdown_render` stayed 47/47 |
| **O20** | `pre code` carries `line-height: 24px` | 46 of 47 — T1 and the grid sweep green |
| **O28** | **192 vs 153, run 2 ending at exactly 600** | fires at the equal-height guard, **not** at `blocks.len() == 2`, which passed — O28's premise measured rather than argued |

### F-C. `cp -p` is NOT a safe restore for an `include_str!`'d asset

Pass 8 restored `base.css` with `cp -p`, which preserved the baseline's **older
mtime**, so Cargo reused the cached build with the falsifier still compiled in —
the checksum read OK while two targets stayed red. **Restore with plain `cp` plus
`touch`, and re-run green before believing any restore.** This applies to
`base.css`, `themes/default.css` and every `tests/*.rs`.

### Two open items for the orchestrator, not defects

- **Do not apply pass 7's O26 fallback** (above).
- `preview_inline_code_keeps_the_baseline_grid_under_cage`'s assertion 6 is
  unreachable under T6, because assertion 5 panics first. **Not a defect** — O22's
  falsifier reaches it and gives it its own verbatim red, so nothing is unproven.
  O27's accumulate-then-assert shape would apply here too, at much lower stakes.

---

## Pass 9 — reviewer — VERDICT: needs revision, narrowly. None of it touches behaviour.

Approved as written: the flake derivations, the resolver, the harness fallback, the
CSS declaration, and all eleven new or changed assertions with their recorded reds.
The reviewer independently re-verified O1 (16 languages matching
`bazel/treesitter-parsers/BUILD.bazel:29-44` element for element, `tsx` before
`typescript`), O2, O3's precedence, O14, O15, O17, O20, O22, O23, O25–O28, §3.2,
§13 (zero bare `as` casts, `chroma` widening to `u16`, `SampleRect` samplers), and
that `base.css` contains no element name in angle brackets. It confirmed **M1's
superseded numbers appear nowhere** and found **no further F2-shaped tautologies and
no further F-A-shaped stale constants** in the new tests.

**O29. O9 STANDS AND MUST BE EXECUTED. The orchestrator's brief to the reviewer was
wrong, not O9.** That brief said "no `.bazel` or `.bzl` file may be modified at
all", which contradicts O9's own narrowing of O3: *comment-only edits to
`.bazel`/`.bzl` are permitted and required; structural edits remain forbidden.*
O9's reasoning is unchanged and correct — a comment cannot change build semantics,
so it is verifiable by inspection, and the "no Bazel here to verify with" criterion
does not reach it, while CLAUDE.md's rule that a false comment is a bug does.

So: **correct `crates/wolf-ffi/BUILD.bazel:775, 812, 846`** ("SKIPS when … not
staged (the cargo / Linux path)"), which this unit falsifies. Also correct
`bazel/treesitter-parsers/extensions.bzl:14`'s "Pinned Neovim 0.12.3 ships only the
markdown and markdown_inline parsers" — R10 measured seven, and §10 row 20 already
assigns the identical claim in that package's README, so leaving one and fixing the
other would be incoherent. **Lines 17-19's relocatability claim is separate, still
true, and stays untouched.** Change no target, `srcs`, `deps`, `data`, `env`, tag or
attribute; keep every edited comment inside the surrounding file's line-length
style. Assigned to the technical-writer.

**O30. Two `flake.nix` comments are defects and go back to the coder.**

- **`:382-384` states the thing F3 forbids restating.** It says the gate's absence
  means "the editor loses markdown treesitter highlighting (the gate's
  FileType=markdown autocmd never installs)" — **measured false** by F2. Pass 1
  wrote this, and `runtime/nvim/init.lua:253-266` now carries the corrected version,
  so the unit ships two files contradicting each other on the one fact it spent a
  finding measuring. Restate as: what Linux loses is the two runtimepath entries —
  the vendored grammars and `queries/markdown/injections.scm` — hence **injected
  fenced-language highlighting** in the editor and every non-builtin fence in the
  preview. Base markdown highlighting is unaffected.
- **`:345` says "the forty-six headless end-to-end smoke tests"** — counted from
  source it is **48** (`^fn …_under_cage`), with 55 `#[test]`s. O24 catalogued the
  count-bearing sites *inside* `smoke_headless.rs`; nobody looked outside it.

`:382-384` is **inside the `shellHook` heredoc**, so editing it changes the shell
derivation: re-run `nix develop --command true` from a fresh shell and confirm both
store paths still resolve. `:345` is an ordinary Nix comment and does not.

**O31. The `setenv`/`getenv` race is a real bug this unit introduced. Coder fixes
it.** `crates/wolf-ffi/tests/treesitter_highlight.rs:151` calls
`std::env::set_var("WOLF_NVIM_BIN", …)`. Before this unit that binary held exactly
one `#[test]`, so the write was single-threaded. Pass 2's O15 test calls
`common::nvim_bin_or_skip()` → `std::env::var` on another thread, and `cargo test`
runs them in parallel by default. glibc's `setenv` may `realloc` the `environ`
array, so the failure mode is a **rare use-after-free crash**, not a wrong value —
which is exactly why Rust made `set_var` `unsafe` in edition 2024.

O17's third bullet reasoned carefully about not *adding* a `set_var` and missed
that the file already had one. Fix by ordering every reader after a single write —
a `OnceLock` in `common` that resolves and sets once. **Deleting `:151` is not
available**: on the Bazel path the binary comes from `WOLF_TEST_NVIM_DIR` while
`wolf_ffi_start` reads `WOLF_NVIM_BIN`. Verify at the **default** thread count, not
`--test-threads=1`, which hides this class of bug.

**O32. Two unguarded invariants are RECORDED as follow-ups, not fixed here.**
Neither is a defect the code failed to anticipate; both are honest gaps to name.

- **`WOLF_TEST_*`-first precedence has no test.** On Linux the first arm is always
  `None`; on macOS the second always is, so no single host can exercise the order.
  The obvious lever — `set_var` — is exactly what O31 forbids.
- **The `flake.nix` ↔ `BUILD.bazel` language-list parity (O1) is guarded only by a
  comment.** O1 calls a divergence "the bug, not a preference", and nothing on
  either side can see a one-sided edit. No host here can run Bazel, so no test is
  proposed — it belongs beside O19's `size = "small"` note in the unit's recorded
  follow-ups.

**O33. Pass 10's document list gains six sites the reviewer located.** Beyond §10's
table, O12, O18, O21 and O24:

- `crates/wolf-ffi/tests/code_block_highlight.rs:17, 231-232, 239-240` and
  `treesitter_parser_abi.rs`'s `//!` — the identical "Bazel/macOS-only" banners O18
  named in one file. Fixing only the file O18 listed leaves two of R9's three tests
  lying.
- `crates/wolf-ffi/tests/treesitter_highlight.rs:13-23` — "two facts, one sentinel"
  and the `highlighter.active` claim. **F2 falsified it**, and
  `smoke_headless.rs:21975-21981` now says so, so two files state opposite things
  about one expression. Also `:33-34`'s "strict no-op / `syntax on` stays the
  highlight source".
- `crates/wolf-ffi/tests/treesitter_highlight.rs:321-328` — the assertion is
  harmless but its message ("NO here means start() did not attach") tells the reader
  to investigate something F2 proves cannot happen. Relabel it as the precondition
  it actually is.
- `crates/wolf-core/DECISIONS.md:119` — the **Deferred** entry's parenthetical
  "(treesitter grammars, `marksman` LSP, formatter)". The `wrapNeovim` clause stays
  true (O3 rejected that route); **narrow the parenthetical, do not delete the
  entry.**
- `smoke_headless.rs:18631-18632` — `write_relaunch_fixtures` claims to write "the
  `RELAUNCH_FIXTURE_NAMES` markdown fixtures a smoke owns", which `cbhl.md` /
  `cgrid.md` are not. Pre-existing drift this unit widens; one clause.
- **O12 is ANSWERED, so pass 10 need not guess**: `crates/wolf-core/src/README.md`
  carries no "seven parsers" and no "Linux renders code blocks plain" claim, and
  `crates/wolf-core/README.md` mentions neither treesitter nor parsers. **Nothing to
  change in either.** `markdown/README.md:61, 62, 70` are confirmed still false;
  `:69` makes no claim this unit falsifies, so §10 row 12's "inspect and leave"
  verdict is correct.

**O24's figures, re-counted from source by the reviewer:** 48 cage smokes, 55
`#[test]`s, 7 compositor-free guards, 4 raw-pixel smokes; **35 OCR and 6 two-launch
unchanged**. Stale "forty-six" at `smoke_headless.rs:14, 111, 128, 215, 258, 268,
274, 291, 318, 10011, 11152, 11943, 24062`; `:139`'s bullet list needs a seventh
entry; `:215`'s "`52 passed`" → 55. **The "thirty-five" occurrences at `:111, 10163,
10175, 10220, 11152` are CORRECT and must not be touched.**

---

## Pass 10 — coder — DONE. All three review defects fixed; gate green, zero delta.

`cargo fmt --check`, `prettier --check`, `cargo clippy --workspace --all-targets -D
warnings` all clean; `cargo test --workspace` **969 passed / 0 failed / 1 ignored,
94 suites, `smoke_headless` 55 passed** — identical to pass 8, so **O13 has nothing
to record.** `git status --porcelain` shows exactly the eight changed files plus the
plan; all four lockfiles unmoved; no `.bazel`/`.bzl` and no `.md` touched.

- **O30 `:382-395`** rewritten to state the loss as the two runtimepath entries and
  injected fenced-language highlighting, naming `highlighter.active[buf]` as the
  expression that *cannot* witness the gate. It agrees with `init.lua:253-266` now
  rather than contradicting it. **Store paths re-verified byte-identical after the
  heredoc edit** (`p9gsp1r5…-wolf-treesitter-parsers`,
  `kb3v6n2y…-nvim-treesitter…/runtime`).
- **O30 `:345`** — "forty-six" → forty-eight, recounted from source (48 `_under_cage`
  fns, all top-level and each directly preceded by `#[test]`; 55 `#[test]` total).
- **O31** — `nvim_bin_or_skip` split into a side-effect-free `resolve_nvim_bin()`
  and a `OnceLock`-cached `resolve_and_publish_nvim_bin()`. The publish is
  **conditional**, so the cargo/devShell path performs **zero** `setenv` calls and
  the Bazel path exactly one, inside the once-init that every caller happens-after.
  The loud skip banner deliberately stays outside the `OnceLock` so no binary's skip
  visibility changes. The Bazel path was **demonstrated preserved, not argued**: a
  staged `fake_nvim_tree` run under `env -u WOLF_NVIM_BIN WOLF_TEST_NVIM_DIR=…` is
  green with the fix and reddens verbatim without it.

### F-D. The reviewer's use-after-free claim is OVERSTATED — record it accurately

On the pinned rustc 1.96.0, `std::env::{var, vars, set_var, remove_var}` all
serialise on `ENV_LOCK` (`library/std/src/sys/env/unix.rs:67`, `getenv` at `:113`,
`setenv` at `:132`). A Rust-side `std::env::var` **cannot** observe a torn `environ`
from a Rust-side `set_var`. The residual exposure is narrower and real: **C-side
`getenv` takes no such lock**, which is precisely why edition 2024 marks `set_var`
`unsafe`.

**Do not record "a latent use-after-free was fixed" as measured fact.** What was
fixed is an unsynchronised-against-C write and an implicit ordering. The fix stands
on its own merits regardless — it removes an unnecessary write entirely on Linux,
makes the ordering explicit rather than incidental to a std implementation detail,
and is the shape edition 2024 will require.

### O34. Two more follow-ups recorded, neither fixed here

- **Thirty redundant `set_var("WOLF_NVIM_BIN", …)` calls in eight other wolf-ffi
  binaries** — `preview.rs` (9), `untitled.rs` (7), `edit.rs` (6), and
  `cmdline_ui.rs`, `file_search.rs`, `full_width.rs`, `history.rs`, `terminal.rs`
  (2 each). With the helper publishing, each is a no-op write that still races its
  neighbours' reads. Pure plumbing, no assertion touched — but pre-existing and out
  of this unit's scope.
- **O32's first gap is now partly closable.** The simulated-Bazel technique pass 10
  used (running the built binary under `env -u WOLF_NVIM_BIN WOLF_TEST_NVIM_DIR=…`)
  exercises the precedence arm on a Linux host **with no process-global mutation**,
  which is what made it untestable before. Non-trivial to wire under Bazel; recorded
  as an option, not a request.

---

## UNIT 20 — COMPLETE. Passes 11, 12 and 13 done.

**Pass 11 (technical-writer)** rewrote 17 files: 10 Markdown documents, `base.css`'s
header (O21's self-contradiction), two Bazel files comment-only (O29), and four test
files' doc comments. Net-negative at `markdown/README.md:70`,
`wolf-gnome/tests/README.md`, `feat-list.md:235`, `DECISIONS.md:119` and
`bazel/treesitter-parsers/README.md:7`. It confirmed §10 rows 1, 12, 14, 19 and O12
already correct and left them alone, and found five extra drift sites §10 missed.

**Pass 12 (coder)** ran the mechanics pass 11 could not. `prettier` reformatted
**nothing**. `markdown_check.sh` raised 7 findings — all introduced by pass 11,
confirmed by running the same check against `HEAD` as a control (`No findings`
there) — comprising four entry-budget overruns and three long lines, each resolved
by **authorial shortening**, not restructuring. It also fixed the four comment
defects pass 11 correctly declined to edit, including `init.lua:291-296`'s
F3-forbidden framing that contradicted `:253-266` of its own file, and deleted
`treesitter_parser_abi.rs`'s 11-line "Coder wiring (NOT done here)" section
instructing a coder to add a target that already exists.

**The Bazel diff was verified mechanically, not by eye** — both files tokenized and
compared against `HEAD` with comments, newlines and indentation discarded.
`crates/wolf-ffi/BUILD.bazel`'s token stream is **identical**;
`bazel/treesitter-parsers/extensions.bzl`'s only differing token is the module
docstring. O29's comment-only constraint is satisfied, and that is the strongest
check available on a host with no Bazel.

**Pass 13 (technical-writer)** closed the last two: a checkable count that pass 11
had hedged into "Several" is restored as **Twelve**, recounted from source with its
counting rule written into the sentence (48 = 35 OCR + 12 readback-only + 1 that
uses neither — `colon_qa_closes_the_window_under_cage`, whose evidence is the exit
status). And `extensions.bzl`'s seven-parser measurement is now attributed to the
0.12.2 it was taken on rather than this package's 0.12.3 pin.

### Final state — verified

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -D warnings`,
`prettier --check`, `./tools/lint/markdown_check.sh` — all clean.
**`cargo test --workspace`: 969 passed / 0 failed / 1 ignored, 94 suites,
`smoke_headless` 55 passed** — identical across passes 8, 10 and 12, so **O13 has
nothing to record: no pre-existing test moved at any point in this unit.**

23 files changed, 4515 insertions / 1230 deletions. **All four lockfiles unmoved**
(O7, §12). The only `.bazel`/`.bzl` changes are the comment corrections O29
requires.

### Not verifiable on this host, and stated as such

No Bazel and no macOS. `bazelisk test //...`, the Swift `WolfKeyMapTests` target,
and the Mach-O parser path were never run. `treesitter_parser_abi` proved ABI
compatibility against **0.12.2** here, not the **0.12.3** the bundle pins. Row C
changes the document macOS renders and no macOS host saw it (O6). All of this is
recorded in `macos-followups.md`, alongside O19's `size = "small"` headroom (0.24 s
against a 60 s cap), O32's two unguarded invariants, and O34's redundant `set_var`
calls.

### Left deliberately undone

Two test recommendations from pass 12, both out of this unit's scope and neither a
defect: a compositor-free guard that would count `_under_cage` functions and
`ocr_to_lowercase` callers from `file!()`'s own source (this unit found one of six
count sites wrong by hand); and a span-level assertion for §7's second prohibition,
which needs `render_html_with_highlights` output rather than `base.css` text, so
`base.css:166` currently promises coverage that half exists.

**The branch is uncommitted. Nothing has been committed or merged.**
