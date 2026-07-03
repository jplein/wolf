# Feature: macOS Tahoe app icon with light/dark switching

## FOLLOW-UP (user request, 2026-07-03): assemble Wolf.icon at build time — no committed PNG duplicates

> **STATUS: COMPLETE.** Implemented, reviewed (PASS), reviewer's minor (visibility scoping on resources/icons exports_files) applied and re-verified. Remaining for user: manual smoke + commit (`git add resources/` — includes the new BUILD.bazel — with the staged mv/rm already in the index).

The user wants the two PNGs inside `crates/wolf-macos/Wolf.icon/Assets/` to stop being committed copies; the `.icon` bundle should be assembled at build time directly from `resources/icons/`.

### Design (orchestrator-approved)

- **Constraint discovered in rules_apple validation** (`_verify_icon_dirs` in actool.bzl): all `app_icons` files must resolve to exactly ONE `*.icon` directory. Source files and generated files live under different roots (source tree vs bazel-out), so a mixed bundle (source icon.json + generated PNGs) would count as TWO `Wolf.icon` dirs and fail. Therefore ALL THREE files must be generated into the same output-root `Wolf.icon/` dir.
- **Label conflict**: a generated file may not share its package-relative path with a source file (same label). So the committed `Wolf.icon/icon.json` must move to a different source path, e.g. `crates/wolf-macos/Wolf.icon.json` (still committed, hand-authored config — only its location changes); a copy_file re-materializes it at `Wolf.icon/icon.json` in the output tree.
- **Mechanism**: `copy_file` from bazel_skylib. Add `bazel_dep(name = "bazel_skylib", version = "1.9.0")` to MODULE.bazel (1.9.0 is what MVS already resolves transitively — no repin expected; follow the file's comment conventions for dep floors).
  ```python
  load("@bazel_skylib//rules:copy_file.bzl", "copy_file")
  copy_file(name = "wolf_icon_json",  src = "Wolf.icon.json", out = "Wolf.icon/icon.json")
  copy_file(name = "wolf_icon_light", src = "<label of light 1024 png>", out = "Wolf.icon/Assets/wolf-light.png")
  copy_file(name = "wolf_icon_dark",  src = "<label of dark 1024 png>",  out = "Wolf.icon/Assets/wolf-dark.png")
  # Wolf target: app_icons = [":wolf_icon_json", ":wolf_icon_light", ":wolf_icon_dark"]
  ```
- **Spaces in source labels**: the source dirs (`resources/icons/Wolf application icon macOS light/` etc.) contain spaces, which Bazel labels may not support. FIRST run a cheap experiment: add `resources/icons/BUILD.bazel` with `exports_files` for the two `icon-silhouette_512x512@2x.png` files and try `bazel query` / the build. If spaces fail (expected), RENAME the four subdirectories to `macos-light/`, `macos-dark/`, `gnome-light/`, `gnome-dark/` (fixes the inconsistent "macOS light"/"macos dark" casing too) and update all doc references + the update recipe.
- **Cleanup**: delete the committed `crates/wolf-macos/Wolf.icon/Assets/*.png` and the now-source-empty `Wolf.icon/` dir; keep `.DS_Store` insurance if a glob remains anywhere (explicit label list needs none).
- **Docs**: update `resources/icons/README.md` (recipe becomes: re-export from Figma over the 1024 file, rebuild — no cp step; new dir names if renamed), `crates/wolf-macos/README.md`, the `Wolf` target comment in BUILD.bazel, `build.md` if it mentions committed copies. prettier + no hard line breaks, as before.
- **Verification**: `bazel build //crates/wolf-macos:Wolf`; `/usr/bin/assetutil --info` on Assets.car must still show `NSAppearanceNameDarkAqua → wolf-dark` and `NSAppearanceNameAqua → wolf-light`; regression `bazel test //crates/wolf-macos:bundle_layout_test //crates/wolf-macos:WolfKeyMapTests`. NO new tests (standing user directive).
- **Answered user question**: only the two 1024×1024 (`512x512@2x`) PNGs are consumed; actool derives all 7 icon sizes per appearance from the 1024 master. All other sizes in resources/icons are unconsumed by the build.

### Follow-up coder output

- **Spaces in labels WORK on Bazel 8.7.0** (verified empirically: query, direct build, full app build) — no directory renames; `resources/icons/BUILD.bazel` (new) `exports_files` the two 1024 masters with space-containing labels.
- Moved `Wolf.icon/icon.json` → `crates/wolf-macos/Wolf.icon.json` (git mv); deleted the committed PNG copies (git rm) and the `Wolf.icon/` source dir; MODULE.bazel + `bazel_dep bazel_skylib 1.9.0` (matches MVS resolution, no graph change); three `copy_file` targets assemble the output-root `Wolf.icon/`; `app_icons` is now the explicit three-label list.
- **Deviation/new research finding:** skylib `copy_file` defaults to `allow_symlink = True` for non-executables, and actool rejects symlinked bundle members (`The "icon.json" entry must be a regular file.`). All three targets need `allow_symlink = False`. The "exactly one *.icon dir" validation never fired.
- Docs updated (icons README recipe is now re-export + rebuild, no cp; wolf-macos README; build.md; resources README), prettier-clean.
- Evidence: build green; assetutil shows DarkAqua→wolf-dark, Aqua→wolf-light, 7 derived sizes; CFBundleIconName "Wolf". Regression 2/2 PASSED. `resources/icons/BUILD.bazel` untracked — git add at commit.

## STATUS of original feature: COMPLETE (pending user's manual smoke test + commit)

All phases done: plan → implementation → review (PASS, no blockers). Docs authored by coder per plan §5 and verified by reviewer (separate technical-writer pass skipped as redundant — orchestrator decision). Work is uncommitted on `main`; `resources/` and `crates/wolf-macos/Wolf.icon/` are untracked and must be `git add`ed at commit time. Remaining for user: manual smoke (Dock/Finder icon, System Settings icon-appearance Dark/Light toggle swaps artwork without relaunch; watch for hairline corner slivers on the dark icon — known cosmetic follow-up if seen).

## Status

- [x] Problem statement written
- [x] Architect plan
- [x] Implementation (build green; dark variant verified in Assets.car)
- [x] Review (PASS — icon.json deviation sanctioned; docs accurate; no-test directive honored)
- [x] Documentation (written by coder, verified by reviewer, prettier-formatted)

> **User directive (2026-07-02):** Do NOT write tests verifying the application icon is applied to the launched app — that is a manual smoke test. No test-writer pass for this feature; verification is the Bazel build succeeding plus manual smoke instructions.

## Problem statement

Wolf.app (the Bazel-built `macos_application` at `//crates/wolf-macos:Wolf`, `minimum_os_version = "26.0"`) currently ships with no app icon. The repo now contains icon artwork under `resources/icons/` in macOS light and dark variants. On macOS Tahoe (26), the app icon must switch automatically between the light and dark artwork when the user changes the icon appearance setting in System Settings. Only macOS is in scope (the GNOME assets are for a future pass).

## Research findings (verified — do not re-derive)

- **Mechanism**: macOS Tahoe uses Icon Composer `.icon` bundles — a directory named `<Name>.icon` containing `icon.json` plus image assets in an `Assets/` subfolder. actool (Xcode 26) compiles the `.icon` bundle into `Assets.car` and emits `CFBundleIconName` into a partial Info.plist that rules_apple merges automatically. No `.icns` and no `Assets.xcassets` are needed or allowed.
- **rules_apple 4.5.3** (pinned in MODULE.bazel) supports `.icon` bundles via the `app_icons` attribute of `macos_application` (support added in 4.1.0). Verified in `bazel-wolf/external/rules_apple+/apple/internal/resource_actions/actool.bzl` (`_validate_standard_app_icon_sets`, `_verify_icon_dirs`):
  - With `minimum_os_version >= 26.0`, legacy `.appiconset` files are a **hard build failure** — the `.icon` bundle is the only supported route.
  - Requires Xcode 26+ (already a project requirement).
  - Exactly one `*.icon` directory must be present in `app_icons` (macOS supports no alternate icons).
  - The icon name passed to actool (`--app-icon`) is derived from the `.icon` directory basename, so `Wolf.icon` → `CFBundleIconName` "Wolf".
- **icon.json dark-variant format** (confirmed from a real Icon Composer-produced file): a layer swaps its image per appearance via `image-name-specializations`:

  ```json
  "image-name-specializations" : [
    { "value" : "wolf-light.png" },
    { "appearance" : "dark", "value" : "wolf-dark.png" }
  ]
  ```

  Layer images are referenced by bare filename and live in the `Assets/` subfolder of the `.icon` bundle. Other confirmed layer/group keys: `name`, `hidden`, `glass`, `position` (`scale`, `translation-in-points`), group `shadow`/`specular`/`translucency`, top-level `fill` (e.g. `"system-light"`), top-level `supported-platforms` (`{"circles": ["watchOS"], "squares": "shared"}`).
- **Source artwork** (both 1024×1024 PNG, complete pre-rendered icons with baked-in rounded-rect backgrounds):
  - Light: `resources/icons/Wolf application icon macOS light/icon-silhouette_512x512@2x.png` — has transparent margins; opaque bbox x:62–961, y:53–961 (rounded rect ~900×900 at origin (62,53), plus a baked drop shadow below it). Must be **cropped to the 900×900 art rect at origin (62,53) and resized to full-bleed 1024×1024**, discarding the baked shadow (Tahoe applies its own shadow and squircle mask; non-full-bleed art would show the fill color around the edges after masking).
  - Dark: `resources/icons/Wolf application icon macos dark/icon-silhouette_512x512@2x.png` — already full-bleed 1024×1024, use as-is.
- **Toolchain gotchas**:
  - The Nix shell exports `SDKROOT` pointing at a Nix apple-sdk-14.4, which breaks ad-hoc `swift` script invocations; use `env -u SDKROOT swift …` for any CoreGraphics-based image preprocessing (a working bbox script pattern exists; `sips` alone cannot do offset crops). The Bazel build itself pins DEVELOPER_DIR via `.bazelrc` and is unaffected.
  - Verification that the dark variant compiled in: `xcrun assetutil --info <bundle>/Contents/Resources/Assets.car` lists icon entries with dark appearance attributes.

## Architect plan (approved)

### Summary

Commit an Icon Composer `.icon` bundle at `crates/wolf-macos/Wolf.icon/` (an `icon.json` plus two derived full-bleed 1024×1024 PNGs) and wire it into the `Wolf` `macos_application` target via `app_icons`. actool (Xcode 26, pinned via DEVELOPER_DIR in `.bazelrc`) compiles it into `Contents/Resources/Assets.car` and merges `CFBundleIconName = Wolf` into the Info.plist. No Swift/Rust/test changes; verification is build success + manual smoke.

### 1. Asset preparation (one-time; copied PNGs are committed)

> **User decision (2026-07-03):** NO cropping or shadow removal by us. The user re-exported the light variant from their Figma project; it is **verified in place** (2026-07-03): `resources/icons/Wolf application icon macOS light/icon-silhouette_512x512@2x.png` is 1024×1024, opaque bbox spans the full canvas, no baked shadow. Both light and dark 1024px assets are full-bleed rounded rects with baked corner radius ≈ 202px and transparent corner notches; Tahoe's system squircle (~22% ≈ 225px at 1024) is more rounded, so the art should fully cover the mask — at worst hairline diagonal slivers showing the `fill`, checked in the manual smoke (cosmetic follow-up, not a blocker). Both PNGs are committed as byte-identical copies of the Figma exports.

- Dark: straight copy — `cp "resources/icons/Wolf application icon macos dark/icon-silhouette_512x512@2x.png" crates/wolf-macos/Wolf.icon/Assets/wolf-dark.png` (note lowercase "macos dark" vs "macOS light").
- Light: straight copy of the user's re-exported full-bleed 1024×1024 PNG → `crates/wolf-macos/Wolf.icon/Assets/wolf-light.png`. Before copying, verify it is actually full-bleed: `sips -g pixelWidth -g pixelHeight` → 1024×1024, and the opaque bbox spans the full canvas (re-run the alpha-bbox check; expect x:0–1023, y:0–1023 like the dark asset).
- Both committed assets remain byte-identical to the Figma exports; `resources/icons/README.md` documents that they are straight copies and that any change starts in Figma.

### 2. New file: `crates/wolf-macos/Wolf.icon/icon.json`

Basename `Wolf.icon` → `--app-icon Wolf` → `CFBundleIconName` "Wolf". Exact content:

```json
{
  "fill" : "system-light",
  "groups" : [
    {
      "layers" : [
        {
          "image-name" : "wolf-light.png",
          "image-name-specializations" : [
            { "value" : "wolf-light.png" },
            { "appearance" : "dark", "value" : "wolf-dark.png" }
          ],
          "name" : "artwork",
          "glass" : false
        }
      ],
      "shadow" : { "kind" : "neutral", "opacity" : 0 },
      "specular" : false,
      "translucency" : { "enabled" : false, "value" : 0 }
    }
  ],
  "supported-platforms" : {
    "circles" : [ "watchOS" ],
    "squares" : "shared"
  }
}
```

Fallbacks: if actool rejects the redundant top-level `image-name`, drop it and keep only `image-name-specializations`; light corner slivers on the dark icon would be the `fill` value (cosmetic follow-up, not a blocker). Do NOT put any non-asset file (e.g. README) inside `Wolf.icon/` — the glob feeds every file to actool.

### 3. BUILD.bazel change (`crates/wolf-macos/BUILD.bazel`)

On the `Wolf` target, immediately after `name` (buildifier order):

```python
app_icons = glob(
    ["Wolf.icon/**"],
    exclude = ["**/.DS_Store"],
),
```

Extend the `Wolf` target's explanatory comment (strong file convention) describing the `Wolf.icon/` bundle → Assets.car + CFBundleIconName, and that with minimum_os_version 26.0 the `.icon` bundle is the only supported route. Optionally extend the header "Targets:" comment. No `Info.plist` change; no `bundle_layout_test.sh` change (user directive).

### 4. Verification

1. `bazel build //crates/wolf-macos:Wolf` (from repo root; `.bazelrc` auto-applies the macos config).
2. Optional read-only inspection: `plutil -p .../Wolf.app/Contents/Info.plist | grep -i icon` → `CFBundleIconName => "Wolf"`; `xcrun assetutil --info .../Wolf.app/Contents/Resources/Assets.car` → icon entries incl. dark appearance. (App may be delivered as `Wolf.zip` beside bazel-bin output — unzip to temp if so.)
3. Manual smoke (user): launch the app, check Dock/Finder icon; System Settings → Appearance → toggle icon appearance Dark/Light → icon swaps without relaunch. Stale generic icon = Finder/Dock icon cache.
4. Optional regression: `bazel test //crates/wolf-macos:bundle_layout_test //crates/wolf-macos:WolfKeyMapTests`.

### 5. Documentation updates

- **New** `resources/icons/README.md`: what each subdirectory holds (macOS light/dark PNG sets; GNOME light/dark PNG+SVG reserved for a future pass, unconsumed); that only the two `icon-silhouette_512x512@2x.png` files are consumed, copied unmodified as `crates/wolf-macos/Wolf.icon/Assets/wolf-{light,dark}.png`; that the assets originate from the user's Figma project (source of truth — any artwork change starts there) and macOS layer images must be full-bleed 1024×1024 with no baked shadow because Tahoe applies its own squircle mask and shadow; the update recipe is re-export from Figma + `cp` + `sips -g` size check.
- **New** `resources/README.md`: short — design-time assets, currently only `icons/`, macOS app icon is the only build-consumed asset today.
- **Edit** `crates/wolf-macos/README.md` (bundle-contents paragraph ~line 75): the `.app` also carries the Tahoe app icon from the committed `Wolf.icon/` bundle; source of truth `resources/icons/`; verified by build success + manual smoke only.
- **Edit** `build.md` ("Building the .app" §, ~line 93): short paragraph on the icon bundle, app_icons wiring, light/dark switching, regeneration pointer.
- `architecture.md` / top-level `README.md`: checked, no stale claims — no changes required.
- All Markdown: no hard line breaks within paragraphs; `prettier --write` on every touched file.

### 6. Implementation order

1. Generate assets (§1). 2. Write icon.json (§2). 3. Edit BUILD.bazel (§3). 4. Build; iterate on icon.json only if actool rejects (fallbacks §2). 5. Optional inspection. 6. Docs + prettier. 7. Hand manual smoke checklist to user.

### 7. Dependencies / tooling

None new. Buildifier attribute ordering; keep Wolf.icon/ asset-only; `.DS_Store` glob exclude as insurance.

## Coder output

**Created:** `crates/wolf-macos/Wolf.icon/icon.json`; `crates/wolf-macos/Wolf.icon/Assets/wolf-light.png` + `wolf-dark.png` (byte-identical copies of the 1024px Figma exports, verified via `cmp` + `sips`); `resources/README.md`; `resources/icons/README.md`.

**Modified:** `crates/wolf-macos/BUILD.bazel` (`app_icons` glob + comments); `crates/wolf-macos/README.md` (bundle-contents paragraph); `build.md` ("Building the .app"). `prettier --write` run on all four Markdown files. No test or Swift/Rust changes.

**Deviation (important, now a research finding):** the plan §2 JSON *built* but produced a wrong Assets.car — with the redundant top-level `"image-name"` present, actool **silently ignored** `image-name-specializations` (all IconGroups referenced wolf-light; wolf-dark absent). Dropping the top-level `image-name` (the plan's first fallback) fixed it. The failure mode is silent ignoring, not rejection. Committed icon.json = plan JSON minus that key.

**Evidence:** `bazel build //crates/wolf-macos:Wolf` succeeded. Info.plist: `CFBundleIconFile => "Wolf"`, `CFBundleIconName => "Wolf"`. `assetutil --info` on Assets.car shows `IconGroup appearance=NSAppearanceNameDarkAqua layer0=Wolf_Assets/wolf-dark` and `NSAppearanceNameAqua layer0=Wolf_Assets/wolf-light` (plus Tintable→wolf-light), 7 icon sizes. Note: `xcrun assetutil` fails in the Nix shell; use `/usr/bin/assetutil`.

**Regression:** `//crates/wolf-macos:bundle_layout_test` PASSED, `//crates/wolf-macos:WolfKeyMapTests` PASSED (cached).

**Housekeeping for commit:** `resources/` and `crates/wolf-macos/Wolf.icon/` are currently untracked — `git add` both when committing. buildifier unavailable in this env; attribute order applied manually.
