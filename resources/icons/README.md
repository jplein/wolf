# resources/icons/

Application-icon exports, in light and dark variants for both desktop targets:

- `Wolf application icon macOS light/` — the light-appearance macOS icon as PNGs at the classic `.iconset` sizes (16–512 px, each with an `@2x` companion; the largest, `icon-silhouette_512x512@2x.png`, is 1024×1024).
- `Wolf application icon macos dark/` — the dark-appearance macOS icon, same size set (note the lowercase "macos" in the directory name).
- `Wolf application icon GNOME light/` and `Wolf application icon GNOME dark/` — GNOME-sized PNGs (48/64/128/256 px) plus a scalable SVG. The light set is what the Linux package installs; the dark set is unconsumed, for the reason given below.

## What the build consumes

Two builds read files from here, and each takes a different set.

macOS takes the 1024×1024 `icon-silhouette_512x512@2x.png` from each of the two macOS directories, exported to the build by the `exports_files` in this directory's `BUILD.bazel`.

No copies are committed anywhere. The `copy_file` targets `//crates/wolf-macos:wolf_icon_light` and `:wolf_icon_dark` copy them unmodified into the output tree as `Wolf.icon/Assets/wolf-light.png` and `Wolf.icon/Assets/wolf-dark.png`, assembling the Icon Composer bundle at build time together with the committed `crates/wolf-macos/Wolf.icon.json` (re-materialized as `Wolf.icon/icon.json` by `:wolf_icon_json`).

actool (Xcode 26) then compiles the assembled bundle into `Wolf.app`'s `Assets.car`, with automatic light/dark switching on macOS Tahoe. It derives every icon size per appearance from these 1024×1024 masters, so all the other sizes in the macOS directories are unconsumed by that build.

Linux takes the four PNGs and the SVG from `Wolf application icon GNOME light/`, named file by file in `flake.nix` rather than as a directory, so the macOS masters beside them never enter the Nix store. The `wolf-gnome` package installs them as `org.wolf.Wolf.png` under `share/icons/hicolor/<n>x<n>/apps/` and `org.wolf.Wolf.svg` under `scalable/apps/`, which is what the desktop entry's `Icon=org.wolf.Wolf` resolves against. `crates/wolf-gnome/tests/packaged_layout.rs` decodes each installed PNG and requires the pixel size its directory names, because the theme spec indexes by directory and never re-measures — an export copied into the wrong one passes every existence check and renders soft.

The dark GNOME export is deliberately not installed. The icon-theme spec indexes by size, scale and context and has no appearance axis at all, and both GNOME exports are full-bleed self-contained round icons carrying their own background plate rather than a recolourable symbolic pair — so a dark set installed under any name would either shadow the light one or add an icon nothing ever looks up. The consequence is stated rather than papered over: the Linux icon does not follow the desktop's light/dark preference, where macOS gets that from `Assets.car`.

## Source of truth and constraints

The Figma project these were exported from is the source of truth — any artwork change starts there, not by editing the PNGs. The macOS layer images must be **full-bleed 1024×1024 with no baked drop shadow**: macOS Tahoe applies its own squircle mask and shadow on top of the layer image, so transparent margins would show the icon's fill color around the edges and a baked shadow would be double-applied.

## Updating the macOS icon

1. Re-export the light and/or dark 1024×1024 PNG from the Figma project over the matching `icon-silhouette_512x512@2x.png` here — there is no manual copy step; the build reads these files directly.
2. Verify the size: `sips -g pixelWidth -g pixelHeight "resources/icons/Wolf application icon macOS light/icon-silhouette_512x512@2x.png"` must report 1024×1024 (same for the dark file).
3. Rebuild: `bazel build //crates/wolf-macos:Wolf`, then check the Dock/Finder icon manually in both icon appearances.

## Updating the GNOME icon

1. Re-export the four PNGs and the scalable SVG from the Figma project over the matching `icon-silhouette_*` files in `Wolf application icon GNOME light/` — the derivation reads these files directly, so there is no copy step. Re-export the dark set alongside them if the artwork changed, so the two stay in step even though only the light set is installed.
2. Verify each PNG against its own filename, since the install maps one export per hicolor directory and nothing re-measures: `file "resources/icons/Wolf application icon GNOME light/icon-silhouette_128x128.png"` must report `128 x 128`, and likewise for 48, 64 and 256.
3. Rebuild and check: `./tools/wolf-package-smoke.sh`, whose layout test decodes every installed icon and asserts its size against the directory it landed in.
4. The `.desktop` entry names the icon by basename (`Icon=org.wolf.Wolf`), so a rename here means editing `flake.nix`'s install destinations and that entry together.
