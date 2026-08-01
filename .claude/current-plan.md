# Plan: Focus search field when invoking Search from the Command Palette

## Status: COMPLETE ✓ (uncommitted — no commit requested)

Fixed: Command-Palette "Search in project" now focuses the search field. Root cause = palette's `activateSelection()` dispatched the focus then `close()`→`dismissPanel()` clobbered it with `makeFirstResponder(paneContainer)`. Fix = `SearchView.focusSearchField()` defers its `makeFirstResponder` via `DispatchQueue.main.async` so it lands AFTER the synchronous dismissal-refocus. Reviewer CONFIRMED-CORRECT (async wins the race deterministically; no regression to menu/⇧⌘F/magnifier; helps the reveal-from-collapsed case too). Docs updated (2 macOS READMEs, prettier-clean). Green under bazelisk: app builds+signs, WolfKeyMapTests + swift_format PASS. Manual-smoke only for on-screen focus (no headless surface). Left uncommitted for review.

## Bug
Invoking "Search in project" from the Command Palette (⌘P) does NOT focus the search text field, unlike the other three paths (menu item, ⇧⌘F, magnifier button) which DO focus it.

## Root cause (confirmed by reading the code)
The palette path DOES call the focus, but it's immediately clobbered by the palette's dismissal:
- `CommandPaletteView.activateSelection()` (`CommandPaletteView.swift:274-281`) runs, IN ORDER:
  1. `onActivate?(row)` (L279) → `WolfWindowController.activatePaletteRow` → `CommandDispatcher.dispatch(.searchInProject, …)` → `controller.showSidebar(panel: .search)` → `SearchView.focusSearchField()` → `window?.makeFirstResponder(searchField)` (sets the search field as first responder).
  2. `close()` (L280) → `PickerPanelHost.dismissPanel()` (`PickerPanelHost.swift:107-112`) which ends with `parentWindow?.makeFirstResponder(paneContainer)` (L112) — UNCONDITIONALLY restoring the editor pane as first responder, OVERWRITING the search-field focus from step 1.
- Net: search-field focus is set, then clobbered ~immediately. The other three paths have no such trailing refocus, so they work.

(Also relevant: the palette panel is key during step 1; dismissPanel deliberately does NOT `makeKeyAndOrderFront` — it relies on the OS to make the parent key next — and restores `paneContainer` as the parent's first responder for the common case.)

## Fix (minimal, robust, localized)
Make `SearchView.focusSearchField()` set the first responder on the NEXT main-runloop tick (`DispatchQueue.main.async`) instead of synchronously. Then, in the palette path, the focus runs AFTER `activateSelection()` completes (including `close()`/`dismissPanel()`'s synchronous `makeFirstResponder(paneContainer)`), so the search field wins. For the other three paths there is no competing refocus, so deferring one tick is harmless (and actually more robust for the reveal-from-collapsed case the earlier reviewer flagged).

Sequence after the fix (palette path):
1. `onActivate` → dispatch → `showSidebar(panel:.search)` → `focusSearchField()` SCHEDULES an async block `[weak self] window?.makeFirstResponder(searchField)`.
2. `close()` → `dismissPanel()` → sync `makeFirstResponder(paneContainer)`; palette orders out; parent becomes key.
3. Next tick: async block runs → `makeFirstResponder(searchField)` on the now-key parent window → search field focused. ✓

### Change
`crates/wolf-macos/Sources/WolfMacOS/SearchView.swift` — `func focusSearchField()`:
```swift
func focusSearchField() {
  DispatchQueue.main.async { [weak self] in
    guard let self else { return }
    self.window?.makeFirstResponder(self.searchField)
  }
}
```
Update its doc comment to explain WHY it defers (so a subsequent palette-dismissal `makeFirstResponder(paneContainer)` cannot clobber it, and it works uniformly for menu / ⇧⌘F / magnifier / palette). No other call sites change — `showSidebar(panel:.search)` and `SidebarContainerView.showSearch()` keep calling `focusSearchField()`.

## Alternatives considered (rejected)
- Reorder `activateSelection()` to `close()` before `onActivate` — changes a SHARED palette path for ALL commands; higher risk.
- Special-case `.searchInProject` in `dismissPanel`/`activatePaletteRow` — `dismissPanel` is generic (all pickers) and restoring `paneContainer` is the right default; threading a special case is messier than deferring the focus.

## Test plan
No new headless tests (first-responder timing is imperative AppKit / window-server, manual-smoke only, consistent with the rest of SearchView). Existing suites must stay green.

## Verification
- `bazelisk build //crates/wolf-macos:Wolf` (compiles + signs).
- `bazelisk test //crates/wolf-macos:WolfKeyMapTests //tools/lint:swift_format` (green).
- Manual smoke: ⌘P → "Search in project" → caret lands in the search field; re-confirm menu item, ⇧⌘F, and magnifier still focus it.

## Docs
Minor: if the macOS READMEs describe `focusSearchField()`'s behavior, note it now defers (async) so palette dismissal can't clobber it. Otherwise inline doc comment suffices.

## Progress log
(updated as agents report)
