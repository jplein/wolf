# wolf-macos decisions

The durable constraints behind this crate. Read this before changing anything here; most of it exists because a reasonable-looking simplification breaks something. Per-file detail lives in the directory READMEs under [`Sources/`](Sources/README.md).

## Platform and build

- Tahoe (macOS 26) and newer only (`LSMinimumSystemVersion` / `minimum_os_version` = `26.0`).
- AppKit, not SwiftUI. Implementation language Swift, Xcode 26+.
- The Bazel build is authoritative: `rules_swift` and `rules_apple` build the `.app`. `rules_xcodeproj` generates an uncommitted `.xcodeproj` for debugging and LLDB only.
- Rust and Swift meet through swift-bridge, confined to [`wolf-ffi`](../wolf-ffi/README.md). The generated bindings are committed.

### Why `:WolfRelease` is a custom rule

`release.bzl` defines the `opt_bundle` rule that `:WolfRelease` instantiates. Compilation mode is a Bazel configuration rather than an attribute, so no `alias` or `macos_application` wiring can set it; `opt_bundle` applies a Starlark outgoing-edge transition to its single `bundle` dep, setting `compilation_mode` to `opt` so the whole subgraph rebuilds optimized without `-c opt` on the command line.

It forwards only `DefaultInfo`, deliberately not the Apple providers, because nothing consumes them here and forwarding Apple providers from a non-Apple rule invites `rules_apple` to treat the target as an embeddable bundle.

The rule is executable: it symlinks the transitioned `macos_application`'s own runner into its predeclared output and forwards the dep's runfiles, which is how that runner locates `Wolf.zip`. The runner's app path is a bare working-directory-relative runfiles path, so dropping the runfiles merge — or passing `--nobuild_runfile_links` or `--enable_runfiles=false` — breaks it.

For frame-stats measurement, exec `"$APP/Contents/MacOS/Wolf"` directly rather than using `bazel run`, which unzips a temporary copy to a path that changes every launch and interleaves the app's stderr with Bazel's client output.

## The pure seam

Swift here is split in two, and the split is the single most important structural fact about the crate.

[`Sources/WolfKeyMap/`](Sources/WolfKeyMap/README.md) is Foundation plus the generated FFI glue, with no AppKit. Every decision that can be expressed as a function of values lives there, so it can be unit-tested with no window server. [`Sources/WolfMacOS/`](Sources/WolfMacOS/README.md) is the AppKit application, and holds only what genuinely needs a window.

The consequence is the crate's testing posture: **AppKit glue is compile-verified and manual-smoke only** — there is no headless window server, so `draw(_:)`, `keyDown:`, view hierarchies, and menu wiring are not covered by an automated test. Coverage comes from three other places instead: the pure seams via [`Tests/WolfKeyMapTests/`](Tests/WolfKeyMapTests/README.md), the bridge surface via the real-nvim tests in `//crates/wolf-ffi`, and the core logic via `//crates/wolf-core`. When you add behavior here, put the decision in `WolfKeyMap` and test it there; leaving it in a view class means it is untested.

## Key translation lives in Rust

Swift sends a structured `FfiKeyEvent`; `wolf-core` owns nvim notation. Do not add notation-building to Swift.

## Threading

`wolf-core` runs tokio on its own threads and fires a wake on `flush`. The wake is the registered `extern "C"` callback (see [`wolf-ffi`](../wolf-ffi/README.md)); `WakeBridge` hops it onto the AppKit main thread and calls `setNeedsDisplay`, which triggers a `draw(_:)` that reads the current frame.

`WolfCoreHandle` is not `Send` and is used only on the main thread. Frame-stats state is main-thread-only too, enforced by a debug-only `dispatchPrecondition`.

## One Neovim per project

A `Project` is one captive Neovim shared by N views. Each view is a top-level `NSWindow` bound to one nvim tabpage; only the focused view live-updates, and the others hold their last painted frame until refocused.

Wakes route through a per-project `wakeToken` looked up in `WindowRegistry`, which fans the wake out to that project's current view. That indirection is why two projects never clobber each other's redraws. Closing a view runs `:tabclose` on its tabpage; the project's Neovim shuts down only when its last view closes.

Each view's working directory is its tabpage's `:tcd`, so it is per-view rather than per-process.

## State is a tree, and the front-end owns every write

Window frames, ordered tabs, each tab's working directory, sidebar visibility, and full pane-split layout persist to `state.json` and restore at launch before the windows are shown. The coordinator walks its live windows and tabs and commits the whole tree through the `wolf-ffi` snapshot builder. Core does not mirror the working directory into a store, and there is no per-window slot.

Committing an identical tree is a no-op, so redundant snapshots cause no disk churn.

Only directories the user actually opened persist. The passive launch working directory is deliberately not written and not recorded in the recent-directories list, which is what keeps Open Recent clean. A frame restores only when it still intersects a live screen, and a tab's working directory only when it is still a directory; an empty or deleted one restores to the empty state rather than falling back to the process working directory.

Project identity is not persisted. Restore groups the saved tree by working directory, so a project whose views moved to different directories is split, and same-directory tabs from different runtime instances are merged. Every restored project is uniform-directory.

## One flat pane view, not nested split views

`PaneContainerView` paints every pane itself, computing each pane's rectangle from the pure `PaneTree` port. It is deliberately not a tree of nested `NSSplitView`s. Divider drag is done by hit-testing the pure `dividers()` list inside the flat view.

Weight policy: a same-orientation split re-equalizes the row or column to equal shares, while removing a pane re-normalizes the survivors proportionally. That is what makes a user-resized layout survive an adjacent pane closing, a tab switch, and quit and relaunch.

Every pane's body paints identically whether active or inactive. The only active/inactive treatment is statusbar text weight and dimming, because that distinction is awkward to express inside Neovim itself.

## Present limitations

- Only windows opened via the overridden split commands (`<C-w>s`/`v`, `:split`/`:vsplit`) become panes. Help, quickfix, preview, and `:diffsplit` windows do not, and floating or external windows are ignored.
- A visible non-current window of a project can blank during a live resize until it is refocused. `wantsLayer` mitigates this but does not eliminate it.
- The grid is capped at a hard-coded 90 columns and centered in a wider pane, with the margins painted in the grid's own background. Making the cap configurable is a possible enhancement.
- Scrollbar thumbs are non-interactive; thumb-drag is deferred.
- Undercurl paints as a straight underline.
- Markdown preview ships one theme, and cursor-follow scroll-sync is unbuilt — the emitted HTML already carries the position markers, but the Neovim-to-web-view cursor signal and the scrolling JavaScript are not written.
- Appearance flows one way only: colorscheme to app chrome works, OS theme to Neovim theming does not.
- Per-pane navigation history is not persisted across launches, and is not cursor-position history — Neovim's own jumplist (`<C-o>` / `<C-i>`) is untouched. Navigating back to a file deleted from disk opens an empty `[New]` buffer, exactly as `:e missing.md` would.

## Frame-time instrumentation

The crate carries an off-by-default frame-time measurement surface for finding where main-thread time goes on the keystroke-to-pixel path. Recording a sample never changes what is painted.

`WOLF_FRAME_STATS=1` enables it, `WOLF_FRAME_STATS_INTERVAL` sets the report period, and `WOLF_FRAME_STATS_FILE` also appends each report to a file. With the flag unset the cost at each call site is one `Bool` test. See [Building Wolf](../../build.md) for the terminal invocation — a double-clicked `.app` inherits `launchd`'s stderr and shows nothing.

Two caveats when reading a report: the `prev.html` bucket times only the synchronous `loadHTMLString` call, not the async WebKit parse, layout, and paint it queues; and `wakes/key` is a counter ratio, so it is an upper bound rather than real attribution.
