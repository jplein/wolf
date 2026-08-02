# wolf-gnome/src

The `wolf-gnome` source modules. Each is a single file (not a subdirectory). The crate is **both a library and a binary**: a `[[bin]]` (`main.rs` → `app.rs`) runs the windowed application, while the library keeps the pure, widget-free seams (`input.rs`, `render.rs`, `geometry.rs`, `cursor.rs`, `tree.rs`, `layout.rs`, `actions.rs`, `command_action.rs`, `workspace.rs`) so they stay testable without a display. The GTK glue that binds them to live widgets lives in `app.rs`, `pane_view.rs`, `picker.rs`, `tree_view.rs`, and `shell.rs`. See the crate [`README.md`](../README.md).

## `main.rs`

The thin binary entry point. `main` only forwards the process exit code from [`app::run`](#apprs); the binary takes **no** command-line arguments (files are opened from within Neovim via `:e`).

## `lib.rs`

Crate root. Declares the public modules (`pub mod actions;`, `pub mod app;`, `pub mod command_action;`, `pub mod cursor;`, `pub mod geometry;`, `pub mod input;`, `pub mod layout;`, `pub mod pane_view;`, `pub mod picker;`, `pub mod render;`, `pub mod shell;`, `pub mod tree;`, `pub mod tree_view;`, `pub mod workspace;`) and re-exports the public surface: `gdk_key` (from `input`), `paint` + `CellMetrics` (from `render`), `grid_size_for` (from `geometry`), `ui_action_for` + `UiAction` (from `command_action`), and `launch_decision` + `LaunchDecision` (from `workspace`). The `actions` items (`ACTION_TABLE`, `MENU_SECTIONS`, `action_for_key`, `parse_accel`, `forbidden_reason`, …), the `cursor` module's `cursor_rect` / `cursor_colors`, the `layout` module's `PaneTree` / `PaneLayout`, the `pane_view` items, the `shell` items, and the `tree` / `tree_view` items are reached via the module path rather than re-exported. There is no `is_picker_trigger` re-export any more — the picker's trigger is now the `win.go-to-file` action (see [`actions.rs`](#actionsrs) and [`shell.rs`](#shellrs)).

## `app.rs`

All GTK4/Adwaita application wiring. `pub fn run() -> glib::ExitCode` builds an `adw::Application` with id `org.wolf.Wolf` and, on `activate`, `build_ui` constructs the window and attaches a freshly started `WolfCore`:

- **Launch decision + `CoreConfig.cwd`.** Before any widget is built, `build_ui` canonicalizes `std::env::current_dir()` and [`workspace::home_dir()`](#workspacers) and calls `workspace::launch_decision`, deriving `(core_cwd, empty)`. `core_cwd` — the project directory, or the home/`FALLBACK_CWD` fallback in the empty state — is what goes into `CoreConfig.cwd` (it replaced an unconditional `current_dir()`), and `empty` drives `tree_view::build`'s `start` flag and `AppShell::set_empty_state`.
- **Construction.** An `adw::ApplicationWindow` (title "Wolf", default 1200×640) whose content is a vertical `gtk::Box` of an `adw::HeaderBar` over a `gtk::Paned` (horizontal). The Paned's **start** child is the file-tree sidebar (built by [`tree_view::build`](#tree_viewrs)); its **end** child is the `gtk::Overlay`, whose main child is the **native pane view** ([`pane_view::build`](#pane_viewrs) — a tree of nested `gtk::Paned`s + per-pane `DrawingArea`s plus a bottom cmdline strip, replacing the former single editor area) and whose overlay children are, in order, the empty state ([`shell::build_empty_state`](#shellrs), added FIRST) and the file picker ([`picker::build`](#pickerrs), added after it so it floats above). The Paned divider position starts at `TREE_PANE_WIDTH` (250 px). The header bar carries an **Open** button (`document-open-symbolic`, tooltip "Open file (Ctrl+Shift+O)") on the start side, which activates `win.go-to-file`, and a `gtk::MenuButton` (`open-menu-symbolic`, tooltip "Main menu", model from `shell::build_menu_model()`) on the end side. Cell metrics are measured **once** (via a throwaway 1×1 `ImageSurface` probe → `CellMetrics::for_font`) and shared by Rc into the pane view.
- **Shell construction + action install ORDER.** After the window exists, `shell::build(ShellParts { … })` takes the core, window, picker, tree, sidebar widget, outer paned, empty-state widget and Open button; then `shell.install_actions(app)` and `shell.set_empty_state(empty)` run — **after** the window is constructed and **before** `window.present()`, so the menu and the header button can resolve their actions. The Open button is wired with `connect_clicked` + the qualified `gtk::prelude::WidgetExt::activate_action(&window, "win.go-to-file", None)`: `adw::ApplicationWindow` implements BOTH `gio::ActionGroup` (bare name, no result) and `gtk::Widget` (full `win.`-prefixed name, reports failure) and the latter is wanted; using the `action-name` property instead would render the button insensitive if the action did not exist yet.
- **Picker/tree focus target.** The pane view's **cmdline strip** (`pane_view::PaneHandle::cmdline_widget`) is a focusable widget in the editor region; it is handed to `picker::build` and `tree_view::build` as their focus-return `DrawingArea` so that, after they close, keys keep bubbling to the window-level key controller. The window grabs focus on it after `present()`.
- **Wake bridge + threading.** A bounded `async-channel` (capacity 1) carries wake signals; the `WakeFn` (the only thing that crosses a thread boundary) `try_send`s from the tokio worker, and a `glib::spawn_future_local` task on the GTK main thread drains it and, per signal, calls `panes.on_render_wake()` (create/split/close panes, consume layout intents, redraw), then `tree.on_render_wake()` (so the tree highlights/reveals the current file carried by the same render wake), then `shell.on_render_wake()` (settle a pending re-root, then drain and dispatch commands). **The shell is LAST, deliberately:** a drained `close_window` shuts the core down, so the other two must have had their turn first. The same `panes` → `shell` pair also runs once directly after the drain task is spawned, to react to the initial attached frame. `WolfCore` is started before the window is built; if it fails, the error is logged and no window appears. Per-pane painting, the mode-aware cursor, and per-grid/global resize now live in `pane_view.rs` (the app no longer installs a draw/resize handler).
- **Crash/stall recovery is NOT wired into the GTK front-end this pass.** The `wolf-core` crash/stall detection + restart surface (`WolfCore::liveness` / `restart_nvim` / `open_files_for_tabpage`, plus swap-file unsaved-edit recovery) is ADDITIVE — `CoreConfig` / `start` are unchanged, so `wolf-gnome` compiles with NO changes. But the GTK app does NOT yet read `liveness()` on each wake, surface a crash/stall overlay + "Restart Neovim" button, or drive `restart_nvim()` — so a crashed captive Neovim still freezes the GTK window on its last frame. Only the macOS (`wolf-macos`) front-end surfaces liveness/restart this pass; wiring it into GTK is deferred (`// TODO(crash-recovery)`).
- **Key controller (+ picker wiring).** `wire_keys` installs a `gtk::EventControllerKey` on the window with **no propagation phase** (i.e. Bubble): while the picker is open it `Proceed`s (keys are left for the focused picker widgets and never forwarded to nvim/core); otherwise the key routes via the pure `gdk_key` seam into `WolfCore::send_input` (`Stop` when handled, else `Proceed`). It carries **no trigger branch of its own any more** — the chord that opens the picker is matched by [`shell.rs`](#shellrs)'s CAPTURE-phase controller, which runs first and stops propagation, so a bound chord never reaches this controller and is never also forwarded to Neovim. Keys reach nvim's current window globally; the cursor renders in whichever pane owns the cursor's grid.
- **Lifecycle / shutdown.** `WolfCore` lives in `Rc<RefCell<Option<WolfCore>>>` on the main thread; `connect_close_request` first closes the picker **and** the tree (each dropping its `FileSearch` so any in-flight traversal is cancelled) before the core shuts down, then takes the core out (in its own statement to release the borrow) and runs `WolfCore::shutdown`. `wire_close` takes the `&TreeHandle` alongside the `&PickerHandle` for this.

Key constants: `APP_ID` (`org.wolf.Wolf`), `FONT_FAMILY` (`DejaVu Sans Mono`), `DEFAULT_FONT_PT` (`14.0`) with the `FONT_PT_ENV` (`WOLF_FONT_PT`) override, `INITIAL_COLS`/`INITIAL_ROWS` (80×24), `WINDOW_WIDTH`/`WINDOW_HEIGHT` (1200×640), `TREE_PANE_WIDTH` (250, now `pub(crate)` so `shell.rs` can restore the divider when the empty state clears), and `WAKE_CHANNEL_CAP` (1). `WINDOW_WIDTH` was widened from the editor-only default so that, after the tree takes `TREE_PANE_WIDTH` on the left, the editor pane keeps roughly its previous width (important for the OCR smoke tests).

## `actions.rs`

The **action namespace as pure data** — no GTK widget types, so the whole namespace is unit-tested headlessly in its own `#[cfg(test)] mod tests` (like `geometry.rs` / `cursor.rs`). One table is the single source of truth for every `gio::SimpleAction` name, its accelerator, the `wolf_core::Command` it corresponds to, and the one-line justification mirrored into the crate README's accelerator table; a second declares the primary menu over those same names. Because the menu, the accelerators and the command dispatch all read the same data, they cannot drift.

```rust
pub enum BindingStatus { Bound, Reserved, MenuOnly }
pub struct ActionBinding { pub action: &'static str, pub accel: Option<&'static str>, pub command: Option<Command>, pub status: BindingStatus, pub note: &'static str }
pub const ACTION_TABLE: &[ActionBinding];

pub struct MenuItemSpec { pub label: &'static str, pub action: &'static str }
pub struct MenuSection  { pub items: &'static [MenuItemSpec] }
pub const MENU_SECTIONS: &[MenuSection];

pub fn binding_for_action(action: &str) -> Option<&'static ActionBinding>
pub fn action_for_command(cmd: Command) -> Option<&'static str>
pub fn command_for_action(action: &str) -> Option<Command>
pub fn bound_bindings() -> impl Iterator<Item = &'static ActionBinding>
pub fn action_for_key(keyval: gdk::Key, state: gdk::ModifierType) -> Option<&'static str>

pub struct ParsedAccel { pub ctrl: bool, pub shift: bool, pub alt: bool, pub key: &'static str }
pub fn parse_accel(spec: &'static str) -> Option<ParsedAccel>
pub fn forbidden_reason(p: &ParsedAccel) -> Option<&'static str>
```

- **`BindingStatus`.** `Bound` = the accelerator is registered and matched now; `Reserved` = spoken for by a later unit, neither registered nor matched, and **never** in `MENU_SECTIONS` (the menu must not show an item that does nothing); `MenuOnly` = no chord at all, reachable from the menu, a button, or a drained command. Reserved rows live in the table anyway so the whole scheme is auditable in one place **and** so the invariant tests check them — a chord that would shadow a Neovim key must fail while it is reserved, not later when it is finally bound. Exactly four rows are `Bound`: `win.go-to-file` (Ctrl+Shift+O), `win.new-buffer` (Ctrl+Shift+N), `win.close-pane` (Ctrl+Shift+W), `app.quit` (Ctrl+Shift+Q).
- **`parse_accel`** is a deliberate re-implementation of the SUBSET of `gtk_accelerator_parse` this crate allows, because `gtk::accelerator_parse` cannot be called from a headless unit test (gtk4-rs asserts an initialized main thread) and a checker nobody can run is not a checker. The grammar is zero or more `<Control>` / `<Shift>` / `<Alt>` tokens (each at most once) followed by ONE key NAME of ASCII alphanumerics (`p`, `o`, `0`, `equal`, `minus`, `grave`, `Left`, `Right`). Anything else returns `None` — **including `<Super>` and `<Primary>`**, which is why those tokens never reach `forbidden_reason`.
- **`forbidden_reason`** rejects the chords that DO parse but would break the invariant: a bare `Ctrl+<single letter>` ("would make `<C-letter>` unreachable") and `Alt` combined with a single-letter key (GTK's mnemonic namespace, and an unmapped forwarded Alt chord splits into `<Esc>` + the base key). `Alt` with a NAMED key (`Left` / `Right`) is allowed — capturing it is precisely what prevents that split.
- **`action_for_key`** is the LIVE chord matcher used by [`shell.rs`](#shellrs)'s capture-phase controller. It scans only `Bound` rows (matching a Reserved row would swallow the key and do nothing), compares Ctrl / Shift / Alt **exactly**, ignores every other modifier bit — so `Ctrl+Shift+Super+O` still activates `win.go-to-file`, which is better than forwarding a Super chord Neovim mangles — and compares the key case-insensitively via the private `key_name_matches` (whether a shifted letter arrives as `o` or `O` is keymap-dependent). An unknown key name resolves to `false` rather than a guess.
- **The one GDK dependency.** `action_for_key` needs `gdk::Key` and `gdk::ModifierType`. Both are plain value types — no widget, no display, no initialized main thread — exactly what `input::gdk_key` and its headless tests already use, so the module stays headlessly testable.

The invariant these tables exist to enforce, its justification, the full macOS→Linux table, the two flagged assumptions and the disabled-action wart are all in the crate [`README.md`](../README.md), "Keyboard accelerators".

## `command_action.rs`

The **pure, GTK-free** mapping from a drained `wolf_core::Command` to the UI action `wolf-gnome` performs for it. Unit-tested headlessly in its own `#[cfg(test)] mod tests`.

```rust
pub enum UiAction {
    OpenFilePicker, OpenFileDialog, OpenFolderDialog, NewUntitledBuffer,
    ClosePane, CloseWindow, MinimizeWindow, QuitApplication,
    Deferred(&'static str),          // implemented on macOS, planned on GTK
    NoGnomeEquivalent(&'static str), // no GNOME counterpart, never implemented
}
pub fn ui_action_for(command: Command) -> UiAction
```

**Totality is the contract.** `ui_action_for` is a `match` with **no `_` arm**, so appending a variant to `wolf_core::Command` is an `E0004` compile error in `wolf-gnome` until the new command is classified — the same guard `pane_view::apply_intents` uses for `LayoutIntent`, and for the same reason: a silently-dropped command is indistinguishable from a broken one. `every_registered_command_maps` adds a runtime belt-and-braces check that walks `COMMAND_SPECS` rather than the match, so the guard survives even if someone adds a catch-all arm.

Of the 26 registered commands: **8 dispatch** today (`CloseWindow`, `QuitApplication`, `MinimizeWindow`, `GoToFile`, `OpenFile`, `OpenFolder`, `ClosePane`, `NewBuffer`), **1** is `NoGnomeEquivalent` (`HideApplication` — GNOME has no application-level hide; the shell owns window visibility), and **17** are `Deferred`, each with a DISTINCT reason naming its concrete blocker (a unit test enforces distinctness, non-emptiness and clause casing, so no two rows can collapse into one indistinguishable entry and the printed diagnostic always names the real blocker). `OpenFile` dispatching does **not** tick the feat-list's "Native file dialogs (open / save / save-as)" row — save and save-as remain absent.

## `shell.rs`

The **GTK glue** for the action namespace: `gio::SimpleAction`s, the primary menu, accelerator registration, the chord controller, the empty state, and the command drain. Mirrors `picker.rs` / `tree_view.rs`: `Rc<ShellInner>` with `RefCell` on the single GTK main thread and a `Weak` upgrade inside **every** closure (`ShellInner` owns the window, and the window owns the actions whose closures would otherwise own `ShellInner` back).

```rust
pub struct ShellParts<'a> { pub core, pub window, pub picker, pub tree, pub sidebar, pub paned, pub empty_state, pub open_button }
pub struct AppShell { /* Rc<ShellInner> */ }  // Clone

pub fn build_menu_model() -> gio::Menu
pub fn build_empty_state() -> gtk::Widget
pub fn build(parts: ShellParts<'_>) -> AppShell

impl AppShell {
    pub fn install_actions(&self, app: &adw::Application)
    pub fn set_empty_state(&self, empty: bool)
    pub fn on_render_wake(&self)
}
```

`ShellParts` is a struct rather than eight positional parameters because `clippy::too_many_arguments` bites at seven. Private methods: `install_chord_controller`, `action_is_enabled`, `drain_and_dispatch_commands`, `perform`, `close_window`, `quit_application`, `open_folder_dialog`, `open_file_dialog`, `chdir_to`, `settle_pending_reroot`, `send_input`.

- **`install_actions`** creates one `gio::SimpleAction` per **implemented** row of `ACTION_TABLE` — "implemented" meaning the row carries a command whose `ui_action_for` is neither `Deferred` nor `NoGnomeEquivalent` — adding `app.*` via `app.add_action` and `win.*` via `window.add_action`, and stashing the `win.go-to-file` action so the empty state can disable it. It then calls `set_accels_for_action` for every `Bound` row and installs the chord controller. **Must run after the window is constructed and before `window.present()`.**
- **`install_chord_controller`** adds a **CAPTURE-phase** `gtk::EventControllerKey` on the window that resolves the event through `actions::action_for_key`, checks `action_is_enabled`, activates the action and returns `Propagation::Stop`. It exists because GTK's own matching is keymap-aware and does not fire under the cage/`wtype` harness (`GdkKeyEvent::matches` returns `KeyMatch::None` even against the event's own keyval and state, because `wtype` uploads a fresh one-keycode keymap per keystroke). `set_accels_for_action` is still called, but **only so `gio::Menu` renders the chord labels**. A DISABLED action is deliberately left to fall through, matching what a real GTK accelerator does. Because it lives on the WINDOW, it also runs ahead of `picker.rs`'s own capture-phase controller (which sits on the picker box) and carries **no `picker.is_open()` guard**, so a bound chord fires even while the picker overlay is up — intended, and a behaviour change from the deleted `is_picker_trigger` branch, which sat behind `wire_keys`'s `picker.is_open()` early return. The picker's own keys are unaffected: `Esc` / `Up` / `Down` / `Enter` / ordinary typing are not bound chords, so `action_for_key` returns `None` and the event proceeds. See the crate [`README.md`](../README.md), "How a chord is actually matched (not by GTK)".
- **`on_render_wake`** runs `settle_pending_reroot()` then `drain_and_dispatch_commands()`. The app calls it LAST on each wake.
- **Borrow discipline (load-bearing).** `drain_and_dispatch_commands` takes a SHORT `core.borrow()`, collects into a `Vec<Command>`, and drops the borrow **in its own statement** before dispatching anything. `UiAction::CloseWindow` / `QuitApplication` synchronously run `app::wire_close`'s `close-request` handler, which does `core.borrow_mut().take()`; an outstanding `borrow()` there is a `RefCell` panic, not a warning.
- **`perform`** is exhaustive with no `_` arm, so a new `UiAction` variant is a compile error here too. `CloseWindow` → `window.close()`, **never** `destroy()` (which skips `close-request` and leaks the nvim child); `QuitApplication` → `close()` on every one of the application's windows, deliberately **not** `gio::Application::quit()`, which bypasses `close-request` and orphans the captive Neovim; `MinimizeWindow` → `window.minimize()`; `ClosePane` / `NewUntitledBuffer` → `InputEvent::ClosePane` / `InputEvent::NewBuffer`; `OpenFilePicker` → `picker.open()`. A `Deferred` / `NoGnomeEquivalent` reaching either the drain or `perform` is **printed**, never silent.
- **`open_folder_dialog`** shows a `gtk::FileDialog` folder chooser; on success `chdir_to` canonicalizes the path, sends `InputEvent::Chdir` and records it as `pending_reroot`. It does **not** re-root the tree synchronously — `start_file_search` resolves its root from `live_cwd()`, which only updates after nvim reports `DirChanged`, so an immediate re-root would re-root at the OLD cwd. `settle_pending_reroot` re-checks `workspace::reroot_ready` on each wake and, when ready, clears the pending value, calls `set_empty_state(false)` and `tree.reroot()`. `open_file_dialog` is the same shape, ending in `InputEvent::Open { path }`. A dialog `Err` is overwhelmingly "the user dismissed it" and is not reported.
- **`set_empty_state(empty)`** toggles the empty-state overlay's visibility, sets `sidebar.set_visible(!empty)`, moves the paned divider to `0` / `TREE_PANE_WIDTH`, desensitizes the header Open button, and enables/disables `win.go-to-file`. **`sidebar.set_visible(false)` is the load-bearing half** — a divider position is advisory, while an invisible start child is never allocated at all. It disables **only** `win.go-to-file`: `win.new-buffer` and `win.close-pane` stay enabled in the empty state, so Ctrl+Shift+N there creates a buffer hidden behind the opaque status page. See the crate [`README.md`](../README.md), "The empty state", for that known limitation.
- **`build_empty_state`** returns an opaque (`background` style class), full-bleed `gtk::Box` holding an `adw::StatusPage` (`folder-symbolic`, "No folder open", "Open a folder to start editing.") with a suggested-action pill button labelled "Open Folder…". It starts hidden and is meant to be added via `gtk::Overlay::add_overlay` so the editor `DrawingArea` keeps its full allocation. The button **self-wires** with `connect_clicked` + `activate_action(OPEN_FOLDER_ACTION)` on itself (widget action activation walks up the same muxer the window installs), which is why there is no `empty_state_widget()` accessor — it would have had no caller.

Named constants: `OPEN_FOLDER_ACTION`, `GO_TO_FILE_ACTION` (`pub(crate)`, also the header Open button's target), `EMPTY_STATE_TITLE` / `_DESCRIPTION` / `_BUTTON` / `_ICON`, and `OPEN_FOLDER_DIALOG_TITLE` / `OPEN_FILE_DIALOG_TITLE`.

## `workspace.rs`

The **pure, GTK-free** workspace decision: is a project open, and where does the captive Neovim start? Unit-tested headlessly in its own `#[cfg(test)] mod tests`.

```rust
pub const FALLBACK_CWD: &str = "/";
pub enum LaunchDecision { EmptyState { nvim_cwd: PathBuf }, Folder { cwd: PathBuf } }
pub fn launch_decision(process_cwd: Option<&Path>, home: Option<&Path>, is_dir: impl Fn(&Path) -> bool) -> LaunchDecision
pub fn reroot_ready(pending: Option<&Path>, live: Option<&Path>) -> bool
pub fn home_dir() -> Option<PathBuf>
```

- **`launch_decision`** returns `Folder { cwd }` ONLY when `process_cwd` is `Some`, `is_dir(cwd)` holds, `cwd != home`, and `cwd != "/"`. Otherwise `EmptyState { nvim_cwd }`, where `nvim_cwd` is `home` when `home` is `Some` and `is_dir(home)`, else `FALLBACK_CWD` (a `$HOME` pointing at a non-directory is no better a place to start nvim than `/`). `is_dir` is an **injected predicate**, not a filesystem call, which is what makes the rule testable with no temp dirs and no real `$HOME`; production passes `Path::is_dir`. See the crate [`README.md`](../README.md), "The empty state", for why the process cwd is the right signal on Linux.
- **`reroot_ready`** is `false` unless BOTH sides are `Some` and equal. It is a plain `==`: `Path: PartialEq` compares COMPONENTS, so a trailing separator is already ignored — but `.` / `..` / symlinks are **not** resolved, which is exactly why callers canonicalize both sides first (`app.rs` for the launch paths, `shell.rs::chdir_to` for the pending re-root).
- **`home_dir`** is the module's one impure function: `std::env::var_os("HOME")`, rejecting unset/empty, then `std::fs::canonicalize`. Read this way rather than adding a `dirs` dependency to `wolf-gnome`.

## `geometry.rs`

The pure pixel-to-grid geometry seam (no GTK/pango):

```rust
pub fn grid_size_for(width_px: i32, height_px: i32, cell_w: f64, cell_h: f64) -> (u16, u16)
```

`cols = floor(width_px / cell_w)`, `rows = floor(height_px / cell_h)`, each clamped to `[1, u16::MAX]`. As a resize guard, the **whole** result collapses to `(1, 1)` if either pixel dimension is non-positive or either cell size is degenerate (zero, negative, NaN, or infinite). The clamp is done in `f64` space _before_ the cast, so the truncating cast to `u16` is saturating-safe by construction. `#[must_use]`.

## `input.rs`

The GDK key-translation seam.

```rust
pub fn gdk_key(keyval: gdk::Key, state: gdk::ModifierType) -> Option<wolf_core::KeyEvent>
```

- **Modifier extraction**: `ctrl` ← `CONTROL_MASK`, `shift` ← `SHIFT_MASK`, `alt` ← `ALT_MASK`, `meta` ← `SUPER_MASK`.
- **Mapping table** (named keys resolved _before_ the printable fallback, since their `to_unicode()` is often a control char): `Return`/`KP_Enter` → `Enter`; `Escape` → `Esc`; `Tab`/`ISO_Left_Tab` → `Tab`; `BackSpace` → `Backspace`; `Delete` → `Delete`; `Left`/`Right`/`Up`/`Down`; `Home`/`End`; `Page_Up`/`Page_Down` → `PageUp`/`PageDown`. The contiguous `F1..=F12` keyval range maps to `Key::F(n)`, with the named base constant `F_KEY_FIRST = 1` (so `F1` → `n = 1`).
- **Printable fallback**: otherwise `keyval.to_unicode()`; a non-control char becomes `Key::Char(c)`.
- **Shift rule**: the printable branch _always_ sets `shift = false` (the keyval already carries the shifted glyph); named keys pass the real `shift` through. `ISO_Left_Tab` is treated as Shift+Tab → `Key::Tab` with `shift = true` even if the SHIFT mask was not set.
- **`None` behavior**: bare modifier / lock keys (`is_modifier_or_lock`: `Shift_L/R`, `Control_L/R`, `Alt_L/R`, `Meta_L/R`, `Super_L/R`, `Hyper_L/R`, `Caps_Lock`, `Shift_Lock`, `Num_Lock`, `Scroll_Lock`); any keyval with no unicode value; and any control-character unicode value.

Internal helpers: `named_key` (the match table), `function_key` (the `F1..=F12` range check via `into_glib()`), and `is_modifier_or_lock`.

## `render.rs`

The Cairo/Pango paint seam.

```rust
pub struct CellMetrics { pub cell_w: f64, pub cell_h: f64, pub baseline: f64, pub font: pango::FontDescription }
impl CellMetrics { pub fn for_font(cr: &cairo::Context, family: &str, size_pt: f64) -> Self }
pub fn paint(cr: &cairo::Context, grid: &wolf_core::Grid, highlights: &wolf_core::HighlightTable, metrics: &CellMetrics)
```

Under `ext_multigrid` the front-end paints a single grid at a time, so `paint` takes the `Grid` and its `HighlightTable` directly rather than a whole `Snapshot`. `app.rs` passes `snap.active_grid()` + `&snap.highlights` (the active window grid in this stage; a per-pane grid later).

- **`CellMetrics` fields** (all in Cairo device units — pixels for an `ImageSurface`): `cell_w` advance width, `cell_h` line height, `baseline` offset from cell top, and `font` the base `pango::FontDescription` (cloned and toggled for bold/italic per cell).
- **`CellMetrics::for_font`** builds a `FontDescription` for `family` at `size_pt` (× `pango::SCALE`), creates a Pango context from `cr` (surface size irrelevant), and reads its `FontMetrics`: `cell_w` = `approximate_char_width` (exact for monospace), `cell_h` = ascent + descent, `baseline` = ascent — each divided by `pango::SCALE`.
- **`paint` algorithm**, per `(row, col)` over the grid: resolve the cell's highlight via `HighlightTable::resolve` (`reverse` + defaults already applied); choose fg/bg with fallback to the editor defaults and finally to the named `FALLBACK_FG` (white) / `FALLBACK_BG` (black); convert 8-bit channels via the `RGB_MAX = 255.0` divisor; **always** fill the background rectangle (the Cairo `Result` is `expect`ed, never dropped); then **skip the glyph for empty text** (`cell.text.is_empty()` — a wide-char trailing placeholder or never-painted cell, where the bg fill alone preserves alignment).
- **`draw_glyph`** (the glyph path): clones the base font, toggles `Weight::Bold` / `Style::Italic` per `ResolvedStyle`, creates a pangocairo layout, sets the text, applies single `Underline` via an `AttrList` when `style.underline`, sets the fg source, moves to the cell top-left, and shows the layout.
- **Attributes applied**: bold, italic, single underline. **Deferred** for Tier 1: undercurl and strikethrough. No system theme is queried.

`FALLBACK_FG` / `FALLBACK_BG` and `draw_glyph` are `pub(crate)` so the `cursor` module and `app.rs` can reuse them (no color/alignment drift).

## `cursor.rs`

The pure, GTK-free cursor geometry and color logic (mirrors `geometry.rs`: no widget, no display, no nvim — fully unit-tested in its own `#[cfg(test)] mod tests`). `app.rs`'s `draw_cursor` is the only GTK glue: it computes the cell box from `CellMetrics`, calls these two functions, issues the opaque fill, and (for a block) redraws the glyph via `render::draw_glyph`.

```rust
pub fn cursor_rect(shape: wolf_core::CursorShape, cell_x: f64, cell_y: f64, cell_w: f64, cell_h: f64, cell_percentage: Option<u32>) -> (f64, f64, f64, f64)
pub fn cursor_colors(snap: &wolf_core::Snapshot) -> (wolf_core::Rgb, wolf_core::Rgb)
```

- **`cursor_rect`** returns the `(x, y, w, h)` to fill for `shape` within the cell box:
  - `Block` → the whole cell, ignoring `cell_percentage`.
  - `Vertical` → a left bar of full height; width = `cell_percentage` of `cell_w`, floored to `MIN_BAR_PX` and capped at `cell_w`.
  - `Horizontal` → a bottom bar of full width (anchored to the cell's bottom edge); height follows the same rule against `cell_h`.
  - `cell_percentage` is a **PERCENT** (1–100): it is divided by `PCT_DENOM` (`100.0`) and clamped to `0.0..=1.0`, defaulting to `DEFAULT_CELL_PCT` (`0.25`, Neovim's convention) when `None`. Named constants: `MIN_BAR_PX` (`2.0`), `DEFAULT_CELL_PCT` (`0.25`), `PCT_DENOM` (`100.0`).
- **`cursor_colors`** returns the `(fill, glyph)` pair. The **`WolfCursor` app-theme group** (`Snapshot.theme.cursor`, from the `wolf_theme` notification) is honored FIRST and unconditionally when its bg is set — `fill` = theme bg, `glyph` = theme fg or (when the theme fg is unset) the underlying cell's bg, the reverse-video glyph — kept in lockstep with wolf-ffi's `cursor_colors` port. When the theme bg is unset (group absent/cleared) it falls through to **reverse video**: it honors the current mode's cursor highlight (`mode_infos[cursor.mode_idx].attr_id`, resolved) as `(fill, glyph) = (bg, fg)` **only when that yields a visible cursor** — both fg and bg resolve to `Some`, the fill `bg` differs from the underlying cell's background (no dark-on-dark over an identical cell), and `bg` differs from `fg` (the glyph is not invisible against its own fill). Otherwise it **inverts** the underlying cell: `fill` = the cell's fg, `glyph` = the cell's bg, each falling back through the editor defaults and finally to `render`'s `FALLBACK_FG` (white) / `FALLBACK_BG` (black). An out-of-range `mode_idx`, empty `mode_infos`, or an out-of-bounds cursor cell all fall through to inversion without panicking (via `.get`).

## `layout.rs`

The **pure, GTK-free** N-ARY pane-layout tree (decision D7, Neovim's frame model), unit-tested headlessly in its own `#[cfg(test)] mod tests` (mirroring `geometry.rs` / `cursor.rs`). It owns every pane-geometry decision reasonable without a display; `pane_view.rs` is the only place that turns it into widgets.

```rust
pub type PaneId = u64;
pub enum Orientation { Horizontal, Vertical }   // matches wolf_core::SplitOrientation
pub struct Rect { pub x: f64, pub y: f64, pub w: f64, pub h: f64 }
pub enum PaneLayout { Leaf(PaneId), Split { orientation, children: Vec<PaneLayout>, weights: Vec<f64> } }
pub struct PaneTree { /* private n-ary frame tree of leaves/splits */ }
impl PaneTree {
    pub fn new(root: PaneId) -> Self
    pub fn split(&mut self, target: PaneId, new_pane: PaneId, orientation: Orientation)
    pub fn remove(&mut self, pane: PaneId)
    pub fn leaves(&self) -> Vec<PaneId>
    pub fn pane_rects(&self, width: f64, height: f64) -> Vec<(PaneId, Rect)>
    pub fn neighbor(&self, from: PaneId, dir: wolf_core::FocusDir, width: f64, height: f64) -> Option<PaneId>
    pub fn layout(&self) -> PaneLayout
}
```

- **Frame model.** Internally a `Frame` is either a `Leaf(PaneId)` or an n-ary `Split { orientation, children, weights }` (a flat ROW for `Vertical`, a flat COLUMN for `Horizontal`). Invariants: a split has `>= 2` children; `weights.len() == children.len()` and the weights sum to 1 (they drive divider positions); and no split sits directly under a same-orientation split (each axis is a single flat row/column). `PaneLayout` is the read-only mirror returned by `layout()`.
- **`Orientation`** matches `wolf_core::SplitOrientation`: a `Horizontal` split STACKS panes top/bottom (divides HEIGHT); a `Vertical` split places them SIDE BY SIDE (divides WIDTH).
- **`split`** — a split whose orientation MATCHES the frame that owns `target` inserts `new_pane` as a FLAT sibling immediately AFTER `target` and re-EQUALIZES the row/column (so re-splitting along the same axis yields equal siblings — e.g. a row `[P1, P2]` vsplit on `P1` becomes equal thirds `[P1, P3, P2]`, NOT a nested 25/25/50 that blanks `P2`). A PERPENDICULAR split instead NESTS: it wraps `target` in a new two-child sub-frame `[target, new_pane]` (target FIRST: LEFT for vertical, TOP for horizontal). The root single-leaf case becomes a two-child split. No-op if `target` is not a leaf.
- **`remove`** drops the leaf's slot+weight, then NORMALIZES bottom-up: FLATTEN any same-orientation child split into its parent (scaling weights), COLLAPSE a now-1-child split into its sole child, and re-EQUALIZE remaining siblings (nvim `equalalways`). This subsumes collapse-to-sibling AND same-orientation merge. Removing the last pane is a no-op (the tree is never empty).
- **`leaves`** / **`pane_rects`** enumerate leaves and their tiling rectangles in a deterministic DFS (child-order) traversal; `pane_rects` divides each split's bounds by cumulative normalized weight fractions, tiling the bounds exactly (no zero rects for valid weights).
- **`neighbor`** returns the nearest pane in `dir` (from `pane_rects` geometry) that STRICTLY overlaps `from` on the cross axis (Left/Right overlap on Y; Up/Down on X), else `None` (edge / unknown pane). Uses `wolf_core::FocusDir` directly.
- **`layout`** returns the read-only `PaneLayout` mirror so the widget layer can build a chain of `gtk::Paned`s without seeing the private `Frame`.

Named constants: `DEFAULT_RATIO` (`0.5`, a fresh two-child split's per-child weight), `NEIGHBOR_EPS` (`1e-9`, strict overlap).

## `pane_view.rs`

The **GTK glue** that turns `layout`'s pure `PaneTree` into a live tree of nested `gtk::Paned`s + one `gtk::DrawingArea` per pane, plus the ★ S3 **cmdline strip**. It mirrors `tree_view.rs` / `picker.rs`: `Rc`/`RefCell`/`Cell` on the GTK main thread, a `Weak` upgrade inside every widget closure (no cycles), and short snapshot/borrow scopes never held across a `WolfCore` call.

```rust
pub struct PaneHandle { /* Rc<PaneInner> */ }  // Clone
pub fn build(core: &Rc<RefCell<Option<WolfCore>>>, metrics: &Rc<CellMetrics>) -> PaneHandle
impl PaneHandle {
    pub fn widget(&self) -> gtk::Widget          // the [pane area | cmdline strip] container
    pub fn cmdline_widget(&self) -> gtk::DrawingArea  // focus-return target for picker/tree
    pub fn on_render_wake(&self)
}
```

- **Per-pane render.** Each pane binds to a Neovim window (`win`) + its grid (`>= 2`, resolved via `win_pos`; pending until then). A pane's draw func paints ITS grid via the pure `paint` seam, and the mode-aware cursor (the `draw_cursor` helper, moved here from `app.rs`) is drawn only in the pane that owns the cursor's grid (`cursor.grid`). Panes NEVER paint grid 1. The active pane carries an `ACTIVE_PANE_CSS` (`wolf-active-pane`) focus-ring `outline` (no layout impact), loaded once into the default display.
- **Cmdline strip (`draw_cmdline`, OD4 minimal-but-correct).** A one-row `DrawingArea` along the bottom now paints from the folded `Snapshot::cmdline` / `Snapshot::toast` rather than grid 1's last row: under `ext_messages` (which implicitly enables `ext_cmdline` and sets `cmdheight=0`) the outer grid 1 empties, so when a command line is active the strip paints `firstc` + the flattened cmdline text with a caret at the byte position `pos` (snapped to a char boundary, column-approximate for single-width cells); when inactive it paints the LAST `toast` line so the emptied strip never shows blank/stale. The Adw completion popup (`ext_popupmenu`) and the multi-line scrollable toast are DEFERRED for GTK (OD4) — only the last toast line shows. Background + colors resolve from the default highlight (`highlights.resolve(0)`).
- **Render-wake orchestration.** `on_render_wake` (1) creates the first pane (reusing `FIRST_PANE_ID = 1`, the tree root) when a window grid appears; (2) drains `WolfCore::drain_intents` and applies each — `Split` mutates the tree + binds a new pane (grid resolved now or pending) and makes it active; `FocusDir` resolves the neighbour in the tree and calls `focus_win`; `DrawerOpen` is drained and DROPPED (see below); (3) detects closed windows (a bound pane whose grid vanished) and collapses them, never emptying the tree; (4) resolves pending pane grids via `grid_for_win` and sizes them; (5) refreshes the active pane from `active_win` (fallback: the cursor's grid); (6) queues redraws.
- **The terminal drawer is a wolf-gnome gap.** `wolf_core::LayoutIntent` carries a third variant, `DrawerOpen { win }`, which `wolf-macos` renders as a full-width, root-bottom pane held outside the pure `PaneTree`. wolf-gnome implements none of that and binds no `wolf_toggle_terminal` trigger, so the intent is unreachable in practice; `apply_intents` matches it explicitly and does nothing. The arm is written out (rather than a `_` wildcard) so that adding a FOURTH variant to the shared enum fails the GTK build loudly instead of being silently swallowed — which is exactly what went wrong when `DrawerOpen` first landed and left `wolf-gnome` failing to compile with E0004.
- **Rebuild reuses areas.** `rebuild_pane_area` detaches each pane's `DrawingArea` from its old parent and rebuilds the widget tree from `PaneTree::layout`, REUSING areas by `PaneId` (so bound grids/contents survive). A `Vertical` (side-by-side) split maps to a HORIZONTAL `gtk::Paned`; `Horizontal` (stacked) to a VERTICAL one. An n-ary split is rendered by `build_chain` as a RIGHT-LEANING chain of binary `gtk::Paned`s (first child = `start`, the rest = a nested chain as `end`); each paned has `set_shrink_start_child(false)`/`set_shrink_end_child(false)` and its divider is positioned at the first child's SHARE of the children that paned spans (so equal thirds give an outer 1/3 over an inner 1/2 — equal columns). Unbound leaves are skipped (a `None` side collapses to the other). Each leaf `DrawingArea` also carries a one-cell `content-width/height` floor so a non-shrinking child can never be allocated to zero pixels. `install_paned_position` re-applies the proportional divider on EVERY `max-position` notify (not one-shot): this converges across the nested multi-pass allocation and re-proportions on resize, so a 3rd same-orientation split no longer starves the pre-existing pane to a blank/gray zero-width strip.
- **Resize.** Each pane `DrawingArea`'s `connect_resize` → `grid_size_for` → `WolfCore::resize_grid` for that pane's grid, then `resize_global` resizes the outer/global grid 1 (`WolfCore::resize`) to the full editor area (so the cmdline spans the width and nvim's overall layout tracks the window). Dragging a paned divider resizes the child areas, which drives the same path.
- **Click-to-focus.** A `gtk::GestureClick` on each pane area calls `WolfCore::focus_win` for that pane's window and marks it active.

Named constants: `FIRST_PANE_ID` (1), `MIN_WINDOW_GRID` (2), `CMDLINE_GRID_ID` (1), `CMDLINE_STRIP_ROWS` (1.0), `ACTIVE_PANE_CSS` (+ its CSS data), `RGB_MAX` (255.0).

### Known v1 limitations (split panes)

These are deliberate scope choices for the first split-panes cut, not bugs. Each has a clear future direction:

- **Only windows opened via the overridden split commands get a pane.** Panes are created from `wolf_layout` `Split` intents, which only the rebound chords / Ex-commands (`<C-w>s`/`v`, `:split`/`:vsplit`/`:new`/`:vnew`) emit. A window created by a NON-overridden path — `:help`, the quickfix / location list, a preview window, `:diffsplit` — still gets a grid and a `WinInfo` in the snapshot but **no pane**, so its content is not shown (and it can hold stale focus). This follows from the intent-driven design. _Future:_ auto-create a pane for any unbound window grid (e.g. an append-split of the active pane).
- **The cmdline strip is single-row and popup/multi-line-toast are DEFERRED (OD4).** The strip now paints natively from `Snapshot::cmdline` / `Snapshot::toast` (not grid 1), but GTK renders only ONE row: an active command line (`firstc` + text + caret) or, when idle, the LAST toast line. The Adw completion popup (`ext_popupmenu`) and a multi-line scrollable toast (`ext_messages`) are deferred for GTK — so multi-line messages (`:messages`, a long wrapped `:`-command, the `press-ENTER` prompt) show only their final line, and command-line completion has no popup (macOS has both). This is the accepted gnome scope (OD4); wolf-core folds all three protocols regardless, so wiring the Adw widgets later is additive.
- **Focus-direction moves use idealized split ratios.** `PaneTree::neighbor` computes neighbours from the tree's nominal EQUAL split weights, not the actual dragged `gtk::Paned` divider positions, so `<C-w>h/j/k/l` can pick a different neighbour than the pixel geometry would after a manual drag.
- **Paned drags reset to equal on the next resize (v1).** The frame's per-child `weights` always equalize on split/close, and `install_paned_position` re-derives each divider from those weights on every `max-position` notify. A user drag changes a paned's `position` (not its `max-position`) so it holds until the next window/container resize or split/close, at which point the divider snaps back to the equal ratio. The `weights` make a future drag-writeback straightforward.
- **Floats and external windows are ignored.** `win_float_pos` / `win_external_pos` (and `msg_set_pos`) are dropped by core, so floating windows and detached UI grids do not appear as panes.

## `picker.rs`

The GNOME file-picker UI. Almost all of it is GTK widget wiring (which cannot be unit-tested headlessly); the small pure seams (`next_selection`, `restored_index`) are factored out and unit-tested in the module's own `#[cfg(test)] mod tests`, like `geometry.rs` / `cursor.rs`.

**Widgets.** `build(overlay, area, core) -> PickerHandle` constructs a floating `gtk::Box` (a `SearchEntry` over a `ScrolledWindow` wrapping a single-selection `ListBox`), adds it as an `Overlay` child over the editor `area`, wires all signals, and returns a `PickerHandle`. The box starts hidden. `PickerHandle` exposes `open()` / `close()` (both idempotent) and `is_open()` (the app queries it to suppress key forwarding while the picker has focus). **Trigger: the `win.go-to-file` action** — Ctrl+Shift+O, the primary menu's "Go to File…" item, or the header-bar Open button. The module has no trigger predicate of its own any more: the chord is resolved from [`actions::ACTION_TABLE`](#actionsrs) by [`shell.rs`](#shellrs)'s capture-phase controller, and `perform(UiAction::OpenFilePicker)` calls `open()`.

**Streaming refresh (the wake pattern).** `open()` calls `WolfCore::start_file_search(wake)`, shows + focuses the entry, and does an initial refresh so the MRU seed appears immediately. The `wake` (a `WakeFn` fired from a tokio worker on each traversal batch) only `try_send`s onto a bounded `async-channel` (capacity `PICKER_WAKE_CHANNEL_CAP = 1`, coalescing a burst; the send is **drop-safe** so a late cancel-path wake is harmless); a `glib::spawn_future_local` task drains it on the GTK main thread and calls `refresh()`. `connect_search_changed` also refreshes on every query change. `refresh()` reads the live `FileSearch` snapshot, calls `wolf_core::rank_matches`, and rebuilds the `ListBox` from the top `PICKER_MAX_ROWS = 200` matches, keeping a parallel `row_paths` vec (row index → absolute path). The drain task holds only a `Weak` ref to the picker state, so dropping the handle ends the task (no cycle).

**Keyboard navigation.** A capture-phase `EventControllerKey` on the picker box: `Esc` closes; `Up`/`Down` move the **list** selection (not the entry's text cursor) via `move_selection` (`next_selection`, clamped `[0, n-1]`, no wrap) and are consumed; every other key proceeds to the `SearchEntry` so typing flows. Selection is **preserved** across a streaming rebuild unless the query changed: `refresh()` uses `last_query` to decide reset-vs-preserve and `restored_index` (reset → row 0; else the clamped previous index; `None` for an empty list) to pick the row to select.

**Activation.** `Enter` in the entry (`connect_activate`) or a `row-activated` signal calls `activate_selection`, which sends `InputEvent::Open { path }` for the selected row (or the first row if none) to the core — the MRU is auto-recorded by the core input pump — then closes the picker. With **zero** rows it is a true no-op (it neither sends `Open` nor closes). `close()` drops the `FileSearch` (cancelling the traversal), hides the box, and returns focus to the editor.

**Pure seams (unit-tested):**

```rust
fn next_selection(current: i32, delta: i32, n_rows: i32) -> Option<usize>
fn restored_index(reset_to_top: bool, prev: Option<usize>, new_len: usize) -> Option<usize>
```

- `next_selection` — `clamp(current + delta, 0, n_rows - 1)`, `None` when `n_rows <= 0` (no wrap; a negative/out-of-range `current` is clamped by the same formula).
- `restored_index` — `None` for an empty list; `Some(0)` when resetting to the top; else the preserved `prev` (defaulting to `0`) clamped into the new range.

## `tree.rs`

A thin **re-export shim**: `pub use wolf_core::tree::*;`. The pure, toolkit-free hierarchical projection of the picker's flat file list — `TreeNode`, `children_of`, `ancestors_to_reveal`, `node_rel_path`, `rel_path_for` — and its headless unit tests now live in `wolf_core::tree` (a single, FFI-expressible source of truth shared by every front-end; see [`wolf-core/src/README.md`](../../wolf-core/src/README.md)). This module re-exports it so the GTK glue in `tree_view.rs` (and `lib.rs`'s re-exports) keeps compiling against the unchanged `crate::tree::{…}` paths.

## `tree_view.rs`

The **GTK glue** that turns `tree`'s pure projection into a live file-tree sidebar (a `gtk::ListView` driven by a `gtk::TreeListModel`). It mirrors `picker.rs`'s patterns: a long-lived `wolf_core::FileSearch`, an `async-channel` + `glib::spawn_future_local` drain that refreshes on each traversal batch, `Rc`/`RefCell` interior mutability on the single GTK main thread, and a `Weak` upgrade in the drain task to avoid a reference cycle.

**`build(core, area, start) -> TreeHandle`** assembles the widgets and returns a handle. The `start` flag decides whether the `FileSearch` begins at build time: `true` for a normal project launch, `false` in the empty state (no project open — the sidebar is hidden and there is nothing to walk), which leaves `inner.search` as `None`. The wake-drain task is spawned either way; it is a no-op while `search` is `None`, since `refresh` already early-returns. `TreeHandle` exposes `widget()` (the root `gtk::ScrolledWindow`, placed as the Paned start child), `on_render_wake()` (called from the app's render drain), `reroot()`, and `close()` (drops the `FileSearch`, cancelling the traversal — called by `wire_close` before shutdown).

**`reroot()`** starts (or restarts) the traversal after an Open Folder has settled: it cancels/drops the existing `FileSearch`, clears `entries`, empties `root_store`, clears `pending_reveal` and `last_current`, then calls `start_file_search` again and `refresh()`es. It is driven ONLY from [`shell.rs`](#shellrs)'s `settle_pending_reroot`, i.e. only after nvim has reported the new cwd that `WolfCore::start_file_search` resolves its root from. A user-typed `:cd` still does not re-root the sidebar.

- **Model.** Each `TreeNode` is wrapped in a `glib::BoxedAnyObject` (no custom GObject subclass). The root `gio::ListStore` holds `children_of(entries, "")` (the cwd's immediate children, so the top level is "expanded by default"). A `gtk::TreeListModel` (passthrough = false, **autoexpand = false**) lazily materializes each directory's children via a create-func that re-runs `children_of` against the latest shared `entries` (files return `None`). A `gtk::SingleSelection` wraps it with `set_autoselect(false)` (no stray row-0 highlight before a reveal), and the `gtk::ListView` uses `set_single_click_activate(true)`.
- **Factory.** A `gtk::SignalListItemFactory` builds a `gtk::TreeExpander` + a `gtk::Label` per row; bind sets `expander.set_list_row(..)`, the label text to the node name, and adds/removes the `CURRENT_FILE_CSS` (`wolf-current-file`, bold via CSS loaded once into the default display) class iff the row's `File` `abs_path` equals `Snapshot::current_file`.
- **Streaming refresh.** The long-lived `FileSearch` fires a drop-safe `wake` (bounded(1) `async-channel`) on each batch; the drain task calls `refresh()`, which copies the latest entries, rebuilds the root store from `children_of(entries, "")`, then retries any `pending_reveal`. The initial `refresh()` shows the MRU seed immediately.
- **Highlight + reveal.** `on_render_wake()` reads `Snapshot::current_file`; if it changed since `last_current`, it records it, stashes it as `pending_reveal`, and `refresh()`es (so fresh binds pick up the highlight) then `reveal`s. `reveal(abs)` maps the path via `rel_path_for`, expands its `ancestors_to_reveal` top-down (re-scanning row positions after each expansion via `find_row`), `selects` the file row, and `list_view.scroll_to(pos, …)` it into view (needs the gtk `v4_12` feature). Because the current file can arrive before the tree has streamed in, an unresolved reveal stays in `pending_reveal` and is retried on each refresh.
- **Single-click activation.** `connect_activate` → `activate(pos)`: a `File` sends `InputEvent::Open { path }` then `area.grab_focus()` (returns focus to the editor); a `Dir` toggles its row's expansion.

**v1 limitation (documented in the code):** rebuilding the root store on each refresh collapses any manual nested expansion; the subsequent reveal re-expands the path to the current file, but other manually-expanded directories are not restored.
