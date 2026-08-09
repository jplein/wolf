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
| Synthetic per-pane scrollbars from `win_viewport`          | `feat/mac-panes` (`9051f0f`) / `linux-feat/grid-geometry`                     | [x]   | [x]   |
| Max column width (90-col cap, centered grid)               | `feat/mac-panes` (`b6f545c`) / `linux-feat/grid-geometry`                     | [x]   | [x]   |
| "Use full width for content" toggle                        | `feat/use-whole-width` / `linux-feat/grid-geometry`                           | [x]   | [x]   |
| Text attributes: bold / italic / underline                 | `feat/paint-special-attributes` (`0cdaedd`)                                   | [x]   | [x]   |
| Text attributes: undercurl / strikethrough / special color | `feat/paint-special-attributes` (`0cdaedd`)                                   | [x]   | [x]   |
| Cursor color from the `WolfCursor` highlight group         | `feat/cursor-color`                                                           | [x]   | [x]   |
| Editor background inherits the theme background            | `feat/cursor-color` (`ca8b3c3`)                                               | [x]   | [x]   |
| Per-pane status bar (filename + display path)              | `feat/status-bar` (`ffc3c49`)                                                 | [x]   | [x]   |
| Active/inactive pane treatment (no dimming)                | `fix/dark-mode` (`745a8f5`)                                                   | [x]   | [x]   |
| Editor padding / insets                                    | `feat/ui-tweaks-2` (`eadf266`, not the branch tip)                            | [x]   | [x]   |
| macOS Option key composes accents (option-as-option)       | `feat/macos-alt-key-handling`                                                 | [x]   | [ ]   |

Divider drag on GTK is the stock `gtk::Paned` handle drag, and the position it lands on is written back into the pure tree's weights, which is what makes it survive a re-layout and reach `state.json`. One divergence from macOS is real and deliberate: GTK renders an n-ary split as a right-leaning paned chain, so a divider governs its head child against the whole remainder, which is rescaled proportionally, where macOS's `setPairFraction` reweights only the adjacent pair.

The per-pane status bar diverges from macOS in two visible ways, each for its own reason: its band is the editor's per-frame background out of the snapshot rather than a titlebar match, since a theme-owned widget background cannot be that colour, and a path too long for its pane is clipped rather than ellipsized, since ellipsizing needs a second Pango layout builder in the widget layer, which is what the one-glyph-path rule exists to prevent.

Mouse and vertical scroll input work on both front-ends, as thin GTK glue over pure seams (`geometry::cell_at` and the `mouse` module).

One text-attribute divergence is worth knowing: `undercurl` is drawn as Pango's wave on Linux, where macOS's `GlyphStyleResolver` folds it into a straight underline. Both take the decoration's colour from `special`, and both strike through in the layout foreground.

Horizontal scroll, non-primary buttons and kinetic scrolling are handled on neither platform, so they are gaps against other editors rather than against macOS.

The content cap, its centred grid, the full-width toggle and the synthetic scrollbar land together on Linux, with three deliberate divergences from macOS. The thumb sits in the pane's right padding gutter rather than over the last text column, which is what macOS's own `drawScrollbars` prose describes and its code does not implement. Only the thumb is drawn and it never fades, because a fade needs the periodic timer [`crates/wolf-gnome/DECISIONS.md`](crates/wolf-gnome/DECISIONS.md) forbids. And the centring offset is floored to whole device pixels, because a half-pixel `cr.translate` resamples every glyph and this front-end has no HiDPI story.

Pointer input is verified at two levels — the seams through a live Neovim, and four cage smokes injecting real pointer events into the running application via [`wolf-pointer-inject`](tools/wolf-pointer-inject/README.md). What that leaves unproven is listed in [`crates/wolf-gnome/tests/README.md`](crates/wolf-gnome/tests/README.md).

## Command line, messages, completion

| Feature                                                           | Branch or commit                                               | macOS | Linux |
| ----------------------------------------------------------------- | -------------------------------------------------------------- | ----- | ----- |
| `ext_cmdline` / `ext_messages` / `ext_popupmenu` folded core-side | `feat/command-line-ui`                                         | [x]   | [x]   |
| Native command-line rendering                                     | `feat/command-line-ui` (`acbe13e`)                             | [x]   | [x]   |
| Completion popup (`ext_popupmenu`)                                | `feat/command-line-ui` (`acbe13e`) / `linux-feat/cmdline-ui`   | [x]   | [x]   |
| Multi-line scrollable message toast                               | `feat/command-line-ui` (`acbe13e`) / `linux-feat/cmdline-ui`   | [x]   | [x]   |
| Neovim `confirm` prompts handled                                  | `fix/sporadic-crash` (`6807c0e`) / `linux-feat/cmdline-ui`     | [x]   | [x]   |
| Prompt toasts dismissed once answered                             | `fix/persistent-prompts` (`abf6e2a`) / `linux-feat/cmdline-ui` | [x]   | [x]   |

GTK paints a one-row bottom strip carrying the command line — its dialog `prompt`, its `firstc`, its text and its caret — plus two painted overlay surfaces above it: the message toast and the completion popup. Painted rather than widgets, because each one's background is the editor's own per-frame background out of the snapshot, which a theme-owned widget background cannot be; overlay children rather than a taller strip, because the strip's height is subtracted from the pane area and one that grew with its content would resize the Neovim grid on every message.

A toast shows as many lines as fit half the space above the strip, and a scroll gesture over it pages back through the older ones without reaching Neovim. Key-driven paging is not a rejected alternative but an impossible one: any key is dismissing input for a non-prompt toast, so a paging key would destroy what it paged. A `confirm` dialog is answerable because its question and its options line are on screen at the same time — the question is a `confirm`-kind message and the options line is the command line's `prompt` — and core, not either front-end, decides when that toast goes.

Two divergences from macOS are deliberate. The popup accepts no input at all, matching the synthesized scrollbar's rule that an indicator beats a control that visibly refuses. And the selected completion row is drawn in reverse video out of the snapshot's own colours rather than a theme accent, since every colour on these surfaces comes from the snapshot and a theme accent colour is not something this front-end can obtain — resolving a light/dark preference gives it no palette.

## Navigation and file access

| Feature                                     | Branch or commit                                                             | macOS | Linux |
| ------------------------------------------- | ---------------------------------------------------------------------------- | ----- | ----- |
| File picker (fuzzy, MRU-first, streaming)   | `feat/wolf-core-initial-implementation` (`43cc0d2`) / `feat/mac-file-picker` | [x]   | [x]   |
| Per-directory MRU cache                     | `feat/mac-file-picker` (`31251f4`)                                           | [x]   | [x]   |
| MRU cache relocated to the state directory  | `feat/nav-buttons` (`cceca62`)                                               | [x]   | [x]   |
| File-tree sidebar                           | `feat/wolf-core-initial-implementation` (`c9ee895`) / `feat/mac-file-tree`   | [x]   | [x]   |
| Sidebar synthetic cwd root node             | `feat/mac-file-tree` / `linux-feat/sidebar`                                  | [x]   | [x]   |
| Sidebar re-roots on Neovim's live cwd       | `feat/macos-ui` (`075c91f`) / `linux-feat/sidebar`                           | [x]   | [x]   |
| Sidebar in the leading position             | `feat/sidebar-ui` (`ebc7300`)                                                | [x]   | [x]   |
| Pickers hosted in a separate floating panel | `feat/markdown-preview` (`d763b3b`)                                          | [x]   | [ ]   |
| Buffer picker (⌘E)                          | `feat/buffer-list` / `linux-feat/buffer-picker`                              | [x]   | [x]   |
| Command palette (⌘P)                        | `feat/commands` (`4b3b53e`)                                                  | [x]   | [x]   |
| Open Recent (global recent-directories MRU) | `feat/commands` / `linux-feat/open-recent`                                   | [x]   | [x]   |
| Per-pane back/forward navigation history    | `feat/nav-buttons` (`597175b`) / `linux-feat/nav-history`                    | [x]   | [x]   |
| Project search UI (ripgrep-backed)          | `feat/search-ui` / `linux-feat/project-search`                               | [x]   | [x]   |
| Native file dialogs (open / save / save-as) | —                                                                            | [x]   | [ ]   |

Both front-ends surface the per-pane navigation history as two toolbar buttons and as menu items — `⌘[` / `⌘]` on macOS, `Alt+Left` / `Alt+Right` on Linux. The one difference left is the jump list: the macOS History menu also lists the active pane's recently visited files, where the GTK menu carries Back and Forward alone, for the reason recorded in [`crates/wolf-gnome/DECISIONS.md`](crates/wolf-gnome/DECISIONS.md).

The GTK command palette lists the registry commands `wolf-gnome` performs plus one Open Recent row per usable recent directory, each of which re-roots the window through the same path the Open Folder dialog takes; there are no Switch-to-Window rows, since `wolf-gnome` has one window.

One row stays unticked for partial coverage: the file-dialog row covers all three dialogs, and `wolf-gnome` has native Open File and Open Folder but no save or save-as, so saving remains `:w` from within Neovim.

The GTK sidebar re-roots from one settle point, on the working directory Neovim reports, so a `:cd` typed into Neovim moves it exactly as the Open Folder dialog and an Open Recent row do, and none of the three can move it twice.

## Windows, tabs, commands

| Feature                                      | Branch or commit                                          | macOS | Linux |
| -------------------------------------------- | --------------------------------------------------------- | ----- | ----- |
| Multiple windows                             | `feat/mac-ui-changes` (`2fd6256`)                         | [x]   | [ ]   |
| Command registry (`:Wolf*` + Lua faces)      | `feat/commands` (`fb0147c`)                               | [x]   | [x]   |
| Commands drained and dispatched to the UI    | `feat/commands` (`fb0147c`)                               | [x]   | [x]   |
| Native application menu bar                  | `feat/commands`                                           | [x]   | [x]   |
| Empty state ("Open folder…" button)          | `feat/commands` (`2a1eb94`)                               | [x]   | [x]   |
| Working directory in the window title        | `feat/status-bar` (`6fd2427`)                             | [x]   | [x]   |
| Unified toolbar (sidebar toggle, nav, title) | `feat/nav-buttons` (`96d2c97`) / `linux-feat/nav-history` | [x]   | [x]   |
| Native tabs, one Neovim per project          | `feat/window-tabs`                                        | [x]   | [ ]   |
| Application state persistence (`state.json`) | `feat/remember-working-directory`                         | [x]   | [x]   |
| Pane-layout persistence across relaunch      | `feat/macos-pane-dividers` (`5f8becb`)                    | [x]   | [x]   |
| Single-pane state-persistence fix            | `fix/state-persistence-single-pane-bug`                   | [x]   | [x]   |
| Copy / paste through the system clipboard    | `feat/copy-paste`                                         | [x]   | [x]   |
| `:q` / `:qa` mapped to close pane / window   | `feat/colon-q-remapping`                                  | [x]   | [x]   |
| ⌘N opens a new untitled Markdown buffer      | `feat/colon-q-remapping` (`0266b96`)                      | [x]   | [x]   |

`wolf-gnome` calls `WolfCore::drain_commands()` on every render wake, behind a total `Command` to `UiAction` mapping in `command_action.rs` with no catch-all arm, so a new core command fails to compile until it is classified. Of the 26 commands, 17 dispatch, 1 has no GNOME equivalent (`HideApplication`, since the shell owns window visibility), and 8 are deferred with a distinct printed reason.

The GNOME counterpart to the macOS menu bar is a header-bar primary menu driven by the same `gio` action namespace as the accelerators, so menu, chords and drained commands cannot drift apart. The accelerator scheme is documented in [`crates/wolf-gnome/DECISIONS.md`](crates/wolf-gnome/DECISIONS.md), "The accelerator namespace".

That header bar is also the unified toolbar: the sidebar toggle, Back and Forward, and Open at the start, the primary menu at the end, and the window title between them.

The `state` module is read and written. `wolf-gnome` snapshots on every wake without a timer or debounce, which is affordable because `StateStore::set_snapshot` ignores an unchanged tree and the writer thread coalesces; it restores the working directory, window size and pane layout at launch.

The single-pane fix is structural: the layout is suppressed by a content predicate, never by leaf arity, which is what made the original bug swallow the commonest shape of all — one window, one tab, one pane, one file.

One thing is deliberately not persisted on Linux: the window position, which Wayland gives a client no way to learn or to validate, so only the size is restored. The working directory, the pane layout and the sidebar preference all come back at launch.

## Markdown preview

| Feature                                           | Branch or commit                       | macOS | Linux |
| ------------------------------------------------- | -------------------------------------- | ----- | ----- |
| Markdown → HTML rendering (`wolf_core::markdown`) | `feat/markdown-preview`                | [x]   | [x]   |
| In-app live preview pane                          | `feat/markdown-preview` (`b159793`)    | [x]   | [x]   |
| Preview pane reuse / placement modes              | `feat/markdown-preview` (`937bb7f`)    | [x]   | [x]   |
| Follow links inside the preview                   | `feat/markdown-preview` (`61204b7`)    | [x]   | [x]   |
| Full path in a preview pane's status bar          | `feat/markdown-preview` (`2116415`)    | [x]   | [x]   |
| Math symbols / Inter font in the preview          | `feat/markdown-preview` (`6f6abd1`)    | [x]   | [x]   |
| "Edit Source" (preview's inverse)                 | `feat/markdown-preview` (`8a72e50`)    | [x]   | [x]   |
| Preview theme follows light/dark                  | `fix/dark-mode` (`3edf74e`, `66ce35e`) | [x]   | [x]   |
| Typography tuning for the preview                 | `feat/typography` (`6838614`)          | [x]   | [x]   |
| Syntax-highlighted code blocks                    | `feat/code-block-syntax-highlighting`  | [x]   | [x]   |
| Preview of anonymous / unsaved buffers            | `feat/preview-anonymous-files`         | [x]   | [x]   |
| Preview in browser (loopback HTTP server)         | `feat/preview-server` (`4728086`)      | [x]   | [ ]   |

The rendering pipeline and the loopback preview server are entirely in `wolf-core` and stay GUI-free by design. `wolf-gnome` hosts the second kind of pane content in a `webkit6::WebView`, the GTK analogue of `WKWebView`: every pane leaf is an overlay whose main child is the editor's drawing area, so a preview replaces a pane's grid without replacing its widget, and a live edit reaches the DOM as a JSON-encoded body swap that preserves the reader's scroll offset where a full reload would not.

The Linux boxes for the rendered document do not rest on the same evidence, and which is which matters. Light/dark is proved by a smoke that samples raw pixels out of its own `grim` captures at the two configured appearances, and that smoke also pins one of the typography commit's four observable changes: the removed body padding. The other three are absences, which no pixel can prove without vacuity, and stay pinned by `markdown_render.rs`'s CSS-text assertions.

Math and Inter rest on a recorded measurement instead, because the only decisive discriminator is deleting the `@font-face` rules from inside the running web view and re-measuring, and a smoke drives the app from outside. The smoke's own readings live in its assertion table in `crates/wolf-gnome/tests/smoke_headless.rs`; the font measurement lives in [`crates/wolf-gnome/tests/README.md`](crates/wolf-gnome/tests/README.md).

Link following and the anonymous-buffer preview are ticked on different evidence again. Link following is proved by a pointer smoke that clicks a rendered link and reads `tabpagebuflist()` back out of the captive Neovim, with the click point re-derived from the smoke's own capture so a stale constant cannot pass; its fixtures sit in a subdirectory precisely so a missing base URI cannot be answered by the working directory instead. The anonymous-buffer row is proved by a never-written `Untitled 2.md` reaching a bound preview, which one readback string establishes for the naming autocmd, the Markdown filetype gate and the preview binding at once.

Preview in browser stays unticked deliberately, and not because it is unimplemented. The code is written and dispatched, and the running GTK application produces a real loopback address and hands it to the platform URI launcher; what is unverified is the final hand-off. The development compositor has neither a browser nor a session bus, and the launcher reports success there while doing nothing at all. Deliberately exercising it is a hazard rather than merely untested, for the reason [`crates/wolf-gnome/tests/README.md`](crates/wolf-gnome/tests/README.md) records, so the tick is one character to add on a host where a human has watched a tab open.

Syntax-highlighted code blocks turns on parser coverage rather than on a code path: `wolf_highlight_code_blocks` is defined at module level in the captive `init.lua`, outside the vendored-tree gate, so it runs on every platform and answers false for a fence whose language has no parser and `highlights` query on the runtimepath.

The pinned Neovim supplies seven of those — `c`, `lua`, `markdown`, `markdown_inline`, `query`, `vim`, `vimdoc` — and a curated sixteen more are vendored, each platform building the same list its own way: Mach-O objects host-compiled by `bazel/treesitter-parsers`, and the same sixteen grammars from nixpkgs exported by `flake.nix`. One gate in the captive `init.lua` picks up whichever is present, so this row and the Appearance table's vendoring row are siblings rather than one depending on the other.

The Linux tick rests on raw pixels out of the running GTK application. A smoke renders one `go` fence and one unknown-language fence carrying byte-identical text and counts chromatic pixels in each: an unhighlighted block is neutral grey on grey, so the control counts zero, and the shared text holds every rendering variable except the highlighter's answer constant. `crates/wolf-ffi/tests/code_block_highlight.rs`, which asserts on the emitted HTML, corroborates it.

Two limits of the pane itself are recorded in [`crates/wolf-gnome/DECISIONS.md`](crates/wolf-gnome/DECISIONS.md) — a click inside a preview cannot focus it, because the web view must take pointer input to be scrollable at all, and nothing has captured a picker composited over a preview, which is why the floating-panel row above stays unticked too.

## Appearance, typography, theming

| Feature                                          | Branch or commit                                      | macOS | Linux |
| ------------------------------------------------ | ----------------------------------------------------- | ----- | ----- |
| User config file (`wolf-config.yaml`)            | `feat/neovim-plugin-vendoring` (`129aa8f`, `be2e86c`) | [x]   | [x]   |
| Config-driven font family / size / line height   | `feat/neovim-plugin-vendoring` (`129aa8f`, `be2e86c`) | [x]   | [x]   |
| Bundled fonts (iA Writer Mono S, JetBrains Mono) | `feat/neovim-plugin-vendoring` (`129aa8f`, `be2e86c`) | [x]   | [x]   |
| Baseline-grid typography                         | `feat/typography` (`2735a6d`)                         | [x]   | [x]   |
| Status bar aligned to the content edge           | `feat/typography` (`460692d`)                         | [x]   | [x]   |
| Follows the OS dark/light appearance             | `fix/dark-mode` (`c81f7a2`)                           | [x]   | [x]   |
| Application icon                                 | `feat/mac-application-icon`                           | [x]   | [ ]   |
| Vendored `orlando` rendering plugin              | `jplein/lua-changes` (`6ee5c16`)                      | [x]   | [ ]   |
| Vendored `lighthouse` colorscheme                | `jplein/lua-changes` (`0603937`)                      | [x]   | [ ]   |
| Vendored treesitter plugins and parsers          | `feat/neovim-plugin-vendoring`                        | [x]   | [x]   |

`wolf-gnome` reads the shared `wolf-config.yaml` through `wolf_core::config` and drives its cell geometry from the configured font family, size and line height; `WOLF_FONT_PT` overrides the configured size, as it does on macOS. Both editor families are vendored into the Nix devShell and exposed through the repo-controlled fontconfig, so a configured `iA Writer Mono S` resolves on Linux.

The `appearance` key resolves on both front-ends, and on Linux it reaches two places: the captive Neovim, as `:set background=dark|light`, and the GTK chrome, as an `adw::ColorScheme` — `auto` following libadwaita's own live reading and a forced value overriding it. What has no Linux counterpart is the other direction: macOS reads live OS colours and pushes them into Neovim's highlight groups with `nvim_set_hl`, and `wolf-gnome` reads no OS colour at all.

Baseline-grid typography is the 24px CSS grid inside the rendered preview document, so it belongs with the Markdown-preview rows above: it is a property of the document `wolf-core` renders, not of the GTK pane that displays it. What the grid guarantees is every value the stylesheet declares, plus one laid-out reading — a smoke measures the pixel distance from the document's top edge to a rule three code-carrying paragraphs below, and gets a whole multiple of the 24px unit. What buys that is one declaration, zero leading on the inline `code` element. It changes the document macOS renders too, and no macOS host was available, so the macOS half of the claim is inferred rather than measured. See [`crates/wolf-core/src/markdown/README.md`](crates/wolf-core/src/markdown/README.md).

Both front-ends align a status bar's text with the grid text below it, out of one inset; the macOS `StatusbarTextSpan` seam has no Linux counterpart, because `editor_inset::grid_rect` carries the centring offset in its own `x` and the painter, the pointer translation and the strip's text all read that one origin by construction.

Two of the three vendored plugin assets are bundled by Bazel into `Wolf.app` only: the captive `init.lua` self-locates `orlando` and `lighthouse` and is a strict no-op when absent, so on Linux they are simply not there. The treesitter trees are the exception — the same gate falls back to two paths the flake devShell exports, so both platforms reach the queries and the grammars. Its Linux evidence is `crates/wolf-ffi/tests/treesitter_highlight.rs`, where a fenced `go` block yields an injected `go` langtree, and `treesitter_parser_abi.rs`, where a nix-built grammar loads and parses under the pinned Neovim.

## Terminal

| Feature                         | Branch or commit                   | macOS | Linux |
| ------------------------------- | ---------------------------------- | ----- | ----- |
| Terminal drawer (⌃\`)           | `feat/terminal-drawer`             | [x]   | [x]   |
| Terminal defaults to full width | `feat/use-whole-width` (`0506e92`) | [x]   | [x]   |

The drawer is an `init.lua` helper driven by `InputEvent::ToggleTerminal`. On GTK it is a full-width strip below the editor tree, held outside the pane tree exactly as macOS holds it outside its own, bound to `<Control>grave` and reachable from the primary menu. The price is the backtick spelling of `<C-@>`, and `Ctrl+Shift+2` is the spelling that survives it; `crates/wolf-gnome/DECISIONS.md` owns that bargain and the ctrl-folding rule that makes claiming a control byte a declared act.

Full width costs neither front-end any code: `b:wolf_full_width` is seeded on the drawer buffer in `init.lua` and rides the ordinary `full_width_for_win` path both front-ends already consume, so neither special-cases a terminal. Measured under `cage` at ten-point type, the editor window reports ninety columns against the drawer's hundred and twenty-three.

## Reliability and performance

| Feature                                              | Branch or commit                                   | macOS | Linux |
| ---------------------------------------------------- | -------------------------------------------------- | ----- | ----- |
| Crash/stall detection (three-state liveness)         | `feat/neovim-crash-handling`                       | [x]   | [x]   |
| Crash/stall overlay + "Restart Neovim" button        | `feat/neovim-crash-handling`                       | [x]   | [x]   |
| In-place Neovim restart with open-file restore       | `feat/neovim-crash-handling`                       | [x]   | [x]   |
| Swap-file recovery of unsaved edits                  | `feat/neovim-crash-handling`                       | [x]   | [x]   |
| Auto-reload of externally changed files              | `feat/auto-refresh`                                | [x]   | [x]   |
| Sporadic hang / beach-ball fixes                     | `fix/sporadic-crash`                               | [x]   | [x]   |
| Large-repo traversal performance (core `TreeIndex`)  | `feat/commands` (`e31f521`, `e0d3933`)             | [x]   | [x]   |
| Large-repo sidebar performance (memo + refresh gate) | `feat/commands` (`e0d3933`) / `linux-feat/sidebar` | [x]   | [x]   |
| Streaming-refresh coalescing (~30 fps)               | `feat/commands` / `linux-feat/sidebar`             | [x]   | [x]   |
| Sidebar full-tree load fix                           | `c649491` / `linux-feat/sidebar`                   | [x]   | [x]   |
| Typing-latency instrumentation + draw fixes          | `feat/slow-typing-latency`                         | [x]   | [ ]   |
| Optimized release build target                       | `feat/slow-typing-latency` (`8ba033c`, `296954c`)  | [x]   | [ ]   |

Liveness, swap recovery, auto-reload and the hang fixes live in `wolf-core` or `init.lua`, so they reach Linux without front-end work. The GTK side reads `liveness()` at the top of every wake and drives `restart_nvim()`: a dead captive Neovim raises a full-bleed opaque overlay with a "Restart Neovim" button, one automatic in-place restart is attempted, and on success the pane view resets, the captured Tier-1 files reopen with the active one last, and a transient banner reports the restart.

The sidebar full-tree load row records a guarantee rather than a repair: the macOS defect was per-view wake routing, which a single-window front-end structurally cannot have, so what the Linux box states is that the sidebar's trailing refresh cannot drop a traversal's final batch.

Two limits are worth stating. The pane topology does not survive a restart — only the Tier-1 file set does, so a crash while the window was split returns one pane. macOS restores no more than that, so it is a shared limit rather than a Linux gap.

`NotResponding` is reachable from `wolf-gnome` at nine points. `wolf-core` marks a stall only from `block_on_rpc`'s timeout arm, and the GTK side issues nine such calls: the layout rebuild at launch, plus eight a running window issues, which [`crates/wolf-gnome/DECISIONS.md`](crates/wolf-gnome/DECISIONS.md) enumerates and keeps current. Only the preview render repeats while the user simply types. The overlay's presentation has end-to-end coverage — a cage smoke `SIGSTOP`s the captive Neovim and presses the chord. Paste adds none, because the clipboard read is asynchronous and the insert is fire-and-forget.

The second limit is that a stall does not clear itself on the GTK side. All eight calls a running window issues refuse to run unless the core reads `Alive`, so a wedged Neovim is never re-probed, and core's own auto-clear rides the success of a later synchronous call while a timed-out one is detached. The route back is the overlay's Restart button, which force-kills.

## Suggested order of work for Linux

Work already carried is recorded in the tables above. What remains, in order:

1. **Multi-window and tabs**, which reshape `wolf-gnome` from a single-window application into a coordinator-plus-views one, and are the largest single item left.
2. **The remaining Markdown-preview row.** Preview in browser is written and dispatched and needs only a host where a human can watch the hand-off land, per the Markdown-preview table above.
