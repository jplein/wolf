# Plan: Fix stale picker chrome on runtime appearance change

## Status: COMPLETE ✅

## Finalization
Technical-writer added a concise per-view note (re-applies layer chrome on `viewDidChangeEffectiveAppearance` via `applyChromeColors()` resolving against current `effectiveAppearance`) to PickerView/CommandPaletteView/BufferPickerView/EmptyStateView bullets in both `crates/wolf-macos/Sources/README.md` and `crates/wolf-macos/README.md`. Orchestrator ran `prettier --write` → both unchanged (`proseWrap: never` conformant). Technical-writer agent id: acdba1034a44844e8.

## Status history: DOCS (all 4 files fixed + reviewer-APPROVED)

## EmptyStateView follow-up: coder done + reviewer APPROVED
Same pattern applied to `EmptyStateView.swift`: single frozen CGColor (`layer?.backgroundColor = .windowBackgroundColor.cgColor`) moved into `applyChromeColors()` (line 88) inside `performAsCurrentDrawingAppearance`; called from init (line 41, after `wantsLayer` line 38) + new `viewDidChangeEffectiveAppearance()` override (76-79). No subviews need redraw (only an `openButton` NSButton, self-adapts). Build `//crates/wolf-macos:Wolf` + `WolfKeyMapTests` PASS. Coder agent ab00123fc8a94c648; reviewer agent ad51304e6d27dbb47 (all 6 checks pass).

## Status history: FOLLOW-UP FIX (reviewer APPROVED the 3 overlays; extending to EmptyStateView)

## Reviewer verdict: APPROVED (3 overlays)
All 7 checks pass with citations: 3 CGColor assignments moved into `applyChromeColors()` (PickerView:193-195, CommandPalette:184-186, BufferPicker:184-186); all `.cgColor` conversions INSIDE `performAsCurrentDrawingAppearance` closure (the critical item — no stale resolution); override calls super→applyChromeColors→needsDisplay; called at end of init after layers exist (sound ordering, first paint correct); structurally identical across all three; no other stale CGColors in these views; early-call safe (non-lazy stored props, optional-chained layer access, no force-unwrap). Reviewer agent id: a37dc2a2f51afdf75.

## Orchestrator ruling: INCLUDE EmptyStateView.swift (reviewer's out-of-scope finding)
`EmptyStateView.swift:39` has the SAME frozen-CGColor bug: `layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor` set once, no `viewDidChangeEffectiveAppearance` override → empty-state background goes stale on the same runtime appearance switch. Directly serves the user's intent (UI follows appearance changes). Trivial identical fix → folding in now rather than leaving a known instance. Routed back to coder.

## Status history: REVIEW (coder done, all built + tests green)

## Coder results (complete)
- Fixed all three files identically: extracted the 3 frozen CGColor layer assignments (panel bg `.windowBackgroundColor`, panel border `.separatorColor`, `searchContainer` bg `.controlBackgroundColor`) into `private func applyChromeColors()`; call it from `init` at the end (after `buildSubviews()`, since `searchContainer.wantsLayer` is set in `buildSearchBar()` — layer must exist first); added `override func viewDidChangeEffectiveAppearance()` that calls `super`, `applyChromeColors()`, then `searchField.needsDisplay = true` + `searchContainer.needsDisplay = true`.
- Appearance-resolution API: `effectiveAppearance.performAsCurrentDrawingAppearance { ... }` (macOS 11+). Deployment target `minimum_os_version = "26.0"` (wolf-macos/BUILD.bazel:119) → modern API used, no NSAppearance.current fallback needed.
- NO other frozen CGColors found. `magnifierView.contentTintColor = .secondaryLabelColor` is dynamic NSColor (left). `searchField` has no explicit textColor (inherits `.labelColor`; needsDisplay handles it). cornerRadius/borderWidth are appearance-independent (left in place).
- Property names identical across all three: `searchField` (NSTextField), `searchContainer` (NSView).
- Build: `//crates/wolf-macos:wolf_macos_lib` PASS, `//crates/wolf-macos:Wolf` PASS (bundled+signed). Test: `//crates/wolf-macos:WolfKeyMapTests` PASS 1/1.
- Divergence: single consolidated call site at end of init (colors originally in two places: panel in init, search-container in buildSearchBar). Construction behavior preserved. Minimal + identical across the three.
- Coder agent id: a106a9eaf22c9233f. (WolfFFI SourceKit diagnostics = index-only artifacts, resolve under Bazel; not real errors.)

## Status history: IMPLEMENTING (coder next — architect/test-writer skipped, right-sized)

## Problem statement
When the OS system appearance changes at runtime (dark→light) while Wolf is in `auto` appearance mode, the file picker's search field background, border, and typed-text color retain the OLD appearance (stay dark → typed text illegible). File entries below repaint correctly; only the static chrome is stale. Repro: start in auto+dark, switch OS to light, ⌘O → search field is dark.

## Root cause (diagnosed)
The three overlay views assign layer colors as CGColor values ONCE at construction and never re-apply. CGColors do NOT auto-adapt to appearance changes, and none of these views override `viewDidChangeEffectiveAppearance()`. File rows adapt only because they use dynamic NSColor (`.labelColor`) redrawn by NSTableView. Overlays are retained NSView subviews of PaneContainerView, created once in `WolfWindowController.build()`, reused across shows (open/close toggles `isHidden`), so stale colors persist.

## Affected files (same bug, all three)
1. `crates/wolf-macos/Sources/WolfMacOS/PickerView.swift` — ⌘O file picker. Frozen CGColors:
   - ~line 147: `layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor` (panel bg)
   - ~line 150: `layer?.borderColor = NSColor.separatorColor.cgColor` (panel border)
   - ~line 386: `searchContainer.layer?.backgroundColor = NSColor.controlBackgroundColor.cgColor` (search field bg)
   - `searchField` (NSTextField) has no explicit `.textColor` (inherits dynamic `.labelColor`) but renders stale because the layer-backed container isn't redrawn. Setup in `init()` → `buildSubviews()` → `buildSearchBar()` (once).
2. `crates/wolf-macos/Sources/WolfMacOS/CommandPaletteView.swift` — same at ~138, ~141, ~366.
3. `crates/wolf-macos/Sources/WolfMacOS/BufferPickerView.swift` — same at ~138, ~141, ~366.

No existing `viewDidChangeEffectiveAppearance`/`updateLayer` override to mirror. PaneContainerView re-resolves from nvim theme every frame in `draw(_:)` — different model, N/A here.

## Required fix (apply identically to all three)
- Extract the frozen CGColor layer assignments (panel bg, panel border, search-container bg) into a private method e.g. `applyChromeColors()`.
- Call it from `init` where the assignments currently are (preserve construction behavior).
- Add:
  ```swift
  override func viewDidChangeEffectiveAppearance() {
      super.viewDidChangeEffectiveAppearance()
      applyChromeColors()
      // mark searchField + container needsDisplay so typed text/placeholder re-resolve
  }
  ```
- Re-resolve dynamic NSColors against the CURRENT appearance: wrap the `.cgColor` reads in `effectiveAppearance.performAsCurrentDrawingAppearance { ... }` (macOS 11+) OR set `NSAppearance.current = effectiveAppearance` around the conversions — so `.windowBackgroundColor.cgColor` etc. resolve to the NEW appearance, not whatever is globally current. Check the crate's min macOS deployment target; prefer `performAsCurrentDrawingAppearance` if 11+. Match existing idioms.
- Ensure typed text + placeholder re-resolve (mark `searchField` for redraw; if any explicit textColor exists, make it dynamic).
- CHECK the actual files for OTHER frozen CGColor assignments (selection highlight, separators on a layer) and include them in the same re-application if found.

## Design notes
- One fix covers BOTH `auto`-mode OS change AND forced `dark`/`light` (forcing sets `NSApp.appearance` → propagates to `effectiveAppearance` → fires `viewDidChangeEffectiveAppearance`). No AppDelegate change.
- Keep minimal + structurally identical across the three near-duplicate overlays. Do NOT introduce a shared base class unless one already exists.

## Testing
- AppKit appearance-change behavior has NO headless harness (no NSApplication in Bazel test process; established manual-smoke-only for the appearance feature). Likely NO automated test to add.
- Coder MUST confirm: `bazel build //crates/wolf-macos:Wolf` (or app target) builds; existing `//crates/wolf-macos:WolfKeyMapTests` still pass.
- Manual-smoke checklist (document): (1) start auto+dark; (2) switch OS→light while running; (3) ⌘O → search field bg/border/text all LIGHT & legible; (4) reverse (light→dark); (5) repeat for command palette + buffer picker; (6) forced dark/light modes still render overlays correctly.

## Documentation
- `crates/wolf-macos/Sources/README.md`: if it describes these overlays, add a brief note that they re-apply layer chrome colors on `viewDidChangeEffectiveAppearance` to follow runtime appearance changes (layer CGColors would otherwise be frozen). Match surrounding style.
- `prettier --write` any edited Markdown; no hard line breaks within paragraphs.

## Workflow decision
Right-sized: SKIP architect + test-writer (fix fully specified above, no automatable test surface). Run coder (implement + build-verify) → reviewer (all three files consistent, correct appearance-resolution API, no missed stale CGColors) → technical-writer (README note).

## Progress log
- (init) Problem statement + fix spec written. Invoking coder.
