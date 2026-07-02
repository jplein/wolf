# Current Plan: Bundle JetBrains Mono + wolf-config.yaml preferences (macOS)

## Problem statement

Bundle the JetBrains Mono font (regular, italic, bold, bold italic + OFL license) into
Wolf, and add plumbing for reading a user preferences file, fully wired on macOS.

### Confirmed scope
- **Front-ends:** macOS only this increment. The shared reader in `wolf-core` must still
  resolve BOTH platform paths (macOS + Linux) in code, but only macOS bundles the font and
  consumes prefs now. GNOME/GTK wiring is deferred — do NOT build/test the GTK front-end
  (NixOS/Cargo-only, unverifiable on this macOS host).
- **Depth:** FULL wiring — apply font-family, font-size, and line-height end-to-end into the
  macOS grid rendering (CellMetrics.swift + consumers). Keep `WOLF_FONT_PT` as an override if
  practical (tests use it), but the config is the new source of the defaults.
- **Font delivery:** hermetic download + sha256 pin, matching the existing
  `bazel/orlando` + `bazel/nvim` + `bazel/lighthouse` pattern.

### Requirements
1. Bundle JetBrains Mono: 4 TTFs (regular, italic, bold, bold italic) + OFL license.
2. Preferences reading plumbing:
   - macOS path: `~/Library/Preferences/Wolf` (preference dir via `dirs`/`directories`).
   - Linux path: `~/.config/wolf` (config dir via same API).
   - File `wolf-config.yaml`:
     ```
     config:
       font-family: "<font name>"
       font-size: <number>
       line-height: <percentage of font-size>
     ```
   - If absent, CREATE with defaults: font-family `"JetBrains Mono"`, font-size 14 (macOS) /
     10 (Linux), line-height 130%.

### Architecture constraints
- YAML parse + path resolution + create-with-defaults live in `wolf-core` — must stay
  GUI-toolkit-free (verify via the cargo tree / bazel query denylist in
  `crates/wolf-core/README.md`).
- Expose resolved config to Swift over the `wolf-ffi` swift-bridge surface.
- Register the bundled TTFs so CoreText resolves "JetBrains Mono".
- Divergence: `config-decisions-report.md` calls the file `config.yaml`; user's explicit spec
  is `wolf-config.yaml` with a top-level `config:` key. Follow the user's spec, update docs,
  flag the divergence.
- Keep every touched source dir's README.md in sync; reformat edited Markdown with
  `prettier --write`, no hard line breaks in paragraphs.

## Status
- [x] Architect plan
- [x] Tests written (config.rs test module, wolf-ffi tests/config.rs, jetbrains_mono_tree_test.sh, bundle_layout_test.sh k–o, CellMetricsMathTests.swift)
- [x] Implementation done, all targets green (49 wolf-core tests incl. valid-YAML-wrong-shape
  fallback; wolf-ffi config; jetbrains_mono_tree_test; bundle_layout_test k–o; WolfKeyMapTests;
  bridge_smoke/frame_pack no-regression; GUI-free denylist clean). Font pin: JetBrains Mono v2.304,
  sha256 `6f6376c6ed2960ea8a963cd7387ec9d76e3f629125bc33d1fdcd7eb7012f7bbf`, FLAT archive
  (`fonts/ttf/*.ttf` + root `OFL.txt`). Declined: non-ASCII FFI round-trip (marginal),
  FontRegistration e2e (headless-untestable, eyeball-only).
- [x] Review — APPROVED, no CRITICAL/MAJOR/MINOR defects. All 5 areas confirmed correct.
- [x] Docs updated — new bazel/jetbrains-mono/README.md + 8 docs updated; config-decisions-report
  gained a "Realized (this increment, macOS)" reconciliation. prettier --write clean (all unchanged).

## STATUS: COMPLETE

---

## Architect's plan

### 1. Summary
New `//bazel/jetbrains-mono` module (clone of orlando/lighthouse) hermetically downloads +
sha256-pins JetBrains Mono, bundling 4 TTFs (Regular/Italic/Bold/BoldItalic) + OFL.txt at
`Contents/Resources/fonts/`. New GUI-free `wolf-core` `config` module resolves the platform
prefs dir, reads/creates `wolf-config.yaml` (`config: { font-family, font-size, line-height }`),
bridges to Swift over a new `wolf-ffi` surface, and fully wires font-family/size/line-height
into macOS `CellMetrics`. Linux path resolution implemented in code; GTK not built/tested.

### 2. Files to CREATE
- `bazel/jetbrains-mono/extensions.bzl` — module extension `jetbrains_mono`, single `http_archive`
  (no `select()`, pure data). Constants `_VERSION="2.304"`,
  `_URL=".../releases/download/v2.304/JetBrainsMono-2.304.zip"`, `_SHA256=<coder computes live>`.
  `build_file_content`: filegroup globbing EXACTLY the 4 TTFs (under `fonts/ttf/`) + `OFL.txt`,
  then `copy_to_directory` `jetbrains_mono_tree` with `replace_prefixes` remapping into inner
  `fonts/` (rules_apple strips the tree's own top dir → lands at `Contents/Resources/fonts/`).
  **Coder MUST `unzip -l` to confirm archive root (wrapped vs flat) + exact TTF filenames.**
- `bazel/jetbrains-mono/BUILD.bazel` — `alias jetbrains_mono_tree`, `sh_test jetbrains_mono_tree_test`.
- `bazel/jetbrains-mono/jetbrains_mono_tree_test.sh` — runfiles resolver (inner dir `fonts/`);
  assert 4 TTFs non-empty + OFL.txt present.
- `bazel/jetbrains-mono/README.md` — clone of orlando README (pin, targets, how-it-works, bump recipe).
- `crates/wolf-core/src/config.rs`:
  - `pub struct WolfConfig { pub font_family: String, pub font_size: f64, pub line_height: f64 }`
  - private serde `ConfigFile { config: ConfigSection }`, `ConfigSection` with
    `#[serde(rename_all="kebab-case")]` mapping font-family/font-size/line-height.
  - consts: `CONFIG_FILE_NAME="wolf-config.yaml"`, `DEFAULT_FONT_FAMILY="JetBrains Mono"`,
    `DEFAULT_LINE_HEIGHT=130.0`, `DEFAULT_FONT_SIZE_MACOS=14.0`, `DEFAULT_FONT_SIZE_LINUX=10.0`.
  - `pub fn preferences_dir() -> Option<PathBuf>` — honor `WOLF_CONFIG_DIR` env override FIRST
    (test/determinism hook); else macOS `dirs::preference_dir()?.join("Wolf")`, Linux
    `dirs::config_dir()?.join("wolf")`.
  - `pub fn default_config() -> WolfConfig` — font_size via `cfg!(target_os="macos")`.
  - `pub fn load_or_create(dir: &Path) -> WolfConfig` — INFALLIBLE (defaults-on-error): absent →
    create parent + write serialized defaults, return defaults; present+parses → parsed;
    present+malformed → `tracing::warn!` + defaults WITHOUT overwriting the user file.
- `crates/wolf-ffi/tests/config.rs` — deterministic (no nvim): `WOLF_CONFIG_DIR`→tempdir; known
  yaml round-trips via `wolf_ffi_load_config()`; absent-file creates defaults.
- `crates/wolf-macos/Sources/WolfMacOS/FontRegistration.swift` —
  `enum FontRegistration { static func registerBundledFonts() }` over
  `Bundle.main.resourcePath+"/fonts"/*.ttf` via `CTFontManagerRegisterFontURLs(.process)`.
  Logs, never fails, on absent dir (dev/Linux parity). Compile+eyeball only.
- `crates/wolf-macos/Sources/WolfKeyMap/CellMetricsMath.swift` — Foundation-only pure seam:
  `CellMetricsMath.cellHeight(fontSize:lineHeightPercent:ascent:descent:leading:) -> Double` and
  `.baseline(cellHeight:ascent:descent:) -> Double`. Guard `lineHeightPercent<=0` → fall back to
  `ascent+descent+leading`.
- `crates/wolf-macos/Tests/WolfKeyMapTests/CellMetricsMathTests.swift` — 130%@14, 100% natural,
  degenerate guard, baseline recentering.

### 3. Files to MODIFY
- `crates/wolf-core/Cargo.toml` — add `serde{derive}`, `serde_yaml="0.9"`, `dirs="5"` (all GUI-free).
- `crates/wolf-core/src/lib.rs` — `pub mod config;` + re-export
  `load_or_create, preferences_dir, default_config, WolfConfig`.
- `crates/wolf-ffi/src/lib.rs` — in the bridge add OPAQUE `type FfiConfig` with
  `font_family()->String`, `font_size()->f64`, `line_height()->f64`, and free
  `fn wolf_ffi_load_config() -> FfiConfig` (standalone, like `wolf_ffi_ancestors_to_reveal`;
  does NOT hang off WolfCoreHandle). Impl calls `preferences_dir().map(load_or_create)
  .unwrap_or_else(default_config)`. **Regenerate committed bindings via
  `bazel run //crates/wolf-ffi:gen_bindings` — do NOT hand-edit generated files.**
- `MODULE.bazel` — `use_extension`/`use_repo` for jetbrains_mono after the lighthouse block.
- `crates/wolf-macos/BUILD.bazel` — add `//bazel/jetbrains-mono:jetbrains_mono_tree` to Wolf
  `resources`; add `CellMetricsMath.swift` to `wolf_key_map` srcs +
  `CellMetricsMathTests.swift` to `WolfKeyMapTests` srcs. (FontRegistration.swift auto-globbed.)
- `crates/wolf-macos/bundle_layout_test.sh` — checks (k)–(o): 4 TTFs + OFL at
  `Contents/Resources/fonts/`.
- `crates/wolf-macos/Sources/WolfMacOS/CellMetrics.swift` — `measured()` → `measured(config: FfiConfig)`:
  point size = `WOLF_FONT_PT` override else `config.font_size`; font = `config.font_family`
  (verify `CTFontCopyFamilyName` case-insensitive; Menlo fallback); cellHeight/baseline via
  `CellMetricsMath`. Update doc comment.
- `crates/wolf-macos/Sources/WolfMacOS/AppDelegate.swift` — in `applicationDidFinishLaunching`,
  BEFORE measuring: `FontRegistration.registerBundledFonts()`, `let config = wolf_ffi_load_config()`,
  `CellMetrics.measured(config: config)`. No other consumer changes (MainWindow/PaneContainerView
  consume the struct fields unchanged).

### 4. Test plan
- wolf-core `#[cfg(test)] mod tests` (tempfile; runs under cargo + `wolf_core_lib_test`):
  `creates_file_with_defaults_when_absent`, `parses_existing_file` (asserts not rewritten),
  `round_trips`, `malformed_file_falls_back_to_defaults`, `missing_field_falls_back_to_defaults`,
  `defaults_are_platform_specific`, `preferences_dir_honors_env_override` (serialize env-mutating tests).
- wolf-ffi `tests/config.rs` + new `rust_test` `config` (no nvim staging, no no-sandbox tag):
  `load_config_round_trips`, `load_config_creates_defaults_when_absent`.
- `//bazel/jetbrains-mono:jetbrains_mono_tree_test` — 4 TTFs + OFL.
- `//crates/wolf-macos:bundle_layout_test` — new checks (k)–(o).
- `//crates/wolf-macos:WolfKeyMapTests` gains `CellMetricsMathTests`.
- NOT headless-testable (state explicitly): font registration, CTFont family resolution, on-screen
  rendering → manual-smoke/eyeball. Only `CellMetricsMath` is a pure extracted seam.
- Do NOT plan any GTK/Linux build or test.

### 5. Impl order
1. `bazel/jetbrains-mono/` + MODULE.bazel; verify URL live + sha256 + `unzip -l`; run tree test.
2. wolf-core config (Cargo.toml → config.rs → lib.rs → in-module tests); `CARGO_BAZEL_REPIN=1`;
   run `wolf_core_lib_test` + cargo test; re-run cargo tree / bazel query denylist.
3. wolf-ffi surface + regenerate bindings + `tests/config.rs` + BUILD target.
4. macOS pure seam `CellMetricsMath` + test + BUILD.
5. macOS wiring (CellMetrics rewrite, FontRegistration, AppDelegate order, BUILD resources/srcs,
   bundle_layout_test); build Wolf; run bundle_layout_test.
6. Docs.

### 6. Docs to update (prettier --write; no hard breaks)
`crates/wolf-core/README.md` + `src/README.md`, `crates/wolf-ffi/README.md`,
`crates/wolf-macos/README.md` + `Sources/README.md`, `bazel/jetbrains-mono/README.md` (new),
`build.md` (bundled fonts + new test targets), `architecture.md` (macOS metrics paragraph +
FfiConfig), `config-decisions-report.md` (reconcile divergence — see risk 9).

### 7. Flagged risks
1. **sha256 must be computed live (BLOCKING).** Proposed v2.304;
   `curl -L .../v2.304/JetBrainsMono-2.304.zip -o f.zip && shasum -a 256 f.zip`.
2. **Archive root layout unverified** — release-asset zip may be wrapped or flat; `unzip -l` and set
   `replace_prefixes` accordingly (wrong → glob matches nothing).
3. **TTF filenames** — assumed `JetBrainsMono-{Regular,Italic,Bold,BoldItalic}.ttf` under `fonts/ttf/`;
   confirm casing.
4. **`dirs` not `directories`** — `ProjectDirs` yields `org.Wolf.Wolf`, wrong; `dirs::preference_dir()/"Wolf"`
   + `dirs::config_dir()/"wolf"` match the spec. `dirs`/`dirs-sys`/`option-ext` not yet vendored → repin.
5. **swift-bridge String-in-struct** — chose opaque FfiConfig + accessors (matches FfiFileEntry).
6. **line-height semantics** — `cellHeight=round(fontSize*lineHeight/100)`, baseline recentered;
   guard `<=0`. Localized in CellMetricsMath.
7. **font-size f64** (spec `<number>`; allows 13.5). serde_yaml parses int 14 into f64 fine.
8. **Defaults-on-error, never clobber a malformed user file** (graceful-degradation posture).
9. **config-decisions-report divergence to record:** (a) file named `wolf-config.yaml` w/ top-level
   `config:` key vs report's `config.yaml`; (b) this is the USER-level file at the platform prefs dir,
   not the project `.wolf/` layer; (c) read directly in Rust core + bridged over swift-bridge, NOT
   from Neovim over RPC. All intentional per user spec — record as realized; project layer + nvim-RPC
   bridge still deferred.
10. `serde_yaml` in maintenance but stable/GUI-free — acceptable.
11. `WOLF_CONFIG_DIR` env test hook can race — serialize env-mutating tests.
