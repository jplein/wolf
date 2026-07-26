# Current plan: render the cmdline `prompt` field (rpc-timeout-plan.md §3)

## Problem statement

This is the third of three fixes described in `rpc-timeout-plan.md`. The first two
(RPC timeout backstop, `SwapExists` autocmd) are already done. This effort is the
front-end-only UX fix: **render the cmdline `prompt` field in the macOS renderer.**

### Background (settled diagnosis, from the plan)

When the embedded Neovim raises a dialog such as `confirm('Choose one', '&A\n&B')`,
it arrives over TWO channels and core captures BOTH:

- The dialog body (`Choose one`) → `ext_messages` `msg_show` kind `"confirm"` →
  `Snapshot.toast`. Already rendered by `ToastView`.
- The choice/answer line (`[A], (B): `) → `ext_cmdline` `cmdline_show` →
  `Snapshot.cmdline`, as the `prompt` field, with `active: true`, `firstc: ""`,
  `content: [""]`. Observed snapshot:
  `Cmdline { active: true, firstc: "", prompt: "[A], (B): ", content: [""], pos: 0, level: 1 }`.

The `prompt` string is already present in `Snapshot.cmdline` and already crosses
the bridge via `FfiCmdline::prompt()`. **The gap is purely in the macOS renderer:**
`CmdlineView` (`crates/wolf-macos/Sources/WolfMacOS/CmdlineView.swift`) reads only
`firstc()` + `text()` in `update()` and draws `firstc + text` in `draw()`, so it
never reads or paints `prompt()`. With `firstc` and `text` both empty, the cmdline
band is shown (`isHidden = !active` → visible) but blank — exactly the observed
"`Choose one` shows, `[A], (B):` does not."

### The fix (per plan §3)

- **Render `prompt`.** In `CmdlineView.update()`, read `cmdline.prompt()`; in
  `draw()`, prepend it to the drawn line (`prompt + firstc + text`), and include
  its width in the caret-offset math (the `prefix` computed for the caret). No
  core, FFI, or toast changes.
- **Answering already works.** Keys flow via `nvim_input`; `Esc` cancels and a
  single-key `A`/`B`/`o`/`e` answers once the prompt is visible. The remaining task
  is presentation, not input plumbing — confirm the answering key is not swallowed
  and the prompt clears on answer.
- **Scope.** Neovim-ish is acceptable: the prompt text painted inline in the
  existing cmdline band, not a native `NSAlert`.

### Repo conventions that shape the implementation

- Pure, AppKit-free display/decision logic lives in the Foundation-only
  `WolfKeyMap` module as a "seam" (`enum` namespace with static funcs), unit tested
  headlessly in `crates/wolf-macos/Tests/WolfKeyMapTests/`. See `PaletteSeam.swift`
  / `PaletteSeamTests.swift` for the pattern. Views in `WolfMacOS` import
  `WolfKeyMap` and consume the seam. `CmdlineView` currently has an inline
  `bytePrefix` static and inline `firstc + text` composition; the testable analog
  should be a `CmdlineSeam` covering line composition + caret-prefix.

### Tests (per plan §3)

- A Swift unit test (the pure-view analog of the existing seam tests) that feeds a
  cmdline-shaped value with `active=true`, empty `firstc`/`text`, non-empty
  `prompt` and asserts the composed/rendered line contains the prompt.
- Optionally a core-side assertion (already true today) that `confirm(...)` yields
  `cmdline.prompt == "[A], (B): "` to pin the contract the view depends on.

### Docs to update

- Any README describing `CmdlineView` / the cmdline render surface — note that
  `prompt` is now painted inline. `crates/wolf-ffi/README.md` already documents
  `FfiCmdline::prompt()` (no change expected there beyond verification).

---

## Architect's plan (authoritative)

### Seam design — NEW `crates/wolf-macos/Sources/WolfKeyMap/CmdlineSeam.swift`

`public enum CmdlineSeam` namespace, `import Foundation` only, mirrors `PaletteSeam.swift`.
Three pure static funcs (scalars only — NOT `FfiCmdline`, which can't be built headlessly):

- `static func composedLine(prompt: String, firstc: String, text: String) -> String`
  → `prompt + firstc + text`. Empty prompt ⇒ identical to today's `firstc + text`.
- `static func caretPrefix(prompt: String, firstc: String, text: String, byteCount: Int) -> String`
  → `prompt + firstc + bytePrefix(of: text, byteCount:)`. Prompt width included so caret sits after it.
- `static func bytePrefix(of s: String, byteCount: Int) -> String`
  → moved VERBATIM from `CmdlineView.bytePrefix` (CmdlineView.swift:116-121), made `public`.

`caretPrefix` MUST be defined in terms of `bytePrefix` (single tested impl backs caret math).

### View changes

**`Sources/WolfMacOS/CmdlineView.swift`** (globbed — no BUILD edit):
- Add `private var prompt: String = ""` in the "Latest projected state" block (after line 33).
- `update()` (~72-82): after `firstc = cmdline.firstc().toString()` add `prompt = cmdline.prompt().toString()`; update doc comment.
- `draw()` line 99: `let line = CmdlineSeam.composedLine(prompt: prompt, firstc: firstc, text: text)`.
- `draw()` line 106: `let prefix = CmdlineSeam.caretPrefix(prompt: prompt, firstc: firstc, text: text, byteCount: pos)`.
- DELETE the static `bytePrefix` (lines 116-121) — now in the seam.
- Update file header (3-6) + class doc (19-22) to say it paints `prompt + firstc + text`.

**`Sources/WolfMacOS/PaneContainerView.swift` line 858** (SECOND `bytePrefix` caller!):
- Replace `CmdlineView.bytePrefix(...)` → `CmdlineSeam.bytePrefix(...)`. Pure relocation.
- Do NOT add prompt to `pumAnchorX` — out of scope (no completion popup on a `confirm()` dialog).

### BUILD.bazel (explicit `srcs` lists, NOT globs — COder makes these edits)
- `wolf_key_map.srcs` (50-84): add `"Sources/WolfKeyMap/CmdlineSeam.swift"` (alphabetical: between Cell… and Command…).
- `WolfKeyMapTests.srcs` (235-265): add `"Tests/WolfKeyMapTests/CmdlineSeamTests.swift"` (between CellMetricsMathTests and CommandSeamTests).

### Tests — NEW `Tests/WolfKeyMapTests/CmdlineSeamTests.swift`
Mirror `PaletteSeamTests.swift` (XCTest, `@testable import WolfKeyMap`, table-driven, assert exact Strings — NOT widths).
- `composedLine`: dialog case (`prompt="[A], (B): "`, firstc/text empty → `"[A], (B): "`); normal `:` (`""`,`":"`,`"wq"`→`":wq"`); normal `/`; prompt+text (`"Name: "`,`""`,`"bar"`→`"Name: bar"`).
- `caretPrefix`: prompt + caret in text (`"[A], (B): "`,`""`,`"Bob"`,2 → `"[A], (B): Bo"`); normal (`""`,`":"`,`"wq"`,1→`":w"`); caret at 0 with prompt (`"P: "`,`""`,`"xyz"`,0→`"P: "`).
- `bytePrefix`: `<=0`→`""` (incl negative); `>=len`→whole; ASCII mid (`"hello"`,3→`"hel"`); multi-byte clean boundary (`"日本"`,3→`"日"`). Skip split-boundary over-assertion.

Core-side `confirm()` assertion: SKIP (already true; booting nvim is out of proportion for a front-end presentation fix).

### Docs (technical-writer)
- `Sources/README.md:59` — CmdlineView bullet: now paints `prompt + firstc + text`, stores prompt, caret from `prompt+firstc+bytePrefix`; add `CmdlineSeam.swift` seam bullet.
- `Sources/README.md:7` — "Twenty-nine … files" → "Thirty".
- `README.md:89` — PaneContainerView narrative: note cmdline prompt now painted inline; keep bytePrefix prose accurate after move.
- `Tests/README.md` — add `CmdlineSeamTests.swift` section (style of PaletteSeamTests entry).
- `crates/wolf-ffi/README.md` — verify only (already documents `FfiCmdline::prompt()`).
- prettier --write edited Markdown; no hard line breaks in paragraphs.

### Implementation order (avoid compile breaks)
1. Create CmdlineSeam.swift. 2. BUILD: add seam to wolf_key_map.srcs. 3+4 TOGETHER: edit CmdlineView.swift (delete bytePrefix) AND PaneContainerView.swift:858. 5. Create CmdlineSeamTests.swift. 6. BUILD: add test to WolfKeyMapTests.srcs. 7. READMEs.

### Verification (coder runs)
- `bazel test //crates/wolf-macos:WolfKeyMapTests`
- `bazel build //crates/wolf-macos:Wolf`
- `bazel test //tools/lint:swift_format` (fix via `//tools/lint:swift_format.fix` if needed)
- `prettier --write` on edited Markdown.

## Status

- [x] architect — detailed plan
- [x] test-writer — created `Tests/WolfKeyMapTests/CmdlineSeamTests.swift`
      (testComposedLineTable, testCaretPrefixTable, testBytePrefixTable). BUILD
      left to coder. Won't compile until coder adds seam + BUILD srcs entries.
- [x] coder — created CmdlineSeam.swift (composedLine/caretPrefix/bytePrefix);
      wired both BUILD srcs entries; edited CmdlineView.swift (prompt property,
      update() reads prompt, draw() uses seam, deleted local bytePrefix, header
      docs); repointed PaneContainerView.swift:858 → CmdlineSeam.bytePrefix.
      ALL PASS: WolfKeyMapTests PASSED 0.6s, `//crates/wolf-macos:Wolf` builds,
      swift_format PASSED. No test changes recommended.
- [x] reviewer — APPROVED. All 6 verification points pass: seam signatures exact
      (caretPrefix delegates to bytePrefix), CmdlineView reads prompt + uses seam +
      local bytePrefix gone, PaneContainerView repointed (no lingering
      CmdlineView.bytePrefix, pumAnchorX left prompt-free), both BUILD srcs entries
      present, tests match shipped signatures, front-end-only scope, caret math
      sound. Only remaining work: README doc sync (below).
- [x] technical-writer — updated Sources/README.md (count 29→30, new CmdlineSeam
      bullet, rewrote CmdlineView bullet), README.md (PaneContainerView narrative:
      prompt painted inline, pumAnchorX uses CmdlineSeam.bytePrefix), Tests/README.md
      (new CmdlineSeamTests section). wolf-ffi/README.md verified (already correct,
      no change). Orchestrator ran prettier --write on all three — all unchanged.

## COMPLETE ✅

Fix §3 of rpc-timeout-plan.md landed: the macOS renderer now paints the cmdline
`prompt` field inline. A `confirm()`/`input()`-style prompt's choice line (e.g.
`[A], (B): `) — already crossing the FFI via `FfiCmdline::prompt()` but previously
dropped by `CmdlineView` — is now composed and drawn via the new pure `CmdlineSeam`
(`composedLine` / `caretPrefix` / `bytePrefix`), with the caret placed after the
full prompt. Normal `:`/`/` command-line rendering is byte-identical (empty prompt).
Front-end only — no core/FFI/toast changes. WolfKeyMapTests + Wolf build + swift_format
all green; reviewer APPROVED; docs synced. NOT committed.
