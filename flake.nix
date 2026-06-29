# Linux (NixOS) and macOS (Darwin) dev environments for Wolf.
#
# Wolf is a Markdown editor that embeds Neovim as its editing engine. This flake
# provides two development shells, selected by host platform:
#
# - Linux (x86_64-linux, aarch64-linux): the shell used to build and test
#   `wolf-core` (and the `wolf-gnome` GTK front-end) with Cargo under Nix — a
#   pinned Rust toolchain (via the oxalica rust-overlay), a pinned hermetic
#   Neovim, and the GTK/Cairo/Pango GUI toolkit for the front-end's testable
#   seams. This shell exports `WOLF_NVIM_BIN`, the absolute path to the pinned
#   `nvim` binary — the *only* way `wolf-core` and its tests are allowed to
#   locate Neovim: never via the system PATH, never by scanning `/nix/store`.
# - Darwin (aarch64-darwin, x86_64-darwin): a minimal shell that provides
#   bazelisk to drive the Bazel build of `wolf-core`, using the host Xcode SDK
#   for the C/link toolchain (not the older SDK shipped inside Nix). It carries
#   no Rust toolchain (rules_rust downloads a pinned rustc) and no GUI packages.
{
  description = "Wolf — Linux (NixOS) + macOS dev environment (Rust toolchain + pinned hermetic Neovim; bazelisk on Darwin)";

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
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin" ] (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        isDarwin = pkgs.stdenv.isDarwin;

        # NOTE: `rustToolchain` and the GTK/Neovim packages below are only ever
        # forced when `linuxShell` is evaluated. On Darwin `devShells.default`
        # resolves to `darwinShell`, which references none of them, so Nix's
        # laziness keeps these Linux-only derivations off the Darwin eval path.
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
        };

        # Darwin dev shell: just enough to drive the Bazel build of `wolf-core`.
        # Provides bazelisk (which honors `.bazelversion`) and pins the host
        # Xcode so Bazel's apple_support C/link toolchain uses the system SDK,
        # not the older SDK shipped inside Nix. Deliberately contains NO Rust
        # toolchain (rules_rust downloads a pinned rustc) and NO GUI packages.
        # The nixpkgs `bazelisk` package ships only a `bazelisk` binary. Bazel
        # workflows (and our verification commands) invoke `bazel`, so expose a
        # `bazel` -> `bazelisk` shim. bazelisk reads `.bazelversion` and fetches
        # the version pinned in `.bazelversion` regardless of which name
        # launched it.
        bazelShim = pkgs.runCommand "bazel-shim" { } ''
          mkdir -p "$out/bin"
          ln -s ${pkgs.bazelisk}/bin/bazelisk "$out/bin/bazel"
        '';

        darwinShell = pkgs.mkShell {
          packages = [
            pkgs.bazelisk
            bazelShim
            pkgs.git
          ];

          shellHook = ''
            export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
          '';
        };

        linuxShell = pkgs.mkShell {
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
      in
      {
        devShells.default = if isDarwin then darwinShell else linuxShell;
      }
    );
}
