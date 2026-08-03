# Feature list: macOS vs Linux

A per-feature parity ledger for the two front-ends, `wolf-macos` (AppKit/Swift, over the `wolf-ffi` swift-bridge surface) and `wolf-gnome` (GTK 4 / libadwaita, consuming `wolf-core` directly). Its purpose is to be a task list for bringing the Linux application up to the macOS application.

## How to read this

- **Feature** — one user-visible capability, or one shared-core capability that both front-ends depend on.
- **Branch or commit** — where the work was done. Feature branches are given by name (all are merged into `main`); a commit hash is given where the work does not correspond to a single branch.
- **macOS** / **Linux** — whether the feature is realized in that front-end today.

A checked **Linux** box on a shared-core feature means the capability is present and reachable on Linux, not merely that the Rust code compiles into `wolf-gnome`. Several capabilities live entirely in `wolf-core` or the captive `runtime/nvim/init.lua` and are therefore already available on Linux for free; those are the cheapest wins to verify, and they are called out in the notes.

Sources of truth for the assessments below: [`architecture.md`](architecture.md), [`crates/wolf-gnome/README.md`](crates/wolf-gnome/README.md), [`crates/wolf-macos/README.md`](crates/wolf-macos/README.md), and the source of `crates/wolf-gnome/src`.

## Foundations

| Feature                                         | Branch or commit                                          | macOS | Linux |
| ----------------------------------------------- | --------------------------------------------------------- | ----- | ----- |
| Embedded Neovim, msgpack-RPC, redraw/grid model | `feat/wolf-core-initial-implementation`                   | [x]   | [x]   |
| Multigrid render state + highlight table        | `feat/wolf-core-initial-implementation`                   | [x]   | [x]   |
| Native paint backend (cells, fg/bg, cursor)     | `feat/wolf-core-initial-implementation` / `feat/macos-ui` | [x]   | [x]   |
| Key translation seam + Shift contract           | `feat/wolf-core-initial-implementation` / `feat/macos-ui` | [x]   | [x]   |
| Hermetic, pinned captive Neovim                 | `feat/macos-ui` (`f9fe4b3`)                               | [x]   | [x]   |
| Neovim bundled into the shipped application     | `feat/macos-ui` (`f9fe4b3`)                               | [x]   | [ ]   |
| Word wrap / editor defaults in `init.lua`       | `f4e104d`                                                 | [x]   | [x]   |

Linux gets the pinned Neovim from the flake devShell via `WOLF_NVIM_BIN`; there is no packaged Linux application that carries its own Neovim the way `Wolf.app` does.

## Editor surface

| Feature                                                    | Branch or commit                                                              | macOS | Linux |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------- | ----- | ----- |
| Split panes (`PaneTree`: split / close / focus)            | `feat/split-panes` / `feat/mac-panes`                                         | [x]   | [x]   |
| Divider drag-to-resize (persisted weights)                 | `feat/macos-pane-dividers` (`5f8becb`, `03a4647`) / `linux-feat/divider-drag` | [x]   | [x]   |
| Mouse input (click to position, drag to select)            | `feat/mac-panes` (`9051f0f`) / `linux-feat/mouse-scroll`                      | [x]   | [x]   |
| Scroll / trackpad input                                    | `feat/mac-panes` (`9051f0f`, `4b7fdc5`) / `linux-feat/mouse-scroll`           | [x]   | [x]   |
| Synthetic per-pane scrollbars from `win_viewport`          | `feat/mac-panes` (`9051f0f`)                                                  | [x]   | [ ]   |
| Max column width (90-col cap, centered grid)               | `feat/mac-panes` (`b6f545c`)                                                  | [x]   | [ ]   |
| "Use full width for content" toggle                        | `feat/use-whole-width`                                                        | [x]   | [ ]   |
| Text attributes: bold / italic / underline                 | `feat/paint-special-attributes` (`0cdaedd`)                                   | [x]   | [x]   |
| Text attributes: undercurl / strikethrough / special color | `feat/paint-special-attributes` (`0cdaedd`)                                   | [x]   | [ ]   |
| Cursor color from the `WolfCursor` highlight group         | `feat/cursor-color`                                                           | [x]   | [x]   |
| Editor background inherits the theme background            | `feat/cursor-color` (`ca8b3c3`)                                               | [x]   | [x]   |
| Per-pane status bar (filename + display path)              | `feat/status-bar` (`ffc3c49`)                                                 | [x]   | [ ]   |
| Active/inactive pane treatment (no dimming)                | `fix/dark-mode` (`745a8f5`)                                                   | [x]   | [ ]   |
| Editor padding / insets                                    | `feat/ui-tweaks-2`                                                            | [x]   | [ ]   |
| macOS Option key composes accents (option-as-option)       | `feat/macos-alt-key-handling`                                                 | [x]   | [ ]   |

Divider drag on GTK is the stock `gtk::Paned` handle drag, and the position it lands on is written back into the pure tree's weights, which is what makes it survive a re-layout and reach `state.json`. One divergence from macOS is real and deliberate: GTK renders an n-ary split as a right-leaning paned chain, so a divider governs its head child against the whole remainder, which is rescaled proportionally, where macOS's `setPairFraction` reweights only the adjacent pair.

Mouse and vertical scroll input work on both front-ends, as thin GTK glue over pure seams (`geometry::cell_at` and the `mouse` module).

Horizontal scroll, non-primary buttons and kinetic scrolling are handled on neither platform, so they are gaps against other editors rather than against macOS. The synthetic per-pane scrollbar stays its own unticked row: scroll input reaches Neovim on Linux, but nothing draws a scrollbar there.

Pointer input is verified at two levels — the seams through a live Neovim, and four cage smokes injecting real pointer events into the running application via [`wolf-pointer-inject`](tools/wolf-pointer-inject/README.md). What that leaves unproven is listed in [`crates/wolf-gnome/tests/README.md`](crates/wolf-gnome/tests/README.md).

## Command line, messages, completion

| Feature                                                           | Branch or commit                     | macOS | Linux |
| ----------------------------------------------------------------- | ------------------------------------ | ----- | ----- |
| `ext_cmdline` / `ext_messages` / `ext_popupmenu` folded core-side | `feat/command-line-ui`               | [x]   | [x]   |
| Native command-line rendering                                     | `feat/command-line-ui` (`acbe13e`)   | [x]   | [x]   |
| Completion popup (`ext_popupmenu`)                                | `feat/command-line-ui` (`acbe13e`)   | [x]   | [ ]   |
| Multi-line scrollable message toast                               | `feat/command-line-ui` (`acbe13e`)   | [x]   | [ ]   |
| Neovim `confirm` prompts handled                                  | `fix/sporadic-crash` (`6807c0e`)     | [x]   | [ ]   |
| Prompt toasts dismissed once answered                             | `fix/persistent-prompts` (`abf6e2a`) | [x]   | [ ]   |

GTK paints a single-row bottom strip: an active command line, or the last toast line when idle. The Adw completion popup and a multi-line scrollable toast are the accepted GTK deferral (OD4), which also means a multi-line `confirm` prompt shows only its final line on Linux.

## Navigation and file access

| Feature                                     | Branch or commit                                                             | macOS | Linux |
| ------------------------------------------- | ---------------------------------------------------------------------------- | ----- | ----- |
| File picker (fuzzy, MRU-first, streaming)   | `feat/wolf-core-initial-implementation` (`43cc0d2`) / `feat/mac-file-picker` | [x]   | [x]   |
| Per-directory MRU cache                     | `feat/mac-file-picker` (`31251f4`)                                           | [x]   | [x]   |
| MRU cache relocated to the state directory  | `feat/nav-buttons` (`cceca62`)                                               | [x]   | [x]   |
| File-tree sidebar                           | `feat/wolf-core-initial-implementation` (`c9ee895`) / `feat/mac-file-tree`   | [x]   | [x]   |
| Sidebar synthetic cwd root node             | `feat/mac-file-tree`                                                         | [x]   | [ ]   |
| Sidebar re-roots on Neovim's live cwd       | `feat/macos-ui` (`075c91f`)                                                  | [x]   | [ ]   |
| Sidebar in the leading position             | `feat/sidebar-ui` (`ebc7300`)                                                | [x]   | [x]   |
| Pickers hosted in a separate floating panel | `feat/markdown-preview` (`d763b3b`)                                          | [x]   | [ ]   |
| Buffer picker (⌘E)                          | `feat/buffer-list`                                                           | [x]   | [ ]   |
| Command palette (⌘P)                        | `feat/commands` (`4b3b53e`)                                                  | [x]   | [x]   |
| Open Recent (global recent-directories MRU) | `feat/commands`                                                              | [x]   | [ ]   |
| Per-pane back/forward navigation history    | `feat/nav-buttons` (`597175b`)                                               | [x]   | [ ]   |
| Project search UI (ripgrep-backed)          | `feat/search-ui`                                                             | [x]   | [ ]   |
| Native file dialogs (open / save / save-as) | —                                                                            | [x]   | [ ]   |

The `wolf-core` halves of the buffer picker, the recent-directories MRU, the navigation history and project search are platform-agnostic and already linked into `wolf-gnome`; only the GTK views and their wiring are missing. Those are the highest-leverage Linux tasks. The GTK command palette lists only the registry commands `wolf-gnome` performs: Switch-to-Window rows need multiple windows, and Open Recent rows need the recent-directories row above.

Two rows stay unticked for partial coverage. The file-dialog row covers all three dialogs, and `wolf-gnome` has native Open File and Open Folder but no save or save-as, so saving remains `:w` from within Neovim. The sidebar re-roots after an Open Folder but not after a user-typed `:cd`.

## Windows, tabs, commands

| Feature                                      | Branch or commit                        | macOS | Linux |
| -------------------------------------------- | --------------------------------------- | ----- | ----- |
| Multiple windows                             | `feat/mac-ui-changes` (`2fd6256`)       | [x]   | [ ]   |
| Command registry (`:Wolf*` + Lua faces)      | `feat/commands` (`fb0147c`)             | [x]   | [x]   |
| Commands drained and dispatched to the UI    | `feat/commands` (`fb0147c`)             | [x]   | [x]   |
| Native application menu bar                  | `feat/commands`                         | [x]   | [x]   |
| Empty state ("Open folder…" button)          | `feat/commands` (`2a1eb94`)             | [x]   | [x]   |
| Working directory in the window title        | `feat/status-bar` (`6fd2427`)           | [x]   | [x]   |
| Unified toolbar (sidebar toggle, nav, title) | `feat/nav-buttons` (`96d2c97`)          | [x]   | [ ]   |
| Native tabs, one Neovim per project          | `feat/window-tabs`                      | [x]   | [ ]   |
| Application state persistence (`state.json`) | `feat/remember-working-directory`       | [x]   | [x]   |
| Pane-layout persistence across relaunch      | `feat/macos-pane-dividers` (`5f8becb`)  | [x]   | [x]   |
| Single-pane state-persistence fix            | `fix/state-persistence-single-pane-bug` | [x]   | [x]   |
| Copy / paste through the system clipboard    | `feat/copy-paste`                       | [x]   | [ ]   |
| `:q` / `:qa` mapped to close pane / window   | `feat/colon-q-remapping`                | [x]   | [x]   |
| ⌘N opens a new untitled Markdown buffer      | `feat/colon-q-remapping` (`0266b96`)    | [x]   | [x]   |

`wolf-gnome` calls `WolfCore::drain_commands()` on every render wake, behind a total `Command` to `UiAction` mapping in `command_action.rs` with no catch-all arm, so a new core command fails to compile until it is classified. Of the 26 commands, 9 dispatch, 1 has no GNOME equivalent (`HideApplication`, since the shell owns window visibility), and 16 are deferred with a distinct printed reason.

The GNOME counterpart to the macOS menu bar is a header-bar primary menu driven by the same `gio` action namespace as the accelerators, so menu, chords and drained commands cannot drift apart. The accelerator scheme is documented in [`crates/wolf-gnome/DECISIONS.md`](crates/wolf-gnome/DECISIONS.md), "The accelerator namespace".

The `state` module is read and written. `wolf-gnome` snapshots on every wake without a timer or debounce, which is affordable because `StateStore::set_snapshot` ignores an unchanged tree and the writer thread coalesces; it restores the working directory, window size and pane layout at launch.

The single-pane fix is structural: the layout is suppressed by a content predicate, never by leaf arity, which is what made the original bug swallow the commonest shape of all — one window, one tab, one pane, one file.

Two things are deliberately not persisted on Linux. Window position is unavailable, because Wayland gives a client neither its own position nor screen geometry, so only size is restored; and `sidebar_visible` is written so the field cannot rot, but never read back.

## Markdown preview

| Feature                                           | Branch or commit                       | macOS | Linux |
| ------------------------------------------------- | -------------------------------------- | ----- | ----- |
| Markdown → HTML rendering (`wolf_core::markdown`) | `feat/markdown-preview`                | [x]   | [x]   |
| In-app live preview pane                          | `feat/markdown-preview` (`b159793`)    | [x]   | [ ]   |
| Preview pane reuse / placement modes              | `feat/markdown-preview` (`937bb7f`)    | [x]   | [ ]   |
| Follow links inside the preview                   | `feat/markdown-preview` (`61204b7`)    | [x]   | [ ]   |
| Full path in a preview pane's status bar          | `feat/markdown-preview` (`2116415`)    | [x]   | [ ]   |
| Math symbols / Inter font in the preview          | `feat/markdown-preview` (`6f6abd1`)    | [x]   | [ ]   |
| "Edit Source" (preview's inverse)                 | `feat/markdown-preview` (`8a72e50`)    | [x]   | [ ]   |
| Preview theme follows light/dark                  | `fix/dark-mode` (`3edf74e`, `66ce35e`) | [x]   | [ ]   |
| Typography tuning for the preview                 | `feat/typography` (`6838614`)          | [x]   | [ ]   |
| Syntax-highlighted code blocks                    | `feat/code-block-syntax-highlighting`  | [x]   | [ ]   |
| Preview of anonymous / unsaved buffers            | `feat/preview-anonymous-files`         | [x]   | [ ]   |
| Preview in browser (loopback HTTP server)         | `feat/preview-server` (`4728086`)      | [x]   | [ ]   |

The rendering pipeline and the loopback preview server are entirely in `wolf-core` and stay GUI-free by design; the Linux gap is the second kind of pane content (a web view — `WebKitGTK` would be the GTK analogue of `WKWebView`) and the pane-kind plumbing around it.

## Appearance, typography, theming

| Feature                                          | Branch or commit                                      | macOS | Linux |
| ------------------------------------------------ | ----------------------------------------------------- | ----- | ----- |
| User config file (`wolf-config.yaml`)            | `feat/typography` (`2735a6d`)                         | [x]   | [ ]   |
| Config-driven font family / size / line height   | `feat/typography` (`2735a6d`)                         | [x]   | [ ]   |
| Bundled fonts (iA Writer Mono S, JetBrains Mono) | `feat/neovim-plugin-vendoring` (`129aa8f`, `be2e86c`) | [x]   | [ ]   |
| Baseline-grid typography                         | `feat/typography` (`2735a6d`)                         | [x]   | [ ]   |
| Status bar aligned to the content edge           | `feat/typography` (`460692d`)                         | [x]   | [ ]   |
| Follows the OS dark/light appearance             | `fix/dark-mode` (`c81f7a2`)                           | [x]   | [ ]   |
| Application icon                                 | `feat/mac-application-icon`                           | [x]   | [ ]   |
| Vendored `orlando` rendering plugin              | `jplein/lua-changes` (`6ee5c16`)                      | [x]   | [ ]   |
| Vendored `lighthouse` colorscheme                | `jplein/lua-changes` (`0603937`)                      | [x]   | [ ]   |
| Vendored treesitter plugins and parsers          | `feat/neovim-plugin-vendoring`                        | [x]   | [ ]   |

`wolf-gnome` hardcodes DejaVu Sans Mono and reads only `WOLF_FONT_PT`; it never calls the `wolf_core::config` module. The three vendored assets are bundled by Bazel into `Wolf.app` only — the captive `init.lua` self-locates them and is a strict no-op when absent, so on Linux they are simply not there. Vendoring them through the Nix flake is a separate, already-anticipated task.

## Terminal

| Feature                         | Branch or commit                   | macOS | Linux |
| ------------------------------- | ---------------------------------- | ----- | ----- |
| Terminal drawer (⌃\`)           | `feat/terminal-drawer`             | [x]   | [ ]   |
| Terminal defaults to full width | `feat/use-whole-width` (`0506e92`) | [x]   | [ ]   |

The drawer is an `init.lua` helper driven by `InputEvent::ToggleTerminal`. `Command::Terminal` reaches the GTK drain and is deliberately deferred there, because `pane_view` drops the `DRAWER_OPEN` layout intent and toggling would open a `:terminal` window GTK never paints. The `<Control>grave` chord is reserved for it in `crates/wolf-gnome/src/actions.rs` but not bound.

## Reliability and performance

| Feature                                              | Branch or commit                                  | macOS | Linux |
| ---------------------------------------------------- | ------------------------------------------------- | ----- | ----- |
| Crash/stall detection (three-state liveness)         | `feat/neovim-crash-handling`                      | [x]   | [x]   |
| Crash/stall overlay + "Restart Neovim" button        | `feat/neovim-crash-handling`                      | [x]   | [x]   |
| In-place Neovim restart with open-file restore       | `feat/neovim-crash-handling`                      | [x]   | [x]   |
| Swap-file recovery of unsaved edits                  | `feat/neovim-crash-handling`                      | [x]   | [x]   |
| Auto-reload of externally changed files              | `feat/auto-refresh`                               | [x]   | [x]   |
| Sporadic hang / beach-ball fixes                     | `fix/sporadic-crash`                              | [x]   | [x]   |
| Large-repo traversal performance (core `TreeIndex`)  | `feat/commands` (`e31f521`, `e0d3933`)            | [x]   | [x]   |
| Large-repo sidebar performance (memo + refresh gate) | `feat/commands` (`e0d3933`)                       | [x]   | [ ]   |
| Streaming-refresh coalescing (~30 fps)               | `feat/commands`                                   | [x]   | [ ]   |
| Sidebar full-tree load fix                           | `c649491`                                         | [x]   | [ ]   |
| Typing-latency instrumentation + draw fixes          | `feat/slow-typing-latency`                        | [x]   | [ ]   |
| Optimized release build target                       | `feat/slow-typing-latency` (`8ba033c`, `296954c`) | [x]   | [ ]   |

Liveness, swap recovery, auto-reload and the hang fixes live in `wolf-core` or `init.lua`, so they reach Linux without front-end work. The GTK side reads `liveness()` at the top of every wake and drives `restart_nvim()`: a dead captive Neovim raises a full-bleed opaque overlay with a "Restart Neovim" button, one automatic in-place restart is attempted, and on success the pane view resets, the captured Tier-1 files reopen with the active one last, and a transient banner reports the restart.

Two limits are worth stating. The pane topology does not survive a restart — only the Tier-1 file set does, so a crash while the window was split returns one pane. macOS restores no more than that, so it is a shared limit rather than a Linux gap.

`NotResponding` is unreachable from `wolf-gnome` for a running application: `wolf-core` marks a stall only from `block_on_rpc`'s timeout arm, and the only such call on the GTK side runs during the launch window. Its presentation is table-tested headlessly and has no end-to-end coverage. The buffer picker, Copy and the Markdown preview are all `block_on_rpc` paths, so any of them makes it reachable.

## Suggested order of work for Linux

Work already carried is recorded in the tables above. What remains, in order:

1. **The two remaining overlay pickers** (buffer picker, project search). Ranking is already shared, each command already lands in the drain as `Deferred`, and each chord is already reserved; each needs a GTK view of the shape the file picker and the command palette already have. The buffer picker also carries the first `block_on_rpc` call on the GTK side, which is what will finally make `NotResponding` reachable — and testable — on Linux.
2. **Config-driven typography**, then the completion popup and multi-line toast.
3. **Markdown preview**, which is the largest single item: it introduces a second kind of pane content on the GTK side.
4. **Multi-window and tabs**, which reshape `wolf-gnome` from a single-window application into a coordinator-plus-views one.
