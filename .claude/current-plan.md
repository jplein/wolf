# Current Plan: Move pickers into separate floating windows (NSPanel)

## Status: CODE COMPLETE & GREEN — invoking reviewer

### Coder result (agent aaabb2593a95cd454)
`bazel build //crates/wolf-macos:Wolf` → Build completed successfully.
`bazel test //crates/wolf-macos:WolfKeyMapTests` → PASSED (1/1). Only pre-existing SwiftBridgeCore
warnings. All plan files created/modified; docs updated + prettier-conformant.

Two ratified deviations:
- **§2c height measurement**: `systemLayoutSizeFitting(_:withHorizontalFittingPriority:verticalFittingPriority:)`
  is a UIKit `UIView` API and does NOT exist on AppKit `NSView` (build failed on it). Coder substituted
  the AppKit equivalent in `refitPanel()`: a temporary `widthAnchor == 600` constraint +
  `layoutSubtreeIfNeeded()` + `fittingSize.height`, then deactivate. ACCEPTED — correct fix; the
  picker's vertical constraint chain is a determinate equality chain so the height is exact.
- **§6 test-src ordering**: placed alphabetically (PickerPanel < PickerRefresh) since §6 names
  "alphabetical" as governing; Bazel src order is not semantically significant. ACCEPTED.

## Status: PLAN APPROVED — test-writer done

## Orchestrator rulings on the architect's open questions (BINDING)

- **OD1 — Parent titlebar dimming while a picker panel is key: ACCEPT as-is** (non-activating panel,
  no mitigation). Spotlight/Alfred-idiomatic, transient, and avoids brittle non-key-panel event
  forwarding. Document as a known cosmetic tradeoff.
- **OD2 — The now-unused `paneContainer` stored property on each view: KEEP the init parameter**
  (minimal diff; `WolfWindowController` already passes it). The HOST owns the refocus/positioning
  `paneContainer`. The coder MAY delete the view's now-dead stored `paneContainer` field if trivial,
  but this is non-blocking — do not let it expand the diff or risk a compile break.

Both defaults are ratified. No user consult needed.

---

## Problem statement

Move each of the three transient pickers out of the `PaneContainerView` drawing surface and into its
own floating window (NSPanel) hosted in front of the owning main window, so it composites ABOVE the
Markdown-preview `WKWebView` instead of behind it.

The three pickers: file picker = `PickerView`, command palette = `CommandPaletteView`, open-buffer
selector = `BufferPickerView`.

### Why

The preview `WKWebView` (a subview of `PaneContainerView`) composites above sibling `NSView`s on
Tahoe regardless of subview order. The prior workaround (hide the preview whenever any overlay opens)
has been reverted by the user. The correct fix takes the pickers off the window's drawing surface
entirely by hosting each in a separate child window/panel, which the window server always composites
in front of the parent window's content (including the `WKWebView`).

### Required behavior (all four must hold for all three pickers)

1. Each picker is its own separate window (not painted into the pane container's drawing surface).
2. The picker window is drawn in FRONT of its owning main window, in a similar position to now
   (top-center, ~24pt below the top, 600pt wide, content-height-driven).
3. Clicking anywhere OUTSIDE the picker dismisses it (same effect as pressing Esc / calling `close()`).
4. Activating another window (Cmd-Tab, Mission Control, clicking another app/window) dismisses it
   (same as Esc).

---

## Architect Plan (authoritative — implement against this + the OD1/OD2 rulings)

### 1. Summary

Host each picker in its own borderless, non-activating child `NSPanel` added via
`addChildWindow(_:ordered:.above)`. The window server composites a child window in front of its
parent's content (including the `WKWebView`). The panel becomes key so the search field's field
editor keeps working; both "click outside" and "another window/app activated" resolve to the panel
resigning key, which drives the existing `close()`. Positioning is extracted into a pure
`WolfKeyMap.PickerPanelSeam` and unit-tested; all AppKit window glue stays compile + manual-smoke.

Key decisions: ONE reusable host (`PickerPanelHost` + `PickerPanel: NSPanel` + `PickerPanelHosting`
protocol), non-activating borderless key panel (accept brief parent titlebar dimming),
`addChildWindow(ordered:.above)` for front-of-parent compositing + auto-follow on parent move, and
content-driven sizing via `systemLayoutSizeFitting` at a fixed 600pt width.

### 2. Files to CREATE

**2a. `crates/wolf-macos/Sources/WolfKeyMap/PickerPanelSeam.swift`** (pure seam, Foundation-only)

Reuse the existing `Rect` value type from `PaneTree.swift` (top-left origin, `Double` fields).

```
public enum PickerPanelSeam {
    public static func frame(
        parentContentWidth: Double,
        parentContentHeight: Double,
        panelWidth: Double,
        panelHeight: Double,
        topMargin: Double
    ) -> Rect
}
```

Semantics (returns the panel frame in parent CONTENT space, TOP-LEFT origin):
- `w = min(panelWidth, max(0, parentContentWidth))`
- `x = max(0, (parentContentWidth - w) / 2)` — centered, never negative
- `y = topMargin` — FIXED offset from content top, NOT clamped (mirrors `EditorInset` convention)
- `available = max(0, parentContentHeight - topMargin)`
- `h = min(panelHeight, available)` — clamps a panel taller than the content below the margin

Doc comment: only `w`/`h` clamp; `x`/`y` are fixed/derived; `x`/`w` center horizontally; `h` caps at
`parentContentHeight - topMargin`.

**2b. `crates/wolf-macos/Sources/WolfMacOS/PickerPanel.swift`** (AppKit glue)

`final class PickerPanel: NSPanel` — the reusable host window. Only reason for the subclass is key
eligibility for a borderless panel:
- `override var canBecomeKey: Bool { true }`
- `override var canBecomeMain: Bool { false }`
- `@available(*, unavailable) required init?(coder:)` per repo convention.
- An `init` that sets `styleMask = [.borderless, .nonactivatingPanel]`, `backing: .buffered`,
  `defer: false`; then `isFloatingPanel = true`, `becomesKeyOnlyIfNeeded = false`,
  `hidesOnDeactivate = false`, `isOpaque = false`, `backgroundColor = .clear`, `hasShadow = true`.

**2c. `crates/wolf-macos/Sources/WolfMacOS/PickerPanelHost.swift`** (AppKit glue) — one per picker.

```
protocol PickerPanelHosting: AnyObject {
    func presentPanel()    // order the panel in front of the parent + make it key
    func dismissPanel()    // order it out + return first responder to the editor
    func refitPanel()      // resize to the view's fitting height + reposition top-center
}
```

`final class PickerPanelHost: NSObject, PickerPanelHosting, NSWindowDelegate`:
- Stored: `private let panel: PickerPanel`; `private weak var parentWindow: NSWindow?`;
  `private weak var paneContainer: PaneContainerView?`; `private let contentView: NSView`;
  `private let onResignKey: () -> Void`; `private var isPresented = false`.
- `init(parentWindow:paneContainer:contentView:onResignKey:)` — builds the panel, sets
  `panel.contentView = contentView`, `panel.delegate = self`.
- `presentPanel()`: guard `!isPresented`; `isPresented = true`;
  `parentWindow?.addChildWindow(panel, ordered: .above)`; `refitPanel()`; `panel.makeKeyAndOrderFront(nil)`;
  register `NSWindow.didResizeNotification` observer on `parentWindow` → `refitPanel()`.
- `dismissPanel()`: guard `isPresented`; `isPresented = false`; remove the resize observer;
  `parentWindow?.removeChildWindow(panel)`; `panel.orderOut(nil)`;
  `parentWindow?.makeFirstResponder(paneContainer)`. Deliberately NO `makeKeyAndOrderFront`/
  `NSApp.activate` (correct for Cmd-Tab dismissal — do not yank the user back).
- `refitPanel()`: guard `isPresented` + `parentWindow` + `paneContainer`. Fixed `width = 600`;
  `h = contentView.systemLayoutSizeFitting(NSSize(width: 600, height: 0), withHorizontalFittingPriority: .required, verticalFittingPriority: .fittingSizeLevel).height`;
  `contentInWindow = paneContainer.convert(paneContainer.bounds, to: nil)`;
  `contentInScreen = parentWindow.convertToScreen(contentInWindow)`;
  call `PickerPanelSeam.frame(...)`; map top-left content rect to screen (bottom-left) frame:
  `originX = contentInScreen.minX + r.x`, `originY = contentInScreen.maxY - r.y - r.h`;
  `panel.setFrame(NSRect(x: originX, y: originY, width: r.w, height: r.h), display: true)`.
- `func windowDidResignKey(_:)`: `guard isPresented else { return }; onResignKey()` — the single
  dismissal funnel for reqs 3 & 4.

`topMargin` (24) and width (600) are `private static let` on `PickerPanelHost`.

**2d. `crates/wolf-macos/Tests/WolfKeyMapTests/PickerPanelSeamTests.swift`** — pure XCTest over
`PickerPanelSeam.frame`, in the `PaneGeometryTests`/`EditorInsetTests` style (eps/approx/assertRect).

### 3. Files to MODIFY — the three picker views

Identical structural edits to **`PickerView.swift`, `CommandPaletteView.swift`,
`BufferPickerView.swift`**. Their search/table/`NSTextFieldDelegate` logic, `refresh()`/
`moveSelection`/`activateSelection`/`updateResultsLayout` internals are UNCHANGED except as noted.

1. Add `weak var host: PickerPanelHosting?` (set by `WolfWindowController` after construction).
2. **Remove `override func viewDidMoveToSuperview()`** entirely (the centerX/top/width pinning).
3. **Remove `isHidden = true` from `init`** (visibility now driven by panel order-in/out). Keep
   `translatesAutoresizingMaskIntoConstraints = false`, `wantsLayer`, `cornerRadius`/`border`/
   `backgroundColor`.
4. **`open()`**: remove `isHidden = false`; before `window?.makeFirstResponder(searchField)` call
   `host?.presentPanel()` (present first so `self.window` is the now-key panel, then focus the field).
   Keep everything else.
5. **`close()`**: remove `isHidden = true`; replace the `if let paneContainer { window?.makeFirstResponder(paneContainer) }`
   block with `host?.dismissPanel()`. Keep the `isOpen` guard and tear down session/coalescer BEFORE
   the dismiss call (so a re-entrant `windowDidResignKey → close()` sees `isOpen == false` and no-ops).
6. **`updateResultsLayout()`**: append `host?.refitPanel()` as the last statement so the panel grows/
   shrinks with the results list (capped by the existing `maxListHeight`).

Per OD2: keep the `paneContainer` init parameter; the view's now-dead stored `paneContainer` field
MAY be deleted if trivial (non-blocking).

### 4. Files to MODIFY — `WolfWindowController.swift`

In `build()` (~lines 221–247):
- **`EmptyStateView` stays a `paneContainer.addSubview` subview — UNCHANGED.**
- For each of `picker`/`palette`/`bufferPicker`: construct the view as today but **remove the
  `paneContainer.addSubview(...)` line**. After construction:
  - `let pickerHost = PickerPanelHost(parentWindow: window, paneContainer: paneContainer, contentView: picker, onResignKey: { [weak picker] in picker?.close() })`
  - `picker.host = pickerHost`; store the host strongly on the controller.
- Keep `paneContainer.onPickerTrigger`, `palette.onActivate`, `bufferPicker.onActivate`, and
  `self.picker/palette/bufferPicker = ...` (the `picker` reference MUST be preserved for
  `Project.onPickerWake → picker?.scheduleRefresh()`).
- The window exists before this block (~line 207), so `parentWindow: window` is valid.

Add three strong stored properties: `private var pickerHost/paletteHost/bufferPickerHost: PickerPanelHost?`.

In `teardown()`: after the existing `picker?.close(); picker = nil` (etc.), null the hosts
(`pickerHost = nil; paletteHost = nil; bufferPickerHost = nil`). `close()` already dismisses each
panel, so no dangling child windows survive.

`openFilePicker`/`openCommandPalette`/`openBufferPicker` unchanged.

### 5. Files to MODIFY — `AppDelegate.swift` (key-window resolution) — REQUIRED regression fix

While a picker panel is key, `NSApp.keyWindow` is the panel, so `controllers.first { $0.owns(NSApp.keyWindow) }`
lookups fail. Each panel is a child window, so `panel.parent` is the `MainWindow`. Add:
- `private var effectiveKeyWindow: NSWindow? { let key = NSApp.keyWindow; return key?.parent ?? key }`

Replace `NSApp.keyWindow` with `effectiveKeyWindow` in the key-window lookup sites (coder to locate
the actual sites; architect named `handleCommandMenuItem`, `validateMenuItem`, `newWindowForTab`,
`menuNeedsUpdate`, `openRecentMenuItem`). Do NOT change `snapshotNow`/`refreshTabTitles`/`otherWindows`
(they walk `controllers`/`tabGroup.windows`; child panels are never in a tab group nor the array).

### 6. Files to MODIFY — `BUILD.bazel`

- Add `"Sources/WolfKeyMap/PickerPanelSeam.swift"` to `wolf_key_map` `srcs` (alphabetical: after
  `PickerRefreshCoalescer.swift`, before `PickerSeam.swift`).
- Add `"Tests/WolfKeyMapTests/PickerPanelSeamTests.swift"` to `WolfKeyMapTests` `srcs` (after
  `PickerRefreshCoalescerTests.swift`, before `PickerSeamTests.swift`).
- No change for the two new `Sources/WolfMacOS/*.swift` files — `wolf_macos_lib` globs them.

### 7. Test plan

**Unit-testable — `PickerPanelSeamTests.swift`.** Cases:
1. Exact top-center: (1200, 800, 600, 300, 24) → `Rect(x: 300, y: 24, w: 600, h: 300)`.
2. Centering, different width: (1000, …, 600, …) → `x == 200`; invariant `x + w/2 == parentContentWidth/2`.
3. topMargin respected: `y == topMargin` (test 24 and 0).
4. Height clamps when panel taller than content-below-margin: (…, 500, …, 900, 24) → `h == 476`, `y == 24`.
5. Width clamps + x floors at 0: (400, …, 600, …) → `w == 400`, `x == 0`.
6. Exact-fit boundary: (…, 324, …, 300, 24) → `h == 300` (available == 300, no clamp).
7. Degenerate content shorter than margin: (…, 10, …, …, 24) → `available == 0` → `h == 0`, `y == 24`.
8. Never-negative floors: `x >= 0` and `h >= 0` across the above.

**NOT unit-testable — manual smoke only** (window-server bound): panel present/dismiss,
`canBecomeKey`/field-editor typing, `addChildWindow(ordered:.above)` front-of-`WKWebView` compositing,
`windowDidResignKey`-driven dismissal, `refitPanel` sizing, reposition-on-parent-resize.

### 8. Docs to update

- `crates/wolf-macos/README.md`: add `PickerPanelSeam` to the `WolfKeyMap` seam list + `PickerPanelSeamTests`
  to the test roll-up; note the pickers are now hosted in borderless non-activating child `NSPanel`s
  (front-of-parent via `addChildWindow(ordered:.above)`).
- `crates/wolf-macos/Sources/README.md`: update the three picker-view entries (drop "subview of the
  pane container / pinned via `viewDidMoveToSuperview` / toggled via `isHidden`"; state panel hosting
  + `PickerPanelSeam` positioning + resign-key dismissal); update the `PaneContainerView.swift` entry
  (only `EmptyStateView` remains a paneContainer overlay); add entries for `PickerPanel.swift`,
  `PickerPanelHost.swift` (+ `PickerPanelHosting`).
- `crates/wolf-macos/Tests/README.md`: add a `PickerPanelSeamTests.swift` section + roll-up mention.
- `architecture.md`: lightly revise the front-end picker paragraphs (~lines 69/71/73) — each picker is
  now its own borderless non-activating child `NSPanel` composited IN FRONT of the editor window
  (renders above the preview `WKWebView`), key so the field types, dismissed on resign-key.
- Per `CLAUDE.md`: run `prettier --write` on every edited Markdown file; no hard line breaks within
  paragraphs.

### 9. Manual-smoke checklist (run for EACH picker: ⌘O, ⌘P, ⌘E)

1. Open position: top-center over the editor content, ~24pt below titlebar, 600pt wide, height fitting
   the empty search field then growing with results (capped, then scrolls).
2. **Front-of-preview** (core requirement): open a Markdown file, View → Preview so the `WKWebView`
   shows; trigger the picker over the preview pane and confirm it renders ABOVE the web view.
3. Type/filter: typing filters; Up/Down move selection; the panel resizes; the field keeps focus/caret.
4. Esc closes: dismisses and returns first responder to the editor (keys reach Neovim).
5. Click-outside closes: click anywhere in the parent (editor/sidebar/titlebar) or another app →
   dismisses; editor regains focus.
6. Another-window/app closes: Cmd-Tab to another app (and separately Mission Control / a second Wolf
   window) → dismisses; the app is NOT force-reactivated.
7. Activate selection returns focus: Enter (open file / run row / switch buffer) → dismisses, editor focus.
8. Idempotency/teardown: ⌘W the parent window while a picker is open → no crash, no orphaned panel.
9. Parent-resize reposition: open a picker, resize the main window → re-centers top-center.
10. File-picker wake path: in a large repo, ⌘O and confirm results stream in (`scheduleRefresh()` wake
    path still refreshes) and the panel keeps refitting.
11. Menu while open: with a picker open, open a menu (View) and confirm command enable/disable +
    invocation still target the parent window (validates `effectiveKeyWindow`).

Accepted cosmetic tradeoff (verify tolerable, not a bug): while the panel is key, the parent titlebar/
statusbar draw dimmed (Spotlight-style). This is the chosen default (OD1).

### 10. Implementation order (avoids compile breaks)

1. Add `PickerPanelSeam.swift` + register in `wolf_key_map` srcs; add `PickerPanelSeamTests.swift` +
   register in `WolfKeyMapTests`.
2. Add `PickerPanel.swift` + `PickerPanelHost.swift` (incl. `PickerPanelHosting`). Host references
   only the protocol + `PickerPanelSeam` + AppKit — compiles before the views are edited.
3. Edit the three picker views (§3).
4. Edit `WolfWindowController.build()`/`teardown()` (§4).
5. Edit `AppDelegate` (§5).
6. Update docs (§8) + `prettier --write`.

### Load-bearing correctness note

In `close()`, set `isOpen = false` and tear down session/coalescer BEFORE `host?.dismissPanel()`,
because `orderOut` inside `dismissPanel` can re-enter `windowDidResignKey → close()`; the `isOpen`/
`isPresented` guards make that re-entry a no-op. The `onResignKey` closure must capture the view
WEAKLY (`[weak picker]`) — the view is the panel's `contentView` (strong), the panel is strong-held
by the host, the host is strong-held by the controller.

### Lint / convention

- Both new `WolfMacOS` classes need `@available(*, unavailable) required init?(coder:) { fatalError(...) }`.
- Keep closures capturing the picker weakly to avoid retain cycles.

### Build / test commands (coder runs)

- `bazel build //crates/wolf-macos:Wolf`
- `bazel test //crates/wolf-macos:WolfKeyMapTests`

## Progress log

- (init) Problem statement written; architect invoked.
- Architect plan complete (agent af12c83b18a7d7bcb). OD1/OD2 ruled on (accept titlebar dimming; keep
  the init param). Invoking test-writer.
