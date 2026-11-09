# wolf-macos decisions

Last verified at commit `500d64fda2e3d0313b1f527edeb8954d468c40d5`.

The durable constraints behind this crate. Read this before changing anything here; most of it exists because a reasonable-looking simplification breaks something. Per-file detail lives in the directory READMEs under [`Sources/`](../crates/wolf-macos/Sources/README.md).

## Platform and build

- Tahoe (macOS 26) and newer only (`LSMinimumSystemVersion` / `minimum_os_version` = `26.0`).
- AppKit, not SwiftUI. Implementation language Swift, Xcode 26+.
- The Bazel build is authoritative: `rules_swift` and `rules_apple` build the `.app`. `rules_xcodeproj` generates an uncommitted `.xcodeproj` for debugging and LLDB only.
- Rust and Swift meet through swift-bridge, confined to [`wolf-ffi`](../crates/wolf-ffi/README.md). The generated bindings are committed.

### Why `:WolfRelease` is a custom rule

`release.bzl` defines the `opt_bundle` rule that `:WolfRelease` instantiates. Compilation mode is a Bazel configuration rather than an attribute, so no `alias` or `macos_application` wiring can set it; `opt_bundle` applies a Starlark outgoing-edge transition to its single `bundle` dep, setting `compilation_mode` to `opt` so the whole subgraph rebuilds optimized without `-c opt` on the command line.

It forwards only `DefaultInfo`, deliberately not the Apple providers, because nothing consumes them here and forwarding Apple providers from a non-Apple rule invites `rules_apple` to treat the target as an embeddable bundle.

The rule is executable: it symlinks the transitioned `macos_application`'s own runner into its predeclared output and forwards the dep's runfiles, which is how that runner locates `Wolf.zip`. The runner's app path is a bare working-directory-relative runfiles path, so dropping the runfiles merge — or passing `--nobuild_runfile_links` or `--enable_runfiles=false` — breaks it.

For frame-stats measurement, exec `"$APP/Contents/MacOS/Wolf"` directly rather than using `bazel run`, which unzips a temporary copy to a path that changes every launch and interleaves the app's stderr with Bazel's client output.

### Why `:InstallWolf` takes the archive through a filegroup

The installer is handed its input as `$(rootpath :wolf_release_archive)`, a filegroup wrapping `:WolfRelease`, and the indirection is not redundant. Location expansion on an executable target yields that target's executable, so `$(rootpath :WolfRelease)` resolves to the runner symlink rather than to `Wolf.zip`. A non-executable filegroup has no executable to prefer, so expansion falls back to `files`. Collapsing it is not a build error: the script receives the runner and fails at extraction time.

It carries `manual` for the same reason `:WolfRelease` does. It depends on the transitioned bundle, so without the tag every wildcard build would rebuild the whole Rust, Swift and treesitter graph a second time at `-c opt`.

## The pure seam

Swift here is split in two, and the split is the single most important structural fact about the crate.

[`Sources/WolfKeyMap/`](../crates/wolf-macos/Sources/WolfKeyMap/README.md) is Foundation plus the generated FFI glue, with no AppKit. Every decision that can be expressed as a function of values lives there, so it can be unit-tested with no window server. [`Sources/WolfMacOS/`](../crates/wolf-macos/Sources/WolfMacOS/README.md) is the AppKit application, and holds only what genuinely needs a window. The process entry point sits apart, in the one-file [`Sources/WolfMain/`](../crates/wolf-macos/Sources/WolfMain/README.md).

The consequence is the crate's testing posture. There is no window server under `bazel test`, but a view in a never-shown window still renders through `cacheDisplay`, so [`Tests/WolfMacOSTests/`](../crates/wolf-macos/Tests/WolfMacOSTests/README.md) checks what a `WolfMacOS` view draws. **Key-window behaviour, event delivery such as `keyDown:`, menus and real focus are compile-verified and manual-smoke only**, and XCUITest is unavailable.

Coverage comes from the pure seams via [`Tests/WolfKeyMapTests/`](../crates/wolf-macos/Tests/WolfKeyMapTests/README.md), the bridge surface via the real-nvim tests in `//crates/wolf-ffi`, and the core logic via `//crates/wolf-core`. When you add behavior here, put the decision in `WolfKeyMap` and test it there; leaving it in a view class means only what it draws can be tested.

## Key translation lives in Rust

Swift sends a structured `FfiKeyEvent`; `wolf-core` owns nvim notation. Do not add notation-building to Swift.

## Threading

`wolf-core` runs tokio on its own threads and fires a wake on `flush` and on each terminal session's frame publish. The wake is the registered `extern "C"` callback (see [`wolf-ffi`](../crates/wolf-ffi/README.md)); `WakeBridge` hops it onto the AppKit main thread and hands it to the token's project, whose current view runs `onRenderWake` and repaints.

`WolfCoreHandle` is not `Send` and is used only on the main thread. Frame-stats state is main-thread-only too, enforced by a debug-only `dispatchPrecondition`.

## One Neovim per project

A `Project` is one captive Neovim shared by N views. Each view is a top-level `NSWindow` bound to one nvim tabpage; only the focused view live-updates, and the others hold their last painted frame until refocused.

Wakes route through a per-project `wakeToken` looked up in `WindowRegistry`, which fans the wake out to that project's current view. That indirection is why two projects never clobber each other's redraws. Closing a view runs `:tabclose` on its tabpage; the project's Neovim shuts down only when its last view closes.

Each view's working directory is its tabpage's `:tcd`, so it is per-view rather than per-process.

## State is a tree, and the front-end owns every write

Window frames, ordered tabs, each tab's working directory, sidebar visibility, and full pane-split layout persist to `state.json` and restore at launch before the windows are shown. The coordinator walks its live windows and tabs and commits the whole tree through the `wolf-ffi` snapshot builder. Core does not mirror the working directory into a store, and there is no per-window slot.

Every Alive core wake requests a commit, alongside the geometry, sidebar, cwd and tab-group triggers. Opening, splitting or closing a pane fires nothing else, and a force quit or crash skips the terminate commit, so without it those files are lost. The request is throttled rather than debounced, so continuous wakes cannot starve the commit, and committing an identical tree is a no-op, so the frequent commits cause no disk churn.

**Each view contributes only a tab it can read whole, and otherwise its cache.** Leaves are read by window handle from `previews` and `win_buffers`, which span every tabpage; `pane_status_for_grid` joins the current-tabpage `windows` map, so it would persist a background view's files as `""`. A view whose cwd or any leaf is not yet reported refuses, and contributes its last tab, which starts as the restored one. The launch commit runs before any pane binds, and a background view does not rebuild until it is visited, so a partial tab there would replace the saved files with nothing.

Two restored views never gather and go on contributing their seed. One whose rebuild failed shows a fallback pane rather than its saved split. One restored to the empty state because its saved folder was missing keeps that folder until the user opens another, so remounting a volume brings the session back.

**Closing the last window is quitting, so it commits the tree with that window in it.** The app terminates after its last window closes, and a commit after the close would find no live view. The commit writes the tree as it stood at the first close of that runloop turn, because a tabbed window's close button closes each tab's window in turn, and by the last close the earlier tabs are torn down. So the closing window keeps all its tabs, and a close-all keeps every window.

A tab closed alone ends its close burst on the next turn, so it drops, and a later close of the last tab keeps only that tab. That AppKit closes a tab group's windows within one turn is a premise no automated test measures. Any other close commits without the closing view, and a close during termination commits nothing over the terminate commit. A walk with no live view or no window commits nothing at all, because it would erase the session.

Saved tabs that no view took at launch, because a core, tabpage or view build failed, are never retried in that session. They are carried into every commit so the next launch tries them again.

Only directories the user actually opened persist. The passive launch working directory is deliberately not written and not recorded in the recent-directories list, which is what keeps Open Recent clean. A frame restores only when it still intersects a live screen, and a tab's working directory only when it is still a directory; an empty or deleted one restores to the empty state rather than falling back to the process working directory.

Project identity is not persisted. Restore groups the saved tree by working directory, so a project whose views moved to different directories is split, and same-directory tabs from different runtime instances are merged. Every restored project is uniform-directory.

## One flat pane view, not nested split views

`PaneContainerView` paints every pane itself, computing each pane's rectangle from the pure `PaneTree` port. It is deliberately not a tree of nested `NSSplitView`s. Divider drag is done by hit-testing the pure `dividers()` list inside the flat view.

Weight policy: a same-orientation split re-equalizes the row or column to equal shares, while removing a pane re-normalizes the survivors proportionally. That is what makes a user-resized layout survive an adjacent pane closing, a tab switch, and quit and relaunch.

Every pane's body paints identically whether active or inactive. The only active/inactive treatment is statusbar text weight and dimming, because that distinction is awkward to express inside Neovim itself.

## How a preview reaches its web view

Four files jointly honor one model, so do not change any of them without reading this section: `WolfKeyMap/PreviewScrollSeam` owns the rules, `PaneContainerView` the bookkeeping, `PreviewScrollStateHandler` the state channel, and `PreviewNavigationDelegate` the navigation count.

A rendered document reaches the page one of two ways, chosen per preview window by `PreviewScrollSeam.updateStrategy`. The typing path swaps only the body in place, which is what stops the inlined base64 font CSS in the head from being re-shipped across the WebKit boundary and re-parsed on every keystroke. Every other case navigates with `loadHTMLString`. The swap is sound only because the in-app render pins one theme and one width, so the head is byte-identical every time; the strategy compares head lengths so that premise is self-enforcing rather than a comment.

The in-place body travels as a real `callAsyncJavaScript` argument, never interpolated into JavaScript source, and it must run in the content world `addUserScript` installed the page script into. A wrong world returns the falsy sentinel on every call, pinning every window on the full-load path silently and with a green suite. That is the one failure here no test catches, so it is what the manual smoke check is for.

Scroll state is pushed out of the page, never pulled. A full load destroys `window`, so nothing can be stashed in the page across a reload, and `evaluateJavaScript` is asynchronous, so state cannot be read in the instant before one. A continuously refreshed Swift-side cache makes the pre-reload capture a plain dictionary read with no race.

Three script-message handlers ride on one preview web view's content controller — click-to-focus, scroll state, and selection — under three names that must differ, since a duplicate name silently replaces the handler already registered and kills that channel with no error. Every one of them must be removed at teardown, or the controller keeps the container alive.

Edit > Copy reads the pushed selection cache for the same reason the pre-reload capture reads the scroll cache: menu validation is synchronous and `evaluateJavaScript` is not. That cache is cleared synchronously on both document-replacement paths rather than trusting the page to report the collapse; a push landing in the frame or two before WebKit commits the replacement can still re-populate it, which is benign because such a push reads the live selection. The rules behind the channel and the copy-source decision are in [`PreviewCopySeam`](../crates/wolf-macos/Sources/WolfKeyMap/README.md).

Navigations are counted, not flagged. Two can straddle one page load, because the reload coalescer does not wait for `didFinish`, and WebKit reports the superseded one as a failure — so a failure callback is a routine part of the typing path, not an error path. A count lets a superseded callback decline to commit anything; a flag would let one clear the gate out from under a still-navigating load, which flickers the pane.

Recovery is one step and self-limiting. A failed in-place update clears that window's live-document descriptor and re-arms the reload gate, so the next pass takes the full path: at most one extra load, no loop, and normal service resumes as soon as one commits.

## Terminal panes

[Terminal panes](../specs/terminal-panes.md) owns the contract. These are the macOS choices behind it, in `PaneContainerView` and the seams it calls.

A terminal pane's content is painted by its own [`TerminalPaneView`](../crates/wolf-macos/Sources/WolfMacOS/README.md#terminalpaneviewswift) subview, for the reason that entry gives, while the container still paints its statusbar and scrollbar and never reads its placeholder grid.

Input methods reach a terminal pane through `NSTextInputClient`, and only a terminal pane: Neovim panes keep `KeyComposer` unchanged. A key goes through `interpretKeyEvents` unless it is a Ctrl, Cmd or option-as-alt chord or a named key, and a commit of one printable ASCII character is sent as a key, so core's `<C-w>` router still sees it. The rules are [`TerminalInputSeam`](../crates/wolf-macos/Sources/WolfKeyMap/README.md#terminalinputseamswift)'s.

Only the project's current view sizes a session, checked as `active_tabpage()` against the view's own tabpage, and within it [`TerminalGeometry.sizer`](../crates/wolf-macos/Sources/WolfKeyMap/README.md#terminalgeometryswift) picks the active pane, then the last sizer. Together they make the spec's most recently focused pane the sizer; without the view check, a background view would fight the current one.

The size takes `fullWidth(forWin:)` rather than `fullWidth(forGrid:)`, because the pane's grid may still be pending. A size is resent when it or the frame's size differs from what was sent, but an exited session applies no resize, so its frame size is never compared.

A session holds focus while its pane is the active pane of the key window, and `syncTerminalFocus` reports the loss before the gain. It runs on every active-pane change, every wake and both key-window edges, and app deactivation resigns the key window. The loss matters beyond mode 1004: core rescans the project on it, so a restart reports it too.

## Present limitations

- Only windows opened via the overridden split commands (`<C-w>s`/`v`, `:split`/`:vsplit`) become panes. Help, quickfix, preview, and `:diffsplit` windows do not, and floating or external windows are ignored.
- A visible non-current window of a project can blank during a live resize until it is refocused. `wantsLayer` mitigates this but does not eliminate it.
- The grid is capped at a hard-coded 90 columns and centered in a wider pane, with the margins painted in the grid's own background. Making the cap configurable is a possible enhancement.
- Scrollbar thumbs are non-interactive; thumb-drag is deferred.
- Markdown preview ships one theme. Cursor-follow anchors to the containing block's top inside a fenced code block, an HTML block or a table, and inside a paragraph hard-wrapped across several short source lines; an image's alt text collapses to a single anchor just after the image. The marker rules are in [the preview position-marker contract](wolf-core-decisions.md).
- In a terminal pane, the cursor and blinking text draw steady, and double, dotted and dashed underlines draw as the single straight line.
- Appearance flows one way only: colorscheme to app chrome works, OS theme to Neovim theming does not.
- A Neovim crash restart reopens each view's open files into one pane, so its splits collapse, and the next commit records that. Restoring the pane topology across a restart would change the restart model.
- Only what a pane shows persists: `state.json` carries buffer paths only as layout leaves, so a file the user switched away from in its pane is not reopened at launch.
- Per-pane navigation history is not persisted across launches, and is not cursor-position history — Neovim's own jumplist (`<C-o>` / `<C-i>`) is untouched. Navigating back to a file deleted from disk opens an empty `[New]` buffer, exactly as `:e missing.md` would.

## Frame-time instrumentation

The crate carries an off-by-default frame-time measurement surface for finding where main-thread time goes on the keystroke-to-pixel path. Recording a sample never changes what is painted.

`WOLF_FRAME_STATS=1` enables it, `WOLF_FRAME_STATS_INTERVAL` sets the report period, and `WOLF_FRAME_STATS_FILE` also appends each report to a file. With the flag unset the cost at each call site is one `Bool` test. See [Building Wolf](../build.md) for the terminal invocation — a double-clicked `.app` inherits `launchd`'s stderr and shows nothing.

One caveat when reading a report: `wakes/key` is a counter ratio, so it is an upper bound rather than real attribution. The per-bucket caveats, including what `prev.html` does and does not time, are in [`Sources/WolfKeyMap/README.md`](../crates/wolf-macos/Sources/WolfKeyMap/README.md).
