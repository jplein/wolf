# Plan: Enable treesitter highlighting + injections in captive Neovim (macOS)

## Status: COMPLETE

### Technical-writer (done)
Fixed all 6 reviewer items + stragglers (MODULE.bazel parsers block, wolf-macos BUILD.bazel/bundle_layout_test headers, treesitter-parsers swift note, init.lua syntax-on comment, treesitter_parser_abi.rs doc comment, decision-report problem statement). Reconciled lockfile-source truthfully (parsers = master-era lockfile, not re-synced to main; re-sync noted as future nicety; pairing proven working). All comment/prose only — no logic/assertion/glob changes. Orchestrator ran prettier on all 6 touched markdown files — all clean. Full test suite (run by coder pre-doc-fixes) remains valid since only comments changed.

### Reviewer: behavior/gate/pins/tests all PASS. Doc/impl mismatches (CLAUDE.md = bug) to fix:
1. HIGH — `runtime/nvim/init.lua` lines 14 + 117-118: "master queries"/"master-branch" → main.
2. HIGH — `MODULE.bazel` lines 123-126: "frozen master branch" + "highlighting is NOT enabled" → main + enabled-on-macOS.
3. MED — `bazel/nvim-treesitter/BUILD.bazel` 5-6 ("does NOT enable highlighting" — contradicts its README) → enabled; `crates/wolf-ffi/tests/common/mod.rs` 179 ("master queries") → main.
4. SUBSTANTIVE (reconcile truthfully) — parser grammar revs were pinned in the ORIGINAL vendoring when nvim-treesitter was MASTER and were NOT re-derived after the main switch. So `bazel/treesitter-parsers/{README.md,extensions.bzl}` saying "master lockfile.json" is TRUE; `build.md` (~133) + `treesitter-decision-report.md` (~48) saying "main lockfile" is the OVERCLAIM (finalize coder over-corrected). FIX build.md/decision-report to state the truth: grammar revisions come from nvim-treesitter's lockfile.json (master-era; parsers not re-synced to main). Add a one-line note that re-deriving parser revs from main's lockfile is a future nicety — the current set is proven working (treesitter_highlight injection + treesitter_parser_abi pass), and tree-sitter grammar ABIs are stable, so the queries(main)⇄parsers(master-era) pairing is fine.
5. LOW — remaining "does NOT enable highlighting / init.lua not modified / deferred" in treesitter-parsers component (`README.md` 7, `BUILD.bazel` 8, `extensions.bzl` 21 & 196, `parsers_tree_test.sh` 24) and `bazel/nvim-treesitter/nvim_treesitter_tree_test.sh` 18-19 → update to reflect enablement. (textobjects "inert/does NOT enable textobjects" statements stay — still accurate.)
NOTE: items in `.rs`/`*_test.sh` are COMMENT/header-doc edits only (no assertion/logic changes) — safe for technical-writer.

### Finalize outcome (done)
Both plugins on main; injection WORKS. `//crates/wolf-ffi:treesitter_highlight` PASSED (line1 highlighter active + line2 go injection both OK). All tree tests, treesitter_parser_abi, orlando/lighthouse_load, bridge_smoke, bundle_layout_test (checks h/i/j; (i) updated to main layout `nvim-treesitter-textobjects/lua/nvim-treesitter-textobjects`), Wolf build — all PASS. No regression (gate no-op confirmed: frame_pack/mouse_scroll/panes/file_search/tree_search unchanged). Lockfile stable. Docs finalized (READMEs, build.md, decision-report, init.lua header) reflecting main + injection ENABLED. Stale textobjects BUILD.bazel comment + nvim-treesitter extensions.bzl master/main self-contradiction fixed. Test hardened (pcall-wrapped parse).

### DECISION REVERSAL: nvim-treesitter master → main (verified by spike)
master's markdown injections.scm uses `#set-lang-from-info-string!`, whose frozen-master handler targets a pre-0.10 query-match API nvim 0.12.3 removed → `E5108` on every fenced-block redraw (injection broken). SPIKE confirmed **main** fixes it: main's `queries/markdown/injections.scm` is pure core-native capture-based injection (no custom predicate); works from queries-on-rtp ALONE (no :TSInstall, no plugin Lua sourced, NO init.lua change). Both `//crates/wolf-ffi:treesitter_highlight` (line1+line2 OK) and a headless repro pass. User asked for main; adopting it.

Spike pin (IN PLACE, keep): nvim-treesitter main `4916d6592ede8c07973490d9322f187e07dfefac`, sha256 `4e720e38f1a74c07fc8bc9d6264e25ab620df93c6576200f43f2ca0ce5fbdec1`. Overlay remaps `runtime/queries` → `nvim-treesitter/queries` (main moved queries under runtime/); lua/ + plugin/ still present; tree test passes unchanged. MODULE.bazel.lock regenerated.

### init.lua block (coder, DONE — keep): gate + rtp prepends + pcall-wrapped vim.treesitter.start + FileType=markdown autocmd + emit-once. Header comment reworded. NO other behavior changed. Highlighting active; with main, injection now renders.

### REMAINING FINALIZE WORK
1. Re-pin nvim-treesitter-textobjects master → main (consistency with nvim-treesitter main; textobjects stays INERT — inert branch doesn't affect current behavior but keep both plugins same-branch per the earlier correction's principle). Investigate main textobjects query/lua layout (likely moved to runtime/queries/ too); adjust overlay globs; regen lock. Report layout → route tree-test assertion changes to test-writer.
2. test-writer: wrap the sentinel `p:parse(true)` in pcall in treesitter_highlight.rs so a FUTURE injection regression surfaces loudly (empty sentinel) instead of masking. + update textobjects tree test if main layout moved.
3. Finalize docs (README enablement bullet + Deferred; build.md active-in-.app + new test target; decision-report follow-up DONE on macOS) reflecting main + injection ENABLED.
4. Full verification suite (no regressions — gate property).

### Test-writer output (done)
Created `crates/wolf-ffi/tests/treesitter_highlight.rs` (fn `markdown_treesitter_highlight_and_go_injection_active`). Modified `crates/wolf-ffi/tests/common/mod.rs` (added `nvim_treesitter_dir_or_none()` → `resolve_runfile("WOLF_TEST_NVIM_TREESITTER_DIR")`). Sentinel Lua uses `highlighter.active[b]~=nil` (line1) + `p:parse(true)` then `p:children()["go"]` with pairs()+`child:lang()=='go'` fallback (line2). Coder must: wire BUILD env/data to match (WOLF_TEST_NVIM_TREESITTER_DIR→//bazel/nvim-treesitter:nvim_treesitter_tree, WOLF_TEST_PARSERS_DIR→//bazel/treesitter-parsers:parsers_tree); ensure init.lua block registers a FileType=markdown autocmd (not just emit-once) so `:e doc.md` lights up; confirm `p:parse(true)` + `:children()` keying for nvim 0.12.3; watch line1=NO = query-loader incompat (flag, don't paper over).

## Problem statement

Enable treesitter highlighting + fenced-code-block injection in `runtime/nvim/init.lua`,
using the already-vendored parsers + nvim-treesitter (master) queries. **Scope:
highlighting + injections ONLY.** Do NOT wire nvim-treesitter-textobjects keymaps
(stays bundled-but-inert). Follow-up to the completed vendoring change.

## CRITICAL SAFETY PROPERTY (must preserve)

Treesitter enablement MUST be gated on the vendored sibling dirs existing next to
init.lua (exactly like the orlando/lighthouse blocks at init.lua lines 64-107:
self-locate `this_dir` via `debug.getinfo`, guard with `uv.fs_stat`). On Linux/cargo
and in any existing test that does NOT stage the vendored dirs, the block is a STRICT
NO-OP — treesitter stays off, `syntax on` remains the highlight source, and NO existing
render/redraw/grid test changes behavior. Only the macOS `.app` bundle (siblings present)
and NEW tests that stage the dirs see treesitter. Reuse the existing `this_dir`/`uv` locals.

## What to implement (init.lua — new conditional block, orlando/lighthouse style)

- Self-locate sibling dirs: `parsers/` (Contents/Resources/parsers/, has parser/<lang>.so),
  `nvim-treesitter/` (master queries: queries/<lang>/highlights.scm + queries/markdown/injections.scm),
  `nvim-treesitter-textobjects/` (present but NOT activated).
- GATE on the nvim-treesitter queries dir existing. When present: prepend the vendored
  dirs that exist (parsers/, nvim-treesitter/, textobjects/) to runtimepath. markdown/
  markdown_inline parsers ship with nvim (don't re-vendor); embedded parsers come from parsers/.
- FileType=markdown autocmd → `vim.treesitter.start()` (+ emit-once for already-open markdown
  buffers). Lights markdown highlighting AND routes fenced blocks via nvim-treesitter's
  markdown injections.scm on rtp.
- Document interaction with `syntax on` (line 62) + lighthouse (102-107): treesitter layers
  via its own namespace; keep syntax on as non-treesitter fallback. Verify ordering vs
  lighthouse; note if lighthouse lacks @capture links (acceptable, colors fall back to default links).
- Change NO other init.lua behavior.
- Reword init.lua header comment (lines 13-16) which currently says treesitter is DEFERRED
  (would become a doc/impl mismatch = bug per CLAUDE.md).

## Tests (no mocks; real pinned nvim)

- New real-nvim test: stage nvim-treesitter tree + parsers tree (+opt textobjects) as siblings
  of a temp init.lua (copy real init.lua in — orlando/lighthouse load-test staging pattern),
  start editor, open markdown buffer with a fenced ```go``` block, assert treesitter is ACTIVE
  (`vim.treesitter.highlighter.active[buf]` set) AND injection works (injected 'go' langtree
  present at a pos inside the block — prefer langtree presence over specific hl group names).
  Skip loudly off-macOS / trees unstaged.
- BUILD target in crates/wolf-ffi/BUILD.bazel staging //bazel/nvim-treesitter:nvim_treesitter_tree
  + //bazel/treesitter-parsers:parsers_tree + //runtime/nvim:init.lua (data + $(rootpath) env),
  tags no-sandbox/local. Add common/mod.rs helper(s) as needed.
- VERIFY existing tests unchanged (gating property). If any grid assertion shifts → the gate
  leaked → FIX THE GATE, do not edit existing test expectations without flagging.

## Docs (source of truth; prettier --write)

- runtime/nvim/README.md: new enablement bullet; update Deferred (highlighting now ENABLED on
  macOS; still deferred = Linux/Nix path + textobjects keymaps + Markdown LSP/formatter).
- build.md: treesitter now active in .app; add new test target.
- treesitter-decision-report.md: "Consequences and follow-up" enablement now DONE on macOS;
  Linux/Nix still deferred.

## Invariants

- wolf-core GUI-free; hermetic (no runtime fetch/compile); init.lua strict no-op off-bundle;
  no Nix/flake/Linux-vendoring changes.

---

## Architect's plan

### Confirmed layout facts
- Parsers rtp prepend = `this_dir .. "/parsers"` (tree lands `parsers/parser/<lang>.so`; nvim resolves `<rtp>/parser/<lang>.so`). Matches treesitter_parser_abi.rs staging.
- Queries rtp prepend = `this_dir .. "/nvim-treesitter"` (inner `nvim-treesitter/queries/<lang>/{highlights,injections}.scm`).
- Textobjects = `this_dir .. "/nvim-treesitter-textobjects"` — prepend if present but do NOT source/setup (inert; only ships textobjects.scm which start() never reads).
- markdown/markdown_inline parsers ship with nvim (don't re-vendor).
- **GATE**: `uv.fs_stat(this_dir.."/nvim-treesitter")` (queries provider). Inside, prepend parsers/ + textobjects/ each only if it independently fs_stats. Absent gate dir → zero rtp mutation, zero autocmd, no error.
- **Startup**: lean core only — FileType=markdown autocmd → `vim.treesitter.start(buf)` + emit-once over already-loaded markdown bufs. Do NOT call nvim-treesitter's `.setup{}` (heavy :TSInstall machinery, non-deterministic under --clean; use vendored plugin purely as on-rtp query provider).
- **syntax on**: leave as-is (non-treesitter fallback; existing tests depend on it; treesitter layers via own namespace). Place new block AFTER lighthouse (line 107). Wrap `vim.treesitter.start` in pcall so query-loader incompat degrades to syntax-on rather than raising.

### init.lua edits
- **A. Header comment (lines 13-16)**: reword — treesitter highlighting+injections now conditionally enabled when vendored nvim-treesitter/ + parsers/ siblings present (macOS .app); strict no-op on Linux/cargo (syntax on remains); LSP/formatter still deferred; textobjects inert.
- **B. New conditional block after line 107** (before `_G.wolf_setup_current_file_tracking`): REUSE existing `this_dir`/`uv`. Gate on nvim-treesitter dir; prepend nvim-treesitter/ (+ parsers/, textobjects/ if present); local pcall-wrapped `vim.treesitter.start(buf)` helper; `WolfTreesitter` augroup (clear=true) + FileType=markdown autocmd → helper(args.buf); emit-once loop over `nvim_list_bufs()` loaded markdown bufs. Block-leading comment in orlando/lighthouse style. Change NO other behavior.

### New test: crates/wolf-ffi/tests/treesitter_highlight.rs
Real-nvim, modeled on orlando_load/treesitter_parser_abi. Stages TWO siblings next to temp init.lua: `nvim-treesitter/` (gate+queries) + `parsers/` (go parser). SKIP guards: nvim_bin_or_skip, new `nvim_treesitter_dir_or_none`, `parsers_dir_or_none`. Stage: copy init.lua→T/init.lua; copy nvim-treesitter inner dir (defensive nested-vs-flat: `.join("nvim-treesitter")` if dir else itself)→T/nvim-treesitter; copy parsers inner→T/parsers (assert T/parsers/parser/go.so + T/nvim-treesitter/queries/markdown/injections.scm pre-launch). Write T/doc.md with a fenced ```go``` block (package main). Launch wolf_ffi_start(T/init.lua, T, ...), poll cols()>0, `:e T/doc.md` (triggers FileType markdown → start). Sentinel Lua (2 lines): line1 `vim.treesitter.highlighter.active[b]~=nil` → OK/NO; line2 after `pcall(get_parser,b)` + `p:parse(true)`, `p:children()["go"]~=nil` (fallback: iterate children, `child:lang()=='go'`) → OK/NO. Poll+assert both OK. shutdown.

### Test harness/build edits
- common/mod.rs: add `nvim_treesitter_dir_or_none()` → `resolve_runfile("WOLF_TEST_NVIM_TREESITTER_DIR")` (match *_or_none convention).
- crates/wolf-ffi/BUILD.bazel: `rust_test treesitter_highlight` — srcs [common/mod.rs, tests/treesitter_highlight.rs], crate_root, tags no-sandbox, size small. data: //bazel/nvim:nvim_tree, //bazel/nvim-treesitter:nvim_treesitter_tree, //bazel/treesitter-parsers:parsers_tree, //runtime/nvim:init.lua. env: WOLF_TEST_INIT_LUA, WOLF_TEST_NVIM_DIR, WOLF_TEST_NVIM_TREESITTER_DIR, WOLF_TEST_PARSERS_DIR. deps [":wolf_ffi"]+all_crate_deps(normal_dev=True).

### Docs
runtime/nvim/README.md (new enablement bullet + Deferred update), build.md (active in .app + new test target), treesitter-decision-report.md (Consequences enablement DONE on macOS, Linux/Nix deferred). prettier --write.

### Impl order
1. common helper. 2. init.lua header+block. 3. test file. 4. BUILD target. 5. docs+prettier.

### Existing-test safety (gate property)
redraw_grid.rs (asserts fg.is_some() — robust), redraw_multigrid.rs, bridge_smoke, other wolf-ffi real-nvim tests, wolf-gnome paint_golden — all stage ONLY nvim_tree + init.lua, NOT the vendored siblings → gate fs_stat absent → strict no-op → assertions unchanged. VERIFY paint_golden BUILD doesn't stage siblings. If ANY grid/golden shifts → gate LEAKED → fix gate, do NOT re-bless expectations without flagging.

### Flagged uncertainties
1. **Query-loader compat (primary risk)**: nvim-treesitter master queries may use a predicate pinned nvim 0.12.3 rejects → start() raises (pcall degrades in prod; test line1=NO surfaces it loudly). If fires: it's a vendoring-pin decision, flag — don't silently work around.
2. **Async injection**: `p:parse(true)` forces full parse incl injections before inspecting children.
3. Defensive nested-vs-flat staging + pre-launch asserts catch mis-staging.
4. `:children()` keyed by lang name; fallback iterate + `child:lang()`.
5. Verify paint_golden staging (don't assume).
