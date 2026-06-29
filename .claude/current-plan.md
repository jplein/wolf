# Plan: wolf-macos first increment (AppKit shell + single window)

## STATUS: COMPLETE ✅

Single-window AppKit/Swift shell over `wolf-core` via a new swift-bridge
`wolf-ffi` crate. All Bazel targets build (incl. the code-signed `Wolf.app` on
the Xcode 26 / Tahoe SDK); all runnable tests pass: `//crates/wolf-ffi:frame_pack`
(headless deterministic packing) + `//crates/wolf-macos:WolfKeyMapTests` (pure
Shift-contract) PASS; `//crates/wolf-ffi:bridge_smoke` loud-skips (no
WOLF_NVIM_BIN on the macOS Bazel host, as designed). GUI denylist for `wolf-core`
clean — swift-bridge confined to `wolf-ffi`. Reviewer APPROVED. Docs reconciled +
prettier-formatted. Nothing committed (per standing rule).

Resolved pins: rules_swift 3.6.1, rules_apple 4.5.3, rules_xcodeproj 4.1.0,
swift-bridge 0.1.59 (apple_support stays 2.6.1; rules_cc 0.2.15→0.2.17 via MVS).

Known follow-ups (documented, deferred): glyph text-attribute rendering
(bold/italic/underline — packed but not painted), a native cmdline strip (cmdline
mode currently swaps the whole view to the outer grid), split panes/multigrid,
picker, file-tree sidebar, mouse/scroll, theming, HiDPI edge cases, bundling the
pinned Neovim into the `.app`, and provisioning WOLF_NVIM_BIN for Bazel-run tests
on macOS.

## Problem statement

Implement the first increment of the macOS front-end (`wolf-macos`): an AppKit
application shell with a single window that renders the embedded Neovim character
grid and forwards keyboard input to the embedded Neovim. A thin native shell over
the unchanged platform-agnostic `wolf-core` (which owns Neovim, the redraw stream,
and the FFI-shaped surface `WolfCore::start(cfg, wake)` → `read_snapshot` /
`send_input` / `resize` / `shutdown`; `WakeFn` fired on flush). Under
`ext_multigrid`, editor window content lives on grids `>= 2`; grid 1 is the outer
grid. `crates/wolf-gnome` is reference only (Rust/GTK, not reusable in Swift).

## Locked design decisions (document prominently AND implement)

1. Supported macOS versions: **Tahoe and newer only**.
2. macOS UI built with **AppKit (NOT SwiftUI)**.
3. Implementation language: **Swift**.
4. **Xcode 26+ only** — Swift features requiring Xcode 26+ are permitted.
5. Build is **all-Bazel and authoritative**: `rules_swift` + `rules_apple` build
   the `.app`; `rules_xcodeproj` generates an **uncommitted** `.xcodeproj` for
   debugging/LLDB only.
6. Rust↔Swift via **swift-bridge**: a new `wolf-ffi` staticlib crate (depends on
   `wolf-core`) holds the `#[swift_bridge::bridge]` module. Generated Swift + C
   header are **committed**, refreshed by an explicit Bazel codegen target.

## Orchestrator decisions on the architect's open items

- **Tahoe = macOS 26.** `Info.plist` `LSMinimumSystemVersion` and
  `macos_application` `minimum_os_version` = **"26.0"**.
- **Font:** SF Mono with **Menlo** fallback; size overridable via `WOLF_FONT_PT`
  (parity with gnome). Accepted as recommended.
- **Commit the shared swift-bridge runtime** (`SwiftBridgeCore.swift`/`.h`)
  alongside the per-crate generated files: **yes**.
- **Bazel rule versions** (`rules_swift`/`rules_apple`/`rules_xcodeproj`): the
  coder determines the **highest BCR releases** that (a) declare Bazel-8.7.0
  compatibility, (b) keep transitive `apple_support >= 2.6.1` (so the host-Xcode
  host-SDK toolchain still resolves — never regress below 2.6.1), and (c) support
  Xcode 26 / the Tahoe (macOS 26) SDK. Verify the resolved graph with
  `bazel mod deps` / `bazel mod explain`; record the final pins in this file.
- **Key translation lives in Rust** (Swift sends a structured `FfiKeyEvent`;
  `wolf-core::key_to_notation` stays the single source of nvim notation).

---

## Architect's plan (approved)

### A. wolf-ffi crate (`crates/wolf-ffi`)

**Bridge design decisions:**
- Key translation in Rust, fed by a transparent `FfiKeyEvent` from Swift →
  mapped to `wolf_core::KeyEvent` → `send_input`. No nvim-notation logic in Swift.
- Snapshot marshalled as an **opaque per-frame handle** `FfiFrame` that resolves
  the active grid against the highlight table **in Rust** (reuse
  `HighlightTable::resolve`/`ResolvedStyle` — never reimplement in Swift), with
  scalar accessors + a single flat `cells() -> Vec<FfiCell>` (row-major). One Vec
  handoff per frame; no nested Vecs, no per-cell FFI.
- Wake = an `extern "Swift" { fn wolf_ffi_wake(); }` free function; start wires
  `WakeFn = Arc::new(|| wolf_ffi_wake())` (captures nothing → `Send + Sync`; wake
  touches no `WolfCore` state). **Fallback:** a hand-written
  `extern "C" fn wolf_ffi_start(..., wake: extern "C" fn())` shim if the
  `extern "Swift"` path misbehaves under Bazel.
- `WolfCore` is non-`Send`: wrap in opaque `WolfCoreHandle`
  (`RefCell<Option<WolfCore>>`), all handle methods main-thread-only; only the
  stateless wake crosses threads.

**Files to create:**
- `crates/wolf-ffi/Cargo.toml` — `[lib] crate-type = ["staticlib","rlib"]`
  (rlib so the Rust integration test links it); deps `wolf-core` (path),
  `swift-bridge`; `swift-bridge-build` for the codegen bin. Workspace fields
  inherited.
- `crates/wolf-ffi/src/lib.rs` — the `#[swift_bridge::bridge] mod ffi` + glue:
  - Opaque `WolfCoreHandle(RefCell<Option<wolf_core::WolfCore>>)`.
  - Opaque `FfiFrame { cols, rows, cursor_row, cursor_col, cursor_shape: u8,
    cursor_visible: bool, default_fg: FfiColor, default_bg: FfiColor,
    cells: Vec<FfiCell> }`.
  - Transparent `FfiColor { rgb: u32, set: bool }`.
  - Transparent `FfiCell { text: String, fg: FfiColor, bg: FfiColor, flags: u8 }`
    (flags bitset: bold/italic/underline/undercurl/strikethrough).
  - Transparent `FfiKeyEvent { kind: u8, ch: u32, fnum: u8, ctrl, shift, alt,
    meta: bool }`; `kind` discriminates Char/Enter/Esc/Tab/Backspace/Delete/
    arrows/Home/End/PageUp/PageDown/F.
  - `extern "Rust"` fns/methods:
    - `wolf_ffi_start(init_lua: String, cwd: String, cols: u16, rows: u16) ->
      Option<WolfCoreHandle>` — build `CoreConfig`, wire wake, `WolfCore::start`;
      `None` on error (log via `tracing`).
    - `WolfCoreHandle::read_frame(&self) -> FfiFrame` — read snapshot; use
      `active_grid()`/`active_grid_id()`; per cell `highlights.resolve(hl_id)` →
      `FfiCell`; `cursor_visible = (cursor.grid == active_grid_id)`; defaults from
      the highlight table.
    - `WolfCoreHandle::send_key(&self, ev: FfiKeyEvent)` — map → `KeyEvent` →
      `send_input(InputEvent::Key(..))`.
    - `WolfCoreHandle::resize(&self, cols: u16, rows: u16)`.
    - `WolfCoreHandle::shutdown(&self)` — `.take()` then `WolfCore::shutdown`.
    - `FfiFrame` scalar accessors + `FfiFrame::cells(&self) -> Vec<FfiCell>`.
  - `extern "Swift" { fn wolf_ffi_wake(); }`.
  - private `key_event_from_ffi` mapper + `pack_style`/`FfiColor` helpers.
- `crates/wolf-ffi/src/bin/gen_bindings.rs` — standalone codegen bin using
  `swift-bridge-build` (`parse_bridges`); writes generated Swift + C header into
  `$BUILD_WORKSPACE_DIRECTORY/crates/wolf-ffi/generated/` (run via `bazel run`,
  which sets `BUILD_WORKSPACE_DIRECTORY` and can write in-tree — a genrule cannot).
- `crates/wolf-ffi/generated/` (committed): `SwiftBridgeCore.swift`,
  `SwiftBridgeCore.h`, `wolf-ffi.swift`, `wolf-ffi.h`, and a `module.modulemap`
  exposing the C headers as a module (e.g. `WolfFFI`).
- `crates/wolf-ffi/README.md`, `crates/wolf-ffi/BUILD.bazel`,
  `crates/wolf-ffi/tests/bridge_smoke.rs`.

**Codegen mechanism (recommended for all-Bazel):** standalone `rust_binary`
`//crates/wolf-ffi:gen_bindings` + `bazel run` rewriting committed files
(`OUT_DIR` from a build.rs is awkward to surface to `swift_library`). **Fallback:**
local `cargo build` with a `build.rs` calling `swift-bridge-build`, copy `OUT_DIR`
artifacts into `generated/`, commit. Document both.

### B. wolf-macos Swift app (`crates/wolf-macos`)

**Swift sources (one-line responsibility each):**
- `Sources/WolfMacOS/main.swift` — entry: `NSApplication.shared`, set delegate,
  `.regular` activation, `app.run()`.
- `Sources/WolfMacOS/AppDelegate.swift` — `NSApplicationDelegate`: resolve
  `init_lua` + cwd, `wolf_ffi_start`, build window + `GridView`, register the view
  with `WakeBridge`, terminate-after-last-window, `shutdown` on terminate.
- `Sources/WolfMacOS/MainWindow.swift` — the `NSWindow` ("Wolf", sized from cell
  metrics × initial cols/rows), `contentView = GridView`, key/front.
- `Sources/WolfMacOS/GridView.swift` — `NSView`: holds `WolfCoreHandle` +
  `CellMetrics`; `isFlipped = true`; `draw(_:)` reads `read_frame()`, fills cell
  bg + draws glyphs via CoreText, draws mode-aware cursor when `cursor_visible`;
  `acceptsFirstResponder`; `keyDown(_:)` → `WolfKeyMap.translate` → `send_key`;
  `setFrameSize`/`viewDidEndLiveResize` → compute (cols,rows) → `resize`.
- `Sources/WolfMacOS/CellMetrics.swift` — measure a monospaced `CTFont` (advance,
  ascent+descent, baseline); SF Mono → Menlo fallback; `WOLF_FONT_PT` override.
- `Sources/WolfMacOS/Cursor.swift` — `cursor_shape` (0/1/2 =
  Block/Horizontal/Vertical) → cursor rect + reverse-video colors (port gnome
  `cursor.rs`).
- `Sources/WolfMacOS/Color.swift` — packed `u32` + `set` → `CGColor`, with
  `FALLBACK_FG`/`FALLBACK_BG` (white/black) mirroring gnome `paint.rs`.
- `Sources/WolfMacOS/WakeBridge.swift` — implements `wolf_ffi_wake()`:
  `DispatchQueue.main.async { registeredView?.needsDisplay = true }`; weak
  singleton set by `AppDelegate`.
- `Sources/WolfKeyMap/KeyMap.swift` — **pure** (Foundation + generated FFI struct
  only, NO AppKit): `translate(keyCode, characters, charactersIgnoringModifiers,
  ctrl/shift/alt/meta) -> FfiKeyEvent?`. Honors the Shift contract: named keys
  first (keep real `shift`); printable fallback takes the **shifted glyph** from
  `characters` with `shift=false`; when ctrl/alt/meta held use
  `charactersIgnoringModifiers` (lowercased) as the `Char`; `nil` for bare
  modifiers / unmodified control chars. Factored out so it is unit-testable
  without a window. (AppKit analog of gnome's `gdk_key` seam.)

**Bundle:** `crates/wolf-macos/Info.plist` — `CFBundleExecutable`,
`CFBundleIdentifier` (`org.wolf.WolfMacOS`), `CFBundlePackageType=APPL`,
`NSPrincipalClass=NSApplication`, **`LSMinimumSystemVersion=26.0`**,
`NSHighResolutionCapable=true`. Plus `README.md`, `BUILD.bazel`.

**Bazel targets (this crate):**
- `swift_library(name="wolf_key_map")` — `Sources/WolfKeyMap/*.swift` + generated
  glue providing `FfiKeyEvent`; dep on the `cc_library` headers. No AppKit.
- `swift_library(name="wolf_macos_lib")` — `Sources/WolfMacOS/*.swift`; deps
  `:wolf_key_map`, `//crates/wolf-ffi:wolf_ffi_cc`; links AppKit/CoreText/
  CoreGraphics/Foundation.
- `macos_application(name="Wolf")` — `deps=[":wolf_macos_lib"]`,
  `infoplists=["Info.plist"]`, `minimum_os_version="26.0"`, `bundle_id`.

### C. Bazel wiring (MODULE.bazel + crate_universe)

- Add `bazel_dep` for `rules_swift` (repo_name `build_bazel_rules_swift`),
  `rules_apple` (repo_name `build_bazel_rules_apple`), `rules_xcodeproj` — pins
  per the orchestrator decision above (highest Bazel-8-compatible, apple_support
  ≥ 2.6.1, Xcode 26/Tahoe). Record final versions here once resolved.
- Add `"//crates/wolf-ffi:Cargo.toml"` to `crate.from_cargo` `manifests`; add
  `crates/wolf-ffi` to root `Cargo.toml` members; add `swift-bridge`
  (+`swift-bridge-build`) to wolf-ffi; update `Cargo.lock`; repin
  `CARGO_BAZEL_REPIN=1 bazel build //crates/wolf-ffi:wolf_ffi`; commit
  `Cargo.Bazel.lock` + `MODULE.bazel.lock`.
- `crates/wolf-ffi/BUILD.bazel`: `rust_static_library`/`rust_library` with
  `staticlib` crate-type → `libwolf_ffi.a` (deps `all_crate_deps` +
  `//crates/wolf-core:wolf_core`); `cc_library(name="wolf_ffi_cc")` exposing
  `generated/*.h` + `module.modulemap` and linking the archive; `rust_binary
  gen_bindings`; `exports_files(["Cargo.toml"])`.
- rules_xcodeproj: `xcodeproj(name="xcodeproj", project_name="Wolf",
  top_level_targets=["//crates/wolf-macos:Wolf"])`; output **uncommitted**; add
  `*.xcodeproj/` (or `/Wolf.xcodeproj`) to `.gitignore`.

### D. Testing (honest scope)

**Testable now:**
- **Pure Swift key translation** — `swift_test`
  `crates/wolf-macos/Tests/WolfKeyMapTests/KeyMapTests.swift` over `:wolf_key_map`:
  assert the Shift contract + named-key/printable/ctrl-combo/`nil` cases with
  scalar inputs (no `NSEvent`, no window). AppKit analog of gnome's headless
  `gdk_key` tests.
- **wolf-ffi Rust integration test** — `crates/wolf-ffi/tests/bridge_smoke.rs`:
  reads `WOLF_NVIM_BIN`; **loud SKIP** if unset (Bazel-on-macOS can't provide it
  yet; on Linux/Cargo devShell it runs); if set, drives the Rust half:
  `wolf_ffi_start` → `read_frame` dims/cursor sane → `send_key` a char →
  `read_frame` reflects it → `resize` → `shutdown`. Cannot call the `extern
  "Swift"` wake; the test wires a no-op/fallback wake (document in the header).
- **Build/link smoke** — `bazel build //crates/wolf-ffi:wolf_ffi` and
  `//crates/wolf-macos:Wolf` compile + link (app need not launch).

**NOT testable now (state why):** live AppKit `draw(_:)` (no headless window
server / no `cage` equivalent provisioned); `keyDown:` end-to-end (needs a real
responder/event loop — logic instead isolated in `WolfKeyMap`); Bazel-run
nvim-backed tests (`WOLF_NVIM_BIN` not provisioned on macOS Bazel). Do NOT mock.

**Minimum verification the coder MUST run:**
- `bazel build //crates/wolf-core:wolf_core` and `//crates/wolf-ffi:wolf_ffi`.
- GUI denylist clean for core (both):
  `bazel query 'deps(//crates/wolf-core:wolf_core)' | grep -iE
  'gtk|gdk|glib|cairo|pango|adwaita|objc|cocoa|core-graphics|winit|wgpu|skia|softbuffer'
  || echo clean` AND `cargo tree -e no-dev -p wolf-core`. Confirm `swift-bridge`
  entered ONLY `wolf-ffi`'s tree.
- `bazel build //crates/wolf-macos:Wolf` (compiles/links).
- `bazel test //crates/wolf-macos:WolfKeyMapTests` (+ the wolf-ffi test where env
  allows).

### E. Documentation impact

- `architecture.md` — fill the `#### wolf-macos` subsection (realized
  single-window shell); add a **prominent design-decisions record** (Tahoe-only,
  AppKit-not-SwiftUI, Swift, Xcode 26+, all-Bazel authoritative + uncommitted
  rules_xcodeproj, swift-bridge via `wolf-ffi` with committed bindings,
  structured-KeyEvent-over-the-bridge, opaque-frame snapshot); update the Build
  section ("currently builds only `wolf-core`" → +`wolf-ffi`+`wolf-macos`); note
  the wake → dispatch-to-main → `setNeedsDisplay` realization.
- `build.md` — update intro + "builds only wolf-core" line; add a macOS
  Swift/AppKit build section (`bazel build //crates/wolf-macos:Wolf`), bindings
  regen (`bazel run //crates/wolf-ffi:gen_bindings`), xcodeproj generation
  (`bazel run //crates/wolf-macos:xcodeproj`, uncommitted), and the repin note.
- `README.md` — add the **Tahoe-only** macOS app system requirement.
- `crates/wolf-core/README.md` — adjust the Deferred list (wolf-macos +
  swift-bridge now in progress).
- New `crates/wolf-ffi/README.md` + `crates/wolf-macos/README.md`.
- Reformat every edited/created Markdown with `prettier --write`; no hard line
  breaks within paragraphs.

### F. Implementation order

1. `wolf-ffi/Cargo.toml` + skeleton `src/lib.rs`; add to root members; add
   manifest to MODULE.bazel; repin; commit lockfiles.
2. Full `#[swift_bridge::bridge]` module + Rust glue.
3. `gen_bindings` bin; run; commit `generated/`.
4. `wolf-ffi/BUILD.bazel` (static lib + cc_library + gen_bindings); verify
   `bazel build //crates/wolf-ffi:wolf_ffi`.
5. Add rules_swift/apple/xcodeproj to MODULE.bazel; resolve/confirm versions.
6. Pure `WolfKeyMap` + its `swift_library` + `swift_test` FIRST.
7. AppKit sources + Info.plist; `swift_library` + `macos_application`; verify
   `bazel build //crates/wolf-macos:Wolf`.
8. `xcodeproj` target + `.gitignore`.
9. Tests (`bridge_smoke.rs`, `KeyMapTests.swift`).
10. Docs; `prettier --write`.

### G. Risks + fallbacks

- swift-bridge codegen under Bazel → committed `generated/` + `bazel run` regen;
  fallback local cargo build.rs regen + commit.
- rules_apple/swift vs Xcode 26 / Tahoe SDK → pin highest Bazel-8 releases, keep
  apple_support ≥ 2.6.1; if `macos_application` bundling breaks on the new SDK,
  verify via `swift_library` compile/link only and **defer bundling** — escalate.
- Wake FFI shape → fallback `extern "C" fn()` pointer shim.
- Non-`Send` `WolfCore` → main-thread-only handle by construction.
- Snapshot cost → per-cell String acceptable at 80×24; parallel-arrays later.
- GUI denylist leak → swift-bridge only in wolf-ffi; `wolf-core/Cargo.toml` gains
  NO new dep.

### H. Project-specific guard note

The Go/golangci conventions from any generic agent prompt do NOT apply (this repo
is Rust + Swift + Bazel). The equivalents: the **no-GUI-toolkit-in-wolf-core**
hard rule (cargo-tree + bazel-query denylists), clippy on hand-written Rust glue
(swift-bridge macro expansion may need a narrow scoped `#[allow]`), and Swift
formatting on the sources (keep `WolfKeyMap` AppKit-free).

---

## Tests written (test-writer) — coder MUST honor these contracts

Files: `crates/wolf-macos/Tests/WolfKeyMapTests/KeyMapTests.swift`,
`crates/wolf-ffi/tests/bridge_smoke.rs`, `crates/wolf-ffi/tests/common/mod.rs`.

Coder follow-ups the tests assume (keep the implementation in lockstep):
- **FfiKeyEvent.kind discriminants** (tests fixed them): mirror `wolf_core::Key`
  variant order — Char=0, Enter=1, …, Left=6/Right=7/Up=8/Down=9, …, F=14. The
  Swift `Kind` enum and Rust `KIND_CHAR` etc. must match `key_event_from_ffi`.
- **Wake symbol linking:** a Rust-only integration test cannot resolve
  `extern "Swift" fn wolf_ffi_wake`. wolf-ffi MUST expose a Swift-free wake path
  (a `#[cfg(test)]`/fallback no-op `wolf_ffi_wake`, or the planned `extern "C"`
  wake shim) so `wolf_ffi_start` links headlessly. (This nudges toward the
  fallback `extern "C" fn()` wake shape — coder's call, but the test must link.)
- **Crate-root surface:** tests reference `wolf_ffi::{wolf_ffi_start, FfiFrame,
  FfiKeyEvent, ...}` at the crate root — `pub use` them from the `ffi` module on
  the rlib surface.
- **`init_lua` is a PATH string** (mirrors `CoreConfig.init_lua`), not Lua text.
- **`shutdown()`** assumed `()`; if it returns `Result`, the test becomes
  `.expect(..)` — keep signatures consistent.
- **macOS virtual key codes:** tests use `kVK_*` (incl. Backspace =
  `kVK_Delete` 0x33, forward-delete = `kVK_ForwardDelete` 0x75); reconcile vs SDK.
- **Poll-not-seq wait:** `FfiFrame` exposes no `seq`, so the bridge test polls
  `read_frame()` with a 10s timeout. A frame-sequence accessor would be nicer
  later but is NOT required now.

## Progress log

- (orchestrator) Plan reset to the new feature. Architect produced a detailed,
  decisive plan. Orchestrator resolved the open items (Tahoe=26.0; SF Mono/Menlo;
  commit swift-bridge runtime; rule versions = coder resolves highest compatible;
  key translation in Rust).
- (orchestrator) test-writer hit a transient spend limit (no files written),
  retried successfully. Tests written + coder contracts recorded above. Next:
  coder (implement wolf-ffi + wolf-macos + Bazel wiring; run the verification).

## Implementation result (coder) — GREEN

- Resolved Bazel pins (highest BCR, Bazel-8 compat, apple_support stays 2.6.1,
  Xcode 26/Tahoe): **rules_swift 3.6.1, rules_apple 4.5.3, rules_xcodeproj 4.1.0,
  swift-bridge/swift-bridge-build 0.1.59**; rules_cc bumped 0.2.15→0.2.17 (MVS).
- Verification (verbatim PASS):
  - `bazel build //crates/wolf-core:wolf_core` → success
  - `bazel build //crates/wolf-ffi:wolf_ffi` → success
  - `bazel build //crates/wolf-macos:Wolf` → success (Wolf.app built + signed,
    out path `…-min26.0-…`; **macos_application bundling did NOT need deferral**)
  - `bazel test //crates/wolf-macos:WolfKeyMapTests` → PASSED 1/1
  - `bazel test //crates/wolf-ffi:bridge_smoke` → PASSED (loud SKIP, WOLF_NVIM_BIN
    unset — expected on the macOS Bazel host)
  - GUI denylist for core CLEAN; swift-bridge absent from
    `deps(//crates/wolf-core:wolf_core)`, present only in wolf-ffi; core Cargo.toml
    gained no dep. (`cargo tree` denylist N/A on macOS — no cargo; bazel query is
    the macOS equivalent.) clippy clean on wolf-ffi/gen_bindings.
- Deviations (justified, neither test modified):
  1. `FfiCell` made **opaque** (not transparent) — swift-bridge 0.1.59 doesn't
     emit `Vec` support for transparent structs (would link-fail on missing
     `Vec_FfiCell` symbols). Same flat row-major Vec-per-frame handoff; Swift
     reads via `text()/fg()/bg()/flags()`, Rust test reads `pub text`.
  2. `cursor_colors()->Vec<FfiColor>` replaced by `cursor_fill()`+`cursor_glyph()`
     (same transparent-struct-Vec reason). Not referenced by tests.
  3. One crate-level `#![allow(clippy::unnecessary_cast)]` for a same-type
     pointer cast in swift-bridge generated sibling code (plan sanctions a narrow
     allow).
- Wake: Swift-free path — `#[no_mangle] extern "C" fn wolf_ffi_set_wake(Option<
  extern "C" fn()>)` registers via a global AtomicPtr; `wolf_ffi_start` wires a
  `WakeFn` calling it (no-op if unregistered). Rust rlib links headlessly; the
  Swift `WakeBridge` registers the dispatch-to-main wake.

## Orchestrator note: Ghostty TCC prompt

The user saw a "Ghostty wants to access data from other apps" prompt during this
run. Cause: building/code-signing `Wolf.app` + running the XCTest bundle as
children of the terminal → macOS TCC attributes the request to Ghostty. Benign;
safe to Deny (tests/build need no cross-app data). Not a code problem.

## Decision on coder's recommended extra test

ACCEPT the headless deterministic Rust unit test: build a `wolf_core::Snapshot`
directly (no nvim) and assert `FfiFrame`'s color packing, flag bits, cursor
reverse-video (`cursor_fill`/`cursor_glyph`), `cursor_shape`, `cursor_visible`,
default fg/bg, and `FfiCell` accessors. This covers the resolution/packing logic
that today only runs when WOLF_NVIM_BIN is set (i.e. never on this host) — real,
non-mock, deterministic. Route to test-writer; coder then runs it.

## Review result — APPROVED with minor notes

Reviewer found no MUST-FIX correctness/boundary defects. Boundary clean
(wolf-core dep-unchanged, swift-bridge confined to wolf-ffi, WolfKeyMap
AppKit-free); Shift contract correct end-to-end; FFI sound (main-thread handle,
stateless Send+Sync wake, no use-after-shutdown); 3 deviations justified; Bazel
wiring + Info.plist/minimum_os_version=26.0 + gitignored xcodeproj all correct.

Routed findings:
- SHOULD-FIX 1 (test-writer) headless packing test absent → DONE: parallel
  test-writer wrote `crates/wolf-ffi/tests/frame_pack.rs` (public-API, no nvim).
- SHOULD-FIX 2 (coder) `CellMetrics.swift:22-23` doc claims bold/italic variants
  are derived per cell, but `GridView` never reads `cell.flags()` — attributes
  are resolved+packed but NOT consumed in paint. DECISION: glyph attribute
  rendering (bold/italic/underline) was NOT in this increment's scope (plan: "per
  cell fg/bg … mode-aware cursor"); correct the comment to say attributes are
  packed but not yet painted (deferred), rather than wiring it now.
- SHOULD-FIX 3 (technical-writer) missing per-source-dir READMEs:
  `crates/wolf-ffi/src/`, `crates/wolf-ffi/tests/`, `crates/wolf-macos/Sources/`,
  `crates/wolf-macos/Tests/` (CLAUDE.md: each source dir needs a README.md).
- NIT 4 (coder) `Cursor.swift:18-19` stale accessor name `cursor_colors()` →
  `cursor_fill()`/`cursor_glyph()` (comment only; build is green).
- NIT 5 / Observation 6 (technical-writer): broad docs pending (architecture.md
  wolf-macos stub, build.md, README.md Tahoe-only, wolf-core README deferred
  list); and document the known limitation that entering cmdline mode swaps the
  whole view to the outer grid (no cmdline strip yet — gnome has one).

## frame_pack.rs — coder wiring needed (from test-writer)

1. Make `FfiFrame::from_snapshot` **pub** in `crates/wolf-ffi/src/lib.rs` (no
   behavior change; `read_frame` keeps delegating). Only app-code edit needed.
2. Add a `rust_test(name="frame_pack", srcs=["tests/frame_pack.rs"],
   crate_root=..., edition="2021", deps=[":wolf_ffi",
   "//crates/wolf-core:wolf_core"])` target to `crates/wolf-ffi/BUILD.bazel`.
Then run it (headless, deterministic — must PASS regardless of WOLF_NVIM_BIN).

## Status

- [x] Architect plan
- [x] Tests written (+ headless frame_pack.rs added)
- [x] Implementation + tests passing
- [x] Review (APPROVED w/ minor notes)
- [ ] Coder cleanup (wire frame_pack + 2 doc-comment fixes)
- [ ] Docs (technical-writer)
