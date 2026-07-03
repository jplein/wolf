# resources/

Design-time assets for Wolf. Currently the only contents are the application icon exports under [`icons/`](icons/README.md).

The only build-consumed assets today are the two 1024×1024 macOS icon PNGs, which `copy_file` targets in `crates/wolf-macos/BUILD.bazel` read directly to assemble the `Wolf.icon/` Icon Composer bundle at build time (no committed copies) — see [`icons/README.md`](icons/README.md) for the details and the update recipe.
