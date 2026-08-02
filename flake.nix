# Linux (NixOS) and macOS (Darwin) dev environments for Wolf.
#
# Wolf is a Markdown editor that embeds Neovim as its editing engine. This flake
# provides two development shells, selected by host platform:
#
# - Linux (x86_64-linux, aarch64-linux): the shell used to build and test
#   `wolf-core` (and the `wolf-gnome` GTK front-end) with Cargo under Nix — a
#   pinned Rust toolchain (via the oxalica rust-overlay), a pinned hermetic
#   Neovim, and the GTK/Cairo/Pango GUI toolkit for the front-end's testable
#   seams, plus the headless-compositor smoke toolchain (including the repo's
#   own `wolf-pointer-inject` Wayland pointer injector, built from `tools/`) and
#   the Markdown formatter that the Linux verification gate (`build.md`)
#   requires. This shell
#   exports `WOLF_NVIM_BIN`, the absolute path to the pinned `nvim` binary — the
#   *only* way `wolf-core` and its tests are allowed to locate Neovim: never via
#   the system PATH, never by scanning `/nix/store`.
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
    {
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin" ] (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        isDarwin = pkgs.stdenv.isDarwin;

        # NOTE: `rustToolchain`, `wolfPointerInject` and the GTK/Neovim packages
        # below are only ever forced when `linuxShell` is evaluated. On Darwin
        # `devShells.default` resolves to `darwinShell`, which references none of
        # them, so Nix's laziness keeps these Linux-only derivations off the
        # Darwin eval path. `wolfPointerInject` in particular is a Wayland
        # program that could not build on Darwin at all; it is safe only because
        # nothing outside `linuxShell.packages` forces the thunk. Do not add a
        # top-level reference to it, and do not wrap it in a `lib.optional`
        # guard — a guard would imply this laziness argument is unsound.
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
            "clippy"
            "rustfmt"
          ];
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

        # The repo's own Wayland pointer injector, built from tools/. A
        # long-lived `zwlr_virtual_pointer_v1` client that both HOLDS the seat's
        # pointer capability open (wlroots advertises
        # WL_SEAT_CAPABILITY_POINTER only while a pointer device exists, and the
        # headless backend creates none) and injects motion / button / axis
        # events on command. Used by the `wolf-gnome` cage pointer smokes; see
        # tools/wolf-pointer-inject/README.md. Linux-only — see the NOTE above
        # about why it stays off the Darwin eval path.
        wolfPointerInject =
          let
            # Take the protocol XML from wlr-protocols rather than vendoring it,
            # so the protocol revision stays tied to the wlroots this flake
            # actually ships.
            protoXml = "${pkgs.wlr-protocols}/share/wlr-protocols/unstable/wlr-virtual-pointer-unstable-v1.xml";
          in
          pkgs.stdenv.mkDerivation {
            pname = "wolf-pointer-inject";
            version = "0.1.0";
            src = ./tools/wolf-pointer-inject;

            # wayland-scanner is a code generator that runs at build time, hence
            # native. pkg-config resolves wayland-client.pc.
            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.wayland-scanner
            ];
            buildInputs = [ pkgs.wayland ];

            # NOTE: wayland.xml is deliberately NOT scanned. wl_registry,
            # wl_seat, wl_output, the WL_POINTER_* enums and wl_fixed_from_double
            # all come from wayland-client.h / wayland-client-protocol.h and
            # libwayland-client.so, which pkgs.wayland provides. Only the wlr XML
            # needs scanning; do not add a redundant second scanner invocation.
            #
            # Deliberately NOT -Werror: a nixpkgs gcc bump must not be able to
            # break `nix develop` for the whole team over a new pedantic warning
            # in wayland-scanner's generated .c. -Wall -Wextra is, however, the
            # ONLY automated check this C file gets — no gate step (rustfmt,
            # clippy, prettier, cargo test) reaches it.
            buildPhase = ''
              runHook preBuild
              wayland-scanner client-header \
                "${protoXml}" wlr-virtual-pointer-unstable-v1-client-protocol.h
              wayland-scanner private-code \
                "${protoXml}" wlr-virtual-pointer-unstable-v1-protocol.c
              $CC -O2 -Wall -Wextra -I. \
                -o wolf-pointer-inject \
                main.c wlr-virtual-pointer-unstable-v1-protocol.c \
                $(pkg-config --cflags --libs wayland-client)
              runHook postBuild
            '';

            installPhase = ''
              runHook preInstall
              install -Dm755 wolf-pointer-inject "$out/bin/wolf-pointer-inject"
              runHook postInstall
            '';
          };

        darwinShell = pkgs.mkShell {
          packages = [
            pkgs.bazelisk
            bazelShim
            pkgs.git
            pkgs.starpls
          ];

          shellHook = ''
            export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
          '';
        };

        linuxShell = pkgs.mkShell {
          packages = [
            rustToolchain
            pkgs.neovim
            pkgs.ripgrep
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

            # GATE-REQUIRED: the twenty headless end-to-end smoke tests in
            # crates/wolf-gnome/tests/smoke_headless.rs run as part of a plain
            # `cargo test` (no `--ignored`). They launch the real window under a
            # headless Wayland compositor and OCR a screenshot, so these five
            # must be present or the suite fails loud. cage = headless wlroots
            # compositor, wtype = synthetic keystrokes, grim = screenshot,
            # tesseract = OCR, wolf-pointer-inject = synthetic pointer (motion /
            # button / axis). Not needed to *build*.
            #
            # wolf-pointer-inject is the repo's OWN tool, built from
            # tools/wolf-pointer-inject by the derivation above, not an upstream
            # nixpkgs package: it is the persistent virtual-pointer injector that
            # holds the seat's pointer capability open for a whole scenario. See
            # tools/wolf-pointer-inject/README.md.
            pkgs.cage
            pkgs.wtype
            pkgs.grim
            pkgs.tesseract
            wolfPointerInject

            # LINT/FORMAT GATE: prettier backs the Markdown format check in the
            # Linux verification gate (see build.md, "The Linux verification
            # gate"). Pinned here so the gate can never silently run a host
            # prettier of a different version against `.prettierrc`.
            pkgs.prettier
          ];

          shellHook = ''
            # WOLF_NVIM_BIN is how wolf-core and its tests locate the hermetic,
            # pinned nvim. Do not rely on PATH.
            export WOLF_NVIM_BIN="${pkgs.lib.getExe pkgs.neovim}"
            # WOLF_RG_BIN is how wolf-core's content search + its tests locate the
            # ripgrep binary on the Linux/cargo path. Do not rely on PATH.
            export WOLF_RG_BIN="${pkgs.lib.getExe pkgs.ripgrep}"
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

            # --- SMOKE: headless Wayland compositor env ------------------------
            # Used by the cage smoke tests in
            # crates/wolf-gnome/tests/smoke_headless.rs, which run as part of the
            # default `cargo test`.
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
