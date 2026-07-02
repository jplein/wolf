# Current Plan: Bundle iA Writer Mono S (2nd font) + make it the default (macOS)

## Problem statement

Add a SECOND bundled font — **iA Writer Mono S** — into Wolf alongside the existing JetBrains
Mono (do NOT remove JetBrains Mono), and make **iA Writer Mono S the new DEFAULT font-family**.
Direct follow-on to the completed JetBrains Mono bundling + `wolf-config.yaml` work; reuse that
exact pattern.

### Confirmed scope
- **macOS only.** GNOME/GTK deferred — do NOT build/test the GTK front-end.
- **Hermetic download + sha256 pin**, mirroring `bazel/jetbrains-mono` / `bazel/orlando` /
  `bazel/lighthouse`.
- **Full wiring:** iA Writer Mono S becomes the default in wolf-core config; bundled TTFs are
  CoreText-registered so it resolves.

### Requirements
1. New hermetic Bazel module (e.g. `//bazel/ia-writer-mono`) downloading + sha256-pinning the iA
   Writer fonts; TreeArtifact carries the 4 iA Writer Mono S faces (Regular, Italic, Bold,
   BoldItalic) + its OFL/SIL license, bundled into `Wolf.app`.
2. KEEP JetBrains Mono bundled exactly as-is (module, resources entry, bundle_layout_test checks
   k–o all stay). Add iA ALONGSIDE it.
3. Land the iA TTFs at `Contents/Resources/fonts/` (SAME dir as JBM) so the existing
   `FontRegistration.swift` (globs `Contents/Resources/fonts/*.ttf`) auto-registers them with NO
   Swift change — IF rules_apple allows two TreeArtifacts to merge into that subdir.
   **BUILD-VERIFY** (`bazel build //crates/wolf-macos:Wolf`). If rules_apple rejects the shared
   dir, fall back to the lowest-churn alternative (single combined tree, or per-family subdirs +
   FontRegistration recursion) — architect decides, coder verifies by building.
4. Flip the wolf-core config DEFAULT font-family from `"JetBrains Mono"` to iA Writer Mono S
   (`config.rs` `DEFAULT_FONT_FAMILY` ~line 22). The default string MUST be the font's actual
   CoreText **name-table family name** so `CTFontCreateWithName` resolves it (else CellMetrics
   silently falls back to Menlo). User wrote "iA Writer Mono S" — coder MUST verify the real
   family name (inspect the TTF name table / fc-scan / register+CTFontCopyFamilyName) and use
   whatever actually resolves; if it differs, use the real name and flag it.

### Architecture / risk notes
- iA fonts live at github.com/iaolo/iA-Fonts. That repo most likely has NO release assets → pin a
  **COMMIT SOURCE TARBALL** (`/archive/<commit>.tar.gz`) like `bazel/orlando` (http_archive
  `strip_prefix` for the wrapping `<repo>-<commit>/` dir), NOT a release-asset zip like
  jetbrains-mono. Coder verifies live: pick a commit, curl tarball, compute sha256, `tar -tzf` to
  confirm exact internal paths. **The iA Writer Mono S static TTFs sit under a directory WITH
  SPACES** (e.g. `iA Writer Mono/Static/…`) — handle spaces in the glob + `replace_prefixes`.
- Glob ONLY the 4 style TTFs + the license (exclude Duo/Quattro, variable fonts, other weights,
  webfonts). Confirm the 4 styles exist as separate static TTFs.
- TreeArtifact inner dir must be `fonts/` (rules_apple strips the tree's own top dir) → files land
  at `Contents/Resources/fonts/`. `copy_to_directory replace_prefixes` remaps the spaced source
  path to flat `fonts/<file>.ttf`.
- Update `config.rs` test expectation `EXPECT_DEFAULT_FONT_FAMILY` (~line 193) + any wolf-ffi
  config-test default-family expectation to the new default. Malformed/absent/round-trip config
  semantics unchanged.
- bundle_layout_test.sh: ADD checks for the 4 iA TTFs + iA license at `Contents/Resources/fonts/`,
  KEEP the existing JBM checks (k–o). Add a jetbrains-mono-style tree test for the new module.
- The wolf-ffi FFI surface (`FfiConfig` / `wolf_ffi_load_config`) does NOT change — only the
  default value flows through. No swift-bridge binding regeneration.

## Status
- [x] Architect plan
- [x] Tests written (ia_writer_mono_tree_test.sh; bundle_layout_test.sh p–t; EXPECT_DEFAULT_FONT_FAMILY
  flipped to "iA Writer Mono S" in config.rs test module + wolf-ffi/tests/config.rs)
- [x] Implementation + tests passing. Pin: iA-Fonts commit
  `f32c04c3058a75d7ce28919ce70fe8800817491b`, sha256
  `826767eaea4ce43ccb7ca71327a5c63ac31206b99df400699e6744df6104e1b5`. Approach (a) — two trees
  merge into Contents/Resources/fonts/ (Wolf build SUCCESS, disjoint file sets, zero Swift change).
  Real CoreText family name VERIFIED = "iA Writer Mono S" (fontTools nameID-1 + live
  CTFontCreateWithName→CTFontCopyFamilyName, not Menlo). All green:
  ia_writer_mono_tree_test, bundle_layout_test (a–t), wolf_core_lib_test, wolf-ffi:config.
  No test-expectation mismatch → no test reroute needed.
- [x] Review — APPROVED. All 5 areas correct (default strings match byte-for-byte; replace_prefixes
  produces inner fonts/; JBM still bundled). Minor: 3 stale Swift inline comments to fix in docs pass —
  FontRegistration.swift L3/L10 ("JetBrains Mono...default grid font"), AppDelegate.swift L36
  ("Register the bundled JetBrains Mono TTFs"); CellMetrics.swift L85 is a benign example (leave).
- [x] Docs updated — new bazel/ia-writer-mono/README.md + 8 docs updated (incl. a stale claim fixed in
  bazel/jetbrains-mono/README.md); 3 stale Swift inline comments reworded (FontRegistration.swift,
  AppDelegate.swift). prettier --write clean (all 9 unchanged).

## STATUS: COMPLETE

---

## Architect's plan

### Verified upstream facts (coder MUST re-confirm live via `tar -tzf`)
github.com/iaolo/iA-Fonts (master), NO release assets → commit-tarball pin (orlando pattern):
- Top dirs: `iA Writer Duo/`, `iA Writer Mono/`, `iA Writer Quattro/`.
- License is PER-FONT-DIR at **`iA Writer Mono/LICENSE.md`** (SIL OFL 1.1) — NOT repo-root.
- `iA Writer Mono/Static/` has exactly the 4 static faces:
  `iAWriterMonoS-{Regular,Italic,Bold,BoldItalic}.ttf`.
- Tentative CoreText family name: **`iA Writer Mono S`** — MUST be verified vs the name table.

### 1. New `//bazel/ia-writer-mono` (mirror bazel/orlando COMMIT-TARBALL, not jetbrains-mono zip)
- `extensions.bzl`: `http_archive` of `.../iaolo/iA-Fonts/archive/<commit>.tar.gz`,
  `strip_prefix = "iA-Fonts-<commit>"`, consts `_COMMIT`/`_SHA256` (coder computes live).
  Overlay: `filegroup runtime_files` with EXPLICIT srcs (not glob — de-risk spaces) =
  the 4 TTFs `iA Writer Mono/Static/iAWriterMonoS-*.ttf` + `iA Writer Mono/LICENSE.md`;
  then `copy_to_directory ia_writer_mono_tree` with
  `replace_prefixes = { "iA Writer Mono/Static": "fonts", "iA Writer Mono/LICENSE.md": "fonts/LICENSE.md" }`
  → inner `fonts/` (4 TTFs + LICENSE.md). rules_apple strips top dir → Contents/Resources/fonts/.
- `BUILD.bazel`: `alias ia_writer_mono_tree` + `sh_test ia_writer_mono_tree_test` (data=[:tree]).
- `ia_writer_mono_tree_test.sh`: clone jetbrains_mono_tree_test.sh; assert inner `fonts/` has
  4 iA TTFs (a–d) + `LICENSE.md` (e), non-empty.
- `README.md`: mirror orlando/jetbrains README (pin, spaced-dir note, replace_prefixes, targets,
  MODULE wiring, bump recipe). Note it SHARES Contents/Resources/fonts/ with JBM.
- `MODULE.bazel`: `use_extension`/`use_repo` after the jetbrains_mono block; regen MODULE.bazel.lock.

### 2. CRITICAL LAYOUT DECISION — two trees, one bundle dir (#1 RISK)
Both `jetbrains_mono_tree` + `ia_writer_mono_tree` (each inner `fonts/`) in Wolf `resources` →
Contents/Resources/fonts/. Best assessment: MERGES (disjoint file sets: JetBrainsMono-*.ttf+OFL.txt
vs iAWriterMonoS-*.ttf+LICENSE.md — no path collision). **Coder MUST BUILD-VERIFY**
`bazel build //crates/wolf-macos:Wolf`.
- (a) PREFERRED if merge builds: just add 2nd tree to resources; ZERO Swift change (FontRegistration
  already globs fonts/*.ttf). Ship this.
- (b) FALLBACK if it errors: single combined tree — new `//bazel/fonts:fonts_tree`
  `copy_to_directory(srcs=["@jetbrains_mono//:runtime_files","@ia_writer_mono//:runtime_files"],
  replace_prefixes={"fonts/ttf":"fonts","OFL.txt":"fonts/OFL.txt","iA Writer Mono/Static":"fonts",
  "iA Writer Mono/LICENSE.md":"fonts/LICENSE.md"})`; replace the 2 font resources with it. Bundle
  paths + bundle_layout_test + Swift unchanged. (NOTE: coder must add a `runtime_files` filegroup to
  the JBM overlay if it doesn't already expose one — JBM currently uses a filegroup; confirm.)
- (c) LAST RESORT: per-family subdirs + FontRegistration recursion (changes JBM layout + k–o paths).
Recommendation: try (a); fall back to (b).

### 3. Default flip — `crates/wolf-core/src/config.rs`
- Line 22 `DEFAULT_FONT_FAMILY: "JetBrains Mono"` → verified iA family name (tentative
  "iA Writer Mono S"). Update field doc (~L39) + default_config doc (~L86).
- **Coder MUST verify the real name-table family BEFORE editing the string:**
  `python3 -c "from fontTools.ttLib import TTFont; f=TTFont('iAWriterMonoS-Regular.ttf');
  [print(r.nameID,r.toUnicode()) for r in f['name'].names if r.nameID in (1,4,6,16)]"`
  or `fc-scan --format '%{family}\n' <ttf>` or `mdls -name com_apple_ats_name_family <ttf>`,
  and definitively `CTFontCopyFamilyName` after register. If it differs, use the REAL name + flag.
- Schema/serialization otherwise unchanged.

### 4. Tests to update/add
- `config.rs` test module L193 `EXPECT_DEFAULT_FONT_FAMILY` → new default (match L22 exactly).
  `CUSTOM_FONT_FAMILY = "Fira Code"` stays distinct. Malformed/partial/wrong-shape/never-clobber
  cases unchanged (assert `== default_config()`).
- `crates/wolf-ffi/tests/config.rs` L44 `EXPECT_DEFAULT_FONT_FAMILY` → new default.
- New `//bazel/ia-writer-mono:ia_writer_mono_tree_test` (4 iA TTFs + LICENSE.md).
- `crates/wolf-macos/bundle_layout_test.sh`: KEEP a–o (incl JBM k–o); ADD p–s (4 iAWriterMonoS-*.ttf
  non-empty at Contents/Resources/fonts/) + t (`fonts/LICENSE.md` non-empty). Update PASS echo.
- Eyeball-only: FontRegistration/CellMetrics on-screen resolution — coder MUST build+launch .app and
  confirm iA family resolves (NOT Menlo fallback).
- No swift-bridge regeneration (FFI surface unchanged).

### 5. Impl order
1. Pick commit + sha256; `tar -tzf` confirm paths. 2. Create bazel/ia-writer-mono/. 3. MODULE.bazel
+ regen lock; run tree test. 4. Add tree to Wolf resources; `bazel build //crates/wolf-macos:Wolf`
(merge verify); fallback (b) if errors. 5. bundle_layout_test p–t; run it. 6. Verify real family
name. 7. Flip config.rs. 8. Update 2 test expectations; run wolf_core_lib_test + wolf-ffi:config.
9. Build+launch .app; confirm iA resolves. 10. Docs + prettier.

### 6. Docs to update (prettier --write; no hard breaks)
New `bazel/ia-writer-mono/README.md`; `crates/wolf-core/README.md` + `src/README.md` (new default);
`crates/wolf-macos/README.md` + `Sources/README.md` (TWO fonts, iA is default);
`build.md` (new module + tree test + bump recipe; both fonts; bundle_layout p–t);
`architecture.md` (default = iA Writer Mono S, both bundled+registered, Menlo fallback);
`config-decisions-report.md` "Realized" note (default now iA Writer Mono S).

### 7. Flagged risks (ranked)
1. **Two trees → one fonts/ dir** — merge vs error; gated by `bazel build Wolf`; fallback (b) ready.
2. Live commit + sha256 (placeholder) — `tar -tzf` before trusting globs.
3. Spaces in `iA Writer Mono/Static/` — prefer explicit srcs; verify by building the tree.
4. Real CoreText family string — tentative; wrong string silently falls back to Menlo (no test
   catches it, only eyeball) → launch verification MANDATORY.
5. All 4 static styles ship — confirm at pinned commit.
