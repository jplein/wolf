# wolf-gnome decisions

The durable constraints behind this crate. Read this before changing anything here; most of it exists because a reasonable-looking simplification breaks something. Per-file detail lives in [`src/README.md`](src/README.md).

## Scope

This is the GTK 4 / libadwaita front-end: a runnable single-window application that consumes [`wolf-core`](../wolf-core/README.md) as an ordinary Rust crate dependency. It renders Neovim natively rather than embedding a terminal, so every pixel comes from a Cairo/Pango paint of a core `Snapshot`.

The window carries a file-tree sidebar, a native pane view with real split panes, a floating file picker, and a bottom command-line strip. Split, close, focus move, and divider drag all work, driven by core's layout-intent channel.

Files are opened through Neovim, never through a file dialog. The picker and the sidebar send `InputEvent::Open`, which core performs as `:e`; saving is `:w` or `:saveas` from inside Neovim. The binary takes no command-line arguments, and hands GTK an empty argv so the real command line is never parsed.

## Platform and build

Linux only, built with Cargo inside the Nix devShell. Run everything through `nix develop --command`, because after a `flake.nix` edit an already-loaded direnv environment is stale and has no GTK.

This crate deliberately has no Bazel target at all. That absence is what keeps the GTK stack out of `wolf-core`'s Bazel dependency graph, so the one hard rule in [`wolf-core/DECISIONS.md`](../wolf-core/DECISIONS.md) holds under the macOS Bazel build as well as under `cargo tree`. Adding a `BUILD.bazel` here breaks that guarantee. The verification queries live in [`build.md`](../../build.md).

The devShell supplies the toolchain that the gtk-rs `-sys` crates probe for (gtk4, cairo, pango, glib, gdk-pixbuf, graphene, harfbuzz), exports `WOLF_NVIM_BIN` so the pinned Neovim is never taken from `PATH`, and pins the font stack. `FONTCONFIG_FILE` points at a generated copy of [`testdata/fonts.conf`](testdata/fonts.conf) exposing only DejaVu Sans Mono with no host font directories, which is what makes Pango measurement and Cairo rasterization reproducible enough to assert on pixels.

## The GUI denylist does not apply here

`wolf-core`'s "no GUI toolkit in the dependency tree" rule constrains core, not this crate. GTK is allowed here because the dependency runs `wolf-gnome` to `wolf-core` and never the reverse, so nothing this crate adds can enter core's tree.

## gtk-rs versions

The gtk-rs crates form one coherent release set: `gtk4` 0.11.x, `gdk4` 0.11.x, `libadwaita` 0.9.x, with `cairo-rs`, `pango`, and `pangocairo` at 0.22.x. In the gtk-rs ecosystem the gtk4-rs repo and the core repo release together but carry different minor numbers, so the invariant is one coherent release set rather than one identical minor. Mismatched `-sys` minors fail to link.

`libadwaita` carries no version feature, because the target is the system GTK already present rather than a newer pinned ABI. The set builds against the flake's GTK 4.22, cairo 1.18, and pango 1.57.

`gtk4` enables `v4_12`, which the sidebar's `ListView::scroll_to` requires. That raises the minimum system GTK to 4.12; the flake satisfies it, and the feature is scoped to this crate. `cairo-rs` enables `png` for the ignored golden-regeneration dev aid.

## The pure seams

The crate is both a library and a binary, and the split is the most important structural fact about it. The binary is the window; the library holds every decision expressible as a function of values, so it can be unit-tested with no display server, no widget, and no Neovim.

Those seams are `gdk_key`, `paint` with `CellMetrics`, `grid_size_for`, the `PaneTree` layout model, `cursor_rect` and `cursor_colors`, `is_picker_trigger` with its list-navigation helpers, and the tree projection re-exported from core. They stay free functions and pure types precisely so no display is needed to exercise them.

The consequence is this crate's testing posture: the GTK glue in `app.rs`, `pane_view.rs`, `picker.rs`, and `tree_view.rs` has no headless coverage. When you add behavior, put the decision in a seam and test it there; leaving it inside a widget closure means it is untested.

## Threading and the wake

`WolfCore` runs Neovim on its own tokio runtime and fires a single wake closure when a frame is ready. The wake runs on a tokio worker and so cannot touch GTK: it only pushes onto a bounded `async-channel`, and a `glib::spawn_future_local` task drains that channel on the main thread and drives the pane view and the sidebar.

The channel holds one slot on purpose. It coalesces a burst of flushes into a single queued redraw, and a dropped send on a full channel is correct because a redraw is already pending. Sends ignore errors, so a late wake arriving after the channel closes is harmless.

`WolfCore` is not `Send`. It lives only on the main thread in an `Rc<RefCell<Option<_>>>` shared by the widget closures, and the wake closure is the only thing that crosses a thread boundary. Closing the window takes the core out in its own statement, releasing the borrow before `shutdown` blocks on teardown.

The picker and the sidebar each run a long-lived `FileSearch` on the same pattern. Their drain tasks hold only a `Weak` reference, so dropping a handle ends the task instead of leaking a cycle, and closing either one drops its search to cancel the traversal.

## Window structure

The window is an `adw::ApplicationWindow` whose content is a vertical box: an `adw::HeaderBar` over a horizontal `gtk::Paned`. The Paned's start child is the file-tree sidebar; its end child is a `gtk::Overlay` whose main child is the pane view and whose overlay child is the picker.

The pane view is a tree of nested `gtk::Paned` with one `gtk::DrawingArea` per Neovim window, built from the pure `PaneTree`, plus a one-row command-line strip along the bottom. The strip is also the focus-return target the picker and the sidebar grab back to, because it is the focusable widget in the editor region from which keys bubble to the window-level key controller.

Input flows one way: a window-level key controller translates through `gdk_key` into `WolfCore::send_input`, and Neovim routes each key to its current window. Resize flows per pane through `grid_size_for` into `resize_grid`, then resizes the outer grid; the authoritative sizes come back from core.

## The headless smoke test

The GTK glue is covered only by opt-in, `#[ignore]`d end-to-end smokes that launch the real binary under `cage`, the headless wlroots compositor, drive it with `wtype`, screenshot the live window with `grim`, and read it back with `tesseract`. They prove the stack actually draws a window a human could see. See [`tests/README.md`](tests/README.md) for the full list.

They need `cage`, `wtype`, `grim`, and `tesseract` on `PATH`, a set and writable `XDG_RUNTIME_DIR` where the Wayland socket lives, and `WOLF_NVIM_BIN`. A missing precondition fails loud rather than skipping quietly, because absence means the smoke toolchain was not provisioned.

Three details are load-bearing. Every smoke holds a shared mutex for its whole body, because two `cage` compositors started concurrently contend for the same Wayland socket and both fail. Each scenario runs inside `cage` as a single shell command, since `cage` exports its `WAYLAND_DISPLAY` only to its own child process tree. The app is launched with `WOLF_FONT_PT` raised so glyphs are large enough to read, and `tesseract` runs with sparse-text page segmentation, because the default mode returns nothing on the sparse light-on-dark editor grid.

## Present limitations

- No native file dialogs; opening goes through the picker or the sidebar, and saving is done from within Neovim.
- No theming in either direction: the front-end neither queries the OS theme nor pushes one into Neovim. Neovim's own colorscheme reaches the grid paint, and the `WolfCursor` theme group reaches the cursor.
- Pointer input is limited to focusing a pane and activating a sidebar or picker row. No pointer or scroll events reach Neovim.
- The command-line strip renders a single row: an active command line, or the last message line when idle. There is no completion popup and no multi-line scrollable message view, so a multi-line message shows only its final line. Core folds all three UI extensions regardless, so adding the Adwaita widgets later is additive.
- Only windows created through the overridden split commands become panes. A window from `:help`, quickfix, a preview, or `:diffsplit` gets a grid but no pane, and floating or external windows are ignored.
- Focus-direction moves resolve against the layout tree's nominal equal weights rather than dragged divider positions, so a manual drag can make a move pick a different neighbour than the pixels suggest. A drag survives until the next resize, split, or close, at which point the divider returns to the equal ratio.
- A sidebar refresh rebuilds the root store, which collapses manual expansion; only the path to the current file is re-expanded.
- Undercurl and strikethrough do not render. Underline renders as a single line.
- No HiDPI scale handling: the cell-measurement probe assumes 96 dpi.
- No input-method composition support.
- Crash and stall recovery is not surfaced. Core exposes liveness and restart, but this front-end neither reads liveness on a wake nor offers a restart affordance, so a crashed Neovim leaves the window frozen on its last frame.
- There is no buffer-picker or command-palette overlay. The core surface behind them is available to this crate; no widget is wired to it.
