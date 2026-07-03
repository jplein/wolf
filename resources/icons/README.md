# resources/icons/

Application-icon exports, in light and dark variants for both desktop targets:

- `Wolf application icon macOS light/` — the light-appearance macOS icon as PNGs at the classic `.iconset` sizes (16–512 px, each with an `@2x` companion; the largest, `icon-silhouette_512x512@2x.png`, is 1024×1024).
- `Wolf application icon macos dark/` — the dark-appearance macOS icon, same size set (note the lowercase "macos" in the directory name).
- `Wolf application icon GNOME light/` and `Wolf application icon GNOME dark/` — GNOME-sized PNGs (48/64/128/256 px) plus a scalable SVG, **reserved for a future GNOME pass and not consumed by any build today**.

## What the build consumes

Only two files here are consumed: the 1024×1024 `icon-silhouette_512x512@2x.png` from each macOS directory, exported to the build by the `exports_files` in this directory's `BUILD.bazel`. No copies are committed anywhere: the `copy_file` targets `//crates/wolf-macos:wolf_icon_light` and `:wolf_icon_dark` copy them **unmodified** into the output tree as `Wolf.icon/Assets/wolf-light.png` and `Wolf.icon/Assets/wolf-dark.png`, assembling the Icon Composer bundle at build time together with the committed `crates/wolf-macos/Wolf.icon.json` (re-materialized as `Wolf.icon/icon.json` by `:wolf_icon_json`). actool (Xcode 26) then compiles the assembled bundle into `Wolf.app`'s `Assets.car` with automatic light/dark switching on macOS Tahoe. actool derives all icon sizes per appearance from these 1024×1024 masters, so every other size in these directories is unconsumed by the build.

## Source of truth and constraints

The Figma project these were exported from is the source of truth — any artwork change starts there, not by editing the PNGs. The macOS layer images must be **full-bleed 1024×1024 with no baked drop shadow**: macOS Tahoe applies its own squircle mask and shadow on top of the layer image, so transparent margins would show the icon's fill color around the edges and a baked shadow would be double-applied.

## Updating the macOS icon

1. Re-export the light and/or dark 1024×1024 PNG from the Figma project over the matching `icon-silhouette_512x512@2x.png` here — there is no manual copy step; the build reads these files directly.
2. Verify the size: `sips -g pixelWidth -g pixelHeight "resources/icons/Wolf application icon macOS light/icon-silhouette_512x512@2x.png"` must report 1024×1024 (same for the dark file).
3. Rebuild: `bazel build //crates/wolf-macos:Wolf`, then check the Dock/Finder icon manually in both icon appearances.
