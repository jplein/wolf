# `crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Preview scratch grid

A preview pane is a second pane content kind: a `WKWebView` subview shows core-rendered HTML over the pane's grid rect, backed by a blank nvim scratch grid. That scratch grid is not painted while the web view fully and opaquely covers it; see `PaneOcclusion` and the `paintsGrid` parameter of `paintPane`.

Painting the scratch grid is not cheap, even though it looks empty. Nvim's blank cells hold a literal `" "`, not `""`, so every blank cell still runs a full `CTLineCreateWithAttributedString` and `CTLineDraw`, plus a per-cell Rust `String` clone across the FFI. Measured with `WOLF_FRAME_STATS=1`, the blank preview grid (`draw.prev`) costs about as much as a text-full editor grid (`draw.pane`), 22 ms each of a roughly 46 ms `draw`.

## Terminal restore

After a crash-restart, terminal sessions are still running but no window shows them. `Project.respawnNeovim` reads each view's terminals with `capturedTerminals()` before the restart, computes the steps with `LivenessSeam.terminalRestore` and hands them over through `requestTerminalRestore`. The view runs them through `queue_bind_terminal` and `toggle_terminal`, both input-pump events, and only from a render wake after `ensureInitialPane` has bound its first pane.

Nothing earlier or blocking is safe, for the two races [When Neovim dies](../../../../../specs/terminal-panes.md#when-neovim-dies) states. Here a blocking bind would overtake the files `Project.respawnNeovim` queued, and a bind before the first wake would change the window `pickInitialWindow` gives the first pane.
