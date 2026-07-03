# Plan: Change macOS file picker shortcut from Shift+Ctrl+P to Command+O

## Problem statement

The macOS file picker is currently opened with the **Shift+Ctrl+P** chord. We
need to change it to **Command+O** (the `meta`/command modifier, not
ctrl+shift).

### Known affected surfaces (from initial survey)

- `crates/wolf-macos/Sources/WolfKeyMap/PickerSeam.swift` — `isPickerTrigger(charactersIgnoringModifiers:ctrl:shift:)` seam
- `crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift` — call site in `keyDown`, plus `onPickerTrigger` doc comment
- `crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift` — comment referencing Shift+Ctrl+P
- `crates/wolf-macos/Tests/WolfKeyMapTests/PickerSeamTests.swift` — the trigger table tests
- Docs: `crates/wolf-macos/README.md`, `crates/wolf-macos/Sources/README.md`, `crates/wolf-macos/Tests/README.md`, and `architecture.md`

### Constraint

Command+O uses the **meta/command** modifier. The seam signature currently takes
`ctrl` and `shift`; changing to command means the seam signature and its call
site (which currently passes `flags.contains(.control)` / `.shift`) must pass the
command modifier instead. Must ensure Command+O is not otherwise needed by
Neovim (command is generally free on macOS).

## Architect's plan

### New seam
`isPickerTrigger(charactersIgnoringModifiers: String, command: Bool) -> Bool`
- `guard command else { return false }`
- `guard let first = charactersIgnoringModifiers.unicodeScalars.first else { return false }`
- `return String(first).lowercased() == "o"`
- Drop `ctrl`/`shift` params. Command does NOT mangle `charactersIgnoringModifiers` to a control scalar, so it still carries "o"/"O". Keep guard order (command check first).

### Files to change
1. `PickerSeam.swift` — new signature + body + doc comment (Command+O, `command && first lowercased == "o"`, other modifiers ignored, empty string false; note macOS chord diverges from GNOME "p" seam).
2. `PaneContainerView.swift` — call site (~216) passes `command: flags.contains(.command)` in place of ctrl/shift; update comments at lines 121 and 213 ("Command+O"). The later `translate(...)` call keeps `meta:` unchanged.
3. `AppDelegate.swift` — line 89 comment "Command+O opens it."
4. `PickerSeamTests.swift` (TEST-WRITER) — `TriggerCase` struct: drop ctrl/shift, add `command`. New rows: Cmd+"o"→true, Cmd+"O"→true, "o" command:false→false, Cmd+"p"→false, Cmd+"a"→false, ""+command:true→false. Update loop call + header contract comment. Leave nextSelection/restoredIndex tables untouched.
5. Docs: `crates/wolf-macos/README.md` (lines 26, 79), `Sources/README.md` (line 10), `Tests/README.md` (line 21), `architecture.md` (line 64 ONLY — GNOME line 54 stays Ctrl+Shift+P). Reword the "<C-p> preserved" rationale → "Command-modified keys are not Neovim keybindings". Reformat .md with `prettier --write`, no hard line breaks in paragraphs.

### Verification (Bazel, from repo root)
- `bazel test //crates/wolf-macos:WolfKeyMapTests`
- `bazel build //crates/wolf-macos:Wolf`
- Manual smoke: Command+O opens picker; Shift+Ctrl+P no longer does; `<C-p>` still reaches Neovim.

### Implementation order
1. PickerSeam.swift  2. PaneContainerView.swift  3. PickerSeamTests.swift  4. AppDelegate.swift  5. docs+prettier
(Steps 1–3 must land together to compile.)

## Status: COMPLETE

- Seam, call site, comments, and tests implemented.
- `bazel test //crates/wolf-macos:WolfKeyMapTests` → PASS (1/1).
- `bazel build //crates/wolf-macos:Wolf` → build succeeded.
- Reviewer: PASS (guard order preserved, letter "o", no stale call sites).
- Docs updated (README.md, Sources/README.md, Tests/README.md, architecture.md macOS line 64); prettier clean. GNOME picker (Ctrl+Shift+P) intentionally unchanged.
