# Plan: Resolve `git merge main` documentation conflicts on `feat/docs-review`

## Status

Architect pass **complete**. Coder pass **complete**. Architect amendment pending on four `entry-budget` lint findings; then reviewer, then technical-writer.

### Coder results

All 14 conflicts resolved, all post-conflict edits applied across 18 `.md` files. **Zero source files modified.** `bazelisk test //...` — **93/93 targets pass**, including `//crates/wolf-core:markdown_cursor`, `:markdown_render`, `//crates/wolf-ffi:preview`, and `//crates/wolf-macos:WolfKeyMapTests` (confirmed to include `PreviewScrollSeamTests.swift`). `prettier --write` was a no-op on first invocation and on re-run. No `long-line`, `changelog`, `allcaps`, or `big-file` findings.

The merge is resolved and **uncommitted**.

**Open: four `entry-budget` findings.** Every targeted entry was already within 2–7 words of the 250-word ceiling before the edits, so HEAD's refactor left no headroom:

| entry                                                | baseline | after |
| ---------------------------------------------------- | -------- | ----- |
| `crates/wolf-core/src/README.md` → `render.rs`       | 243      | 284   |
| `crates/wolf-core/src/README.md` → `markdown.rs`     | 247      | 411   |
| `Sources/WolfKeyMap/README.md` → `FrameStats.swift`  | 246      | 285   |
| `Sources/WolfMacOS/README.md` → `PaneContainerView.swift` | 248 | 413   |

Coder tightening already cleared `redraw.rs` (253→249) and `PreviewScrollSeam.swift` (265→250). **Orchestrator ruling: raising `MAX_ENTRY_WORDS` is not an option** — CLAUDE.md says exceeding the budget means transcribing implementation, so the fix is to cut or to relocate to the crate's `DECISIONS.md`, not to change the ruler. Routed to the architect for a per-entry amendment.

### Coder deviations from the plan (accepted)

1. **§2.4c wording** — plan said "nearest whitespace"; source says word boundary (`crates/wolf-core/src/markdown.rs:147`, const doc at :1208). Coder wrote "word boundary". Pending architect confirmation.
2. **§2.4c four bullets → three** — applied §7's merge guidance pre-emptively and folded the pre-existing `Event::InlineHtml` bullet into the suppression bullet. No decision dropped.
3. **§2.1 dropped one bold** — CLAUDE.md reserves bold for genuine warnings; "not a byte offset" already carries the point.
4. **§3.6c added a `**Gotchas**` heading** — `PaneContainerView.swift`'s entry had none.

### Flake recorded, not caused by this change

`//crates/wolf-core:commands` failed once with a truncated log (5 of 15 tests reported `ok`, no panic, no `test result:` line), then passed on isolated re-run and on the confirming full run. Distinct from the documented `//crates/wolf-core:redraw_grid` flake, which passed here. A docs-only change cannot cause it; noted for follow-up.

**Orchestrator decision — the test-writer step is skipped for this change.** The architect's §4 is explicit that no test is added, changed, or removed: this is a documentation-only resolution layered on an already-clean source merge. `main`'s tests came across cleanly and are the verification surface. Inventing tests here would mean writing tests for prose. The coder's job is to confirm the merged tree is still green, not to extend it. If the coder finds a genuine test gap, it reports it and the orchestrator decides then — it does not write one.

## Problem statement

`git merge main` is in progress in the worktree
`/Users/jplein/Git/jplein/wolf/.worktrees/feat/docs-review`. The merge stopped with 14 conflicted files, all of them Markdown documentation:

| File                                      | Conflict hunks |
| ----------------------------------------- | -------------- |
| `architecture.md`                         | 2              |
| `build.md`                                | 1              |
| `crates/wolf-core/README.md`              | 1              |
| `crates/wolf-core/src/README.md`          | 4              |
| `crates/wolf-core/src/markdown/README.md` | 3              |
| `crates/wolf-core/tests/README.md`        | 2              |
| `crates/wolf-ffi/README.md`               | 1              |
| `crates/wolf-ffi/src/README.md`           | 1              |
| `crates/wolf-ffi/tests/README.md`         | 1              |
| `crates/wolf-gnome/README.md`             | 1              |
| `crates/wolf-macos/README.md`             | 3              |
| `crates/wolf-macos/Sources/README.md`     | 1              |
| `crates/wolf-macos/Tests/README.md`       | 1              |
| `runtime/nvim/README.md`                  | 2              |

Everything else merged cleanly, including every source file and `BUILD.bazel`.

## The two sides

- **HEAD (`feat/docs-review`)** — a pure documentation refactor. It restructured every doc in the repo to the ownership rules in `CLAUDE.md`: one file one owner, per-file `####` entries, the two-question test, no history, ~200 words per entry. It changed no source.
- **`main`** — real feature work (preview scroll seam, keeping edited content in view), adding `crates/wolf-macos/Sources/WolfKeyMap/PreviewScrollSeam.swift`, `Sources/WolfMacOS/PreviewScrollStateHandler.swift`, `Sources/WolfMacOS/PreviewNavigationDelegate.swift`, large `PaneContainerView.swift` changes, `runtime/nvim/init.lua` changes, `crates/wolf-core/tests/markdown_cursor.rs`, and `Tests/WolfKeyMapTests/PreviewScrollSeamTests.swift`. It also documented that new code in the pre-refactor doc structure.

`main`'s own orchestrator plan for that feature is archived at `/private/tmp/claude-501/-Users-jplein-Git-jplein-wolf--worktrees-feat-docs-review/70f13771-5d94-450c-b668-b4e5f0e7c15b/scratchpad/main-plan-archive.md` — background on what the new code does and why. (This plan file replaced it; it is a transient working file, not a deliverable.)

## Resolution contract

1. Keep HEAD's refactored document structure, ownership split, and per-entry format.
2. Carry forward every fact `main` added about its new code — rewritten into HEAD's structure and subject to `CLAUDE.md`'s rules. Do not paste `main`'s prose verbatim where it restates a signature, narrates history, or duplicates another doc.
3. No source file may be modified. This is a docs-only resolution on top of an already-clean source merge.
4. Every source file `main` added must end up documented in exactly one document — the directory README that owns it.
5. Verify: no conflict markers remain, `prettier --write` leaves the files unchanged, `./tools/lint/markdown_check.sh` passes, and the test suites pass on the merged tree.

## Non-goals

- No new tests. See the orchestrator decision under Status.
- No re-litigating HEAD's refactor decisions, and no re-litigating `main`'s feature design.

---

# Architect's plan

## 1. Summary

In almost every hunk the resolution is **take HEAD's side wholesale and delete `main`'s**, because `main`'s side is a fragment of a document structure that no longer exists. The real work is _outside_ the conflict markers: HEAD's refactored documents were written against a tree that did not yet contain `PreviewScrollSeam.swift`, `PreviewScrollStateHandler.swift`, `markdown_cursor.rs`, the `wolf_markdown_cursor` notification, or two-tier marker placement — so they are stale or silently missing entries, and several of them state the opposite of what the merged code does. Those edits are mandatory, not optional; skipping them leaves the merge with a documentation-is-source-of-truth bug.

**Non-negotiable constraints for the coder:** modify no source file (`.rs`, `.swift`, `.lua`, `.css`, `BUILD.bazel`, `.bzl`); do not add or change tests; do not re-litigate either side's design.

---

## 2. Conflict-by-conflict resolution

Line numbers are as they appear in the current working tree. Resolve files in the order given in §5.

### 2.1 `architecture.md` — 2 hunks

**Hunk 1 (lines 177–191), inside `### Output: preview`.**
HEAD's side: three paragraphs — placement modes (`open_preview` / `edit_source` / `preview_file` / `open_in_browser`), the two scratch-buffer gotchas, and `**Content width.**` plus its contract paragraph. `main`'s side: three enormous paragraphs restating the whole scroll-sync feature, the entire `PreviewScrollSeam` internals, and a differently-worded content-width paragraph.

**Resolution: keep HEAD's side verbatim; delete `main`'s side.** Rule: `architecture.md` is "the cross-crate view only… it never restates" a crate's local constraints. `main`'s second paragraph (the in-place body swap, `captureDecision`, `loadCommitOutcome`, the JS ordering contract) is entirely `wolf-macos`-local and belongs to that crate's file entries — it is carried in §3.5/§3.6, not here.

**Then** insert one genuinely cross-crate fact `main` added, as a new paragraph immediately after HEAD's `**Content width.**` contract paragraph (i.e. after the current line 184, before the `### Input: events` heading). Exact text:

> **Cursor position.** A third out-of-band notification closes the loop from the editor cursor to the preview's scroll position. `wolf_markdown_cursor` carries three flat scalars `(source_bufnr, line, col)`, which core upserts into `Snapshot.markdown_cursors` keyed by the source buffer number — the same key space as `markdown_ticks` and `PreviewInfo.source_bufnr`, so a front-end joins all three on one key. An absent entry, or a `line` of `0`, is the cursor-unknown sentinel, and a front-end must not scroll for it.
>
> The column semantics are the contract: `line` is a 1-based hard buffer line and `col` a 0-based **character** column, not a byte offset, so nothing on the path converts. They pair with the `wl-{L}-{C}` position markers `render_html` emits into the preview HTML. Whether a front-end scrolls at all is its own decision, per decision 2.

**Also** append one sentence to HEAD's existing `render_preview` sentence in the paragraph at line 175 (unconflicted), which is the boundary guarantee the macOS in-place body swap depends on:

> Because the in-app render pins one theme and full width, that document's `<head>` is byte-identical on every render, including across a dark/light flip, so a front-end may replace only the body.

**Hunk 2 (lines 231–238), at the end of `## Build`.**
HEAD's side: one sentence, "The `.app` bundles its own pinned Neovim, the captive `init.lua`, the vendored plugin and colorscheme trees, and both font families…". `main`'s side: four bullets that are the tail of `main`'s own deferred list (GTK popup, shared renderer, "scroll-sync **is built**", multiple preview themes). Git mis-aligned them; they are not a build paragraph.

**Resolution: keep HEAD's single sentence; delete `main`'s four bullets.**

**Then — required, outside the markers — rewrite `## Deferred and open` (lines 240–246).** HEAD's bullets 244 and 245 assert the feature is unbuilt and that markers collapse at block granularity. Both are false against the merged source (`runtime/nvim/init.lua`'s `emit_cursor`, `Snapshot::markdown_cursors`, `MarkerInjector`'s tier-2 splitting). Replace bullets 244 and 245 with:

> - **Marker granularity below the block.** A source line longer than `MARKER_COLUMN_STRIDE` is anchored inside its block, at the marker's true position in the rendered inline flow. A paragraph hard-wrapped into several shorter source lines is not: all of its `wl-{L}-0` markers resolve to the block's opening tag, so cursor-follow is paragraph-granular there, as it is inside a fenced code block, a table, or an image's alt text, where injection is suppressed. Generalizing is rejected rather than pending — an anchor between `<tbody>` and `<tr>` is foster-parented out of the table by the HTML parser, and one between `<ul>` and `<li>` is invalid.
> - **Preview scroll-sync is macOS-only.** The GTK shell has no Markdown preview pane, and the browser preview reloads on a poll with no cursor coupling.

Keep bullets 242, 243 and 246 unchanged.

### 2.2 `build.md` — 1 hunk

**Hunk (lines 172–176), inside `## Running the tests`.**
HEAD: "Tests run in the default configuration, not `-c opt`… `@testable import` requires `-enable-testing`…". `main`: a paragraph enumerating every `WOLF_FRAME_STATS` bucket and its two caveats.

**Resolution: keep HEAD; delete `main`'s side.** Rule: `build.md` holds commands, environment and workflow; "what a given test target covers belongs in that directory's README". HEAD already delegates the bucket set at line 222 to `Sources/WolfKeyMap/README.md`.

The one _new_ fact in `main`'s paragraph — that `prev.html` is deliberately shared by `loadHTMLString` and `callAsyncJavaScript` — is carried into the `FrameStats.swift` entry in §3.4, not here. No other edit to `build.md`.

### 2.3 `crates/wolf-core/README.md` — 1 hunk

**Hunk (lines 42–296).** HEAD: 4 lines opening `## Building and testing`. `main`: 250 lines — the entire pre-refactor per-method narrative.

**Resolution: keep HEAD's 4 lines; delete `main`'s entire side.** Rule: a crate README is "the outside view only… No per-file entries." Every fact in `main`'s side about `WolfCore::start`, `render_preview`, `MARKER_COLUMN_STRIDE` and the marker rules belongs in `src/README.md`'s `api.rs` and `markdown.rs` entries, which HEAD already owns.

**No further edits to this file.** Do **not** add `MarkdownCursor` to the `Public API surface` table: that table is already a curated summary that omits `PreviewInfo`, `WinBuffer` and `TabFiles`, and adding one sibling would be inconsistent without adding all.

### 2.4 `crates/wolf-core/src/README.md` — 4 hunks

All four are the same shape: HEAD's side is a `####` heading or a `**Decisions**` label; `main`'s side is a slab of the pre-refactor bullet list that git happened to align there.

| Hunk    | HEAD's side                            | `main`'s side                              | Resolution                  |
| ------- | -------------------------------------- | ------------------------------------------ | --------------------------- |
| 13–15   | `#### \`api.rs\``                      | `render.rs` type bullets incl. `MarkdownCursor` | **Take HEAD**, delete `main` |
| 95–97   | `**Decisions**` (under `redraw.rs`)    | the `markdown.rs` mega-bullet              | **Take HEAD**, delete `main` |
| 125–130 | four `render.rs` Decisions bullets     | `redraw.rs` type/handler bullets           | **Take HEAD**, delete `main` |
| 159–161 | the `logging.rs` `WOLF_LOG` bullet     | markdown-cursor + buffer-tracking prose    | **Take HEAD**, delete `main` |

**Then, outside the markers, three required edits:**

**(a) `redraw.rs` Purpose, line 93.** Replace the parenthesised list so the new notification is named:

> …and routes the out-of-band `wolf_*` notifications (cwd, buffers, previews, markdown ticks and cursors, tabpages, theme, open files, commands, intents) into the snapshot's sibling cells and queues.

**(b) `render.rs` `**Public API**`, after the `Toast::is_prompt` bullet (line 121).** Add:

> - `Snapshot.markdown_cursors` is keyed by the source Markdown buffer number, the same key space as `markdown_ticks` and `PreviewInfo.source_bufnr`, so a consumer joins all three on one key. `MarkdownCursor.line` is 1-based and `col` is a 0-based character column; an absent entry, or a `line` of `0`, means the cursor is unknown.

**(c) `markdown.rs` Decisions, line 282 — this bullet is false and must be rewritten in place.** It currently reads "Markers are injected only before block-level `Start` tags, so one never splits inline content… interior line markers therefore collapse to their block's top." Replace that single bullet with these four:

> - Markers are placed by two rules. A column-zero marker is flushed as a raw span immediately before its block's opening tag; a `col != 0` marker inside a paragraph, heading, blockquote, list or footnote definition is deferred and split into the `Event::Text` that contains it, so it sits at its true position in the rendered inline flow.
> - A `Text` is split only when `source[range]` equals the payload byte for byte. A length check is not enough — a same-length entity such as `&nvap;` passes it and is then corrupted — so a failed check leaves the marker to a later placement point rather than dropping it.
> - The split point snaps to the nearest whitespace within `MARKER_SNAP_MAX_BYTES`, because an inline-box edge breaks glyph shaping across it and cutting mid-word would lose a ligature or kern pair. The id and `data-*` attributes still name the exact `(L, C)`.
> - Injection is suppressed inside code blocks, HTML blocks, tables and image alt text, and those markers keep the before-the-tag placement. A raw span in a table interior is foster-parented out by the HTML parser, and pulldown-cmark builds an `alt` attribute with `raw_text`, which drops an `Event::Html` — which is also why a marker must never be emitted as `Event::InlineHtml`.
> - Only the column-zero markers are guaranteed to be in ascending document order, because a block's inline markers follow a later line's column-zero marker. A consumer sweeping markers in document order has to filter on `data-col="0"`.

Keep the existing bullets at lines 280, 281, 283 and 284 unchanged. If `markdown_check.sh` reports `entry-budget` on this entry (250-word ceiling), tighten by merging the second and third bullets — never by dropping a decision.

### 2.5 `crates/wolf-core/src/markdown/README.md` — 3 hunks

| Hunk  | HEAD's side                          | `main`'s side                                              | Resolution                  |
| ----- | ------------------------------------ | ---------------------------------------------------------- | --------------------------- |
| 9–15  | `#### \`base.css\`` heading          | three pre-refactor bullets about `.wolf-src`, layout, grid | **Take HEAD**, delete `main` |
| 39–45 | the `themes/default.css` paragraph   | the edge-rule and residuals paragraphs                      | **Take HEAD**, delete `main` |
| 69–100| the ten `## Decisions` bullets       | the `## assets/` section                                    | **Take HEAD**, delete `main` |

**Then, outside the markers, two required edits:**

**(a) `base.css` Decisions, replace the bullet at line 21** — it is incomplete against the merged renderer, which places anchors mid-text. Exact replacement:

> - `.wolf-src` is an inline box clamped to zero width and height, with `font-size: 0` and `line-height: 0`. The zero leading is load-bearing twice over: the body sets `line-height` as a length, so a marker that inherited it would get a hard 24px line box, and a marker also sits between two characters of a paragraph, heading, list item or link, where an inherited strut would grow that line.
> - The rule is `visibility: hidden`, never `display: none`. A hidden element still generates a box and reports a real `getBoundingClientRect()`, which is exactly what the macOS cursor-follow resolver measures; a `display: none` marker reports zeros and silently collapses every anchor to the top of the document.

**(b) `## Decisions`, append one sentence to the bullet at line 77** (the "not an anchor, and no non-anchor sibling precedes it" bullet):

> Each is a `body.wolf-markdown > *` child selector, so an anchor placed inside a block is a descendant and can never match one; leading and trailing anchor runs come only from column-zero markers and from markers flushed at a block boundary.

**Explicitly do not carry over** `main`'s "Why the front-end never re-loads the document while you type" paragraph or its asset provenance table. The ~965 KiB head cost is a `wolf-macos` rationale (§3.6), and provenance is already in HEAD's two per-asset entries at lines 51–65.

### 2.6 `crates/wolf-core/tests/README.md` — 2 hunks

| Hunk    | HEAD's side                         | `main`'s side                                                          | Resolution                  |
| ------- | ----------------------------------- | ---------------------------------------------------------------------- | --------------------------- |
| 125–132 | two `commands.rs` Gotchas bullets   | the pre-refactor `markdown_render.rs` narrative                        | **Take HEAD**, delete `main` |
| 136–156 | the `copy_paste.rs` target line     | a whole scroll-sync section plus a stray `### Layout-intent parsing`   | **Take HEAD**, delete `main` |

**Then, outside the markers, two required edits:**

**(a) New entry — `markdown_cursor.rs` is a source file `main` added that no document owns.** Insert it in the alphabetical `## Files` list between `#### \`layout_intents.rs\`` (line 229) and `#### \`markdown_render.rs\`` (line 242), in HEAD's exact three-part shape:

> #### `markdown_cursor.rs`
>
> Real Neovim (`//crates/wolf-core:markdown_cursor`: staged nvim runfiles, `WOLF_NVIM_BIN`, `no-sandbox`).
>
> **Covers**
>
> - `Snapshot.markdown_cursors`: the 1-based line and 0-based character column contract, including `charcol`'s past-end-of-line value in insert mode; entry on buffer enter with no motion; independence from `markdown_ticks`; the filetype, preview-scratch and window-type guards; and the source-bufnr key the FFI join depends on.
>
> **Gotchas**
>
> - `wolf_markdown_cursor` arrives out of band, so every assertion polls `read_snapshot()` and drains wakes. `seq` invariance is deliberately not asserted: a cursor motion legitimately produces its own redraw batch, so the no-bump property is structural.
> - The startup buffer is a markdown `Untitled.md` that is itself reported, so the map legitimately holds more than one entry; resolve the opened file's bufnr from `Snapshot.win_buffers` rather than assuming a single key.
> - A background `bufload` case is what pins the `win_gettype()` guard; without it nvim's synthetic autocmd window reports a fabricated cursor that passes every other guard.

**(b) `markdown_render.rs` Covers bullet, line 248.** Extend the existing bullet so the inline-anchoring half of the coverage is named:

> - The public preview render surface (`render_html`, `render_html_with_highlights`, `code_blocks`, `theme_css`, `PreviewTheme`, `WidthMode`): `wl-{L}-{C}` anchor placement in both tiers — column-zero markers collapsing to a block's opening tag, and column markers landing inside a paragraph, heading or list item — the code-block, table and image-alt exclusions including the nested-in-a-container cases, MathML with inlined fonts, the paired dark/light palettes, and a sweep asserting every vertical dimension in the shipped `base.css` lands on the baseline grid.

### 2.7 `crates/wolf-ffi/README.md` — 1 hunk

**Hunk (lines 9–271).** HEAD: the 5-row `## Layout` table. `main`: 256 lines of the pre-refactor per-method bridge narrative.

**Resolution: keep HEAD's table; delete `main`'s entire side.** Rule: crate README is the outside view; the bridge surface belongs to `src/README.md`, which HEAD owns. No further edits to this file.

### 2.8 `crates/wolf-ffi/src/README.md` — 1 hunk

**Hunk (lines 62–66).** HEAD: the `FfiColor` paragraph. `main`: the pre-refactor `FfiPreview` mega-bullet.

**Resolution: keep HEAD's `FfiColor` paragraph; delete `main`'s side.**

**Then, outside the markers, one required edit — the two new accessors are undocumented.** In the `### Preview` section, insert after the `FfiPreview::source_full_width()` bullet (line 173):

> - `FfiPreview::source_cursor_line() -> u32` / `source_cursor_col() -> u32` — the source buffer's editing cursor, joined in `previews()` from `Snapshot.markdown_cursors` by `source_bufnr`. `line` is 1-based to match the `wl-{L}-{C}` markers and `col` is a 0-based character column, which the front-end rounds down to a marker stride. A `line` of `0` means the cursor is unknown, and the front-end must not scroll for it. Both are opaque-type accessors, so they are additive and `wolf_ffi_wake.h` is unaffected.

### 2.9 `crates/wolf-ffi/tests/README.md` — 1 hunk

**Hunk (lines 48–52), inside `#### \`attribute_highlight.rs\``.** HEAD: the one-paragraph Covers for attribute highlighting. `main`: the pre-refactor `preview.rs` narrative, mis-aligned into the wrong entry.

**Resolution: keep HEAD's paragraph; delete `main`'s side.**

**Then, outside the markers, extend the `preview.rs` Covers paragraph at line 319** so the cursor-join coverage `main` added is named. Append to that sentence:

> …and `preview_file(win, path)` link-follow, plus the scroll-sync cursor join — a row's `source_cursor_line()` / `source_cursor_col()` matching `Snapshot.markdown_cursors` for its own source buffer, and the `line == 0` sentinel a background-loaded source produces.

### 2.10 `crates/wolf-gnome/README.md` — 1 hunk

**Hunk (lines 25–156).** HEAD: one sentence about the binary taking no arguments. `main`: 130 lines of the pre-refactor `## Architecture wiring` narrative.

**Resolution: keep HEAD's sentence; delete `main`'s entire side.** Nothing in `main`'s side concerns the new feature — GTK has no preview pane. No further edits.

### 2.11 `crates/wolf-macos/README.md` — 3 hunks

| Hunk    | HEAD's side                                          | `main`'s side                                 | Resolution                  |
| ------- | ---------------------------------------------------- | --------------------------------------------- | --------------------------- |
| 17–19   | the `main.swift` / `AppDelegate` entry-point paragraph| the `WolfKeyMap.translate` narrative           | **Take HEAD**, delete `main` |
| 161–165 | "The AppKit half of the crate has no automated coverage…" | the frame-stats narrative                 | **Take HEAD**, delete `main` |
| 176–194 | the whole `## Bundle contents` section (table + two paragraphs) | one huge paragraph covering every feature ever shipped | **Take HEAD**, delete `main` |

Rule: `crates/<crate>/README.md` is the outside view with no per-file entries. Every scroll-sync fact in hunk 3 is carried into the three directory READMEs per §3.4–§3.7. No further edits to this file.

### 2.12 `crates/wolf-macos/Sources/README.md` — 1 hunk

**Hunk (lines 8–104).** HEAD: one sentence, "Each directory's README owns the entries for the files in that directory, and nothing else describes them." `main`: 95 lines of per-file `WolfKeyMap` bullets.

**Resolution: keep HEAD's sentence; delete `main`'s side.** Note `main`'s side opens "Forty deliberately AppKit-free files" — a count that would be wrong the moment a file lands; HEAD deliberately carries no count. No further edits.

### 2.13 `crates/wolf-macos/Tests/README.md` — 1 hunk

**Hunk (lines 3–175).** HEAD: two lines pointing at `WolfKeyMapTests/README.md` plus the no-AppKit-target note. `main`: 170 lines including a full seam roll-call (which does name `PreviewScrollSeamTests.swift`) and per-file test sections.

**Resolution: keep HEAD's two lines; delete `main`'s side.** The `PreviewScrollSeamTests.swift` entry lands in `Tests/WolfKeyMapTests/README.md` instead (§3.7). No further edits to this file.

### 2.14 `runtime/nvim/README.md` — 2 hunks

| Hunk    | HEAD's side                                        | `main`'s side                                                    | Resolution                  |
| ------- | -------------------------------------------------- | ---------------------------------------------------------------- | --------------------------- |
| 183–190 | `#### \`wolf_preview_file(win, path)\`` heading    | pre-refactor `wolf_open_preview` bullets                          | **Take HEAD**, delete `main` |
| 202–213 | `#### \`wolf_name_untitled(...)\`` plus paragraph  | pre-refactor `wolf_setup_preview` bullets, incl. `wolf_markdown_cursor` | **Take HEAD**, delete `main` |

**Then, outside the markers, five required edits.** `runtime/nvim/README.md` owns `init.lua`, so every fact `main` added about it lands here.

**(a) `#### \`wolf_setup_preview(chan)\`` (lines 91–95).** Change the opening of line 93 from "Registers two things in one augroup" to "Registers three things in one augroup". Then, after the `wolf_markdown_dirty` paragraph at line 95, insert a new paragraph and rewrite that paragraph's closing sentence:

> `CursorMoved`/`CursorMovedI`/`BufEnter`/`BufWinEnter` autocmds notify **`wolf_markdown_cursor`** with three flat scalars `(source_bufnr, line, char_col)` — a 1-based `line('.')` and a 0-based `charcol('.') - 1`. That column is a character index rather than a byte index, which is what lets it pair with the `wl-{L}-{C}` markers with no conversion anywhere downstream; core upserts it into `Snapshot.markdown_cursors`. Five guards decide whether to emit: the buffer must be valid, must be current, its window must be a normal window, its filetype must be `markdown`, and `b:wolf_preview_source` must be nil.
>
> Exposes `_G.wolf_emit_previews` and `_G.wolf_emit_markdown_cursor`.

(Delete the trailing "Exposes `_G.wolf_emit_previews`." from line 95 so it is not stated twice.)

**(b) `#### \`wolf_emit_current_file\`, …` heading (line 97).** Add the new closure to the heading: `#### \`wolf_emit_current_file\`, \`wolf_emit_buffers\`, \`wolf_emit_full_width\`, \`wolf_emit_previews\`, \`wolf_emit_markdown_cursor\`, \`wolf_emit_drawer\``.

**(c) `#### \`wolf_open_preview(mode)\`` (lines 171–175).** Append one sentence to the `mode` paragraph at line 175:

> It re-emits the source buffer's cursor before any placement branch, because `split_right` and `active` make the preview scratch current and the cursor emit's preview-scratch guard would then bail, leaving a freshly opened preview with no anchor.

**(d) `wolf_preview_file` (lines 192–194).** Append one sentence to the paragraph at line 194:

> It deliberately does not re-emit the cursor: the source is loaded in the background with no window, so it carries no meaningful position.

**(e) `## Decisions` — add one new paragraph** (place it immediately after the "A background `nvim_win_set_buf` does fire `BufEnter` and `BufWinEnter`" paragraph at line 265, whose flavor it matches):

> **The cursor emit needs both buffer-entry events and the `win_gettype()` guard, and neither is redundant.** `:edit foo.md` fires `CursorMoved` before filetype detection, so the steady-state autocmds alone would leave an opened-but-untouched buffer reporting no cursor at all. Adding `BufEnter` and `BufWinEnter` fixes that but opens a hole: `vim.fn.bufload()` fires both inside nvim's synthetic autocmd window, where the target buffer is genuinely current and every other guard passes, over a fabricated line 1 that belongs to no user pane. Only a normal window carries a real cursor, so the guard is what preserves the `line == 0` unknown sentinel a background-loaded source must report.

---

## 3. Files to modify that are **not** conflicted (mandatory)

These are HEAD-authored documents written before `main`'s feature existed. Each is either stale against the merged source or is the sole owner of a file that no document describes.

### 3.1 Coverage audit — every source file `main` added, and its one owner

| Source file `main` added or changed                            | Owning document                       | State                             |
| -------------------------------------------------------------- | ------------------------------------- | --------------------------------- |
| `Sources/WolfKeyMap/PreviewScrollSeam.swift`                    | `Sources/WolfKeyMap/README.md`        | **Missing — gap, add (§3.5)**     |
| `Sources/WolfMacOS/PreviewScrollStateHandler.swift`             | `Sources/WolfMacOS/README.md`         | **Missing — gap, add (§3.6a)**    |
| `Sources/WolfMacOS/PreviewNavigationDelegate.swift`             | `Sources/WolfMacOS/README.md`         | Present but stale — update (§3.6b)|
| `Sources/WolfMacOS/PaneContainerView.swift`                     | `Sources/WolfMacOS/README.md`         | Present but stale — update (§3.6c)|
| `Tests/WolfKeyMapTests/PreviewScrollSeamTests.swift`            | `Tests/WolfKeyMapTests/README.md`     | **Missing — gap, add (§3.7)**     |
| `crates/wolf-core/tests/markdown_cursor.rs`                     | `crates/wolf-core/tests/README.md`    | **Missing — gap, add (§2.6a)**    |
| `runtime/nvim/init.lua` (cursor emit)                           | `runtime/nvim/README.md`              | Stale — update (§2.14)            |
| `crates/wolf-core/src/markdown.rs` (two-tier placement)          | `crates/wolf-core/src/README.md`      | **Contradicts source** — (§2.4c)  |
| `crates/wolf-core/src/{render,redraw}.rs` (`markdown_cursors`)   | `crates/wolf-core/src/README.md`      | Missing — add (§2.4a, §2.4b)      |
| `crates/wolf-ffi/src/lib.rs` (two accessors)                    | `crates/wolf-ffi/src/README.md`       | Missing — add (§2.8)              |
| `crates/wolf-core/src/markdown/base.css` (mid-text anchors)      | `src/markdown/README.md`              | Incomplete — update (§2.5a)       |

No new source file is left undocumented after these edits, and no source file is described in two documents.

### 3.2 Inverse check — prose that must **lose** its home

| `main`'s prose                                                  | Where it must **not** land                        | Where it lands instead                                  |
| ---------------------------------------------------------------- | ------------------------------------------------- | ------------------------------------------------------- |
| Full `PreviewScrollSeam` internals (update strategy, capture rules, JS ordering) | `architecture.md`, `crates/wolf-macos/README.md` | `Sources/WolfKeyMap/README.md` + `Sources/WolfMacOS/README.md` |
| The frame-stats bucket table and its caveats                     | `build.md`, `crates/wolf-macos/README.md`        | `Sources/WolfKeyMap/README.md` → `FrameStats.swift`     |
| "~965 KiB base64 head, so the front-end swaps the body"         | `crates/wolf-core/src/markdown/README.md`         | `architecture.md` boundary sentence + `PaneContainerView.swift` |
| Marker-rule detail                                               | `crates/wolf-core/README.md`, `architecture.md`   | `crates/wolf-core/src/README.md` → `markdown.rs`        |
| Per-test narratives for `markdown_render.rs` / `preview.rs`      | crate-level READMEs                               | the respective `tests/README.md` entries                |

### 3.3 `crates/wolf-macos/DECISIONS.md`

**Line 73 is false.** Replace:

> - Markdown preview ships one theme. Cursor-follow anchors to the containing block's top inside a fenced code block, a table or an image's alt text, and inside a paragraph hard-wrapped across several short source lines; the marker rules are in [`wolf-core/src/README.md`](../wolf-core/src/README.md).

Do **not** add a new `## Preview scroll-sync` section to this file. The in-place-versus-navigate model is a per-file decision belonging to `PreviewScrollSeam.swift` and `PaneContainerView.swift`; a section here would duplicate them.

### 3.4 `Sources/WolfKeyMap/README.md` — `FrameStats.swift`

Add one bullet to the entry's `**Gotchas**` (after line 222):

> - `prev.html` is shared by both document-update paths — the `loadHTMLString` call and the `callAsyncJavaScript` issue — because both measure the same thing, dispatching one update. It times only the synchronous dispatch, never the WebKit parse, layout or JavaScript execution that dispatch queues.

### 3.5 `Sources/WolfKeyMap/README.md` — new `PreviewScrollSeam.swift` entry

Insert alphabetically between `#### \`PreviewLinkSeam.swift\`` (line 442) and `#### \`PreviewSeam.swift\`` (line 461):

> #### `PreviewScrollSeam.swift`
>
> Purpose: Every decision behind Markdown-preview scroll-sync — which marker a source cursor anchors to, whether that anchor's position warrants scrolling, how a fresh render reaches the page, and every JavaScript string the preview web view evaluates. Foundation only.
>
> **Public API**
>
> - `CursorPos` — `line` is 1-based with `0` meaning unknown, `col` a 0-based character column. `anchorColumn(_:)` and `anchorIds(line:col:)` give the id preference order the injected script mirrors.
> - `edgeThreshold(viewportHeight:)` and `followDecision(anchorTop:viewportHeight:)` — the comfort band, inclusive at both ends, and where an anchor lands once it leaves it.
> - `winsNeedingScroll(current:applied:)`, `captureDecision(…)`, `splitDocument(_:)`, `updateStrategy(…)`, `loadCommitOutcome(remaining:pending:succeeded:)` — the per-window gates; each carries its ordered rules on its own doc comment.
> - `stateHandlerName`, `stateScriptJS(handlerName:)`, `followJS(line:col:)`, `restoreJS(_:)`, `inPlaceUpdateArgumentNames`, `inPlaceUpdateJS()`.
>
> **Decisions**
>
> - Every tuning constant is interpolated into the generated JavaScript rather than written twice, so a test proves what the page actually runs.
> - `markerColumnStride` mirrors `wolf_core::markdown::MARKER_COLUMN_STRIDE`; a change on the Rust side has to be mirrored here, and a test pins it.
> - The capture and commit rules live here rather than in `PaneContainerView` because both shipping-blocker defects found in review were ordering bugs in that untestable AppKit block. The caller keeps only a mechanical `switch`.
> - `updateStrategy` compares head lengths rather than hashing, so a future theme or width selector degrades to a full load instead of stale CSS.
>
> **Gotchas**
>
> - The fallback sweep filters on `data-col="0"` and must never be widened: only that subsequence is in ascending document order. See the marker rules in [`wolf-core/src/README.md`](../../../wolf-core/src/README.md).
> - `stateHandlerName` and `PreviewSeam.clickFocusHandlerName` are registered on the same `WKUserContentController`, so they must differ and both must be removed on teardown.

### 3.6 `Sources/WolfMacOS/README.md` — three edits

**(a) New entry, `PreviewScrollStateHandler.swift`,** inserted between `PreviewNavigationDelegate.swift` (line 438) and `Project.swift` (line 452):

> #### `PreviewScrollStateHandler.swift`
>
> Purpose: The `WKScriptMessageHandler` that forwards a preview page's pushed scroll state, posted by `PreviewScrollSeam.stateScriptJS`, into its `PaneContainerView`'s per-window cache.
>
> **Public API**
>
> - `init(win:container:)` — `win` is the Neovim window handle identifying the pane in the container's per-window maps.
>
> **Decisions**
>
> - The page pushes rather than being polled. A full `loadHTMLString` destroys `window`, so nothing survives a reload inside the page, and `evaluateJavaScript` is asynchronous, so state cannot be pulled in the instant before one. A continuously refreshed Swift-side copy makes the pre-reload capture a plain dictionary read.
> - A body missing `s` degrades to `0` rather than dropping the message, so a partially decodable push still refreshes the cache.
> - `container` is weak: the web view's `WKUserContentController` retains this handler strongly.

**(b) `PreviewNavigationDelegate.swift` entry (lines 438–450) — add one Decisions bullet** after the existing weak-reference bullet:

> - The two `didFail…` callbacks are routine bookkeeping, not an error path: they decrement that window's outstanding-navigation count so `PreviewScrollSeam.updateStrategy`'s in-flight rule cannot pin the window on the full-load path forever. A count rather than a flag, because two navigations can straddle one page load and WebKit reports the superseded one as a failure. A cancelled policy decision — every clicked link — also arrives here, so the container ignores a callback for which it counted no navigation.

**(c) `PaneContainerView.swift` entry (lines 315–332) — add three Decisions bullets and one Gotcha.** Keep the existing bullets; append:

> - A `changedtick` bump does not necessarily navigate the web view. `PreviewScrollSeam.updateStrategy` picks per window between `loadHTMLString` and an in-place body swap, and the typing path takes the swap, which is what stops the inlined base64 font CSS in the head from being re-shipped and re-parsed on every keystroke.
> - The in-place body travels as a real `callAsyncJavaScript` argument in the `.page` content world, never interpolated into JavaScript source. A wrong content world returns the falsy sentinel on every call, pinning every window on the full-load path silently and with a green suite.
> - A failed in-place update clears that window's live-document descriptor and re-arms the reload gate, so the next pass takes the full path — one extra reload, no loop, and recovery once a load commits.
>
> **Gotchas** (append)
>
> - Two script-message handlers are registered on one preview web view's content controller — click-to-focus and scroll state. Both must be removed when the web view is torn down, or the controller keeps the container alive.

### 3.7 `Tests/WolfKeyMapTests/README.md` — new entry

Insert alphabetically after `#### \`PreviewSeamTests.swift\`` (line 250) and before `#### \`RestoreSeamTests.swift\`` (line 262):

> #### `PreviewScrollSeamTests.swift`
>
> **Covers**
>
> - `PreviewScrollSeam`'s anchor arithmetic (`anchorColumn`, `anchorIds` and their dedup at column zero), the edge-threshold band including its inclusive boundaries and the short-viewport cap, `winsNeedingScroll`, and the ordered rules of `captureDecision`, `splitDocument`, `updateStrategy` and `loadCommitOutcome`.
> - The synthesized JavaScript: that each tuning constant's rendered value appears in the generated source, and that `inPlaceUpdateJS`'s free identifiers are exactly `inPlaceUpdateArgumentNames`.
>
> **Gotchas**
>
> - Scalar and string inputs only — no AppKit, no `WKWebView`, no Neovim. The on-screen scrolling and the `PaneContainerView` wiring are compile-and-manual-smoke only.
> - `markerColumnStride` is pinned against `wolf_core::markdown::MARKER_COLUMN_STRIDE`; that assertion is the only automated guard on a cross-language constant.

---

## 4. Test plan

**No tests are added, changed, or removed.** Documentation-only resolution on an already-clean source merge; `main`'s tests merged cleanly and are the verification surface. The coder confirms the merged tree is still green, and does not extend it.

**Behaviors to confirm still pass, unmodified:**

- `//crates/wolf-core:markdown_render` and `//crates/wolf-core:wolf_core_lib_test` — the two-tier marker placement.
- `//crates/wolf-core:markdown_cursor` — the Lua-to-core cursor signal.
- `//crates/wolf-ffi:preview` — the FFI cursor join.
- `//crates/wolf-macos:WolfKeyMapTests` — includes `PreviewScrollSeamTests.swift`, already listed in `srcs` at `crates/wolf-macos/BUILD.bazel:289`.

**Wiring already verified, do not touch:** `PreviewScrollSeam.swift` at `BUILD.bazel:91`, `PreviewScrollSeamTests.swift` at `:289`, the `markdown_cursor` `rust_test` at `crates/wolf-core/BUILD.bazel:461` with staged runfiles and `tags = ["no-sandbox"]`, and `Sources/WolfMacOS/*.swift` is a glob so `PreviewScrollStateHandler.swift` compiles. No `BUILD.bazel` edit is needed or permitted.

**Edge cases the coder must check by inspection, not by test:**

- No `<<<<<<<`, `=======` or `>>>>>>>` remains anywhere: `git diff --check` and a repo-wide grep.
- No file outside `*.md` appears in the working-tree diff.
- No entry describes a file that another entry also describes.

---

## 5. Implementation order

1. **Resolve the pure take-HEAD files first** — no judgment, fastest confidence: `crates/wolf-gnome/README.md`, `crates/wolf-ffi/README.md`, `crates/wolf-core/README.md`, `crates/wolf-macos/README.md`, `crates/wolf-macos/Sources/README.md`, `crates/wolf-macos/Tests/README.md`.
2. **Resolve the remaining eight conflicts**, HEAD-side only, deleting `main`'s side, without yet making any post-conflict edit: `architecture.md`, `build.md`, `crates/wolf-core/src/README.md`, `crates/wolf-core/src/markdown/README.md`, `crates/wolf-core/tests/README.md`, `crates/wolf-ffi/src/README.md`, `crates/wolf-ffi/tests/README.md`, `runtime/nvim/README.md`.
3. **Verify zero markers remain** before writing a single word of new prose. This checkpoint separates "merge resolved" from "docs corrected".
4. **Fix the contradictions** — where prose asserts the opposite of the merged code, in dependency order (core outward): §2.4c, §2.5, §2.1 deferred bullets, §3.3.
5. **Fill the additive gaps** — new facts about existing files: §2.4a/b, §2.8, §2.14a–e, §2.1 preview paragraphs, §2.9, §2.6b, §3.4, §3.6b/c.
6. **Add the four missing entries** last, so they can link to prose that already exists: §2.6a, §3.5, §3.6a, §3.7.
7. **Format, lint, test** (§7).
8. **Do not commit.** Leave the merge staged-but-uncommitted for the reviewer. The orchestrator decides when to conclude the merge.

---

## 6. Dependencies

**None.** No new external packages, no Bazel targets, no `MODULE.bazel` or `Cargo.toml` change. `prettier` is already the repo's Markdown formatter and `tools/lint/markdown_check.sh` already exists.

---

## 7. Lint considerations and verification

**Formatting.** Run `prettier --write` on every file touched. `.prettierrc` sets `proseWrap: never`, so each paragraph stays one long line and prettier will never split it — shortening a paragraph is an authorial act the coder must perform. Re-run `prettier --write` afterwards and confirm no further change.

**The Markdown checker.** `./tools/lint/markdown_check.sh <files>` (or `bazel test //tools/lint:markdown`, which covers only per-package `README.md` and `DECISIONS.md` — `architecture.md` and `build.md` must be passed to the script by hand). Advisory and `manual`-tagged, but every finding below is real for this change:

- **`long-line`** — a paragraph over 800 characters. `main`'s prose routinely exceeds this; any paragraph composed from it must be split.
- **`changelog`** — bans `now`, `no longer`, `previously`, `the old`, `renamed from`, `this increment`, `used to be`. `main`'s side is saturated with these. Every replacement above is present tense; do not reintroduce the banned words when paraphrasing.
- **`allcaps`** — `main`'s prose shouts constantly. Use bold sparingly or lowercase. The allowlist covers genuine acronyms and tokens containing a digit.
- **`entry-budget`** — 250 words per `####` entry. Most at risk: `markdown.rs` (§2.4c) and `PreviewScrollSeam.swift` (§3.5). If either trips, tighten by merging bullets, never by dropping a decision.
- **`big-file`** — warning only, at 50,000 bytes. Expect this change to run net-negative in bytes, which is correct.

**No linter ignore directives are needed or appropriate.** The checker has no suppression syntax; none should be invented.

**Test commands (macOS host, so the Bazel path per `build.md`):**

```
bazel test //...
```

`//tools/lint/...` and `//crates/wolf-macos:WolfRelease` are `manual`-tagged and excluded from that wildcard; run the linters explicitly:

```
bazel test --config=lint //tools/lint:markdown
./tools/lint/markdown_check.sh architecture.md build.md
```

**Known pre-existing flake:** `//crates/wolf-core:redraw_grid` (`scroll_up_shifts_grid_content_down`) is a real-Neovim timing flake, bisected to pre-date this branch. A single failure there is not caused by this change; re-run before investigating.

**Final gate:**

- `git diff --name-only --diff-filter=U` is empty.
- A repo-wide grep for `^<<<<<<<`, `^=======`, `^>>>>>>>` returns nothing outside `.claude/`.
- `git status` shows no modified file whose extension is not `.md`.
- `prettier --write` on the touched files is a no-op.

---

# Amendment 1 — closing the four `entry-budget` findings

**Orchestrator: approved.** The relocations are legitimate under CLAUDE.md's charter for `DECISIONS.md` ("cross-cutting constraints … decisions with a multi-year half-life"), not a dodge around the word budget: the marker scheme spans two crates and two languages, and the preview-update model is jointly honored by four files. Both are net-negative. The architect's §E duplicate-ownership removals are mandatory and land in the same pass.

## Ruling summary

| Entry                    | Over by | Ruling                                            |
| ------------------------ | ------- | ------------------------------------------------- |
| `render.rs` (284)        | 34      | Cut in place                                       |
| `FrameStats.swift` (285) | 35      | Cut in place                                       |
| `markdown.rs` (411)      | 161     | Relocate to `crates/wolf-core/DECISIONS.md`        |
| `PaneContainerView.swift` (413) | 163 | Relocate to `crates/wolf-macos/DECISIONS.md`   |

**§3.3 is reversed.** The in-place-versus-navigate model is jointly honored by four files across two directories: `PreviewScrollSeam` owns the rules, `PaneContainerView` the bookkeeping, `PreviewScrollStateHandler` the state channel, `PreviewNavigationDelegate` the navigation count. Leaving it inside one file's entry means the other three can be changed by an agent who never reads it — the exact failure `DECISIONS.md` prevents.

## A. `render.rs` — cut in place

In `crates/wolf-core/src/README.md`, under `#### \`render.rs\``:

1. Trim the `HighlightTable::resolve` bullet to drop its parenthetical (owned by `architecture.md`'s render-state contract): "- `HighlightTable::resolve` applies defaults and reverse video, so a front-end paints `fg`/`bg` verbatim."
2. Delete the sentence "Grid 1 (`OUTER_GRID_ID`) always exists but is never painted as window content." Owned by `architecture.md`'s grid description. Keep the preceding sentence.
3. Trim the `WinInfo.hidden` bullet to: "- `WinInfo.hidden` is always `false`: a `win_hide` removes the window from `windows` outright rather than flagging it. The field exists to keep the FFI and Swift surface stable." The dropped clause is owned by `architecture.md`'s current-tabpage rule.

Keep the new `markdown_cursors` bullet untouched. **Measure, do not trust the ≈248 estimate.** If still over: delete the `HighlightTable::resolve` bullet outright — `architecture.md` states it in full.

## B. `FrameStats.swift` — cut in place

In `crates/wolf-macos/Sources/WolfKeyMap/README.md`, under `#### \`FrameStats.swift\``:

1. Delete the bullet listing `record(_:nanos:)`, `reset()`, `summarize(…)`, `format(_:)`, `percentile(_:_:)`, `isEnabled(env:)`, `reportInterval(env:)` in full — a signature restatement, banned by name. Keep the `FrameStatsBucket` bullet.
2. Trim the percentile bullet to: "- Percentiles are nearest-rank, not interpolated, so every number in the table was actually observed. Only the percentile window is capped; `count`, the sum and `max` stay exact, and a bucket past the cap reports `truncated`."

Keep the new `prev.html` gotcha untouched. Estimate ≈246.

## C. `markdown.rs` — relocate

### C.1 New section in `crates/wolf-core/DECISIONS.md`

Insert between `## Browser preview security` and `## Present limitations` (verify both headings exist; if not, place it before `## Present limitations`). Exact prose:

```markdown
## The preview position-marker contract

`render_html` seeds every rendered document with position markers — `<span class="wolf-src" id="wl-{L}-{C}" data-line="{L}" data-col="{C}"></span>` — one per hard source line at column zero, plus one every `MARKER_COLUMN_STRIDE` characters along a longer line. `L` is a 1-based hard buffer line and `C` a 0-based character column, matching what `wolf_markdown_cursor` reports, so nothing along the path converts between bytes and characters. pulldown-cmark is chosen for `Parser::into_offset_iter`, which pairs each event with its source byte range; a plain event iterator cannot support this scheme at all.

Placement has two tiers. A column-zero marker is flushed as a raw span immediately before its block's opening tag. A `col != 0` marker inside a paragraph, heading, blockquote, list or footnote definition is deferred and split into the `Event::Text` that contains it, so it lands at its true position in the rendered inline flow rather than collapsing to the block top.

Three properties are load-bearing on consumers and must survive any rewrite. Every marker the rule predicts exists exactly once, because a marker is deferred rather than dropped. Only the column-zero markers are guaranteed to appear in ascending document order — a block's inline markers follow a later line's column-zero marker — so a consumer sweeping the document must filter on `data-col="0"`. And `MARKER_COLUMN_STRIDE` is mirrored in Swift as `PreviewScrollSeam.markerColumnStride`, a cross-language coupling one test pins.

Splitting is guarded, and each guard has a reason. A `Text` is split only when `source[range]` matches the payload byte for byte: a length check passes a same-length entity such as `&nvap;` and then corrupts it at a character boundary, and a failed match defers the marker instead of dropping it. The split point snaps to the nearest word boundary within `MARKER_SNAP_MAX_BYTES`, because an inline-box edge breaks glyph shaping across it and a mid-word cut loses a ligature or kern pair; with no boundary in the window it splits at the exact offset. The id and `data-*` attributes always name the exact `(L, C)`.

Injection is suppressed inside code blocks, HTML blocks, tables and image alt text, and those markers keep the before-the-tag placement. A raw span between `<tbody>` and `<tr>` is foster-parented out of the table by the HTML parser. An `alt` attribute is built by pulldown-cmark's `raw_text`, which drops an `Event::Html` outright — which is also why a marker is emitted as `Event::Html` and never as `Event::InlineHtml`, so a leak loses one anchor instead of corrupting the attribute.

What the markers cost the stylesheet is in [`src/markdown/README.md`](src/markdown/README.md); the granularity that remains out of reach is in [`architecture.md`](../../architecture.md).
```

### C.2 Trim the entry

In `crates/wolf-core/src/README.md` under `#### \`markdown.rs\``: delete the four marker-placement bullets (the two-tier rule, the byte-identity guard, the snap rule, the exclusions/document-order bullets) and insert in their place:

> - Marker placement, its byte-identity guard, its exclusions, and the document-order guarantee consumers depend on are one contract with a long half-life: see [the preview position-marker contract](../DECISIONS.md).

One sentence is **not** part of the marker contract and must be preserved as its own bullet:

> - Math and highlighted code are `Event::InlineHtml`, so the writer passes them through verbatim and the injector never reads them as a block start.

Estimate ≈240.

## D. `PaneContainerView.swift` — relocate

### D.1 New section in `crates/wolf-macos/DECISIONS.md`

Insert between `## One flat pane view, not nested split views` and `## Present limitations`. Exact prose:

```markdown
## How a preview reaches its web view

Four files jointly honor one model, so do not change any of them without reading this section: `WolfKeyMap/PreviewScrollSeam` owns the rules, `PaneContainerView` the bookkeeping, `PreviewScrollStateHandler` the state channel, and `PreviewNavigationDelegate` the navigation count.

A rendered document reaches the page one of two ways, chosen per preview window by `PreviewScrollSeam.updateStrategy`. The typing path swaps only the body in place, which is what stops the inlined base64 font CSS in the head from being re-shipped across the WebKit boundary and re-parsed on every keystroke. Every other case navigates with `loadHTMLString`. The swap is sound only because the in-app render pins one theme and one width, so the head is byte-identical every time; the strategy compares head lengths so that premise is self-enforcing rather than a comment.

The in-place body travels as a real `callAsyncJavaScript` argument, never interpolated into JavaScript source, and it must run in the content world `addUserScript` installed the page script into. A wrong world returns the falsy sentinel on every call, pinning every window on the full-load path silently and with a green suite. That is the one failure here no test catches, so it is what the manual smoke check is for.

Scroll state is pushed out of the page, never pulled. A full load destroys `window`, so nothing can be stashed in the page across a reload, and `evaluateJavaScript` is asynchronous, so state cannot be read in the instant before one. A continuously refreshed Swift-side cache makes the pre-reload capture a plain dictionary read with no race.

Navigations are counted, not flagged. Two can straddle one page load, because the reload coalescer does not wait for `didFinish`, and WebKit reports the superseded one as a failure — so a failure callback is a routine part of the typing path, not an error path. A count lets a superseded callback decline to commit anything; a flag let it clear the gate out from under a still-navigating load, which is what put the flicker back.

Recovery is one step and self-limiting. A failed in-place update clears that window's live-document descriptor and re-arms the reload gate, so the next pass takes the full path: at most one extra load, no loop, and normal service resumes as soon as one commits.
```

### D.2 Trim the entry

In `crates/wolf-macos/Sources/WolfMacOS/README.md` under `#### \`PaneContainerView.swift\``:

1. Delete the three preview Decisions bullets added in §3.6c. Replace with: "- Preview updating is a four-file model this class implements only part of; read [how a preview reaches its web view](../../DECISIONS.md) before changing the strategy switch, the navigation count, or the live-document bookkeeping."
2. Replace the occlusion bullet with: "- A grid is skipped only when an opaque, loaded preview web view fully covers it, and the loaded flag survives reloads, because WebKit keeps displaying the previous document while the next one loads." (Containment is owned by `architecture.md` decision 2 and `PaneOcclusion.swift`; the reload behavior is local and must not be lost.)
3. Delete the `onRenderWake()` Public API bullet — a signature restatement.
4. Tighten Purpose to: "Purpose: The editor area — one flat, flipped `NSView` that paints every split pane itself, owns the pure `PaneTree`, hosts the preview web views and overlays, and is the editor's first responder. The flat-view choice is in [DECISIONS.md](../../DECISIONS.md)."

Keep the other Decisions bullets and the Gotcha. **Measure — the ≈250 estimate is too close to call.** If still over, apply in order and stop as soon as it passes: (i) fold the Gotcha's second sentence into the first; (ii) drop `layoutSnapshot()`'s "save and restore the split layout" clause, keeping only the `nil`-for-a-lone-empty-pane fact. Do not cut the remaining Decisions bullets.

## E. Duplicate-ownership removals — required, same pass

1. **`Sources/WolfMacOS/README.md` → `PreviewScrollStateHandler.swift`**: delete the push-not-pull Decisions bullet, replace with "- The push direction, and why it cannot be a pull, is in [DECISIONS.md](../../DECISIONS.md)." Keep the `s`-degrades-to-`0` and weak-reference bullets.
2. **`Sources/WolfMacOS/README.md` → `PreviewNavigationDelegate.swift`**: replace the §3.6b bullet with "- The two `didFail…` callbacks are routine bookkeeping, not an error path: they decrement that window's outstanding-navigation count — see [DECISIONS.md](../../DECISIONS.md) for why it is a count. A cancelled policy decision, meaning every clicked link, also arrives here, so the container ignores a callback for which it counted no navigation."
3. **`Sources/WolfKeyMap/README.md` → `PreviewScrollSeam.swift`** (at exactly 250, so this is also its headroom): delete the `updateStrategy` head-length Decisions bullet; retarget the fallback-sweep Gotcha to "- The fallback sweep filters on `data-col=\"0\"` and must never be widened — see [the position-marker contract](../../../wolf-core/DECISIONS.md)." Drop the restated ascending-order clause; the link carries it. Keep the constant-interpolation, `markerColumnStride`, capture-rules bullets and the two-handler Gotcha.
4. **`crates/wolf-core/src/README.md` → `markdown.rs`** — covered by C.2.

**Checked clean, no action:** `base.css`'s entry describes the stylesheet rule, not placement. `architecture.md`'s deferred bullet describes the remaining limitation, not the rule. Neither `DECISIONS.md` contained overlapping prose before this change.

## F. Confirmations

- **"Word boundary" is correct**; §2.4c's "nearest whitespace" was wrong. `snap_split_point` uses `char::is_whitespace` on either side of a candidate offset, and the source names that a word boundary. Keep the coder's wording. The relocated prose additionally records the no-boundary-in-window fallback (split at the exact offset), which an agent would otherwise assume means the marker is dropped.
- **`//crates/wolf-core:commands` flake — follow-up only, nothing in this pass.** Do not write it into `tests/README.md`: a flake is not a present-tense property of the code, and it would be stale the moment it is fixed. Both it and `redraw_grid` are real-Neovim `no-sandbox` targets that spawn a child; if it recurs, check whether they share a child-reaping or output-capture path before treating them as two bugs.

## G. Verification for this amendment

1. Apply A, B, C, D, then E last.
2. `prettier --write` on the five touched files.
3. `./tools/lint/markdown_check.sh` on them — expect **zero** findings of every class. The new `##` sections are not entry-budget-counted but are still subject to `long-line`, `changelog` and `allcaps`.
4. Confirm both `DECISIONS.md` files stay under the 50,000-byte `big-file` warning.
5. Re-run `bazel test //...`. Green expected; treat `commands` or `redraw_grid` failures as the known flakes and re-run in isolation.
6. Confirm the working-tree diff still contains no non-`.md` file.

---

# Amendment 2 — orchestrator rulings on the two open items

Amendment 1 landed: `render.rs` 284→248, `FrameStats.swift` 285→248, `markdown.rs` 411→211. `bazelisk test //...` green at 93/93. Two items needed a ruling.

## Ruling 1 — `PaneContainerView.swift` stalled at 276

Both amendment tie-breakers were applied and it did not converge (288→276). **Relocate the two-handler teardown Gotcha into `## How a preview reaches its web view`.** It is a lifecycle constraint binding the same four files, so it fits that section's charter, and it is the only option on the table that drops no decision. The five surviving non-preview Decisions bullets stay fenced — do not cut one to make a word count. If it still does not pass afterward, **accept the documented overage** rather than lose a decision an agent would otherwise undo.

## Ruling 2 — the relocations ran net-positive (+219 and +177), and that is accepted

CLAUDE.md's "expect to run net-negative as often as net-positive" targets editing passes over existing docs. This merge lands a real feature across four new source files, so some growth is legitimate, and `DECISIONS.md` is the correct home for both contracts independent of the word count. The sections stay.

**But** new prose does not enter unaudited on the back of a lint fix. One pass over only the two new `##` sections, applying the two-question test sentence by sentence.

Already ruled: D.1's "a flag let it clear the gate out from under a still-navigating load, which is what put the flicker back" is **history** — it narrates a past regression, banned by name. Rewrite present-tense as the failure a flag would cause. The `changelog` linter does not catch this shape, since it greps for specific words; check both sections for other past-tense constructions of the same form.

Kept as load-bearing (each passes question 1): the no-word-boundary-in-window fallback, the exists-exactly-once guarantee, the manual-smoke-check note, the head-length premise being self-enforcing.

## Coder judgment calls — both approved

1. **`crates/wolf-macos/DECISIONS.md:83` was false** against the merged source, claiming the `prev.html` bucket times only the synchronous `loadHTMLString` call when there are two update paths. A documentation-is-source-of-truth bug plus a duplicate-ownership pair §E did not enumerate. Rewritten to drop the stale half and link to the entry; the entry's duplicate `wakesPerKeystroke` gotcha deleted. One owner each.
2. **The pulldown-cmark / `into_offset_iter` bullet** was restated in C.1, so it was deleted from the entry — `DECISIONS.md` states it more completely. Same class as §E.

## Re-verification scope

prettier, `markdown_check.sh`, and the `big-file` check only. **No `bazel test //...` re-run required**: there is a clean 93/93 on a tree whose only subsequent changes are Markdown, and no source file has been touched in any pass.

---

# Audit round — reviewer and technical-writer

Both ran concurrently and independently converged on the same two principal defects, which raises confidence in each.

## Defects found and fixed

1. **Image-alt marker placement was false in three documents.** They claimed an image's alt-text marker "keeps the before-the-tag placement". `MarkerInjector` defers it across the inline `Image` so it lands *after* the `<img>`; `is_block_start` does not match `Tag::Image` and `drains_deferred` deliberately excludes `TagEnd::Image`. Fixed in `crates/wolf-core/DECISIONS.md`, `crates/wolf-macos/DECISIONS.md`, `architecture.md`.
2. **`markerColumnStride` promised a cross-language guard that does not exist.** Three sites claimed it was pinned against `wolf_core::markdown::MARKER_COLUMN_STRIDE`. The assertion is a Swift-side literal (`XCTAssertEqual(markerColumnStride, 100)`); nothing reads across the boundary, and the Rust suite re-declares its own copy. A Rust-side change fails no test in either suite. All three sites now say Swift-side drift only, mirrored by hand.
3. Overstated heading coverage in `crates/wolf-core/tests/README.md`; overbroad "every JavaScript string" in `Sources/WolfKeyMap/README.md`; stale "two caveats" in `build.md`; a duplicate teardown fact; a history-voice sentence the linter cannot catch ("both defects **were** ordering bugs"); an alphabetical break; six precision fixes in the two `DECISIONS.md` sections.

## Orchestrator verification (no agent could run these)

The reviewer had no shell; these were run directly:

- The six pure take-HEAD files are **byte-identical to HEAD** — zero diff lines. No content main auto-merged was lost to a wholesale take-HEAD.
- **No unstaged changes to any non-`.md` file.** Every non-`.md` entry in `git status` is staged-only, from the merge itself.
- Both `BUILD.bazel` files differ from main's version by exactly HEAD's own `markdown_srcs` filegroups — a **clean union**, with main's additions intact.

## Deliberately out of scope — two stale source comments

Both audits independently found **source comments that are wrong**, and one is the root cause of defect 1:

- `crates/wolf-core/src/markdown.rs:168-173` claims "a fenced code block, a table and an image's alt text anchor to their block top by construction", contradicting `:114-120` in the same module. All three doc sites were faithful transcriptions of this stale sentence.
- `Sources/WolfKeyMap/FrameStats.swift:74` describes `previewLoadHTML` as "One synchronous `loadHTMLString` call" — the same two-path staleness already corrected on the Markdown side.

**Not fixed here.** Source must not change in a merge-resolution commit: it mixes two unrelated concerns and makes the merge hard to review. Reported to the user as follow-up work. The corrected prose describes what the code does, so no document works around them.
