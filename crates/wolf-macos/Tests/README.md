# wolf-macos/Tests

- [`WolfKeyMapTests/`](WolfKeyMapTests/README.md) — XCTest coverage of the pure [`WolfKeyMap`](../Sources/WolfKeyMap/README.md) seam.
- [`WolfMacOSTests/`](WolfMacOSTests/README.md) — XCTest coverage of what [`WolfMacOS`](../Sources/WolfMacOS/README.md) views draw, rendered offscreen, and the reusable renderer it uses, plus key equivalents over a preview.

See the testing posture in [wolf-macos-decisions.md](../../../notes/wolf-macos-decisions.md) for what neither covers.

## Unproven on screen

Close Window's and Quit's checks are tested through their [seams](WolfKeyMapTests/README.md#windowcloseseamtestsswift) alone. The [app-launch smoke](../../../build.md#the-app-launch-smoke) quits clean tabs only, and no test clicks a window's buttons or sees a refusal on screen, so these are a person's checks ([why](../../../notes/wolf-macos-decisions.md#closing-a-window-and-quitting-ask-every-tab)):

- **Shift-Cmd-W on a two-tab window.** With the background tab modified, it selects that tab, shows E37 and closes nothing. With both tabs clean it closes both, and as the last window a relaunch restores both tabs.
- **The red close button on a two-tab window**, which checks that AppKit asks every window of the group. It behaves as Shift-Cmd-W does. A tab's own close button closes that tab alone, after the settle, and refuses when it is modified.
- **Cmd-Q with a modified buffer** in a background or minimized window. That window is raised, the tab selected and E37 shown, and nothing quits; after `:w` it quits. The Dock's Quit, and a logout that Wolf cancels, behave the same.
