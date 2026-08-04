# wolf-gnome decisions

The durable constraints behind this crate. Read this before changing anything here; most of it exists because a reasonable-looking simplification breaks something. Per-file detail lives in [`src/README.md`](src/README.md).

## Scope

This is the GTK 4 / libadwaita front-end: a runnable single-window application that consumes [`wolf-core`](../wolf-core/README.md) as an ordinary Rust crate dependency. It renders Neovim natively rather than embedding a terminal, so every pixel comes from a Cairo/Pango paint of a core `Snapshot`.

The window carries a file-tree sidebar, a native pane view with real split panes, a floating file picker, command palette, buffer picker and project search, a GNOME primary menu, an empty state for when no project is open, and a bottom command-line strip. Split, close, and focus move are driven by core's layout-intent channel; a divider drag is the `gtk::Paned` handle's own gesture, landing in the pure tree's weights. Menu items, accelerators, and commands drained from Neovim all resolve through one `gio` action namespace.

Files reach Neovim two ways, and the second is what the project search exists for. The picker, the sidebar, and the menu's native Open File and Open Folder dialogs all end in `InputEvent::Open`, which core performs as `:e`. A search result row is a location rather than a file, so it ends in `InputEvent::OpenAt`, which core performs as `:edit` followed by `nvim_win_set_cursor` inside one input-pump step — the two cannot be interleaved with another event.

Saving is `:w` or `:saveas` from inside Neovim, with no save dialog. The binary takes no command-line arguments, and hands GTK an empty argv so the real command line is never parsed.

## Platform and build

Linux only, built with Cargo inside the Nix devShell. Run everything through `nix develop --command`, because after a `flake.nix` edit an already-loaded direnv environment is stale and has no GTK.

This crate deliberately has no Bazel target at all. That absence is what keeps the GTK stack out of `wolf-core`'s Bazel dependency graph, so the one hard rule in [`wolf-core/DECISIONS.md`](../wolf-core/DECISIONS.md) holds under the macOS Bazel build as well as under `cargo tree`. Adding a `BUILD.bazel` here breaks that guarantee. The verification queries live alongside the rule, in [`wolf-core/DECISIONS.md`](../wolf-core/DECISIONS.md).

The devShell supplies the toolchain that the gtk-rs `-sys` crates probe for (gtk4, cairo, pango, glib, gdk-pixbuf, graphene, harfbuzz), exports `WOLF_NVIM_BIN` so the pinned Neovim is never taken from `PATH`, and pins the font stack. `FONTCONFIG_FILE` points at a generated copy of [`testdata/fonts.conf`](testdata/fonts.conf) exposing three pinned font trees — DejaVu plus the two editor families the shared config can name — and no host font directory at all, which is what makes Pango measurement and Cairo rasterization reproducible enough to assert on pixels.

## The GUI denylist does not apply here

`wolf-core`'s "no GUI toolkit in the dependency tree" rule constrains core, not this crate. GTK is allowed here because the dependency runs `wolf-gnome` to `wolf-core` and never the reverse, so nothing this crate adds can enter core's tree.

## gtk-rs versions

The gtk-rs crates form one coherent release set: `gtk4` 0.11.x, `gdk4` 0.11.x, `libadwaita` 0.9.x, with `cairo-rs`, `pango`, and `pangocairo` at 0.22.x. In the gtk-rs ecosystem the gtk4-rs repo and the core repo release together but carry different minor numbers, so the invariant is one coherent release set rather than one identical minor. Mismatched `-sys` minors fail to link.

`libadwaita` carries no version feature, because the target is the system GTK already present rather than a newer pinned ABI. The set builds against the flake's GTK 4.22, cairo 1.18, and pango 1.57.

`gtk4` enables `v4_12`, which the sidebar's `ListView::scroll_to` requires. That raises the minimum system GTK to 4.12; the flake satisfies it, and the feature is scoped to this crate. `cairo-rs` enables `png` for the ignored golden-regeneration dev aid.

## The pure seams

The crate is both a library and a binary, and the split is the most important structural fact about it. The binary is the window; the library holds every decision expressible as a function of values, so it can be unit-tested with no display server, no widget, and no Neovim.

Those seams cover key and pointer translation, the Cairo paint, the pixel-to-grid geometry, the pane-layout tree and its persistence codec, the cursor rules, the action namespace, the command classification, the launch, re-root and application-state rulebooks, the window title, the project-search rulebook, the per-pane navigation rulebook, the sidebar's refresh gate and its trailing-refresh coalescer, and the crash and stall rulebook. Each is a free function or a plain value type; [`src/README.md`](src/README.md) names them file by file.

The consequence is this crate's testing posture. Widget construction cannot be unit-tested at all, and is covered only by the smokes below. When you add behavior, put the decision in a seam and test it there: a rule left inside a widget closure is a rule the fast suite cannot execute, and on this platform that has repeatedly meant a rule nothing verified.

Several seams are deliberate ports of a macOS counterpart in [`WolfKeyMap`](../wolf-macos/Sources/WolfKeyMap/README.md), so the two front-ends can be compared as values rather than as prose. Where one diverges, the divergence is stated in that file's entry and pinned by a test.

## Threading and the wake

`WolfCore` runs Neovim on its own tokio runtime and fires a single wake closure when a frame is ready. The wake runs on a tokio worker and so cannot touch GTK: it only `try_send`s onto a `bounded(1)` `async-channel`, and a `glib::spawn_future_local` task drains that channel on the main thread.

The single slot is deliberate. It coalesces a burst of flushes into one queued redraw, and a dropped send on a full channel is correct because a redraw is already pending. Sends ignore errors, so a late wake arriving after the channel closes is harmless.

Coalescing is safe even for a signal that fires exactly once, such as a death. `async-channel` reports the channel full only while an item is queued, and the receiver frees the slot before its future resolves, so a dropped send always implies a later full handler run — and liveness is a level-triggered atomic read rather than a message, so that later run observes the death.

The drain order is fixed and every position is load-bearing. The liveness read runs first, before any handler that can early-return, and reports whether painting this wake is worth anything; the pane view and the sidebar run only if it said yes. The shell runs last and ungated: it settles a pending re-root, refreshes the title, takes an application-state snapshot, and only then drains commands, because a drained close tears the core down and a command queued in the instant before a crash must not be stranded behind an opaque overlay.

`WolfCore` is not `Send`. It lives only on the main thread in an `Rc<RefCell<Option<_>>>` shared by the widget closures, and the wake closure is the only thing that crosses a thread boundary. Every core borrow is taken and dropped in its own statement, because the close-request handler does `borrow_mut().take()` and an outstanding borrow on that path is a panic rather than a warning.

Nothing polls. No timer is periodic and no timer drives a read: neither a redraw, a liveness read, nor a state snapshot is scheduled by the clock. A periodic liveness poll in particular is rejected — it would let a lossy wake transport still raise the crash overlay eventually, buying insurance by destroying the evidence the crash smoke exists to gather.

Two one-shot timers exist and neither is a poll. Each is armed by an arriving event, fires once, and disarms: the deferred restart, which exists because the overlay must be presented before a synchronous respawn blocks the main loop, and the sidebar's trailing refresh, which collapses a burst of traversal batches into one rebuild. Neither re-arms itself, so no timer exists while the application is idle.

The picker and the sidebar each run a long-lived `FileSearch` on the same pattern, and the project search runs a transient `SearchSession` that is replaced on every Enter. Their drain tasks hold only a `Weak` reference, so dropping a handle ends the task instead of leaking a cycle, and closing any of them drops its search to cancel the work.

Dropping a `SearchSession` kills its `rg` child, which is the whole of what makes closing the overlay mid-search safe. It is also why the live session is taken and dropped in a statement of its own before the next one starts, rather than being overwritten in place.

## Window structure

The window is an `adw::ApplicationWindow` whose content is a vertical box: an `adw::HeaderBar` over a horizontal `gtk::Paned`. The Paned's start child is the file-tree sidebar; its end child is a `gtk::Overlay` whose main child is the pane view. The pane view is a tree of nested `gtk::Paned` with one `gtk::DrawingArea` per Neovim window, built from the pure pane tree, plus a one-row command-line strip along the bottom.

Each pane leaf is a bare `gtk::DrawingArea`, and the pane's statusbar strip, its editor padding, its text grid and its scrollbar thumb are all painted into that one area with Cairo rather than being widgets of their own. The padding has to be filled in the editor's background, which is a per-frame value out of the snapshot, and this front-end does no theming in either direction — so a widget leaf would draw a theme-coloured frame around a Neovim-coloured grid. A widget strip would also remove only the strip's subtraction and not the padding's, so every pixel-to-grid conversion routes through the inset either way, while the leaf gains a second widget for each rebuild to reuse by id.

**The scrollbar is painted rather than a `gtk::Scrollbar`, and three of its four reasons hold independently of the strip's.** A `gtk::Scrollbar` is a control: it carries a `gtk::Adjustment` and accepts input, so shipping one invites a thumb drag with no feedback path into Neovim, and a control that visibly refuses input is worse than an indicator that never claimed to take any. The gutter it sits in is filled with the editor's own background, which a theme-owned widget background cannot be.

A widget child would also need a `gtk::Overlay` or `gtk::Fixed` around every leaf, rebuilt and re-parented on each layout change, inside the region the focus regime below warns about. And macOS's fade rides a repeating 30 Hz timer, which "Threading and the wake" forbids — so the bar is always visible, which is why only the thumb is drawn and never a track. A permanently visible full-height trough is a GNOME-unidiomatic ornament and a tall high-contrast mark on every screenshot whose buffer overflows; GNOME's own idiom is an overlay indicator with no trough at rest.

A pane's body is painted identically whether the pane is active or not: no scrim, no tint, nothing conditional in the grid paint. The only signal is the strip's text — bold at full strength in the active pane, both lines scaled by `STATUSBAR_INACTIVE_ALPHA` in the others. The focus ring survives alongside it for two reasons, and the first is the one not to undo: `ACTIVE_PANE_CSS` is an `outline`, so it consumes no layout space and never enters the inset arithmetic, a property keeping the leaf a bare drawing area preserves. And OCR reads neither a font weight nor a dim, so the ring is the only active-pane signal a test can see.

The header bar is the unified toolbar: the sidebar toggle, a linked Back/Forward pair and Open at the start, the primary menu at the end, and the window title between them. **No title widget is set**, so Adwaita draws the title itself and the bar keeps its natural height. That is load-bearing rather than incidental — the bar's height is one term of the pane origin `PANE_ORIGIN_Y_EXPECTED` pins, the statusbar strip and the editor padding above the grid being the others, and every pointer smoke derives its targets from that origin, so a title widget silently retargets all seven.

The overlay's children are added empty state, then picker, then command palette, then buffer picker, then project search, then crash overlay, then restart banner, and that order is the stacking. At most one of the four transient overlays is open — the shell closes the other three when it opens one — so their relative order is not observable, while each must sit above the empty state or it would open invisibly in a project-less window.

A crashed core must cover the picker's Open, a palette row, a buffer-picker row, a search result row and the empty state's Open Folder, since each would send into a dead Neovim. The banner is topmost and click-through, so it confirms a restart without swallowing a click meant for the editor.

**GTK keyboard focus never lands on a pane area.** Pane drawing areas are not focusable and nothing grabs focus on them; a click parks focus on the persistent command-line strip instead, which is also the focus-return target the picker, the command palette, the sidebar, and the crash overlay use. Every widget under the pane area is destroyed and rebuilt on each layout change, and a destroyed focus widget leaves the toplevel with no focus at all — after which window-level key dispatch dies outright while Neovim stays alive and silent.

`gtk::Paned` sets its dividers focusable itself and this crate cannot change that, so a rebuild additionally samples the toplevel's focus widget before unparenting anything and returns focus to the strip. That sample must stay above the teardown: the moment a widget is unparented rather than hidden, a later sample stops reporting it.

Input has two regimes. Keys are global: a capture-phase controller on the window matches this crate's own chords and stops propagation, and a bubble-phase controller routes everything else through the key seam into Neovim's current window, so a bound chord is never also forwarded. Pointer input is per pane, on each drawing area. Resize flows per pane through the geometry seam into `resize_grid` and then resizes the outer grid; the authoritative sizes come back from core.

## Pane weights and dividers

**A weight is a proportion of width**, and it is that everywhere it appears: in the pure tree, in `pane_layout::encode`, in `state.json`, and on screen. `geometry::divider_position` therefore scales it by the paned's own span — its width for a horizontal paned, its height for a vertical one — so equal weights give equal widths, and `min_position` / `max_position` are the clamp and never the scale. Scaling by the travel instead spends the end child's minimum size at every fraction, so even a fresh, undragged split comes out lopsided by exactly that minimum.

`divider_position` and `divider_span_fraction` must share that denominator, and the inverse property does not pick which one it is: a round trip through any shared quantity returns the position it started from. Agreement is necessary and nowhere near sufficient — a pair that agrees on the wrong quantity round-trips perfectly while every split on screen is visibly uneven — so `equal_weights_give_equal_widths` is the other half of the contract and the two are read together.

An n-ary split renders as a right-leaning chain of binary `gtk::Paned`, so a divider governs its head child against the whole remainder rather than against its immediate sibling. `PaneTree::set_divider_fraction` therefore rescales that remainder in proportion and leaves every weight before the divider alone. macOS's `PaneTree.setPairFraction` reweights the adjacent pair instead, which suits its flat divider list and would be wrong here: on a chain it moves a second divider the user never touched.

`gtk::Paned` owns the handle, the hit test, the cursor and the drag gesture, so this crate needs no analogue of the macOS `DividerHitTest` or `DividerDragSeam` seams; the pure tree owes it a mutator and that mutator's inverse, and nothing else. The pixel-level minimum pane size is GTK's too, from non-shrinking paned children plus a content floor that leaves one whole cell of grid once the statusbar strip and both padding gutters are taken out.

**A zero weight is dangerous because nothing rejects it.** `PaneTree::from_layout` checks that every weight is finite and non-negative and that the split's total is positive, and a lone zero among positives satisfies all three. It is durable rather than loud: `pane_rects` hands that child a zero-extent rectangle, `neighbor` never returns it because the cross-axis overlap test is strictly positive, and `layout()` copies the zero straight back out, so it survives the `state.json` round trip and comes back on every later launch.

**There are two floors under a divider and neither subsumes the other.** `MIN_DIVIDER_FRACTION` is the arithmetic one: it keeps a dragged weight strictly positive in the tree, where a zero would survive every round trip. `divider_position`'s clamp into `min_position` / `max_position` is the pixel one: it keeps a position inside what GTK will accept for the two children's minimum sizes, which no fraction can know about. A weight can be well clear of the first and still be clamped by the second, and where the clamp binds — a span too small for both floors — equal widths are unreachable and the clamp is the right answer, since averaging through a floor reflows both panes to one column.

The renormalization at the end of `set_divider_fraction` is repair rather than tidiness: `from_layout` never checks that a saved vector sums to 1, and `state.json` carries `weights` as plain JSON through `pane_layout::decode` into it, so a denormalized split is a reachable input. A non-finite fraction is refused outright rather than clamped, because a NaN weight is the loud failure of the pair: `from_layout` rejects the whole tree at the next launch and the saved layout is gone.

Weights sitting in a user's `state.json` were written as fractions of a paned's travel and are read as fractions of its width. The difference is bounded by the end child's minimum over the span, a few percent, and one drag on that divider rewrites it — so no migration is offered, but a restored layout can sit slightly off where it was left, and that is the reason rather than a regression.

## The accelerator namespace

Every action name, its accelerator, its `Command` correspondence, and its menu placement live in one table, so the menu, the accelerators, and the command dispatch cannot drift apart. Chords spoken for by a later unit are declared and marked reserved rather than omitted, which is what lets the invariant below be checked before anything binds them.

**The invariant is that no Neovim key becomes unreachable.** A `Ctrl+Shift+<letter>` accelerator gives up only the terminal-style Ctrl+Shift alias for `<C-letter>`. This crate's key encoder forces `shift: false` on printables, so `Ctrl+Shift+P` encodes to `<C-p>` and `<C-S-p>` cannot be emitted at all; `<C-p>` itself is still delivered by plain `Ctrl+P`, which is never bound. `parse_accel` and `forbidden_reason` enforce that mechanically over every row, reserved ones included, so a shadowing chord fails while it is reserved rather than months later.

`Ctrl+Shift+<letter>` is the namespace because nothing else is available. There is effectively no free `Ctrl+<letter>` across all Neovim modes, and a terminal buffer forwards every key but one. A forwarded, unmapped Alt chord splits into `<Esc>` plus the base key, and a forwarded, unmapped Super chord drops the modifier in normal mode and inserts literal text in insert mode, so neither is a namespace Neovim can be given. The genuinely free keys are too few and too unidiomatic to carry a menu. The rule is memorable: take the macOS chord and replace the command key with Ctrl+Shift.

Two consequences worth knowing. The file picker is `Ctrl+Shift+O`, the command palette `Ctrl+Shift+P`, the buffer picker `Ctrl+Shift+E`, the project search `Ctrl+Shift+F` and the sidebar toggle `Ctrl+Shift+B`. And a disabled action's accelerator falls through to Neovim, which is what a real GTK accelerator does and what the chord controller deliberately reproduces, so in the empty state `Ctrl+Shift+O` reaches Neovim as `<C-o>`, `Ctrl+Shift+F` as `<C-f>` and `Ctrl+Shift+B` as `<C-b>`.

**`Alt+Left` and `Alt+Right` are the namespace's one deliberate exception**, and the asymmetry it creates must not be tidied away. They carry Back and Forward, and capturing them is exactly what prevents the `<Esc>` plus `<Left>` split a forwarded Alt chord produces. So their actions stay enabled even where a press would refuse, unlike every `Ctrl+Shift+<letter>` row, which is disabled instead and gives up only an alias by falling through. The two toolbar buttons desensitize in their place, which costs nothing: a press with nothing to go back to issues no RPC anyway.

## Typography

The shared `wolf-config.yaml` is read once, at window build time, next to the state store and before any widget exists — the same point macOS reads it at, through the same resolution `wolf_ffi_load_config` uses. A config edit therefore takes effect on the next launch. An unresolvable preferences directory degrades with one message to the built-in defaults, matching the state store's convention; `wolf_core::load_or_create` creates the file when it is absent, which is why every smoke pins `WOLF_CONFIG_DIR`.

**`WOLF_FONT_PT` wins over the configured `font-size`**, and that is parity rather than a Linux quirk: `CellMetrics.defaultPointSize` gives the override the same precedence on macOS, the crate README documents it, and the compositor smokes drive it — the one that must let the config decide the size removes it on purpose. The rule lives in a pure seam that takes the override as an argument, so it is testable without touching the process environment.

The cell height and baseline are a Rust port of `WolfKeyMap/CellMetricsMath.swift`, kept as pure `f64` arithmetic in a file named for its counterpart. `CellMetrics::for_font` needs a live Cairo context to obtain the metrics it feeds in, so a rule left inside it would be a rule only the golden-render target could execute. Two divergences from Swift are deliberate and strictly more defensive: a non-finite `line-height` takes the natural-metrics branch, and a degenerate result is floored at one device unit.

The arithmetic is unit-agnostic, and the two platforms hand it different units: macOS rounds in points, this crate in device pixels, because device pixels are what Cairo paints in. At 1x the two coincide, and this crate has no HiDPI handling in any case.

**A family Pango cannot resolve is substituted by fontconfig, and this crate adds no fallback of its own.** macOS needs one because CoreText returns a different family rather than substituting; Pango substitutes per lookup and always yields usable metrics, so a hand-rolled fallback could only override a better answer. A test pins that the description keeps the family the caller asked for.

## Application state

`state.json` is the shared `wolf_core::state` schema, unchanged. Its shape is common to this crate and the macOS front-end, so a Linux-only change to it is a macOS re-verification obligation. `wolf_core::state_dir()` is the single resolution point, and `WOLF_STATE_DIR` overrides it.

A snapshot is taken on every render wake, with no timer and no debounce, plus one final synchronous snapshot at close. That is affordable because two coalescers already sit beneath it: the store is a no-op on an unchanged tree, and the writer thread coalesces before it writes. The crate's no-polling property survives, and a kill loses at most one coalesce window.

**Every snapshot is gated, and the gate prevents data loss rather than churn.** A snapshot taken while Neovim is dead, before the first pane binds a grid, or before Neovim has reported its per-window buffer map would persist an empty layout and overwrite the user's saved one with nothing.

**Suppression is by content, never by arity.** A tab's open buffer paths have no persistence channel of their own — they ride only inside the layout's leaves — so a save side that skipped a lone leaf would silently stop persisting the single most common shape there is: one window, one tab, one pane, one file. The save path suppresses only an unnamed, non-preview lone leaf, and the restore path rebuilds any saved layout with no arity guard at all. That is the shape of a bug already fixed on macOS; do not reintroduce it here.

**Window position is deliberately never persisted, permanently.** A Wayland client is given neither its own position nor screen geometry, so there is no analogue of the macOS check that a restored frame still intersects a screen — a restored coordinate could not be validated even in principle. Only the size is restored, and the position is written as an explicit zero rather than a plausible-looking value a later reader might trust.

**Nothing about the content-width toggle is persisted, deliberately and permanently.** `b:wolf_full_width` is a buffer variable Neovim owns and re-emits on every `BufEnter` and `WinEnter`, and `wolf_core::state` has no field for it — adding one would change a schema shared with macOS for a Linux-only feature. It would also key by pane path what Neovim keys by buffer, so one file open in two panes would get two persisted answers and one live one. The honest consequence is that the toggle does not survive a quit and relaunch, which is parity: macOS persists nothing either.

`sidebar_visible` carries the user's preference, and is restored at launch. What is written is that preference and never what is on screen: the empty state hides the sidebar without changing what the user asked for, so persisting the visible flag would make one project-less session hide the sidebar of every later one. `state_seam::sidebar_is_shown` combines the two at apply time, and a restored `false` is always reversible — from the header bar's toggle or from `Ctrl+Shift+B`.

## Crash and stall recovery

Core reports the captive Neovim's health as `Alive`, `NotResponding`, or `Dead`, and this front-end reads it at the top of every wake, before any painting. `Alive` paints and resets the attempt budget. `Dead` shows an opaque crash overlay, skips the paint, and schedules one automatic in-place restart. `NotResponding` shows a stall overlay and does nothing else.

**A wedged Neovim is never restarted automatically.** A restart force-kills the process, so everything not already in a swap file is lost, and that decision belongs to the user's button press alone.

The overlay is opaque because its job is to hide the frozen last frame, so a user can tell "Neovim died" from "Neovim is idle". Its restart button is present in both modes and drives the same function the automatic path does.

**The automatic restart is deferred by a short timer rather than run inline, and the mechanism was measured rather than reasoned about.** The restart call is synchronous, so running it inside the wake handler blocks the main loop for the whole respawn with the overlay shown but never presented. A GLib idle does not fix that under GDK and Wayland — the frame clock has no ready paint source at the instant the handler returns, so the idle wins — while a real timer does.

**The attempt cap exists to stop a retry loop, never to refuse a user.** A failing respawn re-marks the core dead and fires another wake, which without a cap re-enters indefinitely. An explicit button press resets the counter, at the documented cost that a failed manual restart buys one extra automatic respawn.

Only the open-file set comes back, reopened with the active file last because reopening is a sequence of ordered `InputEvent::Open`s. The pane topology is not restored across a restart, and neither is cursor or scroll position; do not confuse that with a quit and relaunch, which does restore the topology from `state.json`. The pane view is reset first, which is load-bearing rather than cosmetic: a fresh Neovim re-attaches at the configured initial size, and only a widget allocation change tells core otherwise.

**Never issue a blocking RPC on the restart path.** Against a dead Neovim it freezes the main loop for the whole timeout budget on every crash. The open-file mirror is read out of the snapshot instead.

Core marks a stall only from a synchronous call's timeout arm, and three such calls are reachable from this crate: the layout rebuild at launch, before the window is interactive; the buffer picker's candidate snapshot, which a running window issues on Ctrl+Shift+E; and a per-pane navigation move, which a running window issues on Alt+Left or Alt+Right. Copy and a preview render would each add another such path. The sidebar's re-root adds none: `WolfCore::live_cwd` is a `parking_lot` read of a tracked cell, taken once per wake and shared with the title refresh.

Both calls a running window issues are gated in the same shape. `buffer_picker::open_decision` refuses to open unless the core reads `Alive`; `nav_ui::nav_press` refuses a move on the same `Alive`-only arm. Neither can prevent a first stall, and neither may be strengthened until it can — what they prevent is re-probing a Neovim already known to be wedged.

The palette's Open Recent rows add no fourth one: reading the candidates is a SQLite read plus a bounded `stat` walk over the store's capped contents, taken at open and never per keystroke, so the inventory is unchanged.

The project search adds none either. `start_search` resolves its root from the tracked-cwd cell and hands `rg` to a worker, `open_at` is fire-and-forget on the ordered input channel, and `liveness` is an atomic read. `WolfCore::ignorecase()` is a blocking call and is deliberately never issued: its entire product would be the seed for a case-sensitivity toggle this front-end does not ship, bought with a further route into the state the paragraph below describes.

**Nothing in this crate clears `NotResponding`.** Core's auto-clear rides the `Ok` arm of a later synchronous call, and a timed-out one is detached, so its eventual success is never observed — while both blocking calls a running window issues refuse to run in that state. The route back is the stall overlay's Restart button, which force-kills. The honest fix, if that ever becomes a user problem, is a non-blocking liveness probe in core; there is none.

## The headless smoke test

The GTK glue is covered by end-to-end smokes that launch the real binary under `cage`, the headless wlroots compositor, drive it with `wtype` and [`wolf-pointer-inject`](../../tools/wolf-pointer-inject/README.md), screenshot the live window with `grim`, and read it back with `tesseract`. They prove the stack actually draws a window a human could see. They are not opt-in: they run in the ordinary `cargo test` suite, which is most of why it is slow. See [`tests/README.md`](tests/README.md) for the full list.

They need `cage`, `wtype`, `grim`, `tesseract`, and `wolf-pointer-inject` on `PATH`, a set and writable `XDG_RUNTIME_DIR` where the Wayland socket lives, and both `WOLF_NVIM_BIN` and `WOLF_RG_BIN`. A missing precondition fails loud rather than skipping quietly, because absence means the smoke toolchain was not provisioned.

Three details are load-bearing. Every smoke holds a shared mutex for its whole body, because two `cage` compositors started concurrently contend for the same Wayland socket and both fail. Each scenario runs inside `cage` as a single shell command, since `cage` exports its `WAYLAND_DISPLAY` only to its own child process tree. And every smoke's environment comes from one helper, which is the only thing keeping a smoke run out of the developer's real state and configuration directories.

## Present limitations

- No save or save-as dialog; saving is done from within Neovim. Open File and Open Folder are native dialogs.
- No theming in either direction: the front-end neither queries the OS theme nor pushes one into Neovim. Neovim's own colorscheme reaches the grid paint, and the `WolfCursor` theme group reaches the cursor. A configured `appearance` other than `auto` is read, reported once on stderr at launch, and then ignored — dropping it silently would leave someone who wrote `appearance: dark` with no way to learn it does nothing here. `macos-option-as-alt` is ignored with no message at all, because the key's own name says which platform it is for and one config file is shared across both.
- Pointer input covers the primary button and vertical scroll. There is no context menu, no middle-click paste, no horizontal scroll, and no kinetic scrolling, all matching macOS. The scrollbar is a projection with no thumb drag: it shows where the viewport sits and accepts no input at all, which macOS defers too.
- The command-line strip renders a single row: an active command line, or the last message line when idle. There is no completion popup and no multi-line scrollable message view. Core folds all three UI extensions regardless, so adding the Adwaita widgets later is additive.
- A per-pane status bar's text is clipped to its own pane rather than tail-truncated with an ellipsis, so a path too long for its pane ends mid-glyph. macOS ellipsizes; reproducing that here means a second Pango layout builder in the widget layer, which is the thing `render.rs`'s one-glyph-path rule exists to prevent.
- Only windows created through the overridden split commands become panes. A window from `:help`, quickfix, a preview, or `:diffsplit` gets a grid but no pane, and floating or external windows are ignored.
- A snapshot-version advance rebuilds the sidebar's root store, so a directory the user expanded by hand collapses on every streaming traversal batch and on a re-root. Manual expansion does survive a current-file change, which is what the version gate buys; carrying it across a version advance needs the model layer redesigned around per-directory stores.
- The navigation surfaces are Back and Forward alone: two header-bar buttons, two menu items, and `Alt+Left` / `Alt+Right`. There is no jump list of a pane's recently visited files, which the macOS History menu carries. A dynamic list needs a `gio::Menu` rebuilt on each popover open and an action parameterized by stack index, where `MENU_SECTIONS` is const data with five invariants checked over it.
- The content-width toggle carries no menu item and no accelerator. `MENU_SECTIONS` is const data with no stateful check item to show a per-buffer flag in, and a chord would spend one of the scarce free `Ctrl+Shift` letters on a preference. Its surfaces are `:WolfFullWidth` and the command palette's own row, which appears automatically because the palette filters on the same classification the menu resolves through.
- No HiDPI scale handling: the cell-measurement probe assumes 96 dpi.
- No input-method composition support.
- The command palette lists the registry commands this front-end performs plus one Open Recent row per usable recent directory. There are no Switch-to-Window rows, since there is one window. An Open Recent row re-roots that window, through the same pending-re-root path the Open Folder dialog takes, and the directory the window is already in is not offered, since raising the window you are looking at does nothing.
- An Open Recent row exists only for a directory the user changed into: core does not record the launch working directory (see [`runtime/nvim/README.md`](../../runtime/nvim/README.md)), so a project only ever launched into is absent from the palette until the user changes directory.
- There is no Markdown preview and no terminal drawer. Core carries the drawer intent; this crate matches it explicitly and does nothing, so adding a fourth intent variant still fails the build here.
- The empty state disables two actions, the file picker's and the project search's, and routes still go around them. A new-buffer or close-pane chord fires behind the opaque status page, where the result is invisible until a folder is opened. The palette offers a live row for every dispatched command, so its "Go to File…" row reaches the file search on a project-less window while the menu item is insensitive and the accelerator falls through; the search row is refused instead, by `search_overlay::open_decision`'s no-project arm.
- The buffer picker stays enabled in the empty state, and that is a decision rather than an oversight: unlike a file search it has no project root to be rooted wrongly at, and the captive Neovim runs there too, with its `Untitled.md` buffer listed.
