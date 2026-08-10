# Work unit 23 — a picker over a preview

Branch: `linux-feat/picker-over-preview` off `linux` (`e6afba1`, clean and green:
**998 tests / 94 suites, 59 cage smokes**).

## Scope

One `feat-list.md` row: **`Pickers hosted in a separate floating panel`**
(`feat-list.md:91`). Out of scope: native tabs (next unit), preview in browser,
the vendored plugins, the application icon.

**Nothing in `src/` changes.** The GTK answer is already built; what is missing is
that nothing observes it.

---

## Reconnaissance — measured on this host, take as fact

### M1. The feature works. A picker composites over a live Markdown preview.

By hand under `cage`, `PREVIEW_SCENARIO`'s recipe, with Neovim's own readback
(`Preview: pvsrc.md,pvoth.md`) confirming the preview was bound before any capture.
Output 1280x720; web view x 787..1259; file-picker box x 465..1064 (600 px =
`PICKER_BOX_WIDTH`), y 71..470. **Overlap rectangle R = (787,160)–(1065,470),
86,180 px:**

| capture | `PREVIEW_DARK_BG` | white glyph ink | picker fill (29,29,32) |
| --- | --- | --- | --- |
| control (preview live, no overlay) | 81,515 = **94.59 %** | 1,791 = 2.08 % | **0** |
| file picker open | **0 = 0.00 %** | **0 = 0.00 %** | 86,180 = **100.00 %** |

The palette gives 98.7 % fill in the same rect; the buffer picker 100 % in its own
band. A column scan at x = 1000 puts the boundary exactly at the box's own bottom
edge.

### M2. The residual risk `DECISIONS.md:123` names does NOT fire

That line warns of *"a WebKit that renders through a Wayland subsurface, which would
sit outside that ordering entirely"*. Measured against a fixture forcing an
accelerated-compositing layer (`transform: translateZ(0); will-change: transform;`
plus a running `@keyframes` rotation — visibly animating, unquestionably promoted):
in R the control reads **74.99 %** of the layer's `#c00000`, and with the picker
open **0.00 % red / 100.00 % picker fill**. The compositing layer is inside the GTK
scene and is occluded by the sibling overlay child.

**M1 and M2 together retire both halves of `DECISIONS.md:123` — the construction
argument and its named residual risk.**

### M3. Baseline

`cargo test --workspace` exit 0, **998 passed / 0 failed / 1 ignored, 94 suites**;
`smoke_headless` **59 passed in 482.71 s**.

### M4. Only the file picker has a deterministic box

`PICKER_BOX_HEIGHT = 400` (`picker.rs:458`) is fixed. `palette.rs:622-625` and
`buffer_picker.rs:456-459` set width and margin but **no height**, so their boxes
grow with row count — the palette's bottom edge measured y 465 with today's 20
commands and **moves whenever a command is added**. The file picker is the only one
of the three whose sample rectangle is stable.

### M5. The fill colour is derivable in-frame, so no theme constant need be pinned

The band (500,300)–(700,460) — inside the picker box but over the **editor** pane —
is 32,000/32,000 uniformly (29,29,32) with the picker open and 0/32,000 closed.

### R1. The structural claim holds; the prior recon's citations had drifted

Actual locations: `DECISIONS.md:123` (`:95` is the `## Window structure` heading)
and `feat-list.md:173`; the table row is `feat-list.md:91`, the order-of-work entry
`:243`. The chain, verified line by line: `window.rs:382-383` window-level
`gtk::Overlay`; `pane_view.rs:3176-3178`/`:3312-3318` the editor container; each
pane leaf's host its own `gtk::Overlay` (`pane_view.rs:352-354`); the
`webkit6::WebView` added with `p.host.add_overlay(&view)` (`pane_view.rs:1685`) —
**pane-level**; the four transient overlays added to the **window** overlay
(`picker.rs:351`, `palette.rs:630`, `buffer_picker.rs:464`, `search_overlay.rs:765`).

### R3. macOS's row covers three pickers, not four

`d763b3b` is *"Pickers moved to separate dialog"* and touches `PickerView`,
`CommandPaletteView`, `BufferPickerView` plus new `PickerPanel*`.
`PickerPanel.swift:7-11` gives the reason verbatim: the window server always
composites a child window in front of its parent's content, *"including the
Markdown-preview `WKWebView` … that otherwise draws above sibling `NSView`s on
Tahoe regardless of order."* `WolfWindowController.swift:132-134` holds exactly
three hosts. **Project search is NOT one of them** — `SearchView` is a sidebar
panel — so GTK's project-search overlay is outside this row.

### R4. No smoke captures ANY transient overlay while it is open

Wider than `tests/README.md` records. All 51 `grim` calls scanned: the only capture
taken with an overlay-opening chord after the last `Return` is the stall smoke
(`:5177`), where the chord is deliberately **refused** and what is captured is the
stall overlay. `tests/README.md` corroborates twice ("`grim` runs once the overlay
has closed").

**`PREVIEW_SCENARIO` step 5 (`:9405-9409`) already reaches the palette-over-live-
preview state; it simply takes no capture there.** The missing thing is one `grim`.

### R5. Every helper exists; one geometry blocker

`require_smoke_env`, `make_temp_cwd`, `write_relaunch_fixtures`, `cage_command`,
`clear_evidence`, `assert_png_present`, `read_window_buffer_names`,
`decode_capture` → `Capture::pixel` / `count_exactly` / `first_uniform_row` over
`SampleRect`. `PREVIEW_DARK_BG` is already the constant M1 counts;
`CAGE_OUTPUT_W/H_EXPECTED` and `OVERLAY_ORIGIN_X/Y_EXPECTED` match the captures
exactly.

**`PICKER_BOX_MARGIN/WIDTH/HEIGHT` are private `const` in `picker.rs:454-458`** and
an integration test is an external crate. Either they become `pub`, or the smoke
locates the box empirically.

---

## Orchestrator decisions — binding

**O1. The file picker is the subject.** M4: it is the only one of the three with a
fixed box height, and the palette's bottom edge moves every time a command is added
— which would make its band **silently** wrong rather than loudly wrong. The row
covers three pickers on macOS (R3); one of them, measured, is the evidence, and the
other two are recorded as sharing the structure rather than asserted separately.

**O2. Locate the box empirically. Do NOT make the `picker.rs` constants `pub`.**
Widening a private constant so a test can read it pins the geometry the test then
asserts, which is circular. `first_uniform_row` and the fill colour from M5's
editor-pane reference band give the same rectangle with nothing pinned. **This unit
changes no `src/` file** — that property is worth keeping intact.

**O3. The assertion is a PAIRED CONTROL, not a clever falsifier.** The property is
GTK's composition, not a line of Wolf's code, so no edit disables it cleanly. Assert
the overlap rectangle on **two captures in one run**: control (no overlay — a floor
of `PREVIEW_DARK_BG` and a floor of glyph ink) and sample (picker open — zero
`PREVIEW_DARK_BG`, zero white, a floor of fill colour).

**The control half is what stops the sample half passing vacuously when the preview
never rendered** — the same shape the appearance smoke already uses and documents.

**O4. The shipped red run is P1b; P1a is recorded, not taken.**

- **P1b (shipped)** — after `overlay.add_overlay(&picker_box)` at `picker.rs:351`,
  `picker_box.insert_before(&overlay, overlay.first_child().as_ref())`. Pure
  paint-order change, nothing else. Predicted red: R reads `PREVIEW_DARK_BG` where
  fill is due. **It cannot separate "over the preview" from "on screen at all"** —
  say so rather than dress it up.
- **P1a (recorded)** — move the web view to the window-level overlay,
  `DECISIONS.md:105`'s explicitly rejected alternative. It inverts exactly the
  relation under test, but it also changes the view's geometry, so its red is not
  isolated. Record it as the falsifier that *would* isolate the relation, with that
  cost stated, so a later unit takes it deliberately rather than rediscovers it.
- **P1c rejected**: making `open` a no-op reddens the assertion while proving
  nothing about z-order.

**The control's own falsifier is dropping the Preview leg**, which must redden the
**control** assertion and not the sample one. Capture both.

**O5. No OCR. P3 is a trap worth naming in the smoke's comment.** The picker's rows
are filenames the sidebar also renders, so "the picker's rows are on screen" is
satisfiable **with the picker closed** — exactly the vacuity `tests/README.md`'s
fixture rule exists to prevent. And OCR cannot say *where* a word is, so it cannot
say "over the preview". Pixels are the only instrument. The smoke asserts no OCR
term, so it adds no `ASSERTED_OCR_TERMS` row.

**O6. This is the first smoke in the file to capture a transient overlay while
open** (R4), so there is **no sibling to copy the timing from**. The recon's
by-hand run says one `SETTLE` after the chord and one after the query suffices;
that is a prediction the red/green runs convert.

**O7. Hazards carried forward, binding on every agent.** Never `git checkout --` /
`git restore` / `git stash` — all work is uncommitted. Never restore an
`include_str!`'d asset with `cp -p`; plain `cp` + `touch`, and re-run green before
believing a restore. **No constant before its measurement; no assertion without a
captured red.** Any pre-existing test that moves is a **finding to record, never a
threshold to retune**.

**O8. Documents this measurement makes wrong**, for the writer:
`DECISIONS.md:123` (both halves retired by M1/M2), `tests/README.md:229`,
`feat-list.md:91`, `:173`, `:243`.

## Status

Reconnaissance complete. Decisions O1–O8 binding. Test-writer next.
