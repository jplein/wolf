# NixOS-only dev environment for Wolf.
#
# Wolf is a Markdown editor that embeds Neovim as its editing engine. This flake
# provides the development shell used to build and test `wolf-core` on NixOS:
# a pinned Rust toolchain (via the oxalica rust-overlay) plus a pinned, hermetic
# Neovim.
#
# The shell exports `WOLF_NVIM_BIN`, the absolute path to the pinned `nvim`
# binary. This is the *only* way `wolf-core` and its tests are allowed to locate
# Neovim: never via the system PATH, never by scanning `/nix/store`.
#
# This flake is Linux-only (x86_64-linux, aarch64-linux) and emits no
# darwin/macOS outputs. It contains NO GUI toolkit packages.
{
  description = "Wolf — NixOS-only dev environment (Rust toolchain + pinned hermetic Neovim)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            pkgs.neovim
            pkgs.pkg-config

            # GTK4 / Cairo / Pango toolchain for the wolf-gnome front-end's
            # Tier-1 testable seams (gdk_key, paint). These provide the .pc files
            # that the gtk-rs `-sys` crates probe at build time: gtk4 (gtk4.pc +
            # GDK for gdk4-sys), cairo, pango (pangocairo.pc too), glib,
            # gdk-pixbuf, graphene, harfbuzz. Deliberately NOT included (window
            # phase): libadwaita, wrapGAppsHook4, gsettings/icon themes.
            pkgs.gtk4
            pkgs.cairo
            pkgs.pango
            pkgs.glib
            pkgs.gdk-pixbuf
            pkgs.graphene
            pkgs.harfbuzz

            # LAUNCH-ALWAYS: deps the real windowed app needs to start up. The
            # gtk-rs libadwaita -sys crate probes libadwaita-1.pc; the rest are
            # GApplication *runtime* requirements (GSettings schemas + an icon
            # theme + SVG/MIME loaders) without which adw::Application aborts on
            # activate. They are exposed to the app via XDG_DATA_DIRS /
            # GSETTINGS_SCHEMA_DIR in the shellHook below.
            pkgs.libadwaita
            pkgs.gsettings-desktop-schemas
            pkgs.adwaita-icon-theme
            pkgs.hicolor-icon-theme
            pkgs.librsvg
            pkgs.shared-mime-info

            # Deterministic offscreen golden render: pinned font stack so Pango
            # measurement / Cairo rasterization is reproducible.
            pkgs.fontconfig
            pkgs.freetype
            pkgs.dejavu_fonts

            # SMOKE-ONLY: the opt-in headless end-to-end smoke test
            # (tests/smoke_headless.rs, run with `--ignored`) launches the real
            # window under a headless Wayland compositor and OCRs a screenshot.
            # cage = headless wlroots compositor, wtype = synthetic keystrokes,
            # grim = screenshot, tesseract = OCR. Not needed to build or to run
            # the app interactively.
            pkgs.cage
            pkgs.wtype
            pkgs.grim
            pkgs.tesseract
          ];

          shellHook = ''
            # WOLF_NVIM_BIN is how wolf-core and its tests locate the hermetic,
            # pinned nvim. Do not rely on PATH.
            export WOLF_NVIM_BIN="${pkgs.lib.getExe pkgs.neovim}"
            export RUST_SRC_PATH="${rustToolchain}/lib/rustlib/src/rust/library"

            # Pin the font stack for deterministic golden renders. We generate a
            # fontconfig file from the repo-controlled template that exposes ONLY
            # the pinned DejaVu fonts (no host font dirs) and point FONTCONFIG_FILE
            # at it. This keeps glyph measurement/rasterization reproducible.
            __wolf_fonts_conf="$(mktemp -t wolf-fonts.XXXXXX.conf)"
            ${pkgs.gnused}/bin/sed \
              "s|@DEJAVU_FONTS@|${pkgs.dejavu_fonts}|g" \
              "$PWD/crates/wolf-gnome/testdata/fonts.conf" > "$__wolf_fonts_conf"
            export FONTCONFIG_FILE="$__wolf_fonts_conf"

            # --- LAUNCH-ALWAYS: GApplication runtime discovery -----------------
            # The real adw::Application needs to find (a) GSettings schemas (the
            # desktop-wide schemas plus GTK4's own) and (b) an icon theme + the
            # hicolor fallback, or it aborts on activate. We export these
            # explicitly (rather than wrapGAppsHook4) so a plain
            # `nix develop --command cargo run` works. XDG_DATA_DIRS lets GIO/GTK
            # discover glib-2.0/schemas and icons/; GSETTINGS_SCHEMA_DIR points
            # GSettings straight at the compiled schema dirs as a belt-and-braces.
            __gsd_schemas="${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}"
            __gtk4_schemas="${pkgs.gtk4}/share/gsettings-schemas/${pkgs.gtk4.name}"
            export XDG_DATA_DIRS="$__gsd_schemas:$__gtk4_schemas:${pkgs.adwaita-icon-theme}/share:${pkgs.hicolor-icon-theme}/share:${pkgs.shared-mime-info}/share''${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"
            export GSETTINGS_SCHEMA_DIR="$__gsd_schemas/glib-2.0/schemas:$__gtk4_schemas/glib-2.0/schemas''${GSETTINGS_SCHEMA_DIR:+:$GSETTINGS_SCHEMA_DIR}"

            # --- SMOKE-ONLY: headless Wayland compositor env -------------------
            # Used only by the opt-in `--ignored` smoke test (cage/wtype/grim).
            # XDG_RUNTIME_DIR holds the Wayland socket; default it to a private
            # mktemp dir if the host has not set one. wlroots is told to use its
            # headless backend with no real input devices. We do NOT export
            # WAYLAND_DISPLAY — cage creates and exports it for its child tree.
            if [ -z "''${XDG_RUNTIME_DIR:-}" ]; then
              export XDG_RUNTIME_DIR="$(mktemp -d -t wolf-xdg-runtime.XXXXXX)"
            fi
            export WLR_BACKENDS=headless
            export WLR_LIBINPUT_NO_DEVICES=1
          '';
        };
      }
    );
}
