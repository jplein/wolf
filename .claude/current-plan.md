# Current plan: inline-level marker anchoring for preview scroll-follow

## Status

PHASE 6 — **COMPLETE.** Implemented, reviewed (one blocking bug found and
fixed — see B5/Amendment C), docs updated, all gates green.

Final: `//crates/wolf-core:wolf_core_lib_test` 217 passed / 0 failed;
`//crates/wolf-core:markdown_render` 43 passed / 0 failed; 93/93 targets;
clippy + rustfmt clean. 30 tests added across the two files.

ONLY REMAINING ITEM: the §5.3 manual smoke check (WebKit layout — no test in
this repo can measure it). See the "Manual smoke check" section in §5.3.

---

### Superseded status line (PHASE 4)

PHASE 4 — IMPLEMENTED AND GREEN. Architect plan + **AMENDMENT A** (at the end of
this file; it OVERRIDES the plan where they differ) both implemented.

Test results: `//crates/wolf-core:wolf_core_lib_test` 213 passed / 0 failed;
`//crates/wolf-core:markdown_render` 40 passed / 0 failed; `bazelisk test //...`
93/93 targets pass; `bazel test --config=lint //tools/lint:clippy` clean. All 23
new tests confirmed to have actually RUN. Every "must stay green unmodified"
test passed unmodified.

Remaining: rustfmt fix (see Amendment B), reviewer, technical-writer, and the
§5.3 manual smoke check (WebKit layout — not automatable in this repo).

## Prior context

This is a FOLLOW-UP refinement on the Markdown-preview scroll-sync feature on
this branch (`feat/preview-scroll-handling`, commits `6ca6a55` and `28135db`).
The prior plan (the flicker fix) is archived at
`/private/tmp/claude-501/-Users-jplein-Git-jplein-wolf/044923fb-ac34-4bd3-9cae-c7ab40b51ade/scratchpad/flicker-plan-archive.md`
— read it for the invariants of the front-end machinery (D1/D3, the
`captureDecision` rules, `__wolfUpdate`'s single-JS-task ordering) before
changing any of that. **This change is expected to be confined to wolf-core's
Markdown renderer**; the front-end is expected to need no change.

## Problem statement (user report, verbatim)

> If the document being edited is also being previewed, the content being typed
> is generally scrolled into the viewport in the preview. But if a single
> paragraph is very long and a user is typing at the end, it is the beginning of
> the paragraph that is in view, not the end.

## Verified root cause

- `crates/wolf-core/src/markdown.rs:470` `desired_markers` correctly emits
  per-100-column markers `(L, 100)`, `(L, 200)`, … for long lines, in ascending
  byte order.
- `crates/wolf-core/src/markdown.rs:967` `MarkerInjector` injects markers ONLY
  immediately before block-level `Start` tags, flushing every pending marker
  with byte offset `< range.end` (`flush_before`, `:1021`). For a one-line
  paragraph, ALL of that line's column markers have offsets inside the
  paragraph's source range, so they are all emitted before the `<p>` tag and
  collapse to the paragraph's top.
- `crates/wolf-macos/Sources/WolfKeyMap/PreviewScrollSeam.swift:713` `resolve()`
  tier 1 finds `#wl-{line}-{roundedCol}` and measures its rect — which is the
  paragraph top — so `__wolfFollow` reveals the start of the paragraph.
- This is already documented as the accepted v1 limitation in the `markdown.rs`
  module docs (`:38-49`): "The per-100-column markers are mostly redundant in v1
  (they collapse to the same block top) but are REQUIRED by the scheme and
  become meaningful once inline-level anchoring is added."

## Requested change

Add that inline-level anchoring in wolf-core's Markdown renderer, so a column
marker lands at its real position within the rendered inline flow and the
preview follows the cursor to the correct wrapped visual line inside a long
paragraph.

## Design constraints (to be validated, not assumed)

1. Extend `MarkerInjector` so that, in addition to the existing block-start
   flush, a pending marker whose byte offset falls strictly INSIDE an
   `Event::Text` event's source range splits that Text event: text prefix, then
   the marker span as raw `Event::Html`, then the remainder. Column-zero markers
   and markers falling in non-text regions keep the existing behavior.
2. A marker must NEVER land inside `<pre><code>`. Do not split Text while inside
   `Start(CodeBlock)`..`End(TagEnd::CodeBlock)`. `CodeHighlighter`
   (`markdown.rs:743+`) runs BEFORE `MarkerInjector` and rewrites highlighted
   block interiors to `Event::InlineHtml`, but PLAIN (unhighlighted) code blocks
   still yield `Event::Text` — both cases must be excluded. Also do not split
   `Event::Code`, `Event::Html`, or `Event::InlineHtml` (math is already
   `InlineHtml` by this point).
3. A `Text` event's source byte range is not always byte-identical to its text
   payload (backslash escapes, entity references, whitespace normalization).
   Only split when the mapping is trustworthy (e.g. `range.len() == text.len()`);
   otherwise fall back to emitting the marker before the Text event as today.
   Verify pulldown-cmark's actual behavior rather than guessing.
4. Marker ids, the id scheme, CHARACTER-column semantics (OD4), source ordering,
   and `MARKER_COLUMN_STRIDE` must all be unchanged — only DOM PLACEMENT
   changes. Ids must remain unique.
5. `.wolf-src` spans are zero-size / `visibility: hidden`
   (`markdown/base.css:199`) and now land mid-text: confirm an empty inline span
   between two characters does not introduce a word-wrap opportunity or
   otherwise alter rendered text layout. If it can, adjust placement (e.g. snap
   to a nearby safe boundary) while keeping the id at `(L, C)`.
6. The `:first-child` / `:last-child` margin-reset selectors in `base.css`
   (`:244`, `:260`) were written for leading anchor runs — check they are still
   correct with anchors now appearing mid-block.
7. No change should be needed on the Swift/JS side (the 4-tier `resolve()`
   already prefers the rounded-column id first) — but verify, and only change
   `PreviewScrollSeam.swift` if the fix genuinely requires it. If the pure seam
   changes at all, its tests live in
   `crates/wolf-macos/Tests/WolfKeyMapTests/PreviewScrollSeamTests.swift` and
   must be wired into `//crates/wolf-macos:WolfKeyMapTests` (explicit `srcs`
   list, no glob).

## Test requirements

Real Neovim over mocks (CLAUDE.md). **Every new test file MUST be explicitly
wired into its build target or it silently never runs.** Add coverage in
`crates/wolf-core/tests/markdown_render.rs` (and/or the in-module unit tests in
`markdown.rs`) proving:

- for a long single-line paragraph the column markers are emitted INSIDE the
  paragraph's inline content, in source order;
- markers never appear inside `<pre><code>`, for both highlighted and plain code
  blocks;
- escapes / entity references do not corrupt the rendered text;
- the rendered text content is byte-identical to the pre-change render once the
  marker spans are stripped.

## Documentation requirements

Documentation is the source of truth in this repo. Update:

- the `markdown.rs` module docs — the "DOM placement (block-level injection
  only)" section and the "Collapse consequence (v1, documented)" paragraph are
  now wrong;
- `crates/wolf-core/src/markdown/README.md`;
- `crates/wolf-core/src/README.md`;
- `crates/wolf-core/README.md`;
- `crates/wolf-core/tests/README.md`;
- any other README that describes the collapse behavior.

Reformat edited Markdown with `prettier --write`.

---

## Architect plan (APPROVED — implement this literally)

### 1. Summary

`MarkerInjector` currently injects every pending marker whose byte offset is
`< range.end` immediately **before** a block-level `Start` tag, so all of a long
paragraph's per-100-column markers (`wl-{L}-100`, `wl-{L}-200`, …) collapse onto
the paragraph's top and the preview's cursor-follow reveals the paragraph's
beginning no matter where the cursor is. This change makes the injector **defer**
a block's interior per-column markers (`col != 0`) instead of flushing them at
the block top, and place each one at its true position in the rendered inline
flow by **splitting the `Event::Text` that contains it** — prefix `Text`, the
marker as raw `Event::Html`, remainder `Text`. Column-zero markers, marker ids,
`MARKER_COLUMN_STRIDE`, `desired_markers`, character-column semantics (OD4), the
CSS, and the whole Swift/JS front-end are unchanged; only DOM placement of
`col != 0` markers changes. Splitting is gated by an exact
`source[range] == payload` identity check and is suppressed inside code blocks,
HTML blocks, tables and image alt text.

### 2. Resolved design questions (with evidence)

Vendored engine: **pulldown-cmark 0.12.2** (`crates/wolf-core/Cargo.toml:16`
requests `"0.12"`; `Cargo.lock:1045-1046` pins `0.12.2`). Source read at
`~/.cargo/registry/src/index.crates.io-*/pulldown-cmark-0.12.2/`.

#### Q1 — Where exactly to inject (constraint #1: CONFIRMED)

Constraint #1 is confirmed as written, including "column-zero markers and markers
falling in non-text regions keep the existing behavior". The precise rule:

- At a block-level `Start` with range `s..e`, a pending marker with offset
  `o < e` is **emitted immediately (today's behavior)** when `col == 0` **or**
  `o <= s` **or** the block is a `CodeBlock`/`HtmlBlock`/`Table` **or** injection
  is currently suppressed; otherwise it is moved to a `deferred` queue.
- A deferred marker with offset `o` is emitted at the **first** of these that
  applies, in stream order:
  1. `o <= s'` where `s'` is the range start of an incoming `Event::Text` in an
     unsuppressed context → emit immediately before that `Text`;
  2. `s' < o < e'` for an incoming `Event::Text` that passes the identity
     predicate (Q2) in an unsuppressed context → **split** that `Text`;
  3. `Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item)` → drain
     all remaining deferred markers **before** emitting the `End` (so a marker
     never escapes the block it belongs to);
  4. the next block-level `Start` with range start `s''`, draining deferred
     markers with `o <= s''` before that block's own flush;
  5. the trailing `flush_before(usize::MAX)` at stream end.
- **Several markers inside one `Text`**: the split loop walks the deferred queue
  while the head's offset is `< e`, emitting
  `Text(prefix), Html(marker), Text(prefix2), Html(marker2), …, Text(remainder)`.
  Empty slices are never emitted. Split points are forced strictly increasing.
- **Markers in source regions covered by no `Text` event** (`**`, `_`, `[`/
  `](url)`, backtick fences, backslash bytes, autolink/link destinations, entity
  spans, raw inline HTML) are never split; they land via rule 1, 3 or 4 — i.e.
  still **inside the containing block**, never inside an attribute.

#### Q2 — Offset-to-payload mapping

- `OffsetIter::next` yields `(event, item.start..item.end)` — `parse.rs:2075`.
- `item_to_event`, `parse.rs:2113`:
  `ItemBody::Text { .. } => return Event::Text(text[item.start..item.end].into())`.
  **For a real text item the payload IS the source slice of the reported range —
  an exact identity, byte for byte.** Covers plain text and text inside emphasis
  / strong / strikethrough / links.
- `parse.rs:2115-2116`: `SynthesizeText(cow)` / `SynthesizeChar(c)` produce
  `Event::Text` payloads **unrelated** to the range. Producers: entity references
  (`firstpass.rs:1113-1124`, `&amp;` → `"&"` over a 5-byte range),
  smart-punctuation (`firstpass.rs:1138-1190`, not enabled here), and code/HTML
  indent padding with `start == end` (`firstpass.rs:1489-1497`, `1508-1516`).
- **Backslash escapes**: `firstpass.rs:890-926` — on `\` + ASCII punctuation the
  text before the backslash is closed and `begin_text = ix + 1`, so the escaped
  punctuation starts the *next* Text item. `a\*b` yields `Text("a")@0..1` and
  `Text("*b")@2..4`. Identity holds; **the backslash byte is covered by no event
  at all** (a gap).
- **Soft/hard breaks** are their own items (`firstpass.rs:881-888`), so a
  paragraph's inter-line whitespace is a gap; a `Text` event never spans a `\n`
  outside code blocks.
- **Code spans** are `Event::Code` from an allocated Cow (`parse.rs:2114`);
  **inline/block HTML** are `Event::Html` / `Event::InlineHtml`
  (`parse.rs:2118-2120`). None are `Text`, so none are splittable.
- Text-item merging (`parse.rs:1364`, `1373`) merges adjacent items `a..b` +
  `b..c` into `a..c` — identity preserved.

**Predicate to use (exact, not a length heuristic):**

```
self.source.get(range.start..range.end) == Some(text.as_ref())
```

`range.len() == text.len()` is **not** sufficient: some HTML5 named entities
expand to a same-length UTF-8 string (e.g. `&nvap;`, 6 source bytes → `≍⃒`, 6
bytes), which would pass a length check while the bytes differ. **Fallback when
it does not hold:** do not split; leave the marker deferred so it lands via rule
1/3/4 (never dropped). Also require
`text.is_char_boundary(o - range.start)` defensively before slicing.

Escaping is unaffected by splitting:
`pulldown-cmark-escape-0.11.0/src/lib.rs:233-276` escapes byte-by-byte with no
cross-character state, so `escape(prefix) ++ escape(suffix) ==
escape(prefix ++ suffix)`.

#### Q3 — Exclusion contexts

Track a single `no_inject_depth: u32` in `MarkerInjector`, incremented on
`Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock | Tag::Table(_) | Tag::Image { .. })`
and decremented (`saturating_sub(1)`) on
`Event::End(TagEnd::CodeBlock | TagEnd::HtmlBlock | TagEnd::Table | TagEnd::Image)`.
While `no_inject_depth > 0`, **no** marker is emitted, deferred or split.
Additionally those four block starts never defer, so their interior markers keep
today's flush-before-the-tag placement exactly.

- **Highlighted code blocks**: `CodeHighlighter` (`markdown.rs:779-831`) replaces
  interior `Text` with one `Event::InlineHtml` but passes
  `Start(CodeBlock)`/`End(CodeBlock)` through, so the depth counter still
  brackets the block.
- **Plain code blocks** still yield `Event::Text`; the depth counter excludes
  them. Load-bearing: inside a fenced block a `Text` event can span a whole line
  including its `\n`, so an interior col-0 offset could otherwise be "strictly
  inside" a `Text`.
- **HTML blocks** yield only `Event::Html`, so there is nothing to split — but
  the depth guard also prevents rule-1/3/4 emission landing between raw HTML
  lines.
- **Tables**: excluded deliberately. A raw span emitted between `<tbody>` and
  `<tr>` is subject to HTML **foster parenting** and would be relocated out of
  the table by the parser. Today no marker ever reaches a table interior; keep it
  that way.
- **Images — verified corruption hazard.** `html.rs:355-370`: `Tag::Image` writes
  `<img src="…" alt="` then calls `raw_text()`, which consumes events up to the
  matching `End`. Inside `raw_text` (`html.rs:471-509`): `Html(_) => {}` — **a
  raw `Event::Html` is silently DROPPED** — while
  `InlineHtml(text) | Code(text) | Text(text)` are `escape_html`'d **into the
  `alt` attribute**. So (a) markers must keep being emitted as `Event::Html`,
  never `InlineHtml` (fail-safe: a leak drops the anchor rather than corrupting
  the attribute), and (b) `Start(Image)..End(Image)` must raise
  `no_inject_depth`, otherwise a marker inside long alt text would vanish from
  the document and break its id's existence guarantee.
- `Tag::MetadataBlock` is unreachable (neither metadata option is enabled in
  `markdown_options`); note that in the doc comment.

#### Q4 — Layout safety (constraint #5)

A zero-size `visibility:hidden` empty inline `<span>` does **not** introduce a
soft-wrap opportunity: CSS Text 3 specifies line-breaking from text content (UAX
#14) and that inline box boundaries are ignored for this purpose; WebKit
conforms, and `.wolf-src`'s `font-size:0`/`line-height:0` already keep it from
contributing a strut. What an inline box boundary *does* do is **break glyph
shaping across it** — a ligature (Inter has `ffi`/`fi`) or a kern pair straddling
the split point is lost.

**Decision: snap the split point to a nearby word boundary**, keeping the id /
`data-line` / `data-col` at the exact `(L, C)` (constraint #4).

- A byte offset `p` with `s < p < e` is a **word boundary** iff the char ending
  at `p` is `char::is_whitespace` **or** the char starting at `p` is
  `char::is_whitespace`, and `source.is_char_boundary(p)`.
- Search the window
  `[o - MARKER_SNAP_MAX_BYTES, o + MARKER_SNAP_MAX_BYTES] ∩ (s, e)` for word
  boundaries; pick the one minimizing `|p - o|`; tie → the smaller `p`. If none
  exists, split at `o` exactly (a ≥32-byte unbroken token, where
  `word-wrap: break-word` on the body already permits intra-word breaks).
- New private const `MARKER_SNAP_MAX_BYTES: usize = 32`. Rationale to document:
  comfortably below half of `MARKER_COLUMN_STRIDE` (100 *characters*, hence ≥ 100
  bytes), so two consecutive stride markers can never snap to the same point and
  snapped points stay strictly ascending.
- Defensive: the splitter also clamps each snapped point to be `>` the previous
  split point in the same `Text`.

**Testable vs not.** A Rust test can assert the emitted HTML structure exactly —
that the span sits between two specific characters, that `data-col` is unchanged,
and that stripping the spans reproduces byte-identical text. **No Rust test can
measure WebKit layout.** Layout neutrality is a **manual smoke check** (§5.3).

#### Q5 — CSS (constraint #6): NO change required

`base.css:244` (`body.wolf-markdown > *:not(.wolf-src):not(*:not(.wolf-src) ~ *)`)
and `base.css:254`/`:260` are all **child** selectors of `body.wolf-markdown` —
they only consider the body's direct element children.

- Leading/trailing anchor runs at body level come exclusively from col-0 markers
  and markers flushed before/after blocks; their placement is **unchanged**.
- The newly inline-placed anchors are descendants of `<p>`/`<h1>`/`<li>`/`<a>`/
  `<em>`, never direct children of `<body>`, so they cannot match `> *`.
- The `.wolf-src` rule (`base.css:199-208`) is already correct for a mid-text
  anchor.

**Action: no CSS rule edits.** Only explanatory prose in the `base.css` header
(EDGE RULES) and in `src/markdown/README.md`.

#### Q6 — Front-end (constraint #7): NO change required

- Tiers 1–3 of `resolve()` are `getElementById` lookups; ids and the id scheme
  are unchanged, so they keep working — and tier 1 now resolves to the *real
  inline position*, which is exactly the fix.
- Tier 4 (`PreviewScrollSeam.swift:727-730`) sweeps
  `.wolf-src[data-col="0"]` backwards assuming ascending `data-line` in document
  order. Because **col-0 markers keep their existing block-top placement**, that
  assumption is preserved verbatim.
- `markerColumnStride = 100` and `anchorColumn` unchanged; the Swift tests use
  hand-written HTML fixtures and pure arithmetic.

**Action: no functional Swift change; `//crates/wolf-macos:WolfKeyMapTests` needs
no new `srcs` entry.** One optional comment-only addition (§4 item 9).

#### Q7 — Ordering invariant

`next_marker` keeps its exact meaning: **every marker at an index below the
cursor has been either emitted or moved to `deferred`, exactly once.**

- `deferred` is FIFO and always ascending by offset.
- Every emission point that could emit a *cursor* marker (`claim_before`, the
  trailing `flush_before`) first drains the deferred markers that precede it.
  Because a deferred marker's offset is always inside the block that claimed it,
  and the block ends before the next block starts, no cursor marker with a
  smaller offset is ever emitted after a deferred one.
- **Documented, accepted exception:** within one block, a col-0 marker belonging
  to a *later* hard line may be emitted at the block top *before* an
  earlier-offset col>0 marker placed inline (e.g. a paragraph spanning lines 3–4
  where line 3 is 250 characters: `wl-3-0`, `wl-4-0` at the top, then `wl-3-100`,
  `wl-3-200` inside the `<p>`). Harmless because tiers 1–3 are id lookups and
  tier 4 filters on `data-col="0"`; the col-0 subsequence remains ascending. Must
  be stated in the module docs, in `crates/wolf-core/README.md`, and pinned by a
  test (I7/I8).
- Marker offsets from `desired_markers` are pairwise distinct, so no two markers
  share a split point; the splitter still loops so co-located markers emit both,
  in order.

### 3. Files to create

**None.** `crates/wolf-core/tests/markdown_render.rs` already exists and is wired
to `//crates/wolf-core:markdown_render` (`BUILD.bazel:427-434`); the in-module
tests run under `//crates/wolf-core:wolf_core_lib_test` (`BUILD.bazel:20-27`).

### 4. Files to modify

#### 4.1 `crates/wolf-core/src/markdown.rs` — the only behavioral change

**(a) New private const** next to `MARKER_COLUMN_STRIDE`:

```
const MARKER_SNAP_MAX_BYTES: usize = 32;
```

Doc comment: the snap window, why it exists (shaping across an inline box
boundary), and why it must stay below half of `MARKER_COLUMN_STRIDE`.

**(b) New free functions** (next to `is_block_start`, which is **unchanged**):

- `fn defers_interior_markers(event: &Event<'_>) -> bool` — `true` for
  `Start(Paragraph | Heading{..} | BlockQuote(..) | List(..) | Item |
  FootnoteDefinition(..))`; `false` for
  `Start(CodeBlock(..) | HtmlBlock | Table(..) | TableHead | TableRow)` and
  everything else. Contract: "may this block hold its interior `col != 0` markers
  back for inline placement?"
- `fn opens_no_inject_region(event: &Event<'_>) -> bool` —
  `Start(CodeBlock(..) | HtmlBlock | Table(..) | Image{..})`.
- `fn closes_no_inject_region(event: &Event<'_>) -> bool` —
  `End(TagEnd::CodeBlock | TagEnd::HtmlBlock | TagEnd::Table | TagEnd::Image)`.
- `fn drains_deferred(event: &Event<'_>) -> bool` —
  `End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item)`. Contract: "a
  deferred marker must never escape the block it belongs to."
- `fn snap_split_point(source: &str, range: &Range<usize>, offset: usize,
  min_exclusive: usize) -> usize` — returns the byte offset at which to split,
  per Q4, always satisfying `min_exclusive < result < range.end` and
  `source.is_char_boundary(result)`. Never panics (all indexing via `get` /
  `is_char_boundary`).

**(c) `MarkerInjector` struct** — add three documented fields:

```
source: &'a str,                       // identity predicate + borrowed split slices
deferred: VecDeque<(usize, u32, u32)>, // claimed-but-unplaced markers, ascending offset
no_inject_depth: u32,                  // >0 inside code block / html block / table / image
```

**(d) `MarkerInjector::new`** — signature becomes
`fn new(inner: I, markers: Vec<(usize, u32, u32)>, source: &'a str) -> Self`.

**(e) New `MarkerInjector` methods:**

- `fn emit_marker(&mut self, line: u32, col: u32)` —
  `self.pending.push_back(Event::Html(marker_span(line, col).into()))`. Doc:
  **must stay `Event::Html`, not `InlineHtml`** — cite `html.rs:471-509`
  (`raw_text` drops `Html`, escapes `InlineHtml` into `alt`).
- `fn flush_before(&mut self, end: usize)` — **unchanged** behavior and
  signature; still used by the trailing flush.
- `fn claim_before(&mut self, block: &Range<usize>, deferrable: bool)` — walks
  the cursor while `markers[next].0 < block.end`; a marker is pushed to
  `deferred` iff
  `deferrable && self.no_inject_depth == 0 && col != 0 && offset > block.start`,
  else emitted via `emit_marker`. Cursor advances in both cases.
- `fn drain_deferred_upto(&mut self, limit: usize)` — pops and emits deferred
  markers while the head's offset is `<= limit`.
- `fn drain_deferred_all(&mut self)` — `drain_deferred_upto(usize::MAX)`.
- `fn try_split_text(&mut self, text: &CowStr<'a>, range: &Range<usize>) -> bool`
  — returns `false` (caller emits the `Text` verbatim) when
  `no_inject_depth > 0`, when `deferred` has no marker in
  `(range.start, range.end)`, or when the identity predicate fails. Otherwise
  pushes `Text(prefix) … Html(marker) … Text(remainder)` into `pending` and
  returns `true`. All slices are `&self.source[..]` → `CowStr` via `.into()`
  (valid precisely because identity holds). Empty slices are skipped.

**(f) `MarkerInjector::next`** — restructure the `Some((event, range))` arm into,
in order:

1. `if closes_no_inject_region(&event) { self.no_inject_depth = self.no_inject_depth.saturating_sub(1); }`
   — **before** any drain, so the marker after a `</pre>` is emittable.
2. `if drains_deferred(&event) { self.drain_deferred_all(); self.pending.push_back(event); continue; }`
3. `if is_block_start(&event) { self.drain_deferred_upto(range.start); self.claim_before(&range, defers_interior_markers(&event)); if opens_no_inject_region(&event) { self.no_inject_depth += 1; } self.pending.push_back(event); continue; }`
4. `if let Event::Text(t) = &event` and `self.no_inject_depth == 0` and
   `!self.deferred.is_empty()`: `self.drain_deferred_upto(range.start)`; then
   `if self.try_split_text(t, &range) { continue; }`
5. `if opens_no_inject_region(&event) { self.no_inject_depth += 1; }` (covers
   `Start(Image)`, which is not a block start)
6. `return Some(event);`

The `None` arm becomes:
`self.inner_done = true; self.drain_deferred_all(); self.flush_before(usize::MAX);`
then loop to drain `pending`.

**(g) `render_html_with_highlights`** — the one call-site change:
`MarkerInjector::new(with_code, markers, source)`.

**(h) Module docs (`markdown.rs:28-49`)** — replace the "## DOM placement
(block-level injection only)" section and the "**Collapse consequence (v1,
documented)**" paragraph. New content must state: the block-start flush rule for
col-0 and non-deferrable blocks (unchanged); the deferral + split rule for
`col != 0`; the exact identity predicate and its pulldown-cmark citation; the
exclusion contexts and **why images matter** (`Event::Html` is dropped by
`raw_text`); the snap rule and `MARKER_SNAP_MAX_BYTES`; the ordering invariant
plus the documented document-order exception and its dependence on the JS tier-4
`[data-col="0"]` filter; and the **remaining accepted limitation** — a paragraph
hard-wrapped into several short source lines still collapses all its col-0
markers to the block top, so cursor-follow is still paragraph-granular there;
only lines longer than `MARKER_COLUMN_STRIDE` gain intra-block anchoring.

#### 4.2 `crates/wolf-core/tests/markdown_render.rs`

Add the integration tests in §5.2. Also fix three now-stale comments: the file
header (lines 14-19); `first_child_margin_reset_survives_a_multi_anchor_leading_run`
(line ~2074 — its suggestion to use "a heading longer than 100 columns" as an
alternative multi-anchor trigger is now **false**, that marker goes inside the
`<h1>`); and the block comment at lines 1390-1434 describing marker emission.

#### 4.3 `crates/wolf-core/src/README.md` — line 147

Rewrite the `MarkerInjector` sentence. Describe the two-tier placement, the
exclusion set, the identity predicate, the snap, and the remaining hard-wrap
limitation.

#### 4.4 `crates/wolf-core/README.md` — lines 156 and 158

Rewrite line 158 ("Markers are injected ONLY immediately before block-level
`Start` tags … a cursor anywhere inside a wrapped paragraph anchors to that
paragraph's top"). **Separately, line 156 has a pre-existing doc bug**: it states
the chain as `offset_iter → HeadingIdAssigner → MathRenderer → MarkerInjector`,
omitting `CodeHighlighter`, which `markdown.rs:400` and `src/README.md:142` both
include. Fix it in the same edit.

#### 4.5 `crates/wolf-core/src/markdown/README.md` — lines 9 and 37

Add to the `.wolf-src` bullet and the EDGE RULES paragraph: per-100-column
anchors now appear **mid-block, inside inline content**; the edge rules are
`body.wolf-markdown > *` child selectors and therefore unaffected; the
`.wolf-src` rule needs **no** change and its
`font-size:0`/`line-height:0`/`visibility:hidden` are now load-bearing for a
*second* reason (a mid-text anchor must add no line box and must still report a
rect).

#### 4.6 `crates/wolf-core/src/markdown/base.css` — header comment only

In the EDGE RULES block (lines 107-157), add a short paragraph: anchors also
appear mid-block now; the edge selectors match only body-level children, so they
are unaffected; do not "simplify" them. **No rule changes.**

#### 4.7 `crates/wolf-core/tests/README.md` — line 165

Update the `markdown_render.rs` entry: the test count (currently "28 tests") and
the behavior list; add the new inline-anchoring assertions and keep the col-0
collapse claims (still true).

#### 4.8 `architecture.md` — line 265

Rewrite the "accepted v1 marker-collapse GRANULARITY" bullet: intra-block
anchoring now exists for lines past `MARKER_COLUMN_STRIDE`; what remains deferred
is per-line anchoring inside multi-line blocks.

#### 4.9 `crates/wolf-macos/Sources/WolfKeyMap/PreviewScrollSeam.swift` — OPTIONAL, comment only

On `stateScriptJS`'s doc comment (near line 649-653), add one sentence: the
tier-4 sweep's `[data-col="0"]` filter is load-bearing — since wolf-core places
per-100-column anchors inline, col>0 anchors are no longer in globally ascending
document order, so the sweep must never be widened to all `.wolf-src`. No code
change, no test change, no `srcs` change.

#### 4.10 `crates/wolf-core/BUILD.bazel` — **no change**

Verified: `markdown_render` (`:427-434`) already lists `tests/markdown_render.rs`,
and the in-module `#[cfg(test)] mod tests` runs under `wolf_core_lib_test`
(`:20-27`). If the coder deviates and adds a new `tests/*.rs` file, they MUST add
a matching `rust_test` copied from the `pane_status_unit` stanza (`:478-485`) —
headless, no `data`/`env`, no `no-sandbox` tag — and confirm it runs under
`bazelisk test`.

### 5. Test plan

#### 5.1 In-module unit tests — append to `#[cfg(test)] mod tests` in `markdown.rs`

Suggested private helpers: `fn body_of(html: &str) -> &str` (slice between the
`<body …>` `>` and `</body>`) and `fn strip_marker_spans(s: &str) -> String`.

| #   | Name                                                 | Input                                                                                                                                    | Assertion                                                                                                                                                                                                                                                                                                                              |
| --- | ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| U1  | `column_marker_lands_inside_the_paragraph`           | `"lorem ipsum ".repeat(20)` (240 chars, one line)                                                                                        | `id="wl-1-0"` index `<` `<p>` index; `id="wl-1-100"` and `id="wl-1-200"` indices both `>` `<p>` index and `<` `</p>` index, and `wl-1-100` before `wl-1-200`.                                                                                                                                                                           |
| U2  | `split_snaps_to_the_nearest_word_boundary`           | same source                                                                                                                              | Body contains the exact substring `lorem<span class="wolf-src" id="wl-1-100" data-line="1" data-col="100"></span> ipsum` **and** `lorem <span class="wolf-src" id="wl-1-200" data-line="1" data-col="200"></span>ipsum`. Pins both snap directions **and** that `data-col`/`id` stay at the exact `(L, C)`. (Verify the exact character positions against the real fixture rather than trusting this sketch.) |
| U3  | `unbroken_token_splits_at_the_exact_offset`          | `"a".repeat(300)`                                                                                                                        | Body equals exactly `<p>` + 100 `a` + `marker_span(1,100)` + 100 `a` + `marker_span(1,200)` + 100 `a` + `</p>` (allowing the leading `wl-1-0` span before `<p>` and a trailing `\n`).                                                                                                                                                   |
| U4  | `markers_in_one_text_event_stay_ordered_and_unique`  | `"a".repeat(450)`                                                                                                                        | `wl-1-100/200/300/400` each occur exactly once, at strictly increasing indices, all between `<p>` and `</p>`.                                                                                                                                                                                                                           |
| U5  | `column_zero_markers_keep_block_placement`           | `"foo\nbar\nbaz"`                                                                                                                        | All three `wl-{L}-0` precede the single `<p>`.                                                                                                                                                                                                                                                                                          |
| U6  | `plain_code_block_takes_no_inline_marker`            | fenced block whose body line is 200 chars                                                                                                | No `wolf-src` and no `wl-` between `<pre` and `</pre>`; `id="wl-2-100"` exists elsewhere in the document.                                                                                                                                                                                                                                |
| U7  | `highlighted_code_block_takes_no_inline_marker`      | same source via `render_html_with_highlights(.., &[Some(BlockHighlight{ .. })])`                                                          | Same two assertions; plus the inline-styled `<span style=` is still present.                                                                                                                                                                                                                                                            |
| U8  | `image_alt_text_keeps_its_marker_out_of_the_attribute` | `format!("![{}](i.png)", "alt word ".repeat(20))`                                                                                        | The substring from `<img` to the following `/>` contains neither `wolf-src` nor `wl-`; **and** `id="wl-1-100"` still appears somewhere in the document (proves the marker was not silently dropped by `raw_text`'s `Html(_) => {}`).                                                                                                     |
| U9  | `table_interior_takes_no_inline_marker`              | a 2-row table whose first data cell is >120 chars                                                                                        | No `wolf-src` between `<table` and `</table>`; `wl-{L}-100` appears **before** `<table`.                                                                                                                                                                                                                                                 |
| U10 | `html_block_is_not_split`                            | `"<div>\n{150 h's}\n</div>\n"`                                                                                                            | The emitted `<div>…</div>` region contains no `wolf-src`; the 150-char run survives verbatim.                                                                                                                                                                                                                                            |
| U11 | `link_destination_gap_marker_lands_inside_the_paragraph` | `format!("[text](https://example.com/{})", "p".repeat(150))` — char 100 falls inside the URL                                             | `id="wl-1-100"` exists; its index is **after** `</a>` and **before** `</p>`; the `href="…"` attribute contains no `wolf-src`.                                                                                                                                                                                                            |
| U12 | `entity_reference_is_not_split`                      | a line where char 100 falls strictly inside a `&amp;`/`&copy;` source span                                                               | Body contains the rendered entity intact with no `wolf-src` inside it, no `&amp;amp;`; `wl-1-100` exists.                                                                                                                                                                                                                                |
| U13 | `backslash_escape_is_not_corrupted`                  | a line where char 100 lands on/next to `\*` (e.g. 99 filler chars then `\*stars\*` then filler)                                           | Body contains the literal `*stars*` and **no** `<em>`; `wl-1-100` exists.                                                                                                                                                                                                                                                                |
| U14 | `stripping_marker_spans_reproduces_a_vanilla_render` | a fixture with **no headings and no math**: a >300-char paragraph containing `**bold**`, `_em_`, a code span, a long link, an image with long alt, `\*escapes\*`, `&amp;`, a multibyte run, a hard break, a fenced code block with a 150-char line, a table with a long cell, and an HTML block | `strip_marker_spans(body_of(render_html(src, Default, Full))).replace('\n', "")` equals `vanilla.replace('\n', "")`, where `vanilla` is `html::push_html` over `Parser::new_ext(src, markdown_options())`. Comment must explain the `\n` normalization: writing a raw span sets pulldown's `end_newline = false` (`html.rs:86-88`), which flips a following `<p>` to `\n<p>` — a pre-existing artifact, so both sides are transformed identically. |

#### 5.2 Integration tests — append to `crates/wolf-core/tests/markdown_render.rs` (public API only)

| #   | Name                                                       | Input                                                                                                                     | Assertion                                                                                                                                                                                                                                              |
| --- | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| I1  | `long_single_line_paragraph_anchors_inside_the_paragraph` (**the regression test for the reported bug**) | `format!("# Title\n\n{}", "lorem ipsum dolor ".repeat(30))` (540 chars on hard line 3)                                     | `marker_id(3, 0)` precedes `<p`; `marker_id(3, COLUMN_STRIDE)`, `…2*STRIDE`, `…3*STRIDE` all sit between the `<p>` and `</p>` indices, in ascending order. Failure message must name the user-visible symptom ("the preview reveals the start of the paragraph while typing at its end"). |
| I2  | `inline_marker_never_lands_inside_a_fenced_code_block`      | `"# H\n\n```\n{200 chars}\ncode two\n```\n"`                                                                              | Nothing matching `wolf-src` or `wl-` between `<pre` and `</pre>`; `marker_id(4, COLUMN_STRIDE)` still present.                                                                                                                                          |
| I3  | `inline_marker_never_lands_inside_a_highlighted_code_block` | same via `render_html_with_highlights` with one `Some(BlockHighlight)`                                                     | Same, plus `class="language-…"` intact.                                                                                                                                                                                                                 |
| I4  | `image_alt_attribute_never_receives_a_marker`               | public-API mirror of U8                                                                                                    | Same two assertions.                                                                                                                                                                                                                                    |
| I5  | `escapes_and_entities_survive_inline_anchoring`             | a >150-char line combining `\*escaped\*` and `&amp;` positioned around column 100                                          | Literal `*escaped*`, no `<em>`, exactly one `&amp;`, no `&amp;amp;`, `marker_id(1, COLUMN_STRIDE)` present.                                                                                                                                              |
| I6  | `column_zero_markers_still_collapse_to_the_block_top`       | a paragraph of three hard lines where line 1 is 250 chars                                                                  | `wl-1-0`, `wl-2-0`, `wl-3-0` all precede `<p>`; `wl-1-100`/`wl-1-200` sit inside it. Pins the documented ordering exception AND the preserved col-0 rule.                                                                                                |
| I7  | `every_marker_id_appears_exactly_once`                      | a mixed fixture (heading, long paragraph, list, table with a long cell, fenced code with a long line, image with long alt, trailing newline) | Scan all `id="wl-…"` occurrences; assert no duplicates (`HashSet` len == count) and that the set contains every id the locally-recomputed marker rule predicts. The "none emitted twice, none dropped" guard.                                            |
| I8  | `column_zero_markers_appear_in_ascending_document_order`     | same mixed fixture                                                                                                         | Extract the `data-line` of every `data-col="0"` span in document order and assert it is non-decreasing — the exact invariant `PreviewScrollSeam`'s tier-4 sweep depends on.                                                                              |

Existing tests that must remain green **without modification** — the coder
verifies rather than edits:
`multi_line_paragraph_collapses_its_markers_before_the_single_p`,
`container_collapses_interior_line_markers_before_its_opening_tag`,
`heading_marker_sits_immediately_before_its_tag`,
`no_marker_span_inside_a_fenced_code_block`,
`long_line_earns_a_per_hundred_column_marker`,
`per_hundred_column_marker_counts_characters_not_bytes`,
`marker_not_inside_math`, `highlighted_block_has_no_marker_inside_pre`, all four
edge-reset/DOM tests, and the entire baseline-grid sweep. **If any of these
fails, the implementation has deviated from this plan — do not "fix" the test.**

#### 5.3 Manual smoke check (not automatable here)

Open a `.md` with (a) a >400-character single-line paragraph, (b) the same text
hard-wrapped, (c) a long code block; in the in-app preview: typing at the end of
(a) must reveal the *end* of the paragraph; the rendered wrapping of (a) must be
visually identical to `main`; nothing must appear inside the code block.

### 6. Implementation order

1. `markdown.rs`: add `MARKER_SNAP_MAX_BYTES` and the five new free functions.
2. `markdown.rs`: add the three `MarkerInjector` fields and change `new` to take
   `source`; update the single call site in `render_html_with_highlights`.
3. `markdown.rs`: add `emit_marker`, `claim_before`, `drain_deferred_upto`,
   `drain_deferred_all`, `try_split_text`; rewrite `next`. (Land 1–3 together in
   one build to avoid transient dead code.)
4. Run the existing suites (`//crates/wolf-core:wolf_core_lib_test`,
   `//crates/wolf-core:markdown_render`) — every existing test must still pass
   before any new test is written.
5. Add the in-module unit tests (§5.1), then the integration tests (§5.2). Run
   both targets under `bazelisk test`.
6. Rewrite the `markdown.rs` module docs (§4.1h).
7. Update the READMEs + `base.css` header + `architecture.md` (§4.3–4.8); run
   `prettier --write` on every edited `.md` (NOT on `base.css`).
8. Optional: the comment-only `PreviewScrollSeam.swift` note; re-run
   swift-format/lint.
9. `bazelisk test //...` plus `bazel test --config=lint //tools/lint:clippy`.

### 7. Dependencies

**None.** No new crates. `pulldown_cmark::{Event, Tag, TagEnd, CowStr}`,
`std::collections::VecDeque`, `std::ops::Range` are already in scope or vendored.

### 8. Lint considerations

Default clippy (`--config=lint` for `-Dwarnings`). Avoid rather than `#[allow]`:

- `clippy::too_many_lines` on `MarkerInjector::next` — keep `next` a dispatcher;
  work lives in the named helpers.
- `clippy::comparison_chain` / `clippy::manual_range_contains` in
  `snap_split_point` — express the distance comparison as a single `min_by_key`
  over candidate offsets, not an `if/else if` chain.
- `clippy::needless_range_loop` — iterate with `char_indices` /
  `(range.start..range.end).filter(|p| source.is_char_boundary(*p))`.
- Do not add `#[allow(dead_code)]` for the transient gap between steps 1 and 3.
- `MARKER_SNAP_MAX_BYTES` stays **private**; still fully `///`-documented.
- In tests, build fixtures with `repeat()` + `format!`, not giant literals.
- No `unwrap`/`expect` in non-test code: slice via `str::get`,
  `is_char_boundary`, `saturating_sub`.

### 9. Risks and open questions

- **R1 (manual check required).** WebKit layout neutrality of a mid-text empty
  inline span cannot be asserted by any test in this repo. The standards position
  is solid; the residual risk is shaping (ligature/kerning) suppression across
  the boundary, mitigated by the word-boundary snap. Verify by §5.3. If a
  wrapping difference is ever observed, the escalation is to widen the snap to
  "whitespace only, unbounded within the Text event, else do not split at all" —
  do **not** revert to block-top collapse.
- **R2 (behavioral, accepted).** `getBoundingClientRect().top` of an inline
  anchor sits at its zero-height inline box (near the text baseline), a few px
  lower than a block-top anchor's rect. The follow decision's edge threshold
  absorbs this. Documented, no action.
- **R3 (invariant now load-bearing on the JS side).** Global document order of
  markers is no longer ascending across the col-0/col>0 mix. Correctness depends
  on `resolve()`'s tier-4 sweep keeping its `[data-col="0"]` filter. Recorded in
  wolf-core's docs; §4.9 optionally records it at the JS site too.
- **R4 (scope, deliberate).** A paragraph hard-wrapped into several sub-100-char
  source lines is **not** fixed by this change — its col-0 markers still collapse
  to the block top. This follows constraint #1 verbatim. The generalization was
  evaluated and rejected for this pass (anchors between `<tbody>`/`<tr>` are
  foster-parented out of the table; anchors between `<ul>`/`<li>` are invalid;
  and it would break two documented integration tests). ORCHESTRATOR NOTE: the
  user's own `CLAUDE.md` forbids hard line breaks inside a paragraph, so the
  reported corpus is single-line paragraphs — R4 is out of scope for this fix.
- **R5 (pre-existing doc bug found while reading).**
  `crates/wolf-core/README.md:156` documents the render chain without
  `CodeHighlighter`, contradicting `markdown.rs:400` and `src/README.md:142`. Per
  CLAUDE.md this is a bug; fix it in the same pass (§4.4).

---

## AMENDMENT A — bounded drain at block `End` (OVERRIDES the plan above)

Found by the test-writer while writing §5.2 I7: with `Start(List)` claiming all
of its items' markers and `drains_deferred` firing on the **first**
`End(TagEnd::Item)`, item two's `wl-L-100` is emitted inside item **one**'s
`<li>`. The architect verified the fix against the vendored engine.

### A1. VERIFIED: `End` events carry the FULL tag range — no stack needed

`pulldown-cmark-0.12.2/src/parse.rs:2051-2060`, `impl Iterator for
OffsetIter::next`, the `None` (container-close) arm:

```rust
None => {
    let ix = self.inner.tree.pop()?;
    let tag_end = body_to_tag_end(&self.inner.tree[ix].item.body);
    self.inner.tree.next_sibling(ix);
    let span = self.inner.tree[ix].item.start..self.inner.tree[ix].item.end;
    debug_assert!(span.start <= span.end);
    Some((Event::End(tag_end), span))
}
```

The `End` span is read from **the same tree node** (`tree[ix].item`) that
produced the matching `Start` at `parse.rs:2066-2075`
(`Some((event, item.start..item.end))`). It is byte-identical to the `Start`'s
range by construction — the full tag range, not a zero-width or
closing-delimiter range. The tree is fully built by `firstpass` before iteration,
so `item.end` is final at both emissions.

### A2. Amended rule (REPLACES §Q1 rule 3 and §4.1(b)/(f) where they differ)

- `fn drains_deferred(event: &Event<'_>) -> bool` now fires on **every
  block-level `End`**: `TagEnd::Paragraph | Heading(_) | Item | BlockQuote(_) |
  List(_) | FootnoteDefinition | CodeBlock | HtmlBlock | Table | TableHead |
  TableRow`. It must **NOT** fire on `TagEnd::Image` (a raw `Event::Html` emitted
  before `End(Image)` is still consumed and **dropped** by `html.rs:471-509`'s
  `raw_text`), nor on the inline ends (`Emphasis` / `Strong` / `Strikethrough` /
  `Link` / `TableCell`) — excluded to keep the change minimal, not because they
  are known-unsafe.
- The drain is **bounded and strict**:
  `fn drain_deferred_before(&mut self, end: usize)` pops and emits deferred
  markers while the head's offset is `< end`, called with the `End` event's own
  `range.end`.
- The drain is skipped when `no_inject_depth > 0` **after** the step-1 decrement
  (so a nested suppressed region still suppresses).
- Three clearly-named drains now exist; **do not confuse their bounds**:
  - `drain_deferred_at_or_before(limit)` — `<=`, used at a block **`Start`** with
    `range.start`;
  - `drain_deferred_before(end)` — `<`, used at a block **`End`** with
    `range.end`;
  - `drain_deferred_all()` — unbounded, used **only** in `next`'s `None` arm
    before `flush_before(usize::MAX)`.
- Rewritten `next` step (f)2:
  `if drains_deferred(&event) && self.no_inject_depth == 0 { self.drain_deferred_before(range.end); }`
  then fall through to push/emit the `End` normally.

Consequence: `End(Item)` of item one drains only markers with offset `<` item
one's end; item two's `wl-L-100` stays deferred, survives `Start(Item)`'s
`drain_deferred_at_or_before(range.start)` (its offset is greater), and is split
into item two's own `Text`. The wrong-`<li>` bug is fixed, and the invariant
tightens to **"a deferred marker never escapes the *innermost* block that
contains it"**.

### A3. Existing tests unaffected

Both `container_collapses_interior_line_markers_before_its_opening_tag` and
`multi_line_paragraph_collapses_its_markers_before_the_single_p` use **only**
col-0 markers (short list items / `"foo\nbar\nbaz"`). Col-0 markers are never
deferred, so they are still emitted by `claim_before` at `Start(List)` /
`Start(Paragraph)`, before the container's opening tag. Both stay green
unchanged. The bounded drain differs from the unbounded one only when two sibling
blocks' deferred markers coexist in the queue, which only happens for `col != 0`
markers in multi-item containers.

### A4. ADD one list-placement test (the regression guard for this bug)

Append to `crates/wolf-core/tests/markdown_render.rs` (already wired to
`//crates/wolf-core:markdown_render`; no BUILD change):

- **Name:** `inline_marker_lands_inside_its_own_list_item`
- **Fixture:**
  `let long = "lorem ipsum ".repeat(20); let source = format!("- short one\n- {long}\n");`
  (hard line 1 = the short item, hard line 2 = the long item, which earns
  `wl-2-100` and `wl-2-200`)
- **Assertions:**
  1. `marker_id(2, 0)` index `<` the `<ul>` index (col-0 still collapses to the
     container top — the preserved invariant);
  2. `marker_id(2, COLUMN_STRIDE)` index `>` the index of the **first** `</li>`
     and `<` the index of the **last** `</li>` — i.e. inside the *second*
     `<li>`, not the first;
  3. `marker_id(2, COLUMN_STRIDE)` index `<` the `</ul>` index.
- **Failure message** must name the bug: "a deferred column marker drained at the
  _first_ `End(Item)` lands in the wrong `<li>`; the drain at a block `End` must
  be bounded by that End's own `range.end`."

Leave I7/I8 as written (they correctly assert uniqueness and col-0 ordering
only).

### A5. Test-writer's three smaller flags — all confirmed faithful

- **(a) U3, strict full-body assertion — APPROVED, and preferred over the plan's
  looser sketch.** The exact expected body is `marker_span(1,0)` + `"\n<p>"` +
  100×`a` + `marker_span(1,100)` + 100×`a` + `marker_span(1,200)` + 100×`a` +
  `"</p>\n"`. The leading `\n` before `<p>` is real and required: `html.rs:65`
  seeds `end_newline = true`, `html.rs:86-88`'s `write` clears it when the marker
  span is written, and `html.rs:163-167` then emits `"\n<p>"`. The test comment
  must record that this newline is a **pre-existing** artifact of flushing a raw
  span before a block tag, not a product of this change. Build the expected span
  strings from the same literal shape `marker_span` emits.
- **(b) U9, header + delimiter + two data rows with the long cell on hard line 3
  — CORRECT.** GFM requires the delimiter row, so the long cell is on hard line 3
  and the marker is `wl-3-100`. It must appear **before** `<table`
  (`Start(Table)` is non-deferrable, so `claim_before` emits every marker with
  offset `< range.end` at the tag), and nothing matching `wolf-src`/`wl-` may
  appear between `<table` and `</table>`.
- **(c) U12/I5 — the test-writer is right and the plan's sketch was wrong.** With
  column 100 inside `\*escaped\*`, the marker either splits `Text("*escaped")`
  (`escap<span/>ed`) or drains into the gap before the closing delimiter, so a
  contiguous `"*escaped*"` substring assertion cannot hold. Putting the entity at
  column 100 and the escape sequence clear of it is the correct construction.
  **General rule, now part of the plan:** an assertion on a contiguous literal
  substring must place that literal away from any marker column; if a
  marker-adjacent literal must be asserted, assert it on the marker-stripped body
  (the U14 helper) instead.

### A6. Carried-over TODO for the coder (from the test-writer's report)

§4.2's three stale-comment fixes in `crates/wolf-core/tests/markdown_render.rs`
were deliberately left undone by the test-writer (they live in doc comments on
existing tests it was told not to modify). **The coder must do them**: the file
header (lines 14-19); the `first_child_margin_reset_survives_a_multi_anchor_leading_run`
note at ~line 2074, whose "a heading longer than 100 columns" suggestion is now
**false** (that marker goes inside the `<h1>`); and the block comment at lines
1390-1434 describing marker emission. **DONE by the coder.**

---

## AMENDMENT B — findings from implementation (recorded for the record)

### B1. Deviation from §4.1(f) step 6, adopted for CORRECTNESS

The plan has `next` `return Some(event)` directly at step 6. That is unsafe as
written: step 4 can push drained markers into `pending` and *then* have
`try_split_text` return `false` (identity-predicate failure), in which case
returning the `Text` directly would emit it **before** markers that must precede
it. The implementation instead routes every event through `pending`
(`self.pending.push_back(event)` + loop), preserving ordering in all cases at the
cost of one extra loop iteration per event. `entity_reference_is_not_split` and
`escapes_and_entities_survive_inline_anchoring` both exercise this path.

### B2. Silent dependency the plan did not name — worth citing in the docs

pulldown-cmark trims trailing intra-line whitespace out of a text item's
**range**, not just its payload (`firstpass.rs:1218-1227`), so
`"lorem ipsum ".repeat(20)` yields `Text@0..239`, not `0..240`. The identity
predicate therefore holds for that fixture. Had the range included the trimmed
space, U1/U2/U3 would all have failed with no split at all.

### B3. `//tools/lint:rustfmt_core` red — formatting only, in the new test fixtures

Two rustfmt diffs, both inside `markdown.rs`'s `#[cfg(test)] mod tests`, both in
the new fixtures; none in the implementation. Verified by stashing: rustfmt_core
passes at HEAD, so the violations arrived with the new tests. The target is
advisory/non-gating but should be green. ORCHESTRATOR DECISION: fix it via
`bazel run //tools/lint:rustfmt.fix`, whitespace only, no assertion may change.

### B5. AMENDMENT C — §Q3/A2 HAVE A HOLE: nested excluded blocks (BLOCKING)

Found by the reviewer. **The plan is wrong here, not just the code.**

`no_inject_depth` only protects blocks whose markers are still *unclaimed* when
their `Start` arrives. `claim_before` at a **container** `Start` (`List`, `Item`,
`BlockQuote`, `FootnoteDefinition`) defers every `col != 0` marker with
`offset > block.start` across the container's ENTIRE range — including markers
whose offsets lie inside a `CodeBlock` / `HtmlBlock` / `Table` nested in that
container. `Start(CodeBlock)`'s own `claim_before(range, deferrable = false)`
cannot rescue them: they are already in `deferred`, past `next_marker`.

They then survive to `End(CodeBlock)`, where step 1 decrements
`no_inject_depth` 1 → 0 and step 2 then sees
`drains_deferred(End(CodeBlock)) && no_inject_depth == 0` and calls
`drain_deferred_before(cb.range.end)` — pushing the markers into `pending`
BEFORE the `End` event. Output order becomes
`…code text…`, `<span class="wolf-src" id="wl-2-100"></span>`, `</code></pre>`.

Reproducer (headless):

```rust
let long = "x".repeat(2 * MARKER_COLUMN_STRIDE);
let source = format!("> ```\n> {long}\n> ```\n");   // or the same fence inside a list item
// `wl-2-100` / `wl-2-200` appear between `<pre` and `</pre>`.
```

A fenced code block inside a list item (common in this repo's own corpus) and a
table inside a blockquote hit the same path. The table variant emits the span
just before `</tbody></table>` — exactly the foster-parenting hazard §Q3 was
written to prevent. This violates design constraint #2 verbatim and falsifies
three source-of-truth documents (`markdown.rs:81-82`, `src/README.md:147`,
`README.md:158`).

**FIX (chosen — restores the documented behavior exactly rather than
approximating it):** in `place`'s step 3, at a block `Start` that also opens a
no-inject region, drain the deferred markers that fall inside it AHEAD of the
opening tag — add `self.drain_deferred_before(range.end)` immediately after
`drain_deferred_at_or_before(range.start)` for the `opens_no_inject_region`
case. A nested excluded block's interior markers then get exactly the historical
flush-before-the-tag placement, which `defers_interior_markers`' own doc comment
already promises.

(Rejected alternative: capture `was_suppressed = no_inject_depth > 0` before step
1 and gate step 2 on `!was_suppressed`. Also safe, but it moves the anchor
further from its true position than flush-before-the-tag does.)

### B6. Additional test coverage required (reviewer G1–G4)

- **G1 (required, tied to B5):** no test nests a code block, table or HTML block
  INSIDE a list item / blockquote / footnote definition. Every code-block and
  table test uses a TOP-LEVEL block, where nothing is ever deferred — so the
  whole nested path is untested and B5 passes the suite silently.
- **G2 (required):** nothing distinguishes the exact byte predicate from the
  `range.len() == text.len()` heuristic the plan explicitly rejected. Both
  entity tests use `&amp;` (5 source bytes → 1 payload byte), which a length
  check rejects just as well. Needs the `&nvap;`-style fixture (6 source bytes →
  6 UTF-8 bytes) that ONLY the exact slice compare handles.
- **G3:** `snap_split_point` has no direct unit test; its contract is stated in a
  doc comment and pinned nowhere.
- **G4:** no test asserts a split inside an inline element (`<em>` / `<strong>` /
  `<a>` text), which the module docs and `src/README.md` both claim happens.

### B7. Reviewer findings CLOSED without action

- **F2 / F3 (`tests/README.md` count, `architecture.md:265`)** — the reviewer
  read these mid-flight, before the technical writer's edits landed. Verified by
  the orchestrator afterwards: `tests/README.md:165` now reads "40 tests" with
  the full inline-anchoring behavior list, and `architecture.md:265` now reads
  "**Intra-block anchoring now exists**…". Both already correct.
- **B4 flake** — see below, unchanged.

### B4. `//crates/wolf-core:redraw_grid` is flaky — PRE-EXISTING, not ours

Failed once during the full sweep (`scroll_up_shifts_grid_content_down`).
Bisected: with the change stashed, `--runs_per_test=10` at HEAD produced **3
failures out of 10**. A pre-existing real-Neovim timing flake, unrelated to
Markdown. The final `bazelisk test //...` was 93/93 green. ORCHESTRATOR DECISION:
out of scope for this change; report it to the user, do not fix it here.
