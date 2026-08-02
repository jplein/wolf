# Current Plan: macOS "option-as-option" key handling

## Feature request

On macOS, treat the Option key as **Option** (compose accented characters), not as
**Alt** (Neovim `<M-…>` mappings) — mirroring Ghostty's `macos-option-as-alt = false`.

Concrete acceptance example: typing `Option-u` then `u` produces `ü`, which is inserted
into the Neovim buffer (in insert mode) rather than being sent as `<M-u>` followed by `u`.

## Design decisions already made (by the user)

1. **Configurable, default = option-as-option.** Add a config knob (like Ghostty's
   `macos-option-as-alt`). Default to the new option-as-option behavior, but let users who
   rely on `<M-…>` Alt mappings opt back into option-as-alt. Ghostty's default is `false`
   (= option-as-option), which maps cleanly to a new `macos_option_as_alt: bool` defaulting
   to `false`.
2. **Scope: dead-key accents only.** Wire up the macOS text-input machinery enough to
   compose dead keys / Option characters (ü, é, €, etc.) and feed the composed text to
   Neovim via `send_key`. Full IME (CJK) with on-grid preedit/marked-text rendering is
   explicitly **out of scope** for now (a possible later follow-up). Marked text may be
   tracked but need not be drawn.

## Context gathered during discovery (ground truth)

### Current key path (no text-input system today)
- `crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift:636` — `keyDown(with:)` reads
  the raw `NSEvent` and calls `WolfKeyMap.translate(...)` with `alt: flags.contains(.option)`.
  It does **not** use `interpretKeyEvents`, `NSTextInputClient`, marked text, or any IME.
  (Confirmed: no `NSTextInputClient`/`interpretKeyEvents`/`insertText`/`setMarkedText`
  anywhere under `crates/wolf-macos/Sources`.)
- `crates/wolf-macos/Sources/WolfKeyMap/KeyMap.swift` — pure, AppKit-free translation seam
  `WolfKeyMap.translate(keyCode:characters:charactersIgnoringModifiers:ctrl:shift:alt:meta:)
  -> FfiKeyEvent?`. When ctrl/alt/meta is held it takes the glyph from
  `charactersIgnoringModifiers.lowercased()` — so today **Option+u → `<M-u>`** and the "u"
  is discarded. This is the "option-as-alt" behavior to change.
- `PaneContainerView.keyDown` also intercepts the Command+O picker trigger before translation.

### Why the ü example needs the text-input system
`Option-u` is a **dead key**: the composed "ü" only exists because macOS holds composition
state across two keystrokes, and that state lives in the Cocoa text-input system
(TSM / NSTextInputContext). Reading `event.characters` directly cannot produce it. So this
requires routing input through `interpretKeyEvents` + `NSTextInputClient` (feeding composed
text to `send_key`), OR a surgical `UCKeyTranslate` + persisted dead-key-state layer.
**Architect must weigh these two approaches** — the `UCKeyTranslate` layer is more surgical
and lower-risk to the existing Ctrl/Cmd/named-key handling; full `NSTextInputClient` routing
is more "correct"/future-proof for IME but re-architects the whole input path. Note: once a
dead key is pending, the *committing* keystroke (plain `u`) must also flow through the same
composition mechanism, so a "only route Option-modified keys" shortcut is insufficient.

### Config chain (all four layers must be threaded)
- `crates/wolf-core/src/config.rs` — `WolfConfig` struct (l.63), `ConfigSection` on-disk
  shape (l.86, kebab-case; `appearance` uses `#[serde(default = "default_appearance")]` for
  back-compat — a new key should do the same so pre-existing configs still parse),
  `default_config()` (l.121), `write_defaults()` (l.183). Existing serde tests in the same
  file's `mod tests`.
- `crates/wolf-ffi/src/lib.rs` — `FfiConfig` opaque struct (l.1415) + scalar accessors
  (l.1422), wired in `wolf_ffi_load_config()` (l.1458). Bridge exposes it to Swift.
  Existing FFI config test: `crates/wolf-ffi/tests/config.rs`.
- Swift: `FfiConfig` is consumed in `AppDelegate.swift` / `WolfWindowController.swift`
  (and `CellMetrics.swift`); the new flag must be threaded down to `PaneContainerView`.
- The generated bridge files `crates/wolf-ffi/generated/wolf-ffi.swift` and `.h` may need
  regeneration (coder to confirm how the build regenerates them).

### Testing constraints (from CLAUDE.md — CRITICAL)
- Prefer end-to-end / integration tests over mocks; the app talks to a real Neovim, so
  real-nvim tests should start an actual Neovim process.
- **Bazel does NOT auto-discover tests.** Every new test file must be wired into a target:
  - Rust `crates/*/tests/*.rs`: add a matching `rust_test` in the crate `BUILD.bazel`
    (real-nvim tests need staged `//bazel/nvim:nvim_tree` + `//runtime/nvim:init.lua`
    runfiles, wired `WOLF_*` env, `tags = ["no-sandbox"]`; headless tests need none). Runs
    under both `bazelisk test` (macOS) and `cargo test` (Linux/Nix).
  - Swift `crates/wolf-macos/Tests/WolfKeyMapTests/*.swift`: add every new `*Tests.swift`
    file to the **explicit hand-maintained `srcs` list** of `swift_test` target
    `//crates/wolf-macos:WolfKeyMapTests` (NOT a glob) — otherwise it never compiles/runs.
    Only the pure AppKit-free seams are testable here.
- Note: the `NSTextInputClient`/AppKit integration itself is not headlessly unit-testable;
  favour extracting any decision logic into a pure seam that CAN be tested, and cover the
  composed-char → Neovim path where feasible via the Rust FFI real-nvim tests (sending a
  `ch` scalar such as 'ü' and asserting the buffer).

### Docs to keep in sync (source-of-truth rule from CLAUDE.md)
- `crates/wolf-macos/Sources/README.md` (describes `KeyMap.swift` and the seams)
- `crates/wolf-macos/README.md`
- `crates/wolf-ffi/src/README.md` / `crates/wolf-ffi/README.md` (FfiConfig)
- `crates/wolf-core/src/README.md` (WolfConfig)
- Any top-level docs describing config keys.
- Markdown edits: no hard line breaks mid-paragraph; run `prettier --write` after.

## Status: ✅ COMPLETE — final reviewer PASS, all tests green, docs updated & prettier-clean

Round 1 (architect → test-writer → coder → reviewer) complete. All 6 Bazel targets passed.
Reviewer found: (1 Major) Caps Lock dropped in default compose path — regression; (2 Major)
docs not updated — assigned to technical-writer (was intentionally out of coder scope); (3 Minor)
`KeyComposer.reset()` never called → stale dead-key state across focus loss; (4 Nit) force-unwrap.
Findings 1/3/4 routed back to the coder (in progress). Finding 2 → technical-writer (pending).

---

# Implementation Plan (from architect)

## Decision: Approach B — surgical `UCKeyTranslate` + persisted `UInt32` dead-key state
Keep the existing `WolfKeyMap.translate` path intact; add a focused composition step. Lower
regression risk than full `NSTextInputClient` routing. In option-as-option mode, route EVERY
printable-candidate key (not just Option-modified) through the composer — the persisted
`deadKeyState` carries dead-key state so the committing keystroke (plain `u`) composes to `ü`.
A plain key with no pending dead key composes to itself (no regression).

### Composed-string → FFI delivery
`FfiKeyEvent.ch: u32` already accepts arbitrary Unicode scalars (`char::from_u32`). "ü" = one
event `{kind: char, ch: 0x00FC, all mods false}`. Multi-scalar composition = one modifier-free
event per scalar; wolf-core fuses the run into one `nvim_input`. NO FFI struct change needed.

## Seam boundary
**Pure (testable, WolfKeyMap module):**
1. `KeyComposition.routesThroughComposer(keyCode:ctrl:meta:optionAsAlt:) -> Bool` = true iff
   `!optionAsAlt && !ctrl && !meta && !isNamedKey(keyCode)`.
2. `KeyComposition.charEvents(for composed: String) -> [FfiKeyEvent]` = one event per scalar,
   filtering C0/DEL controls, all mods false, kind=char, shift=false.
Also add `public static func isNamedKey(keyCode:) -> Bool` to `KeyMap.swift` (wraps private `namedKey`).

**Untestable AppKit/Carbon glue (WolfMacOS module):**
3. `KeyComposer` class: holds `private var deadKeyState: UInt32 = 0`; `compose(keyCode:shift:option:)
   -> String` via `TISCopyCurrentKeyboardLayoutInputSource` + `kTISPropertyUnicodeKeyLayoutData` +
   `UCKeyTranslate`; returns "" while a dead key is pending. `import Carbon.HIToolbox`. Plus `reset()`.

## Files to CREATE
- `crates/wolf-macos/Sources/WolfKeyMap/KeyComposition.swift` (pure seam; Foundation + FfiKeyEvent only)
- `crates/wolf-macos/Sources/WolfMacOS/KeyComposer.swift` (AppKit/Carbon glue; globbed by wolf_macos_lib)
- `crates/wolf-macos/Tests/WolfKeyMapTests/KeyCompositionTests.swift` (pure seam tests — MUST wire into srcs)
- `crates/wolf-ffi/tests/unicode_key.rs` (real-nvim: insert mode, send ch=0x00FC, assert "ü" in buffer — MUST wire rust_test)

## Files to MODIFY
- `crates/wolf-core/src/config.rs`: `const DEFAULT_MACOS_OPTION_AS_ALT: bool = false;`,
  `fn default_macos_option_as_alt()`, `WolfConfig.macos_option_as_alt: bool`,
  `ConfigSection` field `#[serde(default = "default_macos_option_as_alt")]` (kebab key `macos-option-as-alt`),
  update `default_config` / `load_or_create` / `write_defaults`, and the in-file `mod tests` + `assert_config_eq`.
- `crates/wolf-ffi/src/lib.rs`: `FfiConfig.macos_option_as_alt: bool`, `pub fn macos_option_as_alt(&self) -> bool`,
  populate in `wolf_ffi_load_config()`, AND add `fn macos_option_as_alt(&self) -> bool;` to the bridge `extern "Rust"` block.
- REGENERATE bridge: `bazel run //crates/wolf-ffi:gen_bindings` → commit updated `generated/wolf-ffi.swift` + `.h`.
- `crates/wolf-macos/Sources/WolfKeyMap/KeyMap.swift`: add `public static func isNamedKey(keyCode:) -> Bool`.
- `crates/wolf-macos/Sources/WolfMacOS/WolfWindowController.swift`: pass `config.macos_option_as_alt()` into PaneContainerView init.
- `crates/wolf-macos/Sources/WolfMacOS/PaneContainerView.swift`: add `optionAsAlt: Bool` init param + `keyComposer`;
  in `keyDown` after the Command+O intercept, branch on `routesThroughComposer` → compose → charEvents → send_key
  (empty result = dead key pending, consume & return, no noteKeystroke); else existing translate path unchanged.

## Bazel wiring (MANDATORY)
- `crates/wolf-macos/BUILD.bazel`: add `KeyComposition.swift` to `wolf_key_map` srcs; add
  `KeyCompositionTests.swift` to `WolfKeyMabTests` srcs (hand-maintained list). `KeyComposer.swift` needs no
  srcs edit (globbed). If `import Carbon.HIToolbox` fails to autolink, add a Carbon framework linkopt to the
  `macos_application` "Wolf" target — verify with `bazelisk build`.
- `crates/wolf-ffi/BUILD.bazel`: add `rust_test` `unicode_key` modeled on `bridge_smoke` (srcs = common/mod.rs +
  unicode_key.rs, crate_root, `tags=["no-sandbox"]`, size small, staged nvim data + WOLF_TEST_* env, deps).

## Test plan
- wolf-core `mod tests`: extend `assert_config_eq`; add default-is-false, parses true/false, missing-key-defaults-false
  (back-compat, preserves font fields), round-trip.
- wolf-ffi `tests/config.rs`: present `macos-option-as-alt: true` surfaces true over bridge; absent → false with font fields intact.
- wolf-ffi `tests/unicode_key.rs`: ch=0x00FC → "ü" in buffer; optional multi-scalar.
- Swift `KeyCompositionTests`: routesThroughComposer all branches; charEvents single/multi/control/empty.

## Docs to update (§6)
`wolf-core/src/README.md`, `wolf-ffi/src/README.md`, `wolf-macos/Sources/README.md` (add KeyComposition bullet, bump
"Thirty-eight"→thirty-nine, note KeyComposer + routing in KeyMap bullet), `wolf-macos/README.md`,
`wolf-macos/Tests/README.md`, `wolf-ffi/tests/README.md`, `architecture.md` (config key), check `feat-list.md`.
prettier --write all Markdown; no mid-paragraph hard breaks.

## Implementation order
1. wolf-core config.rs  2. wolf-ffi lib.rs  3. regen bridge  4. wolf-ffi unicode_key test
5. WolfKeyMap seam (isNamedKey + KeyComposition + srcs)  6. KeyCompositionTests + srcs
7. AppKit glue (KeyComposer + PaneContainerView + WolfWindowController)  8. docs  9. run suites & confirm new targets execute

## Edge cases (call-outs, intended behavior)
- Ctrl+Option → translate (`<C-A-x>`), not composed. Option+arrow → named key → translate, `<M-Left>` preserved (matches Ghostty).
- Dead-key pending → compose "" → consume event, no send, no noteKeystroke.
- option-as-alt legacy (flag true) → routesThroughComposer always false → `<M-…>` still works.
- Right vs left Option: single global on/off only (documented non-goal).
