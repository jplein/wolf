# Work unit 18 — preview appearance and typography for `wolf-gnome`

Branch: `linux-feat/preview-appearance` off `linux`.

## Scope — exactly four rows

Three from `feat-list.md`'s "Markdown preview" table:

1. Math symbols / Inter font in the preview (`6f6abd1`)
2. Preview theme follows light/dark (`3edf74e`, `66ce35e`)
3. Typography tuning for the preview (`6838614`)

One from "Appearance, typography, theming":

4. Baseline-grid typography (`2735a6d`)

Not in scope, not to be ticked: follow-links, preview-of-anonymous-buffers,
preview-in-browser, syntax-highlighted code blocks, the floating-panel pickers
row.

## The premise, and the trap

All five referenced commits are **entirely inside `wolf-core`** — CSS, `markdown.rs`
and `wolf-core`/`wolf-ffi` tests. Not one line of `wolf-macos`. So every one of
these four rows is a property of the rendered document, and the document already
compiles into `wolf-gnome`.

`feat-list.md`'s own rule: a checked Linux box means the capability is **present
and reachable on Linux**, not that the Rust compiles. Emitting the CSS is not
evidence. Each row must be proved visible in the running GTK application, or left
unticked with the reason stated.

The four things to prove, each against WebKitGTK rather than against the CSS text:

1. Inter is the face that actually renders body text.
2. Math renders as symbols, not tofu, from the bundled Latin Modern Math face.
3. The preview's palette follows the app's resolved light/dark.
4. The 24px baseline grid is in effect in the laid-out document.

## Established before the architect ran — verify, do not re-derive

- `crates/wolf-core/src/markdown/` holds `base.css` (422 lines, the grid),
  `math.css` (163), `themes/default.css` (110) and `assets/` with two woff2 faces
  plus `.b64` sidecars: `inter-variable.woff2` and `latinmodern-math.woff2`. The
  two faces are exactly the "faces=2" unit 17 saw resolve at `document.fonts.ready`.
- `themes/default.css` ships the **dark** palette unconditionally
  (`background-color: #1b1b1b`) and the light palette behind
  `@media (prefers-color-scheme: light)` (`#f4f4f4`), and declares
  `color-scheme: light dark` **on `body.wolf-markdown`, not on `:root`**.
- The `<style>` block is a compile-time constant, byte-identical on every call,
  including across an appearance flip. There is no per-appearance render.

### The light/dark chain, traced in the shipped binaries

- **WebKitGTK 2.52.4** (`libwebkitgtk-6.0.so.4.16.8`) contains exactly two GTK
  settings notifications: `notify::gtk-application-prefer-dark-theme` and
  `notify::gtk-theme-name`. It contains **no** `gtk-interface-color-scheme` and no
  `org.freedesktop.appearance`. So its idea of the system preference comes from
  those two GtkSettings properties and nothing else.
- **libadwaita 1.9.2** carries the string
  `Using GtkSettings:gtk-application-prefer-dark-theme with libadwaita is
  unsupported. Please use AdwStyleManager:color-scheme instead.` — but upstream
  `adw-style-manager.c` **does set that property itself**, guarded by a
  `changing_gtk_settings` flag that suppresses its own warning:

  ```c
  if (gtk_check_version (4, 23, 1) != NULL) {
    self->changing_gtk_settings = TRUE;
    g_object_set (self->gtk_settings, "gtk-application-prefer-dark-theme", self->dark, NULL);
    self->changing_gtk_settings = FALSE;
  }
  ```

  The propagation is therefore **conditional on the running GTK being older than
  4.23.1**. The devShell pins GTK 4.20/4.22, so it is on today — and a GTK bump past
  4.23.1 silently removes it.

**Hypothesis, not yet measured:** row 2 may already work for free through
adw → GtkSettings → WebKitGTK, on a coupling that is invisible in this repo's
source and that a GTK bump breaks. Two things could still falsify it: WebKit may
gate `prefers-color-scheme: dark` on the page declaring a supported color scheme
at the **document root**, and this theme declares `color-scheme` on `body`; and
`ForceLight` must be proved to produce light rather than merely differing.

### What the harness can and cannot observe

- `cairo` is **already** a `wolf-gnome` dependency with the `png` feature (for the
  `#[ignore]`d golden regeneration). So a test can decode a `grim` PNG with
  `ImageSurface::create_from_png` and sample exact pixels — no new crate, no
  `Cargo.lock` movement. `tests/paint_golden.rs` is the precedent for sampling
  raw ARGB bytes at a computed rectangle.
- `grim -t ppm` also exists, if raw bytes are preferred to cairo.
- The devShell has **no ImageMagick**. `REQUIRED_TOOLS` is
  `["cage", "wtype", "grim", "tesseract", "wolf-pointer-inject"]`; adding a sixth
  is a devShell change and should be avoided.
- OCR cannot distinguish light-on-dark from dark-on-light. A colour claim needs
  pixels, not tesseract.
- Every smoke's seeded config pins `SMOKE_APPEARANCE` = `dark`;
  `configured_appearance_reaches_neovims_background_under_cage` already launches
  twice, at both appearances, and is the precedent for a two-appearance smoke.

## Definition of done

- Every ticked row backed by evidence from the running application. Prefer
  extending the `cage` smoke harness. Where a property genuinely cannot be
  observed by a smoke, say so plainly and pin it another way — unit 17's
  precedent is a recorded measurement plus a payload unit test for scroll
  preservation.
- Every new test wired so it actually runs, confirmed rather than assumed, and
  seen red with the thing it covers broken.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`,
  `cargo build --workspace --all-targets`, `cargo test --workspace` green, with
  cargo's real exit status captured (never piped through `tail`).
- `prettier --write` on every edited Markdown file; `./tools/lint/markdown_check.sh`
  clean.
- Documentation per CLAUDE.md ownership. Tick only proved rows, in both tables,
  and fix prose the ticks falsify — including feat-list's sentence that
  baseline-grid typography "is unreachable until Linux has a preview pane".

## Constraints

- Repo-writing agents are **serialized**. Never two writers at once, and never a
  test run while an agent edits.
- Carried, do not fix: `Cargo.Bazel.lock` is out of sync with `Cargo.lock` (unit 17
  added six webkit packages). No `bazel`/`bazelisk` on this host; it needs
  `CARGO_BAZEL_REPIN=1` on macOS. Note any further `Cargo.lock` movement.
- Syntax-highlighted code blocks is blocked and out of scope, but confirm the
  finding that `render_source_html`'s treesitter path degrades to plain render on
  Linux because the parsers are Bazel-bundled into `Wolf.app` and `flake.nix`
  vendors none — which makes that row depend on the Appearance table's
  "Vendored treesitter plugins and parsers" row.
- A feature or architecture decision only the human can make: **stop and report**.

## Phase 0 measurement — complete, and it decides three of the four rows

Platform: GTK 4.22.4, libadwaita 1.9.0, WebKitGTK 2.52.4, `cage` 0.3.0 at 1280x720.
Method: a transient `src/bin/measure_preview.rs` (deleted; tree clean) rendered the
real `render_html(FIXTURE, PreviewTheme::Default, WidthMode::Full)` — 1,031,775
bytes — into a `webkit6::WebView` inside an `adw::ApplicationWindow`, with
`StyleManager::set_color_scheme` applied before any widget existed, in three
separate processes. Full record in the scratchpad's `FINDINGS.txt` / `RECORD.txt`.

### Q1 light/dark — works, both directions

| appearance | `gtk-application-prefer-dark-theme` | `prefers-color-scheme: dark` | `getComputedStyle(body).backgroundColor` |
| --- | --- | --- | --- |
| `ForceDark` | true | true | `rgb(27, 27, 27)` |
| `ForceLight` | false | false | `rgb(244, 244, 244)` |

Exactly `#1b1b1b` / `#f4f4f4`. The light run was done on a desktop reading dark
(`adw.is_dark` true before, false after), so it overrides rather than coincides.

**The root-declaration hypothesis is falsified.** `htmlColorScheme` computes to
`normal` — only `body` carries `light dark` — and dark takes anyway. Injecting
`documentElement.style.colorScheme = 'light dark'` changed nothing in either run.
WebKitGTK does not gate `prefers-color-scheme` on a root declaration.

**No libadwaita warning** on stderr in any run: the `changing_gtk_settings` guard
suppresses it. So the coupling is live, invisible in this repo's source, and a GTK
bump past 4.23.1 removes it silently. That is the gotcha this unit owes a document.

### Q2 fonts — both faces load and both are the used faces

`faceCount: 2`, both `status: "loaded"`: `Inter` (weight `100 900`) and
`Latin Modern Math`. Math leaf advances vary — ∑ 25.73, i 3.86, = 14.94, ∫ 16.84,
x 9.14, α 10.23 — so the symbols are real, not tofu.

**`document.fonts.check()` is not evidence and must not be used as a discriminator.**
With both `@font-face` rules deleted from the CSSOM (`faceCount: 0`), `check('16px
Inter')` and `check('16px "Latin Modern Math"')` both still return true: it answers
"can these characters be rendered", system fonts included.

The decisive A/B is deleting the `@font-face` rules and re-measuring:

| probe | with faces | faces deleted |
| --- | --- | --- |
| `font-family: Inter` span width | 624.34375 | 662.203125 |
| `font-family: "Latin Modern Math"` span width | 598.578125 | 662.203125 |
| display-math element height | 44.859375 | 34.75 |
| first `mo` (∑) box | 25.734375 x 23 | 14.046875 x 14 |

### Q3 baseline grid — the unit is in effect, the invariant is not held

`getComputedStyle(body).lineHeight` is `24px`, a length. First-to-second line box
of a wrapped paragraph is exactly 24 (tops 74, 98). Body padding is `0px` on all
four sides and the first block's top offset is 0.

But twelve of sixteen top offsets are off-grid from `H3` onward, because three
border-box heights are: `P 26`, `UL 74`, `PRE 123`. (`TABLE 145` and the
display-math `P 44.859375` are the residuals the wolf-core README already documents,
and the `hr` at 896 re-exposes the documented 1px.) A single 2px overflow shifts
everything below it, so the drift is cumulative and visible.

Attributed by measurement rather than by reasoning:

| element | baseline | `code{font-family:inherit}` | `code{line-height:0}` |
| --- | --- | --- | --- |
| P | 26 | 25 | 24 |
| UL | 74 | 73 | 72 |
| PRE | 123 | 120 | 120 |

The cause is the inline `code` box. `line-height: 24px` inherits as a length, so
the code inline box is also 24px tall — but its font is a different family, so its
half-leading splits differently around its baseline, and the union of the two
baseline-aligned 24px boxes exceeds 24. `base.css`'s header explains why vertical
padding on inline code is safe, which is true, and says nothing about font metrics,
which is the actual mechanism. The wolf-core sweep cannot see it: every declared
value is legal.

### Q4 typography tuning — every observable property present

All seven observable changes from `6838614` measure as intended in both
appearances: `body` padding `0px` on four sides with the first block at top offset
0; `h1`/`h2` `paddingBottom 0px`, `borderBottomWidth 0px`, `borderBottomStyle none`;
`blockquote` all four border widths 0; and all three border colours computing to
`currentColor` rather than to a theme rule.

### Environment findings that constrain the tests

- **The WebProcess does not see the devShell's `FONTCONFIG_FILE`.** It lives under
  `/tmp/nix-shell-<id>/`, which WebKit's mandatory sandbox does not bind, so the
  WebProcess falls back to the host `/etc/fonts`. Anything inside the preview that
  resolves through the CSS fallback stack is therefore host-dependent and must not
  be asserted on. What is reproducible is the two `@font-face` families, which win
  over system fonts by spec. Pointing `FONTCONFIG_FILE` at a file under
  `$XDG_CONFIG_HOME` killed the WebProcess outright, so do not try it.
- **Pixel sampling of a `grim` PNG works** and is an unambiguous light/dark
  discriminator. `cairo::ImageSurface::create_from_png` decodes it at 1280x720,
  stride 5120, and the format is `Format::Rgb24`, **not** `ARgb32` — grim writes an
  opaque PNG, so a test asserting `ARgb32` fails. Byte order is still B,G,R,X on
  little-endian, so `paint_golden.rs`'s sampling reads correctly. Dark frame:
  `#1b1b1b` 79.8% of pixels, then `#161616` 16.9%. Light frame: `#f4f4f4` 79.8%,
  then `#eaeaea`. The two appearances share no dominant colour. In the real app the
  preview is an overlay inside a pane, so a smoke must sample inside the pane rect.

## The treesitter blocker, checked — the stated claim is materially wrong

Confirmed: highlighting is an RPC into the captive Neovim rather than a Rust crate
(no `syntect`/`tree-sitter` anywhere in the workspace); the fallback in
`nvim.rs` is total, per block, and entirely silent (no `tracing::` call on the
path); the curated 16 parsers and both nvim-treesitter trees are Bazel-staged into
`Wolf.app` only; and `flake.nix` vendors no plugin and no parser.

**But there is no "Linux/cargo no-op" on this path.** `wolf_highlight_code_blocks`
is defined at module level in `runtime/nvim/init.lua`, deliberately outside the
vendored-tree gate, so it exists on every platform. Its gate is per block and per
language. The pinned `pkgs.neovim` (neovim-unwrapped 0.12.2) already ships seven
parsers with matching queries — `c`, `lua`, `markdown`, `markdown_inline`, `query`,
`vim`, `vimdoc` — on a runtimepath entry present even under `--clean`. Measured
against the real pinned `nvim` and the real repo `init.lua`: `lua` returns 4 spans,
`c` returns 3, while `go`, `rust`, `json`, `yaml`, `css` and `bash` return false.
So a fenced `lua` or `c` block already renders as inline-styled spans in the Linux
preview today.

The blocker is therefore **parser coverage**, not a no-op. And it is narrower than
the Appearance table's "Vendored treesitter plugins and parsers" row in three ways
(the preview highlighter never uses `vim.treesitter.start`, never uses the markdown
injections query, and never needs `nvim-treesitter-textobjects`) and broader in one
decisive way: `bazel/treesitter-parsers` emits **Mach-O** objects from the
apple_support host toolchain, so ticking that row buys Linux nothing. Linux needs
ELF parsers from a flake-side vendoring path that does not exist and is explicitly
deferred in `treesitter-decision-report.md`. A later unit must be scoped as a
sibling of the macOS row, not a consumer of it.

Recorded, not fixed here — the comments claiming a Linux no-op are false at five
sites (`crates/wolf-core/src/nvim.rs` around the highlight path, `markdown.rs`, and
`crates/wolf-ffi/tests/code_block_highlight.rs`). Correcting them is outside this
unit's four rows.

## Orchestrator decisions — binding

1. **Do not touch shared `wolf-core` CSS behaviour.** The measured one-line fix
   `body.wolf-markdown code { line-height: 0 }` is **not** applied here. It changes
   a document macOS also renders, no macOS host exists to verify it, and nothing in
   either suite can observe it. It goes to the human as an open question (D1).
   Comment-only corrections to `base.css`'s header are in scope and expected —
   they change no rendering.
2. **Row 4 is ticked**, with the residual documented and explicitly labelled
   shared with macOS rather than a Linux gap. The 24px unit is measurably in
   effect on Linux — computed `line-height` is the 24px length, consecutive line
   boxes are exactly 24 apart, the body is flush at 0 — and the deviation is
   produced by the same WebKit inline-layout code on both platforms, so macOS's
   existing tick rests on exactly the same evidence. Leaving Linux unticked would
   assert a divergence that does not exist, which is a worse falsehood for a parity
   ledger. The honesty this owes is the residual, not the tick.
3. **The optional "Inter is first in the stack" CSS-text test is declined.** This
   unit's premise is that emitting the CSS is not evidence; adding a CSS-text
   assertion as part of it muddies that. Recommended to the human as a separate
   item.
4. **`feat-list.md` does not contain the sentence this unit was briefed to
   correct.** Line 178 already reads "is a property of the document `wolf-core`
   renders rather than of the GTK pane that displays it" — the "unreachable until
   Linux has a preview pane" clause was corrected in an earlier unit. Do not go
   looking for it and do not invent an edit.

## The implementation plan

**No new files.** `tests/smoke_headless.rs` is the only place `cage` may be
spawned (`cage_is_only_spawned_through_the_isolating_helper` enforces it), and a
second test file would be a second process with its own `CAGE_LOCK`, serializing
nothing.

**No new dependency.** `cairo` is already a `wolf-gnome` dependency with the `png`
feature. `Cargo.lock` must not move and no `CARGO_BAZEL_REPIN` obligation is
created. `REQUIRED_TOOLS` stays at five — no ImageMagick, no devShell change.

### One new smoke: `preview_document_follows_the_configured_appearance_under_cage`

Two launches in one `cage`, at the two configured appearances, structurally
`APPEARANCE_SCENARIO` with `PREVIEW_SCENARIO`'s preview leg inside each launch.
It asserts on **raw pixels decoded out of its own captures** — the first of its
kind in the file, and the only route to a colour claim, because tesseract cannot
tell light-on-dark from dark-on-light.

**The two launches must not share a state directory.** `cage_command` hands both
one `WOLF_STATE_DIR`, so launch 1's `state.json` would restore a two-pane layout
with a live preview leaf into launch 2, changing launch 2's `preview_placement`
input and coupling this smoke's evidence to the restore path. Split
`smoke_state_dir` into `smoke_state_dir_at(cwd, suffix)` exactly as
`smoke_config_dir_at` already is, and give each launch its own sibling directory.

**Why the preview is deterministically the right half** — must be in the doc
comment, because the geometry rests on it. One pane open means
`preview_ui::preview_placement` answers `MODE_SPLIT_RIGHT`; `wolf_do_split('v')`
emits a Split intent naming the original window as `from`; `pane_view::apply_split`
builds `Frame::Split { children: [Leaf(target), Leaf(new_pane)] }` and
`PaneTree::pane_rects` walks children left to right. So the new pane is the right
half whatever Neovim's `splitright` says. Assertion 4 makes that self-checking
rather than assumed.

**Geometry is derived, never hardcoded**, in the
`the_column_cap_cannot_bind_at_the_ocr_smokes_geometry` idiom: a sample band inside
the right (preview) pane and a control band inside the left (editor) pane, both
built from `CAGE_OUTPUT_*_EXPECTED`, `OVERLAY_ORIGIN_*_EXPECTED` and
`editor_inset::EDITOR_PADDING_PX`. The sample band must clear the `gtk::Paned`
handle (whose width this file deliberately never models), the statusbar strip and
the command-line strip. Each constant's comment names what it clears, not its number.

**The colours are re-typed, not imported** — `PREVIEW_DARK_BG` `(0x1b,0x1b,0x1b)`
and `PREVIEW_LIGHT_BG` `(0xf4,0xf4,0xf4)`, in this file's
re-declare-do-not-import discipline that `PREVIEW_OCR_STATUS` sets. Importing the
crate's own value would make the assertion a comparison with itself.

**PNG decoding gotchas, both measured, both of the kind a reader will "simplify"
back:** the decoded format is `Format::Rgb24`, **not** `ARgb32` — grim writes an
opaque PNG, so a test asserting `ARgb32` fails — and the little-endian byte order
is `B, G, R, X` where the fourth byte is padding, not alpha, so nothing may read it.

#### Assertions, in order

| # | assertion | kind | falsifying edit |
| --- | --- | --- | --- |
| 1 | `status.success()` | precondition | none |
| 2 | both PNGs present and each decodes at exactly `CAGE_OUTPUT_W_EXPECTED` x `CAGE_OUTPUT_H_EXPECTED` | precondition | none — this is what makes the derived rect meaningful |
| 3 | each launch's `tabpagebuflist()` readback, sorted, is the preview plus the source | precondition | none — establishes a preview pane exists at all |
| 4 | in the **control** band, fewer than 2% of pixels equal this launch's theme colour | control, self-checking | none — its failure means the editor background collides with the theme colour, and the response is to re-derive the colours, never to weaken it |
| 5 | in the **sample** band, >=90% of pixels equal this launch's own theme colour and <1% equal the other appearance's | **discriminator — row 2** | delete `manager.set_color_scheme(decided.color_scheme);` from `appearance::apply_at_launch` |
| 6 | with `web_top` self-located from the pixels, the first band of rows carries >=`MIN_FLUSH_INK_PX` non-theme pixels | **discriminator — row 3** | add `padding-top: 48px;` to `body.wolf-markdown` in `base.css` |

**The pair of launches is what discriminates, and the doc comment must say so.**
Under `cage` there is no settings portal, so `StyleManager::is_dark()` is
host-determined and constant for the run, and this file may not assert a value for
it. Whichever way it reads, one launch is the weak half and the other the strong
one, and no single constant satisfies both. A future edit must not drop one launch
as redundant.

**Assertion 5's falsifier is isolated**, which is the point: it is the first test
in the repository that observes `set_color_scheme` at all, and it closes
`tests/README.md`'s "That the chrome's colour scheme reaches a pixel" gap. Deleting
that line leaves the whole fast suite and every existing smoke green.

**Assertion 6 is deliberately insensitive to where the pane puts the web view** —
`web_top` is self-located, so a change to `pane_view::apply_preview_frame` moves
the band with the view and cannot redden it. State that, or someone will try
`apply_preview_frame` as a revert and conclude the test is vacuous.

**`web_top` self-location is sound because of assertion 4**: everything above the
web view in the sampled columns is the pane's statusbar strip and padding, painted
with the editor's background — and assertion 4 is what proves that background is
not the theme colour. That is why assertion 4 is read first and named a control.

### Which rows a smoke can carry, and which it cannot

**Row 2 — yes**, assertion 5. **Row 3 — its headline change only**, assertion 6.
The commit's seven observable changes are the removed body padding, the `h1`/`h2`
bottom borders and padding, and the `blockquote` left border; only the first is a
positive pixel statement, and proving an absence from pixels is the vacuity trap
this harness's conventions exist to prevent. The other three stay pinned by
`markdown_render.rs`'s CSS-text assertions. Say that split in the doc comment.

**Row 1 — no smoke can carry it**, and this is not a fixture problem. Three
measured reasons: OCR reads no font and cannot read the math glyphs at any size;
the only decisive discriminator was deleting the `@font-face` rules from the CSSOM
and re-measuring geometry, which needs `evaluate_javascript` inside the running
view and a smoke drives the app from outside with `wtype`; and any comparison
against "the fallback" is host-dependent because the WebProcess does not see the
devShell's fontconfig. **Do not retry the `FONTCONFIG_FILE` route — it killed the
WebProcess.**

The alternative pin is two things that already exist plus one that must be written
down: `markdown_render.rs` pins both `@font-face` families and the math markup
contracts; `pane_view.rs`'s own test module pins that the document reaches WebKit
with its head intact and that a live edit cannot strip it
(`head_offset_answers_the_head_position_and_never_the_sentinel`,
`head_offset_moves_only_when_the_head_moves`,
`body_inner_carves_the_real_documents_body_and_none_of_its_head`, the last
asserting the carved body carries no `@font-face`). **Add no test for this row** —
a wolf-gnome test asserting the head carries the two families would duplicate
wolf-core's and be a comparison with itself. What is missing is only the statement
plus the measurement record, which is documentation.

**Row 4 — no test, either way.** It is a laid-out-document property; the
declared-value sweep structurally cannot see it, and measuring a 24px pitch out of
a screenshot would be more fragile than anything in this file.

### Red runs the coder owes, and must paste into the assertion table

1. Delete `manager.set_color_scheme(...)` from `appearance.rs` → assertion 5 red on
   at least one launch. Record **which**, and what the ambient scheme under `cage`
   turned out to be — that reading is a new measurement and belongs in the doc.
2. Delete the `@media (prefers-color-scheme: light)` block from `themes/default.css`
   → assertion 5 red on the light launch. Record whether `markdown_render.rs`
   reddened alongside; a red smoke over a red fast suite is the theme being wrong,
   over a green one it is the applier.
3. Add `padding-top: 48px;` to `body.wolf-markdown` in `base.css` → assertion 6 red
   on both launches. Confirm the fast suite stays green (the sweep accepts
   multiples of 24, and the `padding` shorthand assertion is untouched); if not,
   fall back to restoring `padding: 48px 40px` and record the pairing honestly.
4. Confirm assertions 1-4 have no falsifier and do not invent one.

`FLUSH_BAND_PX` owes a `MEASURED:` note taken from a retained green-run PNG — the
actual first-ink row. **Do not tune a threshold to make a run pass; re-measure.**
If assertion 6 proves flaky, drop it and record the consequence: row 3 then rests
on the Phase 0 measurement alone, and the feat-list prose must say so.

### Documentation ownership

- **`crates/wolf-gnome/DECISIONS.md` owns the light/dark coupling** — new section
  `## The preview's light and dark`. Not `src/README.md`: `appearance.rs` makes one
  call and everything downstream belongs to the versions the crate is built
  against, not to any file in it. Not `architecture.md`: delete `wolf-gnome` and
  the GtkSettings chain vanishes, while architecture's existing sentence survives,
  so by CLAUDE.md's own test it is a crate-local constraint. Content: the full
  chain, the `gtk_check_version (4, 23, 1)` gate that silently ends it, the
  measured silence, and the falsified root-declaration hypothesis (do not move
  `color-scheme` to `:root` — it was tried and changes nothing).
- **`crates/wolf-gnome/src/README.md`** — `appearance.rs` entry gains one
  **Gotchas** bullet that is purely a link to that section. `pane_view.rs` needs no
  change.
- **`crates/wolf-gnome/tests/README.md`** — smoke counts; a new "A colour claim
  needs pixels, not OCR" convention paragraph; **delete** the "That the chrome's
  colour scheme reaches a pixel" unproven bullet (it is now proved, and the delete
  is the point); **add** unproven bullets for which face the WebProcess uses, for
  the GTK version gate, and for the grid residual.
- **`crates/wolf-core/src/markdown/README.md`** — three in-place rewrites: the
  `themes/default.css` entry's macOS-only `NSApp.effectiveAppearance` parenthetical
  (state the platform-neutral fact and link each front-end's mechanism doc); the
  "24px baseline grid" bullet (true of declared values, false of the laid-out
  document — say what the sweep actually guarantees); and the "`line-height` is a
  length" bullet, which is misleading exactly where it matters because it names
  inline `code` as a case the length solves and that is the case it does not solve.
  "Accepted residuals" gains the inline-`code` residual with the measured numbers.
- **`crates/wolf-core/src/markdown/base.css`** — header comment only. The padding
  claim stays and is correct; add the mechanism it is silent about, with the
  arithmetic: a box extends `(L + A - D) / 2` above the baseline, so the union of
  two baseline-aligned boxes is `L + (max(A-D) - min(A-D)) / 2`.
- **`crates/wolf-gnome/Cargo.toml`** — the `cairo` comment says the `png` feature
  is for the `#[ignore]d` dev aid; it is now also part of the CI gate. Rewrite in
  place.
- **`feat-list.md`** — four ticks; line 157 rewritten (it claims the unticked rows
  are "unimplemented rather than blocked", which the treesitter finding falsifies);
  the preview prose gains what each tick rests on, naming row 1's as a measurement
  rather than a test; line 178 gains the residual as shared with macOS; line 221's
  "three document-rendering rows" rewritten.
- **Must not change**: `architecture.md`, `build.md`, `Cargo.lock`,
  `Cargo.Bazel.lock`.

### Order — repo writers are serialized, and no test runs while an agent edits

1. Test-writer: `tests/smoke_headless.rs` only. Order inside the file: helper split
   → constants → scenario → smoke → doc bookkeeping. Leave the `measured` column an
   explicit `TODO(coder)`.
2. Coder: the four red runs, restoring after each and filling `measured`; then the
   `Cargo.toml` and `base.css` comment corrections; then the full gate.
3. Technical-writer: the Markdown files, wolf-core first; `prettier --write` and
   `markdown_check.sh` on each. Delete before adding where the plan says delete.
4. Reviewer.
5. Orchestrator: commits, merge into `linux`.

## The smoke, measured — green and proved falsifiable

`preview_document_follows_the_configured_appearance_under_cage` runs and the target
reports `50 passed`. Every number below is from a real run against a retained
capture, and no threshold was tuned to fit.

| quantity | measured |
| --- | --- |
| `web_top` | 159 on both launches, exactly the derived prediction |
| first non-theme row | 165 — offset 6 into the band, so the leading `.wolf-src` anchor costs no line box |
| flush-band ink | 1,987 px on each launch against a 200 floor |
| assertion 5 own-colour fraction | 1.0000, other appearance 0.0000, over all 76,464 band pixels |
| assertion 4 control | exactly 0.0000 — 0 of 76,680 |

The editor's own background reads `(20,22,27)` dark and `(224,226,234)` light, so it
follows the appearance too and collides with neither theme colour. Four independent
launch-pairs gave bit-identical numbers, so assertion 6 is not flaky and row 3 does
not fall back to the Phase 0 measurement.

**The ambient colour scheme reaching WebKit under `cage` on this host is dark.**
That is why revert 1 reddens the **light** launch and leaves the dark one green:
run 1 is the weak half here, run 2 the strong one. Recorded in the smoke's doc
comment, because it is what tells a later reader which half is load-bearing.

### The three red runs

1. `set_color_scheme` deleted from `appearance::apply_at_launch` → assertion 5 red
   on the light launch, cargo exit 101. **Isolated**: a full workspace run with the
   line still deleted left every other target green and `smoke_headless` at
   `49 passed; 1 failed`, the failure being this smoke alone.
2. The `@media (prefers-color-scheme: light)` block deleted from
   `themes/default.css` → assertion 5 red on the light launch, and
   `markdown_render.rs` reddened alongside at `43 passed; 2 failed`. Red smoke over
   a red fast suite is the theme being wrong; over a green one it is the applier.
3. `padding-top: 48px` added to `body.wolf-markdown` → assertion 6 red on both
   launches (0 ink in the first 24 rows, and 0 in the first 48), with the fast suite
   staying green exactly as predicted, so no fallback revert was needed.

Assertions 1-4 have no falsifier and none was invented.

Recorded weakness: assertion 5's loop panics on the first failing launch, so revert
3's second-launch reading was taken from that run's retained capture rather than
reported by the harness. Making the harness report both would mean collecting
failures instead of asserting per launch — a test-code change, not made.

### Gate

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo build --workspace --all-targets` and `cargo test --workspace` all exit 0.
Neither lock file moved. `crates/wolf-gnome` has no `BUILD.bazel` and appears in no
Bazel target, so `cargo test`'s discovery satisfies the wiring rule.

Comment-only corrections applied: `crates/wolf-gnome/Cargo.toml`'s `cairo` entry
(the `png` feature now gates CI, not only the `#[ignore]d` dev aid) and
`base.css`'s compensation paragraph (the half-leading mechanism and its arithmetic).
No CSS declaration was touched and `code { line-height: 0 }` was not added.

## Review — needs-revision, all five items closed

The reviewer found no defect in the test's geometry, its falsifiers, its
completeness or the four honesty judgements. What blocked was documentation this
unit falsified and did not correct, and all of it was mechanical:

1. `tests/README.md` claimed no test reaches `apply_at_launch` and no revert of it
   was run. Both halves falsified by this unit, in the dangerous direction — the
   sentence sat on the path to deleting the new discriminator. Rewritten; the
   step order and the `Weak` remain genuinely unproven and are still recorded.
2. `appearance.rs`'s doc comment named only the older smoke as end-to-end
   evidence, eight lines above the exact line assertion 5's falsifier deletes.
   Both effects are now named against the smoke that observes each.
3. Two stale smoke counts outside any writer's remit: `flake.nix` and
   `config_seam.rs`. Corrected; a tree-wide grep confirms no third.
4. `feat-list.md` claimed the typography row was pixel-proved. The smoke pins one
   of that commit's four observable changes; the other three are absences held by
   `markdown_render.rs`'s CSS-text sweep. Corrected, and the two records pointed
   at the right files.
5. `tests/README.md` asserted no test *can* observe which face the WebProcess
   uses. Narrowed to the three routes actually measured and rejected — OCR,
   `document.fonts.check()`, and any comparison against a host fallback.

`crates/wolf-gnome/tests/README.md` crossed the advisory 50,000-byte ceiling in
this unit. Four restatements it does not own were cut, plus one more, taking it to
50,328 measured. The remaining 328 would have to come from pre-existing material,
which is a separate item. The four measured font numbers were deliberately not cut
— they carry a tick.

Recorded, not fixed: the plan's "seven observable changes" (Phase 0 counted seven
measured properties) and the smoke's "four observable changes" (four grouped
changes) describe the same commit at different granularity. The shipped comment is
the one to trust.

## Final state

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -D warnings`,
`cargo build --workspace --all-targets` and `cargo test --workspace` all exit 0.
`cargo test -p wolf-gnome` reports `50 passed`, and `--list` discovers 50 tests
with the new smoke among them. `prettier --check "**/*.md"` clean repo-wide;
`markdown_check.sh` reports no findings, one advisory `big-file` warning.

Neither `Cargo.lock` nor `Cargo.Bazel.lock` moved. Every changed line in
`base.css` begins with a comment marker — verified mechanically — and
`code { line-height: 0 }` is absent.

The load-bearing falsifier was re-run against the final tree: deleting
`set_color_scheme` reddens the light launch at cargo exit 101 and nothing else,
with restoration confirmed by checksum rather than by eye.

## Status

Rows 1, 2, 3 proved; row 4 ticked with a documented residual. D1 open for the
human. Commits and the merge next.

