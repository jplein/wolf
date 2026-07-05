# Plan: Coalesce the file-tree sidebar traversal wake (large-repo main-thread freeze)

## Status: COMPLETE ✅ — implemented, reviewed (approved), docs verified

### Outcome
- FileTreeView now coalesces the streaming traversal wake via a reused
  `PickerRefreshCoalescer` (`scheduleRefresh()`); `cancelPending()` in `close()` and
  at the top of `reroot(to:)`. Immediate refresh preserved on start/onRenderWake/
  setSidebarVisible/reroot-final.
- `TreeWakeBridge` now calls `tree?.scheduleRefresh()` (COALESCED).
- `PickerRefreshCoalescer` doc notes it is shared by picker + tree.
- READMEs (crates/wolf-macos/README.md, Sources/README.md) updated + prettier'd;
  stale `picker?.refresh()` fixed; count word corrected to "Sixteen".
- Build: `bazel build //crates/wolf-macos:Wolf` PASS.
  Tests: `bazel test //crates/wolf-macos:WolfKeyMapTests` PASS
  (`PickerRefreshCoalescerTests` is the regression guard for the shared seam).
- No new tests (coalescing contract already covered; tree glue is non-headless AppKit).
- Reviewer: APPROVED (no retain cycle, no stale-tree fire hazard, docs match code).
- Technical writer: no other docs affected (architecture.md / wolf-core READMEs
  describe unchanged core-side wake; coalescer is macOS-only).
- REMAINING: manual smoke (needs a window server) — open ~/Git/NixOS/nixpkgs and
  confirm the sidebar streams in without the multi-second freeze and the picker stays
  responsive.

## Status (historical): IN PROGRESS — architect plan approved, implementing

## Problem statement

Opening a large repo (e.g. `~/Git/NixOS/nixpkgs`, ~150k files) blocks the AppKit
main thread for several seconds and makes the file picker sluggish.

### Root cause (validated by architect)

The file-tree sidebar's traversal wake is **un-coalesced**. `TreeWakeBridge.install()`
calls `tree?.refresh()` directly on EVERY streaming traversal batch (one wake per
256-file `TRAVERSAL_BATCH` in `wolf-core/src/files.rs`; `spawn_traversal` wakes after
every batch and on completion, and each `append_batch` bumps `version`). For a large
repo that's hundreds of main-thread wakes; because `version` advances on every batch,
the `TreeRefreshGate` version gate never suppresses during a live traversal — each wake
runs `childrenMemo.invalidate()` + `outlineView.reloadData()` + a fresh O(N)
`TreeIndex::build`. That back-to-back O(N) storm blocks the main thread and starves the
picker.

### Pattern to mirror (already tested)

`PickerWakeBridge` → `picker?.scheduleRefresh()`; `PickerView` owns a lazy
`PickerRefreshCoalescer` (schedule = `DispatchQueue.main.asyncAfter(.now() + refreshInterval)`,
perform = `{ [weak self] in self?.refresh() }`), exposes `scheduleRefresh()` →
`request()`, and calls `cancelPending()` in `close()`. `PickerRefreshCoalescer` is
Foundation-only/generic, covered by `PickerRefreshCoalescerTests`.

## Decision: REUSE `PickerRefreshCoalescer` as-is (no rename)

It is already generic/toolkit-free (injected `schedule`/`perform`); nothing picker-
specific but the name. Reuse = one-line usage + doc note. A rename would churn BUILD,
PickerView, the test file, and READMEs for no behavioral gain. Resolve the CLAUDE.md
docs-match rule with a one-sentence doc note that the seam is shared by picker + tree.

## Implementation steps (order matters)

1. **`crates/wolf-macos/Sources/WolfKeyMap/PickerRefreshCoalescer.swift`** — doc-only:
   update the file-level + class doc to say the seam is shared by BOTH the file picker
   and the file-tree sidebar (both stream a large traversal); note the name is
   historical / the seam is generic. No code change.

2. **`crates/wolf-macos/Sources/WolfMacOS/FileTreeView.swift`**:
   - Add a `lazy var refreshCoalescer = PickerRefreshCoalescer(schedule: { work in DispatchQueue.main.asyncAfter(deadline: .now() + PickerRefreshCoalescer.refreshInterval, execute: work) }, perform: { [weak self] in self?.refresh() })` (place near the `refreshGate` property; doc comment mirroring `PickerView` lines 104-107). `WolfKeyMap` already imported.
   - Add `scheduleRefresh()` calling `refreshCoalescer.request()` (right after `refresh()`; doc mirroring `PickerView.scheduleRefresh()`).
   - `close()`: add `refreshCoalescer.cancelPending()`; note it in the doc.
   - `reroot(to:)`: add `refreshCoalescer.cancelPending()` near the top (reroot rebuilds
     the outline wholesale and calls `refresh()` itself, so a stale pending fire against
     the OLD tree must be cancelled first); note it in the doc.
   - **Do NOT change** the direct `refresh()` calls in `start()`, `onRenderWake()`,
     `setSidebarVisible()` — these stay immediate.

3. **`crates/wolf-macos/Sources/WolfMacOS/TreeWakeBridge.swift`**:
   - Change `.tree?.refresh()` → `.tree?.scheduleRefresh()`.
   - Update file-level + enum doc to say the wake now asks for a COALESCED refresh
     (mirror `PickerWakeBridge` wording; adapt "re-rank" → "reload/rebuild rows"; note
     the render-wake reveal path stays immediate).

4. **README edits** (source of truth; run `prettier --write` after, no hard line breaks):
   - `crates/wolf-macos/README.md`:
     - `PickerRefreshCoalescer` paragraph (~line 30): shared by picker + tree.
     - `FileTreeView.swift` entry (~line 36): tree wake drives COALESCED
       `scheduleRefresh()` via `PickerRefreshCoalescer`; start/render-wake/visibility
       refresh immediately.
     - `TreeWakeBridge.swift` entry (~line 44): `tree?.refresh()` → COALESCED
       `tree?.scheduleRefresh()`.
     - Fix pre-existing stale `PickerWakeBridge.swift` entry (~line 43):
       `picker?.refresh()` → `picker?.scheduleRefresh()`.
   - `crates/wolf-macos/Sources/README.md`:
     - `TreeWakeBridge.swift` entry: same COALESCED edit.
     - `PickerWakeBridge.swift` entry: fix stale `picker?.refresh()` →
       `picker?.scheduleRefresh()`.
     - `FileTreeView.swift` entry: coalesced via `PickerRefreshCoalescer.scheduleRefresh()`.
     - Minimal-scope: add a bullet for `PickerRefreshCoalescer.swift` (shared picker+tree
       debounce seam) and fix the "Twelve" count if it becomes wrong. (Enumerating the
       two tree seams is out of scope; just don't worsen the count.)

## Test plan: NO NEW TESTS

The coalescing contract is fully covered headlessly by `PickerRefreshCoalescerTests`
(burst → single trailing fire, reschedule-after-fire, `refreshInterval` in frame band).
Reusing the same class for the tree exercises the identical tested seam. The only
tree-specific new code is glue in `FileTreeView` (an `NSOutlineView` AppKit shell,
"manual smoke only", not headless-testable). A seam-level regression test would
duplicate the existing test or require extracting a seam purely to test 3 lines of glue
— out of scope. Existing `WolfKeyMapTests` must still pass as the regression guard.

## Behavior that MUST be preserved

- Immediate `refresh()` on: `start()`, `onRenderWake()` (reveal/bold current buffer),
  `setSidebarVisible()`, and the final `refresh()` at the end of `reroot(to:)`. Only the
  streaming traversal wake path becomes coalesced.
- Late wake after close stays a harmless no-op; `cancelPending()` clears the armed flag.
- Existing `TreeRefreshGate` visibility + version gating inside `refresh()` unchanged.

## Verification (coder runs)

- `bazel build //crates/wolf-macos:Wolf`
- `WolfKeyMapTests` target (confirm exact label from crate BUILD), e.g.
  `bazel test //crates/wolf-macos:WolfKeyMapTests`
- Manual smoke (not automatable): open ~/Git/NixOS/nixpkgs; sidebar streams in without
  the multi-second freeze; picker stays responsive.

## Files
- `crates/wolf-macos/Sources/WolfMacOS/FileTreeView.swift`
- `crates/wolf-macos/Sources/WolfMacOS/TreeWakeBridge.swift`
- `crates/wolf-macos/Sources/WolfKeyMap/PickerRefreshCoalescer.swift`
- `crates/wolf-macos/README.md`
- `crates/wolf-macos/Sources/README.md`
- (reference, no change) PickerView.swift, PickerWakeBridge.swift,
  PickerRefreshCoalescerTests.swift, wolf-core/src/files.rs
