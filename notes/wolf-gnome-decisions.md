# wolf-gnome decisions

Last verified at commit `24af6404466d9cc099af52ccec8d7c89155bc6f5`.

The durable constraints behind this crate. Read this before changing anything here; most of it exists because a reasonable-looking simplification breaks something. Per-file detail lives in [`src/README.md`](../crates/wolf-gnome/src/README.md). The large sections live in topic files under `notes/wolf-gnome/`, each verified on its own, and their entries here are short ones linking them.

## Scope

This is the GTK 4 / libadwaita front-end: a runnable multi-window application that consumes [`wolf-core`](../crates/wolf-core/README.md) as an ordinary Rust crate dependency. It renders Neovim natively rather than embedding a terminal, so every pixel outside a preview comes from a Cairo/Pango paint of a core `Snapshot`, or of a session's `TerminalFrame` in a terminal pane. One process holds N windows and one captive Neovim per window; [The coordinator and its windows](wolf-gnome/the-coordinator-and-its-windows.md) owns that split.

Split, close, and focus move are driven by core's layout-intent channel; a divider drag is the `gtk::Paned` handle's own gesture, landing in the pure tree's weights. Menu items, accelerators, and commands drained from Neovim all resolve through one `gio` action namespace.

Files reach Neovim two ways, and [`README.md`](../crates/wolf-gnome/README.md) states both. What is durable here is that `InputEvent::OpenAt`'s edit and cursor move happen inside one input-pump step, so no other event can interleave between them.

## Platform and build

Linux only, built with Cargo inside the Nix devShell. Run everything through `nix develop --command`, because after a `flake.nix` edit an already-loaded direnv environment is stale and has no GTK.

This crate deliberately has no Bazel target at all. That absence is what keeps the GTK stack out of `wolf-core`'s Bazel dependency graph, so the one hard rule in [`wolf-core-decisions.md`](wolf-core-decisions.md#the-one-hard-rule) holds under the macOS Bazel build as well as under `cargo tree`. Adding a `BUILD.bazel` here breaks that guarantee.

The devShell supplies the toolchain that the gtk-rs and `webkit6` `-sys` crates probe for (gtk4, cairo, pango, glib, gdk-pixbuf, graphene, harfbuzz, webkitgtk and libsoup), exports `WOLF_NVIM_BIN` so the pinned Neovim is never taken from `PATH`, and pins the font stack through a generated `FONTCONFIG_FILE`; [`tests/README.md`](../crates/wolf-gnome/tests/README.md) owns what that file exposes and why it omits every host font directory.

The binary locates the captive `init.lua` in three branches, in order: the `WOLF_INIT_LUA` override, then `<exe dir>/../share/wolf/nvim/init.lua` when that file is there, then an `env!("CARGO_MANIFEST_DIR")` path into the checkout it was compiled in. The middle branch is what makes an installed binary work anywhere; the last is what keeps `cargo run` and every compositor smoke working, so it is passed unconditionally. The rule is a pure seam and only the two reads that feed it — the environment and `current_exe()` — belong to `app.rs`.

The override is deliberately not existence-checked while the executable-relative guess is, and the asymmetry is the point. A typed override is an instruction rather than a candidate: checking it would turn a typo into a silent fall-through to a different `init.lua`, where the failure that replaces the check is loud — Neovim launched with a missing `-u` never attaches, and the app prints `failed to start core` and opens no window.

This crate also has a package output, `packages.wolf-gnome` in `flake.nix`, whose install layout and the executable-relative constant are one contract rather than two. Its evidence comes from `#[ignore]`d tests that one committed script always runs with the store path exported, not from the default suite, because a Nix builder can run none of them. [`build.md`](../build.md) owns the commands and [`tests/README.md`](../crates/wolf-gnome/tests/README.md) the claims.

## The GUI denylist does not apply here

`wolf-core`'s "no GUI toolkit in the dependency tree" rule constrains core, not this crate. GTK is allowed here because the dependency runs `wolf-gnome` to `wolf-core` and never the reverse, so nothing this crate adds can enter core's tree.

## gtk-rs versions

The gtk-rs crates form one coherent release set: `gtk4` 0.11.x, `gdk4` 0.11.x, `libadwaita` 0.9.x, with `cairo-rs`, `pango`, and `pangocairo` at 0.22.x. `webkit6` 0.6.x belongs to that set too, requiring `gtk4` and `gdk4` at ^0.11 and `glib` and `gio` at ^0.22 — exactly what this workspace already locks. In the gtk-rs ecosystem the gtk4-rs repo and the core repo release together but carry different minor numbers, so the invariant is one coherent release set rather than one identical minor. Mismatched `-sys` minors fail to link.

`libadwaita` carries no version feature, because the target is the system GTK already present rather than a newer pinned ABI. The set builds against the flake's GTK 4.22, cairo 1.18, and pango 1.57.

`gtk4` enables `v4_12`, which the sidebar's `ListView::scroll_to` requires and which `webkit6` silently depends on as well: it names `gtk::Accessible`, a type `gtk4` gates behind `v4_10` from 0.11.4 onward, so trimming the feature below that breaks the build with an error naming accessibility and pointing at neither the feature nor `webkit6`. That raises the minimum system GTK to 4.12; the flake satisfies it, and the feature is scoped to this crate. `cairo-rs` enables `png` for two consumers, and the second gates the suite: the ignored golden-regeneration dev aid writes a PNG with it, and the preview-appearance smoke decodes its `grim` captures with it.

## The pure seams

The crate is both a library and a binary, and the split is the most important structural fact about it. The binary is the window; the library holds every decision expressible as a function of values, so it can be unit-tested with no display server, no widget, and no Neovim.

Every seam is a free function or a plain value type, and [`src/README.md`](../crates/wolf-gnome/src/README.md) names them file by file rather than repeating the inventory here.

The consequence is this crate's testing posture. Widget construction cannot be unit-tested at all, and is covered only by the smokes below. When you add behavior, put the decision in a seam and test it there: a rule left inside a widget closure is a rule the fast suite cannot execute, and on this platform that has repeatedly meant a rule nothing verified.

Several seams are deliberate ports of a macOS counterpart in [`WolfKeyMap`](../crates/wolf-macos/Sources/WolfKeyMap/README.md), so the two front-ends can be compared as values rather than as prose. Where one diverges, the divergence is stated in that file's entry and pinned by a test. Only such a divergence names the macOS front-end in this crate's sources, and its comment links the counterpart's README entry; [the wording check](../tools/lint/README.md#what-wolf_gnome_wording-checks) enforces that against a closed allowlist.

## Threading and the wake

`WolfCore` runs Neovim on its own tokio runtime and fires a single wake closure when a frame is ready. The wake runs on a tokio worker and so cannot touch GTK: it only `try_send`s onto a `bounded(1)` `async-channel`, and a `glib::spawn_future_local` task drains that channel on the main thread.

The single slot is deliberate. It coalesces a burst of flushes into one queued redraw, and a dropped send on a full channel is correct because a redraw is already pending. Sends ignore errors, so a late wake arriving after the channel closes is harmless.

A terminal session's publish fires the same wake. It bumps no `Snapshot.seq`, so it redraws every pane of the current tab like any other wake, which is why a terminal pane paints through a backing surface ([Terminal panes](wolf-gnome/terminal-panes.md)).

Coalescing is safe even for a signal that fires exactly once, such as a death. `async-channel` reports the channel full only while an item is queued, and the receiver frees the slot before its future resolves, so a dropped send always implies a later full handler run — and liveness is a level-triggered atomic read rather than a message, so that later run observes the death.

The drain order is fixed and every position is load-bearing. The liveness read runs first, before any handler that can early-return, and reports whether painting this wake is worth anything; the pane view and the sidebar run only if it said yes. The shell runs last and ungated: it settles a pending re-root, refreshes the title, contributes its window's application-state subtree, and only then drains commands, because a drained close tears the core down and a command queued in the instant before a crash must not be stranded behind an opaque overlay.

`WolfCore` is not `Send`. It lives only on the main thread in an `Rc<RefCell<Option<_>>>` shared by the widget closures, and the wake closure is the only thing that crosses a thread boundary. Every core borrow is taken and dropped in its own statement, because the close-request handler does `borrow_mut().take()` and an outstanding borrow on that path is a panic rather than a warning.

Nothing polls. No timer is periodic and no timer drives a read: neither a redraw, a liveness read, nor a state snapshot is scheduled by the clock. A periodic liveness poll in particular is rejected — it would let a lossy wake transport still raise the crash overlay eventually, buying insurance by destroying the evidence the crash smoke exists to gather. The frame-time instrument holds to the same property with its flag set, which is what shapes its cadence ([Frame-time instrumentation](wolf-gnome/frame-time-instrumentation.md)), and so does the [readiness probe](#the-readiness-file), which is checked on events the window already receives.

Three one-shot timers exist and none is a poll. Each is armed by an arriving event, fires once, and disarms: the deferred restart, which exists because the overlay must be presented before a synchronous respawn blocks the main loop; the sidebar's trailing refresh, which collapses a burst of traversal batches into one rebuild; and the preview's trailing render, armed by an arriving edit ([The preview render's budget](#the-preview-renders-budget)). None re-arms itself, so no timer exists while the application is idle.

The preview's is the one to read carefully, because a burst can leave several of them alive at once — bounded by `ceil(PREVIEW_TRAILING_DELAY / inter-key interval)`, and zero while the user is idle. Each fires once and performs nothing unless it carries the live generation, since an armed GLib source cannot be withdrawn without a warning. A reader who finds five queued has found the design rather than a leak.

One asynchronous callback exists and it is not a timer. `gdk::Clipboard::read_text_async` resolves on the main thread some turns after the paste chord, and its closure holds a `Weak` to the shell and reads the core through the same `Option`, so a read landing after the close handler has taken the core out performs nothing rather than panicking.

A `StyleManager` `dark` notification is neither a timer nor a poll either. It is a GObject property notification, armed once at launch and in `auto` mode alone, fired only when libadwaita's own property changes, so nothing re-arms while the application is idle and the property above is intact.

The picker and the file tree each run a long-lived `FileSearch` on the same pattern, and the sidebar's search panel runs a transient `SearchSession` that is replaced on every run. Their drain tasks hold only a `Weak` reference, so dropping a handle ends the task instead of leaking a cycle, and closing any of them drops its search to cancel the work.

Dropping a `SearchSession` kills its `rg` child, which is the whole of what makes a new run, an empty-query Enter or closing the window safe mid-search. It is also why the live session is taken and dropped in a statement of its own before the next one starts, rather than being overwritten in place.

## The preview render's budget

Typing beside a live Markdown preview would issue one blocking `render_preview` per keystroke, which [The measured reading](wolf-gnome/frame-time-instrumentation.md#the-measured-reading) records as the largest single cost on the typing path. A burst collapses instead into three kinds of render and no others: one on the first edit after a quiet gap, one whenever the burst has run `PREVIEW_MAX_STALENESS` unrendered, and one after the burst ends. The rule is a pure seam, [`preview_coalescer.rs`](../crates/wolf-gnome/src/README.md#preview_coalescerrs), which owns why the leading edge needs both of its conjuncts and why nothing cancels an armed fire.

The leading edge is the half a reader feels: opening a preview, or touching a document after a pause, puts the change on screen at once rather than after a debounce nobody asked for. A pause qualifies only when it exceeds roughly `PREVIEW_TRAILING_DELAY` plus `PREVIEW_LEADING_IDLE`, measured from the last keystroke, because the idle threshold is measured from the last render and a burst's trailing fire lands one trailing delay after its last key.

The trailing edge is a timer rather than the next wake, and that is the sharp half of the design: after the last keystroke there may be no further wake at all, so a wake-driven trailing edge is precisely the droppable one. The cap covers what a trailing debounce structurally cannot, since a user who never pauses would otherwise see no render until they stopped.

Sustained typing therefore renders on a period of `PREVIEW_MAX_STALENESS` plus the interval to the next request, never the cap alone: the cap render fires once the burst has been pending that long, recording it clears the pending burst, and the clock restarts only at the next keystroke — about 535 ms at a 35 ms inter-key interval. Measured directly, an uninterrupted 300-key burst over 10.343 s paid twenty renders — one leading edge, eighteen cap renders and one trailing fire — and a 60-key burst over 2.704 s paid six.

`preview_ui::may_render` is read twice per render, alone among the nine gates [Crash and stall recovery](wolf-gnome/crash-and-stall-recovery.md) lists. The first read admits the request and touches no coalescer state; the second runs inside the pass itself, because a deferred blocking call can arrive at a Neovim that wedged after the arm. A refusal at fire time records no render, so the burst stays pending and the next due wake re-arms it at a zero delay.

The trailing edge is undroppable, and the cases that could drop it are answered rather than argued:

- The fire re-derives the pane set, the liveness reading and the document from live state, so a pane closed mid-burst is simply not due and there is no stale content to withdraw.
- A tab switched away mid-burst stops re-arming, and its already-armed fire still runs and bypasses the tab gate deliberately: the gate protects `pane_status`, which joins the current-tabpage-only `Snapshot::windows`, and the render path reads none of it. The cost is bounded to one deferred blocking render per held tab. The pane view gains no reference to the tab layer to consult that gate, which would couple the two backwards.
- A closing window is covered twice over: the armed closure holds a `Weak`, and the close handler's `core.borrow_mut().take()` leaves `may_render(None)` false — which is why that gate takes an `Option`.

## The coordinator and its windows

Six things are process-scoped, each because a second copy would be wrong rather than wasteful: the one `StateStore`, the config read, the `CellMetrics`, the `app.` actions, the launch plans and the readiness probe. [The coordinator and its windows](wolf-gnome/the-coordinator-and-its-windows.md) gives each reason, and why restore is positional.

## Window structure

The widget tree, the overlay stacking order, and the focus regime that keeps GTK keyboard focus off every pane area, since a destroyed focus widget kills key dispatch. [Window structure](wolf-gnome/window-structure.md) records each, with why the pane chrome is painted rather than built from widgets.

## Terminal panes

Each terminal pane paints through a backing surface of its own, and its input, sizing and session focus follow the selected tab's active pane. [Terminal panes](wolf-gnome/terminal-panes.md) records the GTK glue's choices behind the spec.

## The command line, the message toast and the completion popup

`ext_messages` empties grid 1 and forces `cmdheight=0`, so all three arrive as folded snapshot fields. The strip at the window's bottom is one grid row and stays one grid row. **A strip that grew with its content would resize the Neovim grid on every message**, because its height is subtracted from the pane area — a buffer reflow triggered by an `:echo`, and a `resize_grid` issued on the message path. The toast and the popup are therefore children of the window's existing `gtk::Overlay`, which consumes no layout space at all, and each is sized to its own content so it never covers more than it draws.

**Both are painted, for the two reasons already recorded here**: each surface's background is the editor's own, a per-frame snapshot value a theme-owned widget background cannot be, and their text is `hl_id`-styled chunks drawn through `render::draw_glyph`, which a widget would need a second Pango layout builder for. `adw::Toast` fails on its own terms as well: it is a timed single-label banner whose timeout self-dismissal contradicts core's dismissal contract.

**"Scrollable" is a height budget plus an offset, moved by a scroll gesture that is consumed rather than forwarded.** `gtk::ScrolledWindow` is refused for the reason `gtk::Scrollbar` is. Key-driven paging is not refused but impossible: `InputEvent::is_toast_dismissing` makes every `Key` dismiss a non-prompt toast, so a paging key would destroy what it paged. The toast takes pointer input and the popup does not, which is what makes the scroll exemption `specs/architecture.md` records structural here — a gesture landing on the toast never reaches a pane's controller, so it never reaches core.

**Nothing on this path blocks.** Every read is `read_snapshot()`, so the blocking calls listed under "Crash and stall recovery" are still the whole inventory.

**Answering a `confirm` dialog needs both fields drawn at once.** Its question is a `confirm`-kind message and its options line is `Cmdline::prompt`; the strip draws the prompt and the toast draws the question, and a front-end showing only one of them leaves the dialog unanswerable. Dismissal belongs to core and must not be reproduced here — an invalid key re-prompts, so a front-end that hid the toast on the keypress would leave the user answering a question it had already erased.

## Pane weights and dividers

A weight is a proportion of width everywhere it appears, and a divider drag rescales the remainder of a right-leaning chain of `gtk::Paned` rather than the adjacent pair. [Pane weights and dividers](wolf-gnome/pane-weights-and-dividers.md) gives the reasons and the two floors under a divider.

## The accelerator namespace

Every action lives in one table, and no Neovim key may become unreachable, so the namespace is `Ctrl+Shift+<letter>` with `Alt+Left` and `Alt+Right` the one exception. [The accelerator namespace](wolf-gnome/the-accelerator-namespace.md) states the invariant and what each chord costs.

## Typography

The shared `wolf-config.yaml` is read once per process, at activate, next to the state store and before any window exists — the same point macOS reads it at, through the same resolution `wolf_ffi_load_config` uses. A config edit therefore takes effect on the next launch. An unresolvable preferences directory degrades with one message to the built-in defaults, matching the state store's convention; `wolf_core::load_or_create` creates the file when it is absent, which is why every smoke pins `WOLF_CONFIG_DIR`.

**`WOLF_FONT_PT` wins over the configured `font-size`**, and that is parity rather than a Linux quirk: `CellMetrics.defaultPointSize` gives the override the same precedence on macOS, the crate README documents it, and the compositor smokes drive it — the one that must let the config decide the size removes it on purpose. The rule lives in a pure seam that takes the override as an argument, so it is testable without touching the process environment.

The cell height and baseline are a Rust port of `WolfKeyMap/CellMetricsMath.swift`, kept as pure `f64` arithmetic in a file named for its counterpart. Why the rule sits outside `CellMetrics::for_font`, its unit-agnosticism, and its two guards are in [`src/README.md`](../crates/wolf-gnome/src/README.md#cell_metrics_mathrs), per the delegation above. This crate has no HiDPI handling, so the two platforms' units coincide anyway. The advance width stays unrounded where macOS takes `ceil(advance)`, because rounding it would move every pointer smoke's pixel targets.

**A family Pango cannot resolve is substituted by fontconfig, and this crate adds no fallback of its own.** macOS needs one because CoreText returns a different family rather than substituting; Pango substitutes per lookup and always yields usable metrics, so a hand-rolled fallback could only override a better answer. A test pins that the description keeps the family the caller asked for.

**The pane statusbar strip takes its family from the desktop and its sizes from the editor.** The grid keeps the configured editor family, while the strip asks `GtkSettings` for `gtk-font-name` and paints wolf's own two sizes in that family — macOS parity rather than a Linux choice, since `PaneContainerView.swift` draws the same strip with `.boldSystemFont(ofSize:)` and `.systemFont(ofSize:)` at `statusbarPrimaryFontSize` and `statusbarSecondaryFontSize`. The desktop font's point size is parsed and discarded deliberately: `statusbar_layout` reserves the band's height from the two sizes alone, so adopting it would move the strip height, and with it the grid rect's origin, the pane content floor and every measured constant the compositor smokes carry.

## The preview's light and dark

A Markdown preview follows the configured appearance through a chain whose links are all in libadwaita and WebKitGTK rather than in this crate, and one version bump ends it. What the document itself carries — a dark palette, a `prefers-color-scheme: light` block, and one compile-time-constant stylesheet serving both appearances — belongs to [`wolf-core/src/markdown/README.md`](../crates/wolf-core/src/markdown/README.md).

`appearance::apply_at_launch` calls `AdwStyleManager::set_color_scheme`; libadwaita sets `GtkSettings:gtk-application-prefer-dark-theme` from it; WebKitGTK reads exactly that property and reports it to the page as `prefers-color-scheme`; the theme's light block takes. `libwebkitgtk-6.0.so` carries exactly two GTK settings notifications — `notify::gtk-application-prefer-dark-theme` and `notify::gtk-theme-name` — and neither `gtk-interface-color-scheme` nor `org.freedesktop.appearance` appears in it at all, so those two properties are its whole idea of the system preference.

**libadwaita propagates that property only while the running GTK is older than 4.23.1.** The flake pins 4.20/4.22, so the chain is live. Raising GTK past that version removes it, and the preview stops following the configured appearance with nothing in this repository changed.

**The write is silent.** libadwaita carries a warning string for exactly this property and suppresses its own warning while it performs the write; zero occurrences reached stderr across six `cage` runs. Nothing logs it, so `preview_document_follows_the_configured_appearance_under_cage` in [`tests/README.md`](../crates/wolf-gnome/tests/README.md) is the only tripwire.

**Do not move the theme's `color-scheme` declaration to `:root`.** Measured and falsified: the declaration sits on `body.wolf-markdown`, `getComputedStyle(documentElement).colorScheme` computes `normal`, and dark takes anyway; injecting it on the root changed nothing in either direction.

## Following a link in the preview

Every link clicked in a preview is cancelled and acted on out of band, and every divergence from macOS narrows what may be launched. [Following a link in the preview](wolf-gnome/following-a-link-in-the-preview.md) records the decision handler, the base URI and its security bound.

## Application state

Every tab is gathered on every render wake behind a gate that prevents data loss, in page order, and neither window position nor the content-width toggle is persisted. [Application state](wolf-gnome/application-state.md) gives the reasons.

## Crash and stall recovery

A dead Neovim is restarted automatically and a wedged one never is, no blocking RPC may run on the restart path, and every blocking call a running window issues is gated on `Alive`. [Crash and stall recovery](wolf-gnome/crash-and-stall-recovery.md) holds the inventory of those calls.

## The headless smoke test

The GTK glue is covered by end-to-end smokes that launch the real binary under `cage`, the headless wlroots compositor, drive it with `wtype` and [`wolf-pointer-inject`](../tools/wolf-pointer-inject/README.md), screenshot the live window with `grim`, and read it back with `tesseract`. They prove the stack actually draws a window a human could see. They are not opt-in: they run in the ordinary `cargo test` suite, which is most of why it is slow. See [`tests/README.md`](../crates/wolf-gnome/tests/README.md) for the full list.

A missing precondition fails loud rather than skipping quietly, because absence means the smoke toolchain was not provisioned; the environment they need is [`build.md`](../build.md)'s.

Three further smokes at the end of the same file drive the installed store artifact through the same helper, scrubbing from the child every devShell export that artifact must supply for itself. They are `#[ignore]`d because their precondition is a built package, and they live in that file rather than one of their own because `cage` may be spawned in one place only, and the guard that enforces it reads only that file.

The following details are load-bearing. Every scenario runs under `dbus-run-session`, so each cage has exactly one session bus, private to it and gone when it exits. That is what stops a launch reaching the developer's own bus and handing off to their running `wolf-gnome`, which would make the smoke test the wrong process.

That bus activates nothing: its configuration is generated per cage with no service directory, so no desktop portal can answer on it. A bus built from an installed `session.conf` activates the host's portal through the inherited `XDG_DATA_DIRS`, and that portal supplies the `gtk-xft-dpi` whose absence blanks a preview created on a not-yet-allocated host, so it masks that defect on a NixOS host while every container shows it. The app pins that dpi itself ([`app.rs`](../crates/wolf-gnome/src/README.md#apprs)), and the smokes stay portal-less so the pin stays under test.

With no portal GTK also leaves its font and xft settings unset, which leaves host-dependent glyph residue in a capture. Each cage therefore gets a generated `gtk-4.0/settings.ini` as its only `XDG_CONFIG_DIRS` entry, pinning those keys and never the dpi, which would satisfy the app's pin and hide its removal; `XDG_CONFIG_HOME` points at the smoke home so no user-level file overrides it. `GSETTINGS_BACKEND=memory` keeps the backend the same on every host, off a dconf writer the bus cannot activate.

Every cage also gets a private mode-0700 `XDG_RUNTIME_DIR`, which separates its Wayland socket, WebKit's proxy sockets and its runtime files from every other cage's. It is defence in depth for the hand-off as well: the developer's own runtime directory holds their session `bus`, which GLib falls back to wherever no address is set.

Every smoke holds one of a bounded number of cage slots for its whole body, because load stretches the scenarios' fixed settles; [`build.md`](../build.md#building-and-testing) owns the count. Each scenario runs inside `cage` as a single shell command, since `cage` exports its `WAYLAND_DISPLAY` only to its own child process tree. And every smoke's environment comes from one helper, which is the only thing keeping a smoke run out of the developer's real state and configuration directories.

## Frame-time instrumentation

`WOLF_FRAME_STATS` arms an off-by-default measurement that keeps the no-timer property and matches the macOS report exactly. [Frame-time instrumentation](wolf-gnome/frame-time-instrumentation.md) records its design and [the measured reading](wolf-gnome/frame-time-instrumentation.md#the-measured-reading).

## The readiness file

`WOLF_READY_FILE=<path>` is a test-only hook, off by default: once a launch's windows accept keystrokes, the process writes its pid there, and the smoke harness types nothing until the file names the pid it launched. No fixed sleep is a sound budget for that moment, because launch-to-input is a few hundred milliseconds on an idle host — longer with a cold shader cache — and unbounded under load. The predicate is [`ready_seam.rs`](../crates/wolf-gnome/src/README.md#ready_seamrs)'s; the coordinator gathers the readings and writes the file.

**A keyboard on the seat is the delivery precondition.** A headless `cage` seat has no keyboard until a virtual keyboard has existed, and the first key the first virtual keyboard sends after the window maps is lost. The harness primes the seat with a key-free `wtype ""`, and the app waits until that virtual keyboard appears in its seat's device list, so no key is sent before the client can receive one. The window must also have a focus widget for the window-level key controller to receive from.

**`is_active` and `Seat::keyboard()` are both rejected readings.** Under `cage` the toplevel turns active only after a real key arrives, so waiting for it deadlocks. `Seat::keyboard()` answers GDK's logical "Core Keyboard", which exists on a seat with no keyboard at all, so testing it is vacuous. `seat.devices(KEYBOARD)` lists only non-logical devices, so it turns non-empty when the virtual keyboard appears and stays so after the `wtype` that created it exits.

**The input window is the last mapped.** All windows share one Wayland connection, so the ordinal of each window's first frame-clock `after-paint` is its map order. Only that window must also have every bound pane's requested grid applied by Neovim, which is what makes pointer targets and `winwidth` readbacks mean what they say. An older, covered window may never be redrawn — inferred from wlroots' occlusion handling, not measured — so requiring it to be sized could hang.

**Every launch window must finish its restore.** A pending saved layout, or a restore queue that still owes pages, is consumed on a later wake with blocking RPCs that would interleave with typed keys. An abandoned restore issues no further RPC and counts as finished. Every window must also have a live core, a bound pane and a painted frame.

**"Neovim is not at a prompt" is deliberately not a condition.** The file means keys are delivered, not that they are interpreted as intended, so a startup prompt stays visible as the bug it is.

**Triggers.** The predicate is checked at the end of each wake, on the seat's `device-added` and after each frame-clock paint, all wired only in a window built while the probe is pending; there is no timer. Each change of the unmet condition is reported on stderr as `wolf-ready: waiting on <condition>`, through the same never-panic write the frame-time instrument uses.

**Lifetime and format.** The probe fires once per process and is never re-armed, so a Neovim restart or a later New Window is not covered. The file holds the pid and a newline, written through a `.tmp` and a rename, so a reader never sees a partial pid and a scenario's two launches can share one path. There is no margin after the write: a keystroke still lost means a condition is missing, and a sleep would hide which.

## Present limitations

What this front-end does not do, and why each gap stays, is listed in [Present limitations](wolf-gnome/present-limitations.md).
