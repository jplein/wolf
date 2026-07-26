# Plan: Treesitter syntax highlighting for code blocks in the Markdown preview

## Status: ARCHITECTURE PENDING

## Problem statement

Add treesitter-based syntax highlighting to fenced code blocks in the Markdown
preview. When previewing a code block, use the captive Neovim runtime to obtain
the treesitter highlight groups for the block, resolve those groups' properties
(colors + emphases), and emit them as inline-styled spans in the preview HTML.
When a code block cannot be treesitter-highlighted in Neovim, fall back to the
current behavior: plain `<pre><code>`, no highlighting, no character-grid scraping.

This is task #4 in `markdown-tasks.md`.

## Investigated context (facts for the architect)

- `crates/wolf-core/src/markdown.rs` `render_html(source, theme, width_mode)` is the
  pure, IO-free renderer. Today it emits code blocks as plain
  `<pre><code class="language-xxx">` with no color. It is the only place that builds
  preview HTML.
- `crates/wolf-core/src/nvim.rs` `render_source_html(nvim, source_bufnr, width_mode)`
  (~line 1044) is the ONLY place with a live nvim handle that feeds render_html. It
  reads buffer lines over RPC and calls render_html. Shared by in-app preview
  (`render_preview`), the browser preview server (`browser_preview.rs`), and static
  disk renders — so highlighting added here flows to all three.
- The captive nvim already has treesitter parsers + highlight queries staged and
  proven working on macOS (see `crates/wolf-ffi/tests/treesitter_highlight.rs` and
  `runtime/nvim/README.md`). On Linux/cargo the treesitter block is a strict no-op
  (`syntax on`) — highlighting must degrade gracefully to plain code blocks there.
- Highlight groups resolve to colors via `nvim_get_hl(0, {name=..., link=false})` —
  the pattern `_G.wolf_setup_theme_tracking` already uses in `runtime/nvim/init.lua`
  (~line 371) for `wolf_theme`.
- The preview uses Wolf's OWN independent light/dark theme
  (`markdown/themes/default.css`), NOT the live nvim colorscheme — but code-block
  colors intentionally DO come from nvim's colorscheme per this feature.

## Locked design decisions (do NOT re-litigate)

1. Highlighting is produced NVIM-SIDE via a new Lua helper (e.g.
   `_G.wolf_highlight_code(text, lang)` in `runtime/nvim/init.lua`) using
   `vim.treesitter.get_string_parser(text, lang)` + the `highlights` query, resolving
   each capture's group via `nvim_get_hl(0, {name, link=false})`. Returns nil/empty
   when the language has no parser/query so the caller falls back.
2. `markdown.rs` stays pure. `render_html` gains a new parameter carrying
   per-code-block highlight data (document-order index suggested as the key; architect
   may choose a cleaner key). `markdown.rs` exposes a pure way to enumerate fenced code
   blocks (lang + text, document order) so `nvim.rs` can ask nvim to highlight each,
   then pass results back into `render_html`. When a block has highlight data, emit
   inline-styled spans inside `<pre><code>`; otherwise emit the current plain output.
3. Colors delivered as INLINE per-span styles:
   `<span style="color:#RRGGBB;font-weight:bold;font-style:italic;text-decoration:underline">`.
   No generated stylesheet, no CSS classes. HTML is standalone and never saved/published.
4. Attributes carried: foreground color + bold + italic + underline ONLY. No background
   color, no other attributes.
5. Fallback: if a code block is not treesitter-highlighted in nvim (unknown/unsupported
   language, missing parser/query, or the whole treesitter block is a no-op as on
   Linux/cargo), fall back to CURRENT behavior — plain `<pre><code>`. Do NOT scrape the
   character grid.
6. Appearance responsiveness IN SCOPE: wire an appearance-change (NSApp.effectiveAppearance
   dark/light flip) to force a preview re-pull. Today the preview only re-pulls when
   `(sourceBufnr, sourceTick)` changes (`PaneContainerView.swift` ~line 196
   `lastPreviewLoads` / ~256 `previewReloadCoalescer` / `reloadDirtyPreviews` ~990-1030).
   An appearance flip changes neither, so the front-end must invalidate that per-window
   gate on appearance change so freshly re-sourced nvim colorscheme colors are picked up.

## Testing requirements (real Neovim, per CLAUDE.md)

- Definitive test: a wolf-ffi integration test mirroring
  `crates/wolf-ffi/tests/treesitter_highlight.rs` — stage the nvim-treesitter + parsers
  bundle, open a markdown doc with a fenced go block, call `render_preview`, assert
  the HTML contains inline-styled spans with resolved colors for the code tokens; assert
  an unknown language yields plain `<pre><code>`. SKIP gracefully when the bundle isn't
  staged (Linux/cargo).
- Pure `markdown.rs` unit tests for the new render path (synthetic highlight data in →
  inline-styled span output; none → unchanged plain output).

## Documentation requirements (per CLAUDE.md)

- Create `preview-code-block-syntax-highlighting-decisions.md`.
- Update `crates/wolf-core/src/README.md` (markdown.rs + nvim.rs sections),
  `crates/wolf-core/src/markdown/README.md` if CSS touched, `runtime/nvim/README.md`
  (new Lua helper), and mark task #4 DONE in `markdown-tasks.md`.
- Reformat edited Markdown with `prettier --write`.

## Architect's plan (authoritative)

### Summary
Fenced code blocks render plain today. Add per-token treesitter highlighting sourced
from captive Neovim. New unconditional Lua helper `_G.wolf_highlight_code_blocks`
highlights a BATCH of blocks in ONE RPC round-trip, resolving each capture to its
effective group color + bold/italic/underline via `nvim_get_hl(0,{name,link=false})`.
`nvim.rs::render_source_html` orchestrates: read buffer → `markdown::code_blocks(&source)`
→ `highlight_code_blocks(nvim,&blocks)` → `render_html_with_highlights(...)`. Pure
`markdown.rs` stays IO-free: a new `CodeHighlighter` iterator adapter replaces each
highlighted block's interior `Event::Text` with an `Event::InlineHtml` of escaped text
interleaved with inline-styled spans, preserving `Start`/`End` so marker injection and
the `<pre><code>` wrapper are unaffected. Fallback to plain per-block. Plus macOS
front-end: force preview re-pull on dark/light appearance flip.

### 3.1 `crates/wolf-core/src/markdown.rs` (core change)
NEW public types (near `PreviewTheme`/`WidthMode`, `#[derive(Clone,Debug)]`, re-export
from crate root like `render_html`):
- `pub struct CodeBlockInfo { pub lang: Option<String>, pub text: String }` — one per
  fenced/indented block, document order. `lang` = first whitespace token of fenced info
  string, lowercased (`None` for indented or bare fence). `text` = concat of block's
  `Event::Text` bodies (byte-identical to what the render pipeline concatenates).
- `pub struct HighlightStyle { pub color: Option<u32>, pub bold: bool, pub italic: bool, pub underline: bool }`
  — `color` packed `0xRRGGBB` (mirrors `render::Rgb::from_packed`); `None` = no fg.
- `pub struct HighlightSpan { pub row: u32, pub start_col: u32, pub end_col: u32, pub style: HighlightStyle }`
  — half-open `[start_col,end_col)` BYTE-column range, 0-based `row`, single-line,
  non-overlapping, sorted by `(row,start_col)` (Lua guarantees; markdown.rs defensive).
- `pub struct BlockHighlight { pub spans: Vec<HighlightSpan> }` — all spans for ONE block.

NEW/changed fns:
- private `fn markdown_options() -> Options` — the exact Options set currently built
  inline in `render_html` (TABLES,FOOTNOTES,STRIKETHROUGH,TASKLISTS,HEADING_ATTRIBUTES,
  MATH). Use in `render_html_with_highlights`, `heading_slugs` call sites, `code_blocks`.
- `pub fn code_blocks(source:&str) -> Vec<CodeBlockInfo>` (`#[must_use]`) — fresh
  `Parser::new_ext(source, markdown_options())` pass. On `Start(Tag::CodeBlock(kind))`
  push new block (lang from `CodeBlockKind::Fenced(info)` first token lowercased,
  empty→`None`; `Indented`→`None`); on `Text` inside block append; on
  `End(TagEnd::CodeBlock)` finalize. Index i ALIGNS with the i-th `Start(CodeBlock)`
  that `CodeHighlighter` counts.
- keep `pub fn render_html(source,theme,width_mode)->String` signature — body becomes
  one-line delegate `render_html_with_highlights(source,theme,width_mode,&[])` (empty
  slice = every block plain = current behavior; ZERO churn at existing call sites).
- `pub fn render_html_with_highlights(source:&str, theme:PreviewTheme, width_mode:WidthMode, code_highlights:&[Option<BlockHighlight>]) -> String`
  (`#[must_use]`) — real impl (moved body). Pipeline becomes:
  `offset_iter → HeadingIdAssigner → MathRenderer → CodeHighlighter → MarkerInjector → html::push_html`.
  `CodeHighlighter` runs BEFORE `MarkerInjector` so injector still sees the real
  block-level `Start(CodeBlock)` (its flush trigger); highlighter passes `Start`/`End`
  through and swaps only interior `Text`.
- NEW `struct CodeHighlighter<'a,I>: Iterator<Item=(Event<'a>,Range<usize>)>`. Fields
  `inner`, `highlights:&'a [Option<BlockHighlight>]`, `next_index:usize`,
  `pending:VecDeque<(Event,Range)>`. `next`: (1) drain `pending` first. (2) On
  `Start(Tag::CodeBlock(_))`: `idx=next_index; next_index+=1; hl=highlights.get(idx).and_then(Option::as_ref)`.
  If `None` → return `Start` unchanged (plain passthrough). If `Some(block)` → eagerly
  drain `inner` accumulating interior `Text` into a `String` until
  `End(TagEnd::CodeBlock)` (capture its `end_range`); unexpected interior events pushed
  to `pending` verbatim; build `inner_html=build_highlighted_html(&text,block)`; push to
  `pending`: `(InlineHtml(inner_html.into()), end_range.clone())` then
  `(End(TagEnd::CodeBlock), end_range)`; return the original `Start`. (3) else passthrough.
  Result: `<pre><code class="language-go">` + raw highlighted inner (InlineHtml written
  verbatim like MathRenderer's MathML) + `</code></pre>`; no marker inside `<pre>`.
- `fn build_highlighted_html(text:&str, block:&BlockHighlight) -> String` — split on
  `'\n'`; per row push `'\n'` if `row>0`; collect row's spans sorted by `start_col`;
  walk with byte `cursor=0`, defensively clamp `start`/`end` to `[cursor,line.len()]`,
  SKIP span if clamped start≥end or start<cursor (overlap guard); emit escaped gap
  `line[cursor..start]`, then `<span style="{style_css}">`+escaped `line[start..end]`+
  `</span>`, `cursor=end`; then escaped `line[cursor..]`.
- `fn escape_html_into(out:&mut String, s:&str)` — `&`→`&amp;`,`<`→`&lt;`,`>`→`&gt;`
  (local helper; don't depend on `pulldown_cmark::escape` visibility).
- `fn style_css(style:&HighlightStyle) -> String` — EXACT order: `color:#{:06x};`
  (when Some, `&0x00FF_FFFF`), then `font-weight:bold;`, `font-style:italic;`,
  `text-decoration:underline;` per flags. Deterministic for tests.
- add `CodeBlockKind` to `pulldown_cmark` use. Extend module doc with a
  "Code-block syntax highlighting" section.

### 3.2 `crates/wolf-core/src/nvim.rs` (orchestration)
`render_source_html` (~1044): after building `source`:
```
let blocks = crate::markdown::code_blocks(&source);
let highlights = if blocks.is_empty() { Vec::new() } else { highlight_code_blocks(nvim,&blocks).await };
Ok(crate::markdown::render_html_with_highlights(&source, PreviewTheme::Default, width_mode, &highlights))
```
NEW `async fn highlight_code_blocks(nvim:&Neovim<Compat<ChildStdin>>, blocks:&[markdown::CodeBlockInfo]) -> Vec<Option<markdown::BlockHighlight>>`:
- build Lua arg `Value::Array` of per-block `[text, lang]` (`lang=b.lang.as_deref().unwrap_or("")`).
- call `nvim_exec_lua("return wolf_highlight_code_blocks(...)", Value::Array(vec![Value::Array(lua_blocks)]))`.
- ANY error (RPC err, non-array reply, wrong len) → `vec![None; blocks.len()]` (total
  graceful fallback; this is what degrades Linux/cargo cleanly).
- decode `Value::Array` per block: `Boolean(false)`/`Nil`→`None`; `Array(spans)`→ parse
  each as 7-elem `[row,start_col,end_col,fg,bold,italic,underline]` into `HighlightSpan`
  (`fg<0`→`color:None` else `Some(fg as u32)`; bools via `as_bool().unwrap_or(false)` or
  `as_i64()!=0`)→`BlockHighlight`→`Some`; else `None`. Index defensively (`get(i)`).
- update `render_preview` + `render_source_html` doc comments to mention the highlight pass + fallback.

### 3.3 `runtime/nvim/init.lua` (Lua highlighter)
Add `_G.wolf_highlight_code_blocks(blocks)` at MODULE LEVEL (like `_G.wolf_create_tabpage`),
NOT inside the conditional treesitter block, so it exists on every platform (returns
`false` where no parser/query).
Contract: arg = array of `{text,lang}` (lang `""` when none). Return array `results`;
`results[i]` = `false` (not highlightable) OR array of span tuples
`{row,start_col,end_col,fg,bold,italic,underline}`. NEVER return empty Lua table
(msgpack ambiguity) — non-highlightable OR empty-result → scalar `false`; each span a
fixed 7-elem array. `fg` = packed `0xRRGGBB` int or `-1`; attrs = booleans.
`highlight_one(text,lang)`:
1. `lang==nil or ""` → false.
2. `ok,parser=pcall(vim.treesitter.get_string_parser,text,lang)`; not ok/parser → false.
3. `query=vim.treesitter.query.get(lang,"highlights")`; not query → false.
4. `ok2,trees=pcall(parser:parse())`; not ok2/trees/trees[1] → false. `root=trees[1]:root()`.
5. `lines=vim.split(text,"\n",{plain=true})` for per-line byte lengths.
6. cache `styles={}`; `resolve(name)`: `pcall(nvim_get_hl,0,{name="@"..name,link=false})`;
   if empty AND name has `.`, retry trimming trailing `.component`s (`function.call`→
   `function`); return `{fg=h.fg or -1, bold=h.bold==true, italic=..., underline=...}`
   or `nil` if no fg AND no attr (skip empty style — no `<span style="">`).
7. `for id,node,metadata in query:iter_captures(root,text,0,-1)`: `name=query.captures[id]`;
   `srow,scol,erow,ecol=node:range()` (byte cols); `prio=tonumber(metadata.priority) or 100`;
   split capture into per-line segments `srow..erow` (`seg_start=(r==srow) and scol or 0`,
   `seg_end=(r==erow) and ecol or #lines[r+1]`); `style=resolve(name)` skip if nil; apply
   into per-line byte-indexed winner map, overwrite when `(prio,order)` beats incumbent
   (`prio>` OR (`prio==` AND later `order`); `order` monotonic counter → later capture
   wins ties, matches nvim).
8. coalesce each line's winner bytes into non-overlapping runs (merge adjacent identical
   style) → `{r,run_start,run_end,fg,bold,italic,underline}`; skip un-won bytes (plain gaps).
9. no spans → false; else return span list.
`wolf_highlight_code_blocks`: `for i,b in ipairs(blocks) do results[i]=pcall-wrapped highlight_one(b[1],b[2]) (false on error) end; return results`.
Add a documenting comment block; note self-degrade to `false` on Linux/cargo.

### 3.4 `crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift` (appearance re-pull)
Add override (mirrors existing `viewDidChangeEffectiveAppearance` in BufferPickerView/
CommandPaletteView/EmptyStateView):
```
override func viewDidChangeEffectiveAppearance() {
  super.viewDidChangeEffectiveAppearance()
  lastPreviewLoads.removeAll()
  previewReloadCoalescer.request()
}
```
Clearing `lastPreviewLoads` makes `PreviewSeam.winsNeedingReload(current:, loaded:[:])`
return every bound preview window (first-bind semantics) → `reloadDirtyPreviews`
re-renders. Forced dark/light modes: effective appearance doesn't change on OS toggle,
so this correctly no-ops. Ordering vs AppDelegate KVO (`systemAppearanceChanged`→
`setBackground`): handled by the coalescer's `asyncAfter(refreshInterval)` debounce —
synchronous `set_background` RPC enqueues on the FIFO channel before the ~33ms-later
coalesced `render_preview`, so nvim re-sources first. NO AppDelegate/Project.swift change.
(Confirm the exact `request()` method name on `PickerRefreshCoalescer` when implementing;
use whatever the existing `previewReloadCoalescer` call site uses.)

### 3.5 `crates/wolf-ffi/BUILD.bazel` (new test target)
Add `rust_test(name="code_block_highlight",...)` right after the `treesitter_highlight`
stanza (~line 726), COPY it exactly except `srcs=["tests/common/mod.rs","tests/code_block_highlight.rs"]`,
`crate_root="tests/code_block_highlight.rs"`. Same tags/size/edition, same `data`
(nvim_tree, nvim_treesitter_tree, parsers_tree, init.lua), same four env vars
(WOLF_TEST_INIT_LUA, WOLF_TEST_NVIM_DIR, WOLF_TEST_NVIM_TREESITTER_DIR, WOLF_TEST_PARSERS_DIR), same deps.

### 3.6 CSS — NO change
`default.css`/`base.css` untouched: inline `style=` on child `<span>` overrides inherited
`pre code` color; `<pre>` background/padding/24px-baseline rules unaffected. State this
explicitly in decisions doc + markdown/README note.

### 3.7 No FFI surface change
Highlighting fully internal to `render_source_html`; `render_preview` already returns the
HTML. NO new `WolfCoreHandle` method, NO new `command_kind` discriminant, NO `generated/`
regen. Appearance re-pull is front-end-only. `crates/wolf-ffi/src/lib.rs` untouched.

## Test plan

### Pure unit tests — `markdown.rs` `#[cfg(test)] mod tests`
- `code_blocks_enumerates_lang_and_text` — one block, `lang==Some("go")`, `text=="package main\n"`.
- `code_blocks_document_order_and_indented` — go/indented/rust → `[Some("go"),None,Some("rust")]`.
- `code_blocks_bare_fence_is_none` — bare fence → `lang==None`.
- `render_html_with_highlights_emits_styled_spans` — BlockHighlight `[0,7)` row0 `color:Some(0xFF8800),bold:true`
  for go text `package foo\n` → contains `<span style="color:#ff8800;font-weight:bold;">package</span>`,
  still `<pre>` + `class="language-go"`.
- `render_html_with_highlights_escapes_text` — text w/ `<`/`&`/`>` → escaped `&lt;`/`&amp;`, not raw.
- `render_html_with_highlights_none_is_plain` — `&[None]` for one go block → same plain body as 3-arg `render_html`, no `<span style=`.
- `render_html_empty_highlights_matches_render_html` — `render_html_with_highlights(src,..,&[])` byte-equals `render_html(src,..)` for a doc w/ a code block.
- `highlighted_block_has_no_marker_inside_pre` — scan between `<pre` and `</pre>`, no `wolf-src` span; `wl-1-0` still exists elsewhere.
- `render_html_with_highlights_defensive_overlap` — overlapping/out-of-range span → no panic, well-formed (non-nested) spans.

### Pure API tests — `crates/wolf-core/tests/markdown_render.rs`
A couple of the above at the public-API level (exercise re-exported
`render_html_with_highlights`, `BlockHighlight`, `HighlightSpan`, `HighlightStyle`, `code_blocks`). Minimal.

### Definitive real-nvim integration — `crates/wolf-ffi/tests/code_block_highlight.rs` (new)
Combine `treesitter_highlight.rs` staging + `preview.rs` render_preview driving. `mod common;`,
copy char/enter/send_ex_command helpers + FLUSH_TIMEOUT/POLL_INTERVAL.
- Graceful SKIP (loud eprintln, `return`) when `nvim_bin_or_skip()` None OR
  `nvim_treesitter_dir_or_none()` None OR `parsers_dir_or_none()` None.
- Stage tempdir T like `treesitter_highlight.rs` (copy init.lua, recursive copy
  nvim-treesitter/ + parsers/, defensive inner-dir, pre-launch go.so + injections.scm asserts).
- Two fixtures: `hl.md` = `# H\n\n```go\npackage main\nfunc f() {}\n```\n`;
  `plain.md` = `# H\n\n```notalang\nsome text here\n```\n`.
- `wolf_ffi_start(T/init.lua,T,COLS,ROWS)`; poll `read_frame().cols()>0`.
- Positive: `:e hl.md`; poll `is_markdown_active()`; `open_preview("split_right")`>0; wait 2 windows;
  `previews()` for hl.md; `source_bufnr()`; `html=render_preview(bufnr)`. Assert
  `html.contains("<span style=\"color:#")`, `html.contains("package")`, `html.contains("class=\"language-go\"")`.
- Negative: `:e plain.md`; wait current_file ends plain.md; open_preview; previews() for plain.md;
  `render_preview`. Assert `!html.contains("<span style=\"color:#")` (unknown lang → plain).
- `shutdown(); drop(tmp)`. `//!` header noting the `//crates/wolf-ffi:code_block_highlight` target must be added.
- SIGNATURE NOTE: `<span style="color:#` (no space) is unambiguous — CSS uses `color: #`
  (space), markers use `class="wolf-src"`.

### macOS pure seam test — `crates/wolf-macos/Tests/WolfKeyMapTests/PreviewSeamTests.swift`
Confirm/add a test: `PreviewSeam.winsNeedingReload(current:<non-empty>, loaded:[:])` returns
ALL current keys (the exact mechanism the re-pull relies on). The AppKit override itself is
manual-smoke (documented in decisions doc).

## Implementation order (avoid compile breaks)
1. markdown.rs: types + markdown_options + code_blocks + style_css/escape/build_highlighted_html
   + CodeHighlighter + render_html_with_highlights (move body) + render_html delegate + re-exports.
2. In-module unit tests.
3. nvim.rs: highlight_code_blocks + rewire render_source_html.
4. init.lua: wolf_highlight_code_blocks.
5. markdown_render.rs additions.
6. code_block_highlight.rs + BUILD target.
7. PaneContainerView.swift override + PreviewSeamTests confirmation.
8. Docs.

## Lint notes
- `#[must_use]` on `code_blocks` + `render_html_with_highlights` (match `render_html`/`theme_css`).
  Doc-comment every new public item.
- `build_highlighted_html` byte-slicing on treesitter byte cols (valid UTF-8 boundaries) +
  defensive clamp (skip if start<cursor / start>=end / beyond len) → no panic, no unsafe/unwrap.
  Avoid `clippy::string_slice` via clamped/validated indices or `get(range)`.
- `highlight_code_blocks` never unwrap/expect on decode; `None` per block. No
  `await_holding_lock` (no lock across the new await).
- FFI test reuses `tests/common/mod.rs` (`#![allow(dead_code)]`).

## Documentation plan
1. NEW `preview-code-block-syntax-highlighting-decisions.md` (repo root): problem/scope;
   locked decisions recap; architecture (enumerate↔CodeHighlighter alignment, why
   Start/End preserved, interior Text→InlineHtml, no double-escape/no marker in pre);
   Lua contract; fallback matrix; CSS invariant (no change); appearance re-pull; testing.
2. `crates/wolf-core/src/README.md`: markdown.rs section (render_html_with_highlights,
   render_html delegate, new types, code_blocks, CodeHighlighter placement — update chain
   string); nvim.rs section (render_source_html/render_preview highlight pass + fallback).
3. `runtime/nvim/README.md`: document `_G.wolf_highlight_code_blocks` (batch contract,
   return shape, capture/color resolution, precedence, false fallback).
4. `crates/wolf-core/src/markdown/README.md`: one-line note — highlighting via inline
   `<span style>`, NO CSS change, 24px grid unaffected.
5. `markdown-tasks.md`: mark task #4 DONE + decisions-doc link (format of #1–#3).
6. `prettier --write` all edited Markdown; no hard line breaks in paragraphs.

## Verification commands (coder)
- `cargo test -p wolf-core markdown` and `cargo test -p wolf-core`.
- `cargo test -p wolf-ffi` (Linux/cargo will SKIP the staged test loudly).
- Bazel (macOS): `bazel test //crates/wolf-ffi:code_block_highlight //crates/wolf-ffi:treesitter_highlight //crates/wolf-ffi:preview` and `bazel test //crates/wolf-core/...`.
- Swift: `bazel test //crates/wolf-macos:WolfKeyMapTests`.
- Lint: `cargo clippy --all-targets`; the repo's Swift lint for wolf-macos.
- `prettier --write` on edited Markdown. `bazel build //...` sanity.

### Two load-bearing facts
- pulldown-cmark fenced block = `Start(CodeBlock(Fenced(info)))` → 1+ `Text` (concat =
  body INCLUDING trailing `\n`) → `End(CodeBlock)`. `code_blocks()` and `CodeHighlighter`
  MUST concat the SAME `Text` bodies so nvim `(row,byte-col)` aligns with `text.split('\n')`.
- Unambiguous HTML signature `<span style="color:#` (no space) — CSS uses `color: #`
  (space), markers use `class="wolf-src"`. Use for FFI positive + negative asserts.

## Architect Q&A / open confirmations for coder
- Confirm `PickerRefreshCoalescer` trigger method name at the existing `previewReloadCoalescer` call site (plan assumes `request()`).

## Status log

- [x] architect — detailed plan (above)
- [x] test-writer — created/modified:
  - `crates/wolf-core/src/markdown.rs` — 9 in-module unit tests (code_blocks_*,
    render_html_with_highlights_*, highlighted_block_has_no_marker_inside_pre,
    defensive_overlap).
  - `crates/wolf-core/tests/markdown_render.rs` — 3 public-API tests (require crate-root
    re-exports of code_blocks/render_html_with_highlights/BlockHighlight/HighlightSpan/HighlightStyle).
  - `crates/wolf-ffi/tests/code_block_highlight.rs` (NEW) — real-nvim integration test
    `code_block_go_highlighted_and_unknown_lang_plain` (graceful SKIP; positive go block
    asserts `<span style="color:#` + `package` + `class="language-go"`; negative unknown
    lang asserts NO `<span style="color:#`). Negative case uses `open_preview("reuse_preview_pane")`.
  - `crates/wolf-macos/Tests/WolfKeyMapTests/PreviewSeamTests.swift` — added
    `testEmptyLoadedReloadsEveryCurrentWin` (already in WolfKeyMapTests.srcs, no BUILD change).
  CODER MUST: (1) add `rust_test(name="code_block_highlight",...)` to `crates/wolf-ffi/BUILD.bazel`
  after the treesitter_highlight stanza (srcs=[common/mod.rs, code_block_highlight.rs],
  crate_root=code_block_highlight.rs, same data/env/deps); (2) re-export the 4 new types +
  `code_blocks` + `render_html_with_highlights` from `crates/wolf-core/src/lib.rs`.
  All FFI accessors verified present. Tests won't compile until coder implements.
- [x] coder — implemented all of §3.1–§3.5 + §3.7. markdown.rs (4 types, markdown_options,
  code_blocks, render_html_with_highlights + CodeHighlighter/build_highlighted_html/
  style_css/escape_html_into, render_html delegate); lib.rs re-exports; nvim.rs
  (highlight_code_blocks + decode_block_highlight/value_is_true + rewired
  render_source_html + doc comments); init.lua (_G.wolf_highlight_code_blocks, module
  level, pcall-wrapped, false sentinel); wolf-ffi/BUILD.bazel (code_block_highlight target);
  PaneContainerView.swift (viewDidChangeEffectiveAppearance → lastPreviewLoads.removeAll()
  + previewReloadCoalescer.request() — CONFIRMED request() is the real trigger, used at
  lines 977/1164). VERIFICATION (macOS Bazel — cargo absent in devshell):
  - `//crates/wolf-core:wolf_core_lib_test` + `:markdown_render` PASS (9 in-module + 3 API tests).
  - `//crates/wolf-ffi:code_block_highlight` **ACTUALLY RAN real nvim (not skipped), PASS 0.66s**
    — proves `<span style="color:#…">` for go + plain for unknown lang end-to-end.
  - `//crates/wolf-core/... //crates/wolf-ffi/...` 59/59 PASS.
  - `//crates/wolf-macos:WolfKeyMapTests` PASS (incl. new PreviewSeamTests case).
  - `bazel build //...` PASS (whole app incl. Swift override).
  - clippy (`-Dwarnings`) PASS (fixed one while_let_on_iterator → for by_ref()).
  - Deviations: none functional. decode split into decode_block_highlight/value_is_true helpers.
  - ⚠ FOLLOW-UP: `rustfmt_core` reports 3 advisory (non-gating) formatting diffs, ALL inside
    the test-writer's `#[cfg(test)]` block in markdown.rs (~lines 1500/1519/1541:
    render_html_with_highlights_escapes_text / _none_is_plain / _empty_highlights_matches_render_html).
    Coder couldn't edit test code → route to test-writer.
- [x] test-writer (rustfmt pass) — hand-formatted the 3 tests to canonical rustfmt style
  (could not run rustfmt locally). Coder to CONFIRM `rustfmt_core` green in closing pass.
- [x] reviewer — APPROVED. All 11 points PASS (byte-col alignment, pipeline placement,
  fallback, inline style output, purity, backward-compat, Lua helper, decode symmetry,
  appearance re-pull, no FFI surface change, tests match). Non-blocking notes: (a) advisory
  rustfmt (routed above); (b) CLARIFICATION for docs — the browser server's STATIC
  arbitrary-`.md` disk-render path (browser_preview.rs:239) intentionally stays PLAIN (no
  live nvim buffer to highlight); ONLY the live-pinned-buffer path via `render_source_html`
  (in-app preview, browser live preview) gets highlighting. Decisions doc must state this.
- [x] technical-writer — CREATED `preview-code-block-syntax-highlighting-decisions.md`
  (problem/scope, locked decisions, enumerate↔CodeHighlighter alignment, Lua contract,
  fallback matrix, which-paths-highlight incl. the static-disk-stays-plain clarification,
  CSS-invariant, appearance re-pull, testing). UPDATED `crates/wolf-core/src/README.md`
  (lib.rs re-exports + markdown.rs + nvim.rs sections, render-chain string), `runtime/nvim/README.md`
  (`_G.wolf_highlight_code_blocks`), `crates/wolf-core/src/markdown/README.md` (no-CSS-change
  note), `markdown-tasks.md` (task #4 DONE + link). No code/doc divergence found. Noted the
  plan-referenced `preview-math-and-fonts-decisions.md` doesn't exist in this worktree
  (pre-existing dangling links from committed READMEs — out of scope).
- [x] Orchestrator closeout — ran `prettier --write` on all 5 edited Markdown docs (all
  already clean, no changes). Confirmed advisory `rustfmt_core`: it FAILED (test-writer's
  hand-format guessed wrong — rustfmt wanted the asserts EXPANDED, not collapsed); applied
  the toolchain's own nightly rustfmt to `markdown.rs` (whitespace-only, touches only the 3
  test spots) → `//tools/lint:rustfmt_core` now PASSES; re-ran `//crates/wolf-core:wolf_core_lib_test`
  + `:markdown_render` → PASS after formatting.

## COMPLETE ✅

Treesitter syntax highlighting for Markdown-preview code blocks is implemented,
reviewer-APPROVED, fully tested (incl. a real-nvim FFI test that ACTUALLY RAN on macOS),
documented, and format-clean. NOT committed.
