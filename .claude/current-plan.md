# Work unit 17 — Markdown preview foundation for `wolf-gnome`

Branch: `linux-feat/markdown-preview` off `linux`.

## Scope

Four `feat-list.md` "Markdown preview" rows and no more:

1. In-app live preview pane (`b159793`)
2. Preview pane reuse / placement modes (`937bb7f`)
3. "Edit Source" (preview's inverse) (`8a72e50`)
4. Full path in a preview pane's status bar (`2116415`)

Not in scope, not to be ticked: follow-links, math/Inter font, preview theme
following light/dark, typography tuning, syntax-highlighted code blocks,
anonymous-buffer preview, preview-in-browser, and the "Pickers hosted in a
separate floating panel" row.

## WebKitGTK viability — measured, closed

`webkitgtk_6_0` from the repo's own pinned nixpkgs is `webkitgtk-2.52.4+abi=6.0`,
prebuilt in `cache.nixos.org`. **Incremental** devShell closure cost: 21 store
paths, 210,011,768 bytes (0.196 GiB), 58.1 MiB compressed, 26.17 s warm fetch.
`webkit6` 0.6.1 resolves against the workspace's exact pins with six new packages
and zero version bumps, and builds in 28.66 s.

It **renders under `cage` with zero extra environment variables**. OCR:
`WEBKIT RENDER OK` / `PREVIEW PANE ALIVE`. A plain-`gtk::Label` control OCRs its
own distinct string, proving the reader sees live pixels. A `WebView` in a
`gtk::Paned` renders and clips correctly.

Caveats carried, not designed around: the spike host has a real GPU and cage
opened a DRM render node, so a render-node-less machine is inferred (forced
pixman plus software GL also passed); and the probe document was tiny, while the
real document inlines ~965 KiB of base64 fonts.

## Design decisions

**Where the web view lives.** Each pane leaf becomes a `gtk::Overlay` host whose
main child is the existing `gtk::DrawingArea` and whose only overlay child is a
`webkit6::WebView`, added and removed as the pane becomes and stops being a
preview. Framed by margins from the same `editor_inset::grid_rect` under the same
cap the grid would have used. `set_measure_overlay` stays `false`, or the web
view's size request would inflate the pane minimum the whole crate is calibrated
against. A window-level overlay child is rejected: a divider drag rebuilds
nothing and issues no wake, so a margin-positioned window child would sit stale.

**What occludes what — no `PaneOcclusion` analogue is needed.** `draw_pane`
already fills the whole allocation with the editor background before anything
else, so skipping the grid leaves the editor background rather than garbage —
which is the exact failure macOS's zero-tolerance containment rule exists to
prevent. The preview scratch buffer also has no lines, so its grid paints
background and nothing else. The skip is therefore a one-bit predicate with no
containment test and no tolerance. The statusbar strip is never skipped.

**The floating-panel problem does not appear.** The four overlays are children of
the window-level `gtk::Overlay`; the web view is a child of a per-pane overlay
nested inside that overlay's main child. GTK 4 paints overlay children after the
main child and all its descendants, so a picker composites over a preview by
construction. Residual risk if WebKit renders via a Wayland subsurface — the
smoke's picker-over-preview capture is what would catch it. Do not tick that row.

**Focus.** `set_focusable(false)` plus `set_can_focus(false)` on both the web view
and its host, so focus cannot enter the subtree. `focus_inside_pane_area()`'s
guard already covers everything under `pane_area` and must not be weakened.

**Placement and reuse** — a pure seam, ported from `PreviewSeam`/`EditSeam`:
`pane_count <= 1` → `split_right` / `split_left`; `pane_count == 2` with the
right polarity and exactly one counterpart → `reuse_preview_pane` /
`reuse_editor_pane`; else `active`.

**Blocking RPCs go from four to seven**: `open_preview`, `edit_source`, and the
coalesced `render_preview`. Each gated on `Some(Liveness::Alive)` through a total
match, the shape `nav_ui::nav_press` already uses.

**The reload trigger** joins `Snapshot.previews` to `Snapshot.markdown_ticks` on
`source_bufnr`, keyed as `PreviewLoad { source_bufnr, source_tick }`. Keying on
the tick alone is wrong — two freshly opened files share a `changedtick`.
Nothing on this path may be gated on `Snapshot.seq`, because the `wolf_*`
notifications fire the wake without bumping it.

**Status bar (row 4).** `WolfCore::pane_status` returns `[No Name]` for a preview
pane, because preview windows are omitted from `win_buffers` entirely. A preview
pane's status comes from `PreviewInfo` instead: `"Preview: " + basename`, with
`core.display_path(source_path)` beneath it. `statusbar_layout` is untouched.

**A preview pane's cap comes from `PreviewInfo.source_full_width`**, never from
`full_width_for_win`, whose entry for a preview window is meaningless.

**`snapshot_readiness`** returns `win_buffers.len() + previews.len()`, or an
all-preview tab stalls persistence.

## Corrections, as measured

1. `flake.nix` needs **`pkgs.libsoup_3` as well as** `pkgs.webkitgtk_6_0` —
   `webkitgtk-6.0.pc` requires `libsoup-3.0`, absent from the devShell. Applied.
2. **Corrected — the earlier claim in this file was wrong.** `webkit6` *does*
   compile against the locked `gtk4` 0.11.3, where `Accessible` is exported
   ungated. The `v4_10` gate appears in **0.11.4**, which `^0.11` admits on any
   `cargo update`, so the coupling is **latent rather than live**. Proven with a
   throwaway crate pinned to `=0.11.4` + `v4_8`, which fails at **two** sites,
   not one: `web_view.rs:27:129` and `web_view_base.rs:10:128`.
3. `WEBKIT_FORCE_SANDBOX=0` does not exist in this ABI. The sandbox is mandatory.
4. **A dangling `DBUS_SESSION_BUS_ADDRESS` kills the app before its first
   frame.** Confirmed under `cage` in three cases: non-existent path fatal, unset
   fine, existing-but-dead path fine. The proximate message on this host is
   `bwrap: Can't create file at /.flatpak-info: Permission denied`, not the
   `Can't find source path` quoted earlier. `cage_command` neither sets nor
   clears the variable, so the smokes are safe here **by luck**. Fix:
   `.env_remove("DBUS_SESSION_BUS_ADDRESS")` in `cage_command`, plus a fifth
   entry in `cage_is_only_spawned_through_the_isolating_helper`'s asserted set.

## The reload measurement

Real document: **1,106,790 bytes** (body alone 79,943; head base64 986,960).
Six runs, six samples each, under `cage`:

| measurement | n | min | median | max |
| --- | --- | --- | --- | --- |
| full `load_html`, `Started`→`Finished` | 36 | 57.3 ms | **59.20 ms** | 70.6 ms |
| `evaluate_javascript` body swap | 36 | 5.3 ms | **12.10 ms** | 16.7 ms |
| first load in a fresh process | 6 | 154.6 ms | 157.60 ms | 178.0 ms |

**The spike's 477-511 ms was a cold-start WebProcess-spawn figure**, not steady
state. The two inlined faces are genuinely inside the 59 ms window
(`document.fonts.ready` resolved at `Finished`, `status=loaded faces=2`), so the
number is not misleading.

Concurrent loads **serialize cleanly**: a second load issued 33 ms into a first
does not reach `Started` until the first `Finished`; no dropped load, no
`load_failed`, correct final document. A five-load burst at 33 ms produced four
cycles — a still-pending request is superseded, so the backlog is bounded at one
— with the final document landing 117.6 ms after the last keystroke.

## The reload decision — measured, closed: JSON-encoded body swap

**A full `load_html` destroys the scroll offset.** Measured on a 1,114,719-byte
document rendered by the real `wolf_core::markdown::render_html` (body 87,872
bytes; `scrollHeight` 10,802 px, so a 2,000 px offset is real and the
measurement is not vacuous), under `cage`, across three processes:

| measurement | n | min | median | max |
| --- | --- | --- | --- | --- |
| `scrollY` before a full `load_html` | 24 | 2000 | 2000 | 2000 |
| **`scrollY` after `Finished`** | 24 | **0** | **0** | **0** |
| `scrollY` around a JSON body swap | 24 | 2000 | 2000 | 2000 |
| full `load_html`, `Started`→`Finished` | 24 | 54.53 ms | 55.66 ms | 57.50 ms |
| body swap, layout deferred | 12 | 4.21 ms | 5.31 ms | 6.48 ms |
| body swap, layout forced in-window | 12 | 11.80 ms | 12.41 ms | 14.92 ms |

Zero in 24 of 24, having been 2000 immediately before. The earlier medians
re-confirm: 55.66 ms `Started`→`Finished` against the recorded 59.20 ms — which
matches this run's call→`Finished` of 58.94 ms almost exactly, so the two
harnesses were timing slightly different endpoints. The recorded 12.10 ms swap
figure corresponds to the forced-layout variant.

**Decision: the JSON-encoded `evaluate_javascript` body swap.** It preserves
scroll natively with no restore at all, costs 5.3 ms against 55.7 ms, ships
~102 KB per keystroke instead of 1.11 MB, and avoids re-decoding ~1 MB of
inlined base64 font every time. The one argument the reload had — no
HTML-into-JS escaping — is answered by `serde_json::to_string` plus `JSON.parse`,
which is three lines and immune to quotes in prose.

Save-and-restore around a full reload was measured too and does work — restored
offset exact in 24/24, ~2.04 ms after `Finished`, with exactly one rendered frame
at offset 0. A `grim` sampler that provably catches a genuine single painted
frame 2.9% of the time caught nothing in 210 captures of that path, so the frame
is very likely never presented. It is rejected anyway: it is strictly more
machinery for strictly worse numbers, and letting the restore slip four frames
(measured) makes the flash plainly visible.

**Do not "simplify" the body swap back to `load_html`.** That is the edit this
measurement exists to prevent, and its symptom — the reader thrown to the top of
the document on every keystroke — is exactly what hollows out the "live preview
pane" row this unit ticks.

### Two numbers that were not in the earlier table

**Cold open costs ~255 ms from the call, not ~145 ms.** `Started`→`Finished` for
a first load in a fresh process is 143-152 ms, but call→`Finished` is
254.57 / 256.51 / 265.22 ms: about 110 ms elapses before `Started` fires at all,
spawning the WebProcess, and the `Started`-anchored figure hides all of it. Any
smoke that waits on a preview must budget from the call. Steady-state reloads
have no such gap.

Residual and unmeasured, identical under either strategy: scroll is preserved in
PIXELS, so an edit above the viewport shifts the reader's content even though the
offset is kept.

## Process note

The test-writer and this coder were run concurrently against one worktree, which
produced spurious clippy failures from half-applied work. That was an
orchestration error. Repo-writing agents are serialized from here.

## The Bazel repin — first unit in this series that owes one

`MODULE.bazel`'s `crate.from_cargo` reads `//:Cargo.lock`, and `Cargo.Bazel.lock`
carries `wolf-gnome` in `workspace_members` with rendered `gtk4`/`libadwaita`
nodes plus a top-level `checksum`. Adding `webkit6` moves `Cargo.lock` and
invalidates it. There is no `bazel` or `bazelisk` on this host, so the branch
necessarily lands with the two out of sync. Recommendation: an **unconditional**
`[dependencies]` entry matching `gtk`/`adw`/`cairo`, since `gtk4-sys` already
sits unconditionally in `Cargo.Bazel.lock` with a green macOS build.

## Test-writer pass 1 — complete

Created `crates/wolf-gnome/src/preview_ui.rs` (full API, bodies `todo!()`, ten
tests). Modified `pane_layout.rs` (`LeafContent`, `encode` signature),
`command_action.rs` (`UiAction::OpenPreview` / `EditSource`,
`EXPECTED_DISPATCHED_IDS` 15 → 17, arms left `Deferred` so the table is red) and
`lib.rs` (`pub mod preview_ui;`).

**It corrected a vacuity in the orchestrator's own specification.** The five
`needs_reload` rows I gave were *all* satisfiable by comparing `source_bufnr`
alone — nothing distinguished the tick. A sixth row, "same buffer, advanced tick
→ true", is the ordinary live-edit case and the only one that fails a
bufnr-only implementation; row 4 remains the one that fails a tick-only one.
`NAMED_RELOAD_ROWS = 6`.

**Two production edits outside its remit, both needed or nothing compiles, both
owed real work by the coder:**

1. `pane_view.rs` `layout_snapshot` builds `LeafContent` with `preview: false`
   and a `TODO(work unit 17)`. Behaviour-preserving, but the coder must supply
   the real per-pane preview flag or preview panes persist as editor leaves.
2. `shell.rs` `AppShell::perform` gained one combined arm for the two new
   variants (an `E0004` otherwise). **This stub is covered by nothing** — no
   fast-suite test reaches `perform`. The cage smoke must drive Preview through
   the palette so this path is exercised end to end, or it ships silently dead.

## Documentation debt (for the technical writer)

- `src/README.md`: new `#### preview_ui.rs` entry; `#### pane_layout.rs` has two
  stale lines (`encode(layout, paths)`, and "`preview` is always written false").
- `src/lib.rs` crate-level seam list: no `preview_ui` bullet.
- `command_action.rs` module doc says "the fifteen commands"; seventeen once the
  arms flip. `Command::PreviewInBrowser`'s deferral reason claims it needs "the
  preview row first", which this unit makes false.

## The readiness hazard, sized against the real gate

`state_seam::may_snapshot` (state_seam.rs 197-204) is
`bound_panes >= 1 && reported_windows >= 1`, **not** a `bound == reported`
equality. So a mixed editor/preview tab is unaffected: any single editor pane
keeps `win_buffers` non-empty.

The stall is real but narrow — it needs EVERY window in the tab to be a preview,
which drives `win_buffers.len()` to 0 while `bound_panes` stays positive. That is
reachable: preview into the active pane (`MODE_ACTIVE`) from a one-pane window.
`snapshot_readiness` returning `win_buffers.len() + previews.len()` is the right
fix, and the failure it prevents is losing the saved layout for an all-preview
tab, not a general persistence stall.

## Resumption workflow (orchestrator)

Repo-writing agents are SERIALIZED. Order:

0. Coder: the scroll-offset measurement, scratchpad only, no repo writes.
1. Architect (read-only, concurrent with 0): the remaining wiring plan.
2. Coder: implement everything but the new cage smoke.
3. Test-writer: the cage smoke + the `cage_command` guard's fifth entry.
4. Coder: run it, capture failure evidence with the feature disabled.
5. Reviewer.
6. Technical writer: the documentation debt above, plus `feat-list.md`.
7. Orchestrator: commits, then merge into `linux`.

## Amendment: the implementation spec

Four places the code contradicts the sections above, and the second changes a
scope decision.

1. The readiness hazard is the narrow one described two sections up, not a
   general stall. `snapshot_now` early-returns before it writes, so the failure is
   "persistence silently stops updating", not "the saved layout is wiped".
2. **`Command::Preview` and `Command::Edit` have no `actions::ACTION_TABLE`
   row.** After the flip their only routes are the command palette and the
   `:WolfPreview` / `:WolfEdit` Ex commands. Adding accelerator rows is OUT of
   scope: it reddens `actions::bound_set_is_exactly_this_unit` and buys no
   feat-list row. This is also why the cage smoke must go through the palette.
3. `wolf-gnome` has no `BUILD.bazel` at all, so the new smoke needs no
   `rust_test` target; `cargo test`'s discovery satisfies the CLAUDE.md rule. The
   `Cargo.Bazel.lock` repin obligation is unaffected.
4. `DECISIONS.md` carries the blocking inventory in three more places than the
   documentation-debt list records: lines 101, 181 and 191.

`crates/wolf-core/src/nvim.rs` 400-419 documents that `render_preview` renders at
`WidthMode::Full` — the web-view FRAME applies the column cap, not the HTML — and
that the `<head>` is byte-identical on every call, only the body changing. That is
the premise the body-swap branch rests on.

### `preview_ui.rs` — the seven bodies

The ten tests determine all seven bodies except the eight refusal strings.
`preview_placement`: `pane_count <= 1` FIRST (the `dominated` case forces the
order) → `MODE_SPLIT_RIGHT`; then `pane_count == 2 && !active_is_preview &&
other_preview_panes == 1` → `MODE_REUSE_PREVIEW_PANE`; else `MODE_ACTIVE`.
`edit_placement` mirrors it with the polarity INVERTED — `active_is_preview` is
true, not negated.

Both presses match `Option<Liveness>` with no `_` arm, liveness first, and the
accepted arm CALLS the placement function rather than re-deriving a mode.
`may_render` writes its three false arms separately rather than as an or-pattern
with a wildcard, so a new `Liveness` variant is an `E0004` as the module docs
promise. `needs_reload` is `(None, _) => false`, `(Some, None) => true`,
`(Some(c), Some(l)) => c != l`. `preview_status` forces BOTH paths empty for an
empty source and returns the bare word with no trailing space.

The eight refusal strings must be pairwise distinct across both directions,
non-empty, and not sentence-cased. Write them in `nav_ui.rs`'s diction; the three
liveness pairs are deliberately near-twins differing in the noun phrase, so
`preview not opened: …` reads correctly beside `source not edited: …`.

### `pane_view.rs`

`PaneWidgets` gains four fields: `host: gtk::Overlay` (parented into the `Paned`
tree, with the `DrawingArea` as its `set_child`), `preview:
RefCell<Option<PreviewInfo>>`, `web: RefCell<Option<webkit6::WebView>>`, and
`loaded: Cell<Option<PreviewLoad>>`. `PaneWidgets::new` builds the host, so all
three call sites are unchanged. Import `webkit6::prelude::WebViewExt` TARGETED —
the `webkit6::prelude::*` glob re-exports `gtk::prelude::*` and collides with the
existing `adw::prelude::*`.

The web view is created LAZILY when a pane first becomes a preview and removed
when it stops being one; N eager WebProcesses at launch would move every existing
smoke's timing budget. `set_measure_overlay(view, false)` is set EXPLICITLY even
though `false` is GTK's default, because the reason it must stay false — the
view's size request would inflate the pane minimum the crate is calibrated
against — is exactly what gets simplified away.

The join happens in `refresh_pane_state`, keeping its two-phase borrow shape.
One snapshot guard, bound to a local, reads `previews.get(&win)` (keyed by the
preview WINDOW handle, which is `PaneWidgets::win`, so it is a map lookup rather
than a scan) and `markdown_ticks.get(&source_bufnr)`; the guard drops; then the
guard-free per-pane core calls. An editor pane takes `pane_status` +
`full_width_for_win` as today. A preview pane takes `display_path` +
`preview_ui::preview_status` and its cap from `PreviewInfo.source_full_width`,
and `pane_status` is NOT called for it at all.

`draw_pane` extends its single existing borrow statement to a four-tuple carrying
`is_preview`, then skips exactly two things: the grid block and the scrollbar
thumb. The whole-allocation background fill and the entire statusbar strip are
never skipped. The cursor goes with the grid — under an opaque web view it would
paint invisibly.

`build_layout_widget` and `rebuild_pane_area` switch to `p.host`. **The web view
is a child of the host, so a rebuild never unparents it** and it keeps its
WebProcess and its loaded document across every split, close and divider drag.
Getting this wrong is silent and costs a respawn plus a fresh ~160 ms load on
every layout change.

### `shell.rs`

One new reader `preview_shape`, in `read_active_win`'s idiom: one core borrow,
one snapshot guard bound to a local. `pane_count` is `win_buffers.len() +
previews.len()`; `other_editor_panes` counts `win_buffers`' keys (which are
exactly the editor windows) minus the active one; `other_preview_panes` the same
over `previews`. Then `open_preview` / `edit_source` in `copy_selection`'s exact
shape, printing `preview not opened: {reason}` and `source not edited: {reason}`.
The `i64` returns are explicitly discarded with a one-line reason: the front-end
learns the new pane from the next wake's `wolf_preview` notification.

**Never fold `is_markdown_active()` into a scope that already holds a snapshot
guard** — it takes its own, and that is the deadlock `shell.rs`'s borrow
discipline exists to prevent.

### The cage smoke

One test, two legs: `preview_pane_renders_and_edit_source_returns_under_cage`.
Two fixtures whose filenames cannot satisfy their own assertions, both registered
in `RELAUNCH_FIXTURE_NAMES` and `ASSERTED_OCR_TERMS`. Open the other file, vsplit
via the `<C-w>v` chord (not a typed `:vsplit` — the keymap is what emits the
`wolf_layout` Split intent), open the Markdown file in the new pane, then drive
Preview through the palette. The placement is `MODE_ACTIVE`, so the source text is
no longer displayed anywhere and an OCR hit on the body can only be WebKit's
pixels. Then drive Edit through the palette; the placement is
`MODE_REUSE_EDITOR_PANE`, so the other pane's buffer is replaced.

Four discriminators: OCR of the rendered body (falsified by dropping the
`load_preview_document` call); OCR of `preview` in the statusbar (falsified by
letting a preview pane keep `core.pane_status`); a `writefile` readback showing
exactly one `Preview: ` buffer among two windows (falsified by restoring today's
stub arm in `perform`); and a second readback showing the source bound into the
other pane with `zother.md` gone (falsified by copying `preview_placement`'s
polarity into `edit_placement`). The `CommandMru` check is a precondition, not a
discriminator — `record` runs BEFORE the dispatch.

### Deliberately not tested, not ticked

Picker-over-preview compositing (the residual Wayland-subsurface risk) and
scroll-offset preservation. No smoke here can read a scroll offset.

### Known present-tense limitation to record, not fix

A click inside a preview pane never reaches `on_pane_clicked`, because the web
view takes pointer input — which it must, or the pane cannot be scrolled. So a
preview pane cannot be focused by clicking; `<C-w>w` and nvim's own
post-placement focus are the routes. **Do not "fix" this with
`set_can_target(false)`**, which kills scrolling and with it most of what makes
the pane a preview rather than a picture.

### Order

`preview_ui` bodies → `pane_layout::encode` → `command_action` arms →
`pane_view` structure (host overlay only, no behaviour change; re-run
`click_then_close_keeps_keyboard_input_alive_under_cage` here, before any preview
code exists, so a regression is diagnosable as a widget-tree bug) → the
`pane_view` join → the web view → `shell.rs` → the smoke → failure evidence →
correction 4.

## A test claims more than it does

`the_two_placements_are_inverses` does NOT catch the polarity inversion.
Measured: with `edit_placement`'s reuse clause written as `!p.active_is_preview`,
the whole `--lib` suite goes 253 → 252 passed with `edit_placement_table` the
single failure; the inverses test stays green. It asserts each function's RANGE
over the sweep, and an inverted clause still produces all three modes — just for
different inputs. So `edit_placement_table`'s row 4 is the sole guard on the
single most likely edit in the file, and `preview_ui.rs`'s module doc (lines
25-33) overstates it.

Resolution: strengthen the test to state the separation it claims, rather than
weaken the doc to match. The doc describes the intended design and the design is
right; one row is too thin a guard for it.

## The payload must be double-encoded, and that was a measured bug

The first implementation encoded the body ONCE, so the JS literal's value was the
HTML itself and `JSON.parse` was handed HTML. Every keystroke printed
`preview body swap failed: about:blank:1:68: SyntaxError: JSON Parse error:
Unrecognized token '<'` while the preview silently kept showing the stale
document. `preview_swap_payload` applies `serde_json::to_string` twice — inner
makes JSON text, outer makes the JS string literal, the script's `JSON.parse`
undoes the inner — then escapes U+2028/U+2029 in the outer text.

**This would have shipped green.** The initial open goes through `load_html` and
looks perfect; only the SECOND load into a view takes the swap branch. A smoke
that opens a preview and asserts on it, without then editing the source, cannot
see it. That is why the smoke needs a live-edit leg.

## Measured: the suite guards none of this

With `AppShell::open_preview` neutered by an early `return` — which removes the
RPC, the preview window, the `PreviewInfo`, the web view, the statusbar and the
persisted flag — the whole workspace stayed green: 253 lib passed, 48 smokes
passed, exit 0. No assertion anywhere in the repository covers a single
behaviour this unit adds. Closing that is the test-writer's pass.

## The all-preview tab is reachable, so `snapshot_readiness`'s fix is not dead

The coder doubted it, reasoning that `preview_placement` never answers
`MODE_ACTIVE` for a lone pane. True, but the route runs through three panes: at
`pane_count == 3` every shape falls to `MODE_ACTIVE`, so pressing Preview in each
pane in turn converts all three, driving `win_buffers.len()` to 0 while
`bound_panes` stays at 3. The two-pane route is genuinely closed — with one
preview already present, `other_preview_panes == 1` sends the press to
`reuse_preview_pane` instead.

Not covered by a test: the smoke always has two panes, and a three-pane cage
scenario costs a launch to guard one integer. Recorded here instead.

## Review verdict: needs revision, on evidence rather than on code

The reviewer found no defect in the borrow discipline, the blocking gates, the
totality, the reload strategy or the widget structure, and confirmed nothing out
of scope was built or ticked. Four items block:

1. The smoke's assertion table has six empty `measured` cells. The red runs were
   all performed and captured; they were never pasted back into the table the
   file's own header says is the coder's to fill.
2. Row 7's prose is wrong, confirmed independently: under the stub-`perform`
   revert, assertion 5 PASSES — the split pane still draws `pvsrc.md`'s raw
   Markdown, which contains the asserted heading — and assertion 6 reports first.
3. `head_offset` and `body_inner` are pure, decision-carrying functions with zero
   coverage, in a module that already unit-tests their two siblings. The head
   guard's four fall-back conditions and `body_inner`'s first-open/last-close
   asymmetry are exactly what `load_preview_document` is built on.
4. `command_action.rs`'s test `desc` for `PreviewInBrowser` still asserts a
   blocker this unit removed, though the runtime reason beside it was corrected.

### Recorded, not fixed

- `refresh_previews` sets `loaded` even on the path where the document never
  reached a view. Unreachable today because `sync_preview_view` runs over the
  same ids first, but the invariant is load-bearing and undocumented at the write
  site.
- `body_inner`'s first-`<body` scan runs over the head too, and the head carries
  the font licence notices inside an HTML comment. A licence text containing the
  literal `<body` would move the start offset into the head. `wolf-core`'s
  `the_notices_cannot_terminate_an_html_comment` guards `<!--`/`-->`/`--!>` and
  nothing else. Shared with macOS, so parity rather than regression.
- A non-UTF-8 source path goes through `to_string_lossy` and is persisted as
  `abs_path`, so it would not reopen. Bounded by `PaneStatus`'s `String` fields.
- `wolf-core`'s GUI-dependency denylist grep does not name `webkit`, `soup` or
  `javascriptcore`. Not broken — `webkit6` requires `gtk4`, which the existing
  pattern catches — but one class behind the workspace.

## Status: complete

Whole workspace green — 93 test targets, 0 failed; 261 `wolf-gnome` lib tests and
49 smokes. `cargo fmt --check`, `prettier --check "**/*.md"`, `cargo clippy
--workspace --all-targets -D warnings` and `cargo build --workspace
--all-targets` all clean. `markdown_check.sh` reports no findings across the
seven edited documents.

Every discriminator has a fresh red run captured against the final tree, pasted
into the smoke's own `measured` column. The reordering worked: the pixel-free
readback now aborts first under the stub-`perform` revert, and assertion 6 was
confirmed independently by OCR-ing the retained capture NOT to fail under it.

Four `feat-list.md` rows ticked and no others.

Owed and not doable here: `Cargo.Bazel.lock` is out of sync with `Cargo.lock`,
which gained six packages (`webkit6`, `webkit6-sys`, `javascriptcore6`,
`javascriptcore6-sys`, `soup3`, `soup3-sys`). There is no `bazel`/`bazelisk` on
this host. It needs `CARGO_BAZEL_REPIN=1` on a macOS host. `wolf-gnome` has no
`BUILD.bazel`, so no test-target wiring is owed — only the lock.
