# wolf-gnome

The GTK 4 / libadwaita platform front-end for Wolf, the Markdown editor that
embeds Neovim (see [`architecture.md`](../../architecture.md), esp. the
front-end section and decision 2 on native paint). It consumes
[`wolf-core`](../wolf-core/README.md) as a normal Rust crate dependency.

## Scope

This crate is a **runnable single-window GTK 4 / libadwaita application**. The
window is one `adw::ApplicationWindow` (with an `adw::HeaderBar`) whose content is
a horizontal `gtk::Paned`: a **file-tree sidebar** on the left (see
[The file-tree sidebar](#the-file-tree-sidebar)) and, on the right, a **native
pane view** inside a `gtk::Overlay` that also hosts the file picker. The pane view
is a tree of nested `gtk::Paned`s — one `gtk::DrawingArea` per Neovim window,
driven by a pure `PaneTree` layout — plus a bottom **cmdline strip** that paints
the outer grid (where the command line and messages live under `ext_multigrid`).
**Split panes** work: split (`<C-w>s`/`v`, `:split`/`:vsplit`), close (`<C-w>c`),
focus moves (`<C-w>h/j/k/l`), and divider drag-to-resize, driven by `wolf-core`'s
layout-intent channel. There are still **no native file dialogs** — but a built-in
**file picker** (textbox + fuzzy-matched list, see [The file picker](#the-file-picker))
opens files into the active pane, and saving is from within Neovim (`:w`,
`:saveas`).

The application is built on top of pure, widget-free **seams** that can also be
verified *headlessly*, with no display server, no window, and no event loop:

- `gdk_key` — GDK keyval + modifier state → `wolf_core::KeyEvent`.
- `paint` — render one `wolf_core::Grid` (+ its highlight table) onto a Cairo
  context.
- `grid_size_for` — pixel size + measured cell size → grid `(cols, rows)`.
- `PaneTree` (`layout.rs`) — the pure **N-ary frame** pane-layout tree
  (split/remove/leaves/pane_rects/neighbor), matching Neovim's window model: a
  frame is a leaf or an oriented split of N children with per-child weights; a
  split in the SAME orientation as its containing frame FLATTENS into an equal-sized
  sibling row/column, while a PERPENDICULAR split nests, and removing a pane
  re-equalizes and collapses/merges back to a canonical tree. Unit-tested
  headlessly; `pane_view.rs` is the only place that turns it into widgets.
- `cursor_rect` / `cursor_colors` (`cursor.rs`) — the cursor's shape geometry and
  its reverse-video `(fill, glyph)` color pair, computed without GTK.
- `is_picker_trigger` (`picker.rs`) — whether a keyval + modifier state is the
  picker's Ctrl+Shift+P trigger; plus the file-private `next_selection` /
  `restored_index` list-navigation helpers, all unit-tested headlessly.
- `children_of` / `ancestors_to_reveal` / `node_rel_path` / `rel_path_for`
  (`tree.rs`) — the pure hierarchical projection of the picker's flat file list
  into the file-tree sidebar's per-directory children + reveal helpers,
  unit-tested headlessly.

These stay free functions independent of any widget, which is exactly what makes
them testable without a display. The application wiring that binds them to a live
[`wolf_core::WolfCore`] lives in [`src/app.rs`](src/README.md) and the thin
binary entry point in [`src/main.rs`](src/README.md).

## Running

After editing `flake.nix` the already-loaded direnv environment is stale, so run
through `nix develop --command`, which re-evaluates the flake:

```sh
nix develop --command cargo run -p wolf-gnome
```

This needs a display (a Wayland or X11 session), or it can be run under the
headless smoke compositor (see [The headless smoke test](#the-headless-smoke-test)).
It requires `WOLF_NVIM_BIN` to point at the pinned Neovim (the devShell exports
it; nvim is never taken from `PATH`). The font size defaults to **14.0 pt** and
can be overridden via the `WOLF_FONT_PT` environment variable (the smoke test sets
it to `28` so glyphs are large enough to OCR). The font family is `DejaVu Sans
Mono`, the GApplication id is `org.wolf.Wolf`, and the default window is 1200×640
(widened from the editor-only default to leave the editor pane its usual width
after the 250 px file-tree sidebar).

## Architecture wiring

`app::run` builds an `adw::Application`, and on `activate` `build_ui` constructs
the window and attaches a freshly started `WolfCore`:

- **Wake marshalling.** `WolfCore` runs Neovim on its own tokio runtime and fires
  a single `WakeFn` on a tokio worker thread when a new frame is ready. Because
  that thread cannot touch GTK, the wake only `try_send`s onto a bounded
  `async-channel` (capacity 1, which coalesces a burst of flushes — a dropped
  "full" send just means a redraw is already pending). A
  `glib::spawn_future_local` task on the GTK main thread drains the channel and, per
  signal, calls `panes.on_render_wake()` and `tree.on_render_wake()`.
- **Pane view + draw.** The editor area is the native pane view (`pane_view.rs`).
  On each wake it creates/splits/closes panes from `WolfCore::drain_intents`,
  resolves each pane's bound grid via `grid_for_win`, marks the active pane from
  `Snapshot.active_win` (fallback: the cursor's grid), and queues redraws. Each
  pane's draw func paints ITS window grid via the pure `paint` seam; the pane that
  owns the cursor's grid also draws a **mode-aware cursor** at the cursor cell. The
  shape comes from the current mode's `ModeInfo.shape` (a block in normal mode, a
  thin vertical/horizontal bar in insert/replace), drawn **opaque** via
  `cursor_rect`. The `(fill, glyph)` colors come from `cursor_colors`, which uses
  **reverse video**: it honors the mode's cursor highlight only when that yields a
  visibly distinct pair, otherwise inverts the underlying cell. For a block cursor
  the cell's glyph is redrawn in the inverted color; a bar cursor leaves it
  untouched. The bottom **cmdline strip** paints the outer grid's last row so typed
  `:`/`/` input stays visible.
- **Input.** A `gtk::EventControllerKey` on the window routes each key through the
  pure `gdk_key` seam; a `Some` result is sent via `WolfCore::send_input` and the
  event is consumed (`Propagation::Stop`), a `None` is left to propagate. Keys
  reach nvim's current window globally; a `gtk::GestureClick` on a pane calls
  `WolfCore::focus_win` to focus it.
- **Resize.** Each pane's `connect_resize` maps its pixel allocation to a grid size
  via the pure `grid_size_for` seam and calls `WolfCore::resize_grid` for that
  pane's grid (then resizes the outer grid via `WolfCore::resize`); the
  authoritative sizes come back via core's `grid_resize`. Dragging a `gtk::Paned`
  divider drives the same path.
- **Lifecycle.** `WolfCore` is non-`Send`; it lives only on the main thread in an
  `Rc<RefCell<Option<WolfCore>>>`, shared by the draw/key/resize/close closures.
  Closing the window takes the core out (in its own statement, to release the
  `RefCell` borrow) and runs `WolfCore::shutdown` for a clean teardown. Only the
  `WakeFn` ever crosses a thread boundary. If `WolfCore::start` fails, the error
  is logged and no window is built.

## The file-tree sidebar

The left pane of the `gtk::Paned` is a file-tree sidebar (implemented in
[`src/tree_view.rs`](src/README.md), built on the pure
[`src/tree.rs`](src/README.md) projection). It lists the working directory as a
hierarchy and tracks the file being edited:

- **Same source as the picker.** It is populated by a long-lived
  `wolf_core::FileSearch` started at build time — the exact mechanism the picker
  uses (the streaming `ignore` traversal seeded by the MRU; `.git/`/`.wolf/`/
  `.gitignored` excluded consistently). The tree is a hierarchical projection of
  that flat file list: directories are derived from file paths, and only
  directories that (transitively) contain a listed file appear. It streams in as
  the traversal progresses.
- **Layout.** A `gtk::ListView` over a `gtk::TreeListModel` + `gtk::TreeExpander`
  (the modern, non-deprecated tree widgets). The **top level is expanded by
  default**; all other directories start collapsed.
- **Single-click.** A single click on a **file** opens it (`InputEvent::Open`,
  recorded in the MRU) and returns focus to the editor; a single click on a
  **directory** toggles its expansion.
- **Highlight + reveal the current file.** The file currently being edited (from
  `wolf_core::Snapshot::current_file`, delivered on the render wake) is shown in
  bold and **revealed**: its ancestor directories are expanded, its row is
  selected, and it is scrolled into view. This happens after **both** picker
  navigation and Neovim-driven navigation (e.g. `:e`), since core tracks the active
  buffer via an autocmd regardless of how it changed.

The pure, headlessly-tested seams are in `tree.rs` (`children_of`,
`ancestors_to_reveal`, `node_rel_path`, `rel_path_for`); the GTK wiring in
`tree_view.rs` cannot be unit-tested headlessly and is exercised by the
`tree_sidebar_shows_files_under_cage` smoke (see [`tests/README.md`](tests/README.md)).

**v1 limitation:** a `current_file` change rebuilds the tree's root store, which
collapses any manual nested expansion (the reveal re-expands the path to the new
current file, but other manually-opened directories are not restored).

## The file picker

The window has a built-in file picker (implemented in
[`src/picker.rs`](src/README.md)). It opens two ways: the **Ctrl+Shift+P** key
chord (deliberately non-conflicting — every Neovim key, including `<C-p>`, is
preserved) and a header-bar **Open** button. It floats over the editor as an
`Overlay` child: a search textbox over a results list.

- **Filtering.** Typing fuzzy-matches against the file list (a streaming
  traversal of the cwd, seeded by the MRU cache — both provided by `wolf-core`).
  Matching is **case-insensitive** and **MRU files rank first**; among the rest, a
  query that is a strict prefix of a filename word is preferred over a merely
  scattered subsequence. An empty query lists all files (MRU-first). The actual
  ranking is `wolf_core::rank_matches`; the picker only renders the top results.
- **Navigation + activation.** `Up`/`Down` move the list selection (focus stays
  in the textbox, so typing keeps working; selection is preserved as streamed
  results arrive). `Enter` (or clicking a row) opens the selected file; `Esc`
  closes the picker. While it is open, all keys go to the picker and are not
  forwarded to Neovim.
- **Open + MRU.** Selecting a file sends it through `wolf-core` as
  `InputEvent::Open { path }`, which Neovim loads via `:e`; the core's input pump
  records the open in the MRU cache, so it ranks first next time. There is no
  separate "select" path.

The only pure, headlessly-tested seam here is `is_picker_trigger` (the trigger
predicate); the navigation clamp/preserve logic is factored into the pure
`next_selection` / `restored_index` helpers (also unit-tested), with the full
end-to-end behavior covered by the opt-in picker cage smokes (see
[`tests/README.md`](tests/README.md)).

## The seams

### `gdk_key` (from [`input`](src/input.rs))

```rust
pub fn gdk_key(keyval: gdk::Key, state: gdk::ModifierType) -> Option<wolf_core::KeyEvent>
```

This is the exact shape of a future `gtk::EventControllerKey` `key-pressed`
callback, kept as a free function so it can be unit-tested without a display
(`gdk::Key` constants are compile-time; the printable path reads
`keyval.to_unicode()`).

Mapping:

- **Named keys** (resolved first, since their `to_unicode()` is often a control
  char): `Return` & `KP_Enter` → `Enter`; `Escape` → `Esc`; `Tab` &
  `ISO_Left_Tab` → `Tab`; `BackSpace` → `Backspace`; `Delete` → `Delete`;
  `Left`/`Right`/`Up`/`Down`; `Home`/`End`; `Page_Up`/`Page_Down` →
  `PageUp`/`PageDown`; the contiguous `F1..=F12` range → `Key::F(n)`.
- **Printable fallback**: any other keyval whose `to_unicode()` yields a
  non-control char → `Key::Char(c)`.

Modifier masks: `ctrl` = `CONTROL_MASK`, `shift` = `SHIFT_MASK`, `alt` =
`ALT_MASK`, `meta` = `SUPER_MASK`.

**Shift-glyph contract:** GDK already delivers the *shifted* glyph in the keyval
(Shift+`a` arrives as the `A` keyval, `to_unicode()` = `'A'`). So the printable
branch **always sets `shift = false`** and lets the glyph carry the case — this
matches `wolf_core::KeyEvent`, where `shift` is ignored for `Key::Char`. **Named
keys keep the real `shift`** (so `<S-Tab>`, `<S-Left>`, `<S-F5>`). The X11 quirk
`ISO_Left_Tab` *is* Shift+Tab, so it maps to `Key::Tab` with `shift = true` even
if the platform did not also set the SHIFT mask.

Returns `None` for: bare modifier / lock keys (`Shift_L/R`, `Control_L/R`,
`Alt_L/R`, `Meta_L/R`, `Super_L/R`, `Hyper_L/R`, `Caps_Lock`, `Shift_Lock`,
`Num_Lock`, `Scroll_Lock`); and any keyval whose `to_unicode()` is absent or a
control character.

### `paint` + `CellMetrics` (from [`render`](src/render.rs))

```rust
pub struct CellMetrics { pub cell_w: f64, pub cell_h: f64, pub baseline: f64, pub font: pango::FontDescription }
impl CellMetrics { pub fn for_font(cr: &cairo::Context, family: &str, size_pt: f64) -> Self }
pub fn paint(cr: &cairo::Context, grid: &wolf_core::Grid, highlights: &wolf_core::HighlightTable, metrics: &CellMetrics)
```

`paint` is pure and widget-free: it paints ONE grid (under `ext_multigrid` the
front-end paints a single window grid at a time, so it takes the `Grid` + its
`HighlightTable` rather than a whole `Snapshot`) onto **any** Cairo surface — an
offscreen `ImageSurface` in tests, a `DrawingArea`'s context in production.

`CellMetrics::for_font` measures a monospace cell using `cr` only to obtain a
Pango context (its surface size is irrelevant): `cell_w` from Pango's approximate
char width (exact for monospace), `cell_h` = ascent + descent, `baseline` =
ascent.

Per-cell algorithm:

1. Resolve the highlight via `wolf_core::HighlightTable::resolve` (which has
   already applied `reverse` and the editor defaults).
2. Pick fg/bg, falling back to a built-in `FALLBACK_FG` (white) / `FALLBACK_BG`
   (black) when even the defaults are unset. 8-bit channels are divided by 255 to
   Cairo's 0.0–1.0 range.
3. **Always fill the cell's background rectangle** — this keeps wide-trailing /
   never-painted cells aligned. The Cairo `Result` is handled, not dropped.
4. If `cell.text` is empty (a wide-char trailing placeholder or never-painted
   cell) skip glyph rendering; otherwise lay the text out with Pango/pangocairo
   and draw it in fg.

Attributes applied: **bold**, *italic*, and single **underline**. Undercurl and
strikethrough are deferred. No system theme is queried (deferred).

### `grid_size_for` (from [`geometry`](src/geometry.rs))

```rust
pub fn grid_size_for(width_px: i32, height_px: i32, cell_w: f64, cell_h: f64) -> (u16, u16)
```

The resize handler's pure pixel-to-grid math, kept GTK/pango-free so it is
unit-testable headlessly. `cols = floor(width_px / cell_w)`, `rows = floor(height_px
/ cell_h)`, each clamped to `[1, u16::MAX]`. As a resize guard, the **whole** result
collapses to `(1, 1)` if either pixel dimension is non-positive or either cell size
is degenerate (zero, negative, NaN, or infinite).

## Building and testing

After editing `flake.nix` the already-loaded direnv environment is stale (it has
no GTK), so the build gate is to run everything through `nix develop --command`,
which re-evaluates the flake:

```sh
nix develop --command cargo build -p wolf-gnome
nix develop --command cargo test  -p wolf-gnome
```

The devShell provides the GTK toolchain (`gtk4`, `cairo`, `pango`, `glib`,
`gdk-pixbuf`, `graphene`, `harfbuzz`) and pins a deterministic font: it points
`FONTCONFIG_FILE` at a generated copy of
[`testdata/fonts.conf`](testdata/fonts.conf) exposing **only** DejaVu Sans Mono
(no host font dirs), so Pango measurement / Cairo rasterization is reproducible.

The round-trip test spawns **real Neovim** (no mocks, per project policy) located
via `WOLF_NVIM_BIN` (also exported by the devShell). See
[`tests/README.md`](tests/README.md).

### The headless smoke test

A suite of **opt-in, `#[ignore]`d** end-to-end tests launches the real application
under a headless Wayland compositor and confirms it actually draws a window a human
could see. Run them explicitly:

```sh
nix develop --command cargo test -p wolf-gnome -- --ignored
```

They require the flake's smoke toolchain (`cage`, `wtype`, `grim`, `tesseract` on
`PATH`), a set and writable `XDG_RUNTIME_DIR` (where the Wayland socket lives), and
`WOLF_NVIM_BIN`. Each runs its whole scenario inside `cage`, drives the window with
`wtype`, screenshots the live window with `grim`, and OCRs it with
`tesseract --psm 11` ("sparse text" mode, needed for the light-on-dark editor
grid). The base smoke types `hello` in insert mode and asserts the OCR loosely
contains it; further smokes cover the file picker, the file-tree sidebar, and the
four split-pane operations (vertical / horizontal split, focus move, close pane).
Missing preconditions fail loud rather than silently skipping. See the full list in
[`tests/README.md`](tests/README.md).

## gtk-rs versions

The gtk-rs crates are one coherent gtk-rs release set: `gtk4` 0.11.x / `gdk4`
0.11.x / `libadwaita` 0.9.x with `cairo-rs` / `pango` / `pangocairo` 0.22.x. (In
the gtk-rs ecosystem the gtk4-rs repo and the "core" repo are released together
but carry different minor numbers; the invariant is "one coherent release set",
not "one identical minor" — mismatched `-sys` minors fail to link. `libadwaita`
0.9 carries no version feature, targeting the system GTK already present.) These
are built against the flake's GTK 4.22, cairo 1.18, and pango 1.57. `cairo-rs`
enables the `png` feature for the `#[ignore]`d golden-regeneration dev-aid test.

`gtk4` enables the **`v4_12`** feature, which the file-tree sidebar's
`gtk::ListView::scroll_to` (used by reveal to scroll the current file into view)
requires. This raises the minimum **system GTK to 4.12**; the flake's GTK 4.22
satisfies it. The feature is `wolf-gnome`-only and does not affect `wolf-core`'s
GUI denylist.

## Deferred to later phases

The window, the file picker, the file-tree sidebar, and native split panes exist,
but the following remain deferred:

- Native file dialogs (open / save / save-as) — opening now goes through the
  built-in file picker; saving is still from within Neovim (`:w`, `:saveas`).
- Theming (querying the OS theme and pushing it into Neovim).
- Mouse and scroll input.
- A NATIVE command line / messages / popup menu (`ext_cmdline` / `ext_messages` /
  `ext_popupmenu` as Adw widgets). The grid-drawn command line / messages are
  shown today in the bottom cmdline strip (which renders only the outer grid's
  LAST row — multi-row prompts are truncated until `ext_messages`).
- HiDPI scale handling (the cell-measurement probe is 96-dpi).
- IME / `IMContext` composition input.
- Floating windows (`win_float_pos`) and windows opened by non-overridden paths
  (`:help`, quickfix, preview, `:diffsplit`) — see the split-pane known
  limitations in [`src/README.md`](src/README.md).

## Note on the GUI denylist

The `wolf-core` "no GUI toolkit in the dependency tree" denylist does **not**
apply to this crate — GTK is allowed here. It still holds for `wolf-core`,
because the dependency goes `wolf-gnome` → `wolf-core`, never the reverse, so
GTK never enters core's tree.
