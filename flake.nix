# Dev shells for Linux and aarch64 Darwin (nixpkgs 26.11 dropped x86_64-darwin), and a Linux
# package; there is deliberately no Darwin package output, because `Wolf.app` is Bazel's.
{
  description = "Wolf — Linux (NixOS) + macOS dev environment (Rust toolchain + pinned hermetic Neovim; bazelisk on Darwin)";

  inputs = {
    # One input for the toolchain, GTK and runtime assets, and it must not lag the NixOS host.
    # See notes/flake.nix.md#single-nixpkgs-input
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
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ] (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        isDarwin = pkgs.stdenv.hostPlatform.isDarwin;

        # The Linux-only bindings below stay off the Darwin eval path by laziness alone; never guard them.
        # See notes/flake.nix.md#darwin-eval-laziness
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
            "clippy"
            "rustfmt"
          ];
        };

        # nixpkgs `bazelisk` ships only `bazelisk`, and Bazel workflows call `bazel`, hence this shim.
        # bazelisk fetches the `.bazelversion` pin whichever name launched it.
        bazelShim = pkgs.runCommand "bazel-shim" { } ''
          mkdir -p "$out/bin"
          ln -s ${pkgs.bazelisk}/bin/bazelisk "$out/bin/bazel"
        '';

        # The repo's Wayland pointer injector (tools/wolf-pointer-inject/README.md), for the cage pointer
        # smokes. Linux-only: see notes/flake.nix.md#darwin-eval-laziness
        wolfPointerInject =
          let
            # The protocol XML comes from wlr-protocols, not vendored, so it tracks the shipped wlroots.
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

            # Only the wlr XML is scanned: pkgs.wayland provides the core protocol. No -Werror, so a gcc bump
            # cannot break `nix develop`; -Wall -Wextra is the only automated check main.c gets.
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

        # The devShell export and the package wrapper both read these, so the tested and shipped
        # binaries are one store path.
        wolfNeovim = pkgs.neovim;
        wolfRipgrep = pkgs.ripgrep;

        # Scanned by crates/wolf-gnome/tests/vendored_pins.rs: one asset per line, `commit` then `sha256`,
        # double-quoted, `};` on the same line. Bump together with bazel/<asset>/extensions.bzl.
        wolfVendoredPins = {
          ia-writer-mono = { commit = "f32c04c3058a75d7ce28919ce70fe8800817491b"; sha256 = "826767eaea4ce43ccb7ca71327a5c63ac31206b99df400699e6744df6104e1b5"; };
          orlando = { commit = "c960cf3e1c24fbdd2e6fb30ab9c8022cdc7b2bf9"; sha256 = "794911361108eb509ab00ec64ed03c229418be32f45ad7a385b6d2d6b057f3c5"; };
          lighthouse = { commit = "514dec2c79701b74082defc19090bd1dae57765b"; sha256 = "a452624fee9ce7b82e11877b013f578aa1c3befda6b6564536c3264a40b8e0d2"; };
        };

        # iA Writer Mono S static faces and licence, from the tarball bazel/ia-writer-mono pins too.
        # Linux-only: see notes/flake.nix.md#darwin-eval-laziness
        iaWriterMono =
          let
            pin = wolfVendoredPins.ia-writer-mono;
          in
          pkgs.stdenv.mkDerivation {
            pname = "ia-writer-mono-s";
            # The pin is the commit, not a release.
            version = "0-unstable-2023";
            src = pkgs.fetchurl {
              url = "https://github.com/iaolo/iA-Fonts/archive/${pin.commit}.tar.gz";
              sha256 = pin.sha256;
            };
            sourceRoot = "iA-Fonts-${pin.commit}";
            dontConfigure = true;
            dontBuild = true;

            # Every path is quoted: the in-archive directory name contains
            # spaces ("iA Writer Mono/Static/").
            installPhase = ''
              runHook preInstall
              for f in Regular Italic Bold BoldItalic; do
                install -Dm644 "iA Writer Mono/Static/iAWriterMonoS-$f.ttf" \
                  "$out/share/fonts/truetype/iAWriterMonoS-$f.ttf"
              done
              # A font we ship carries its licence with it (SIL OFL 1.1,
              # upstream LICENSE.md).
              install -Dm644 "iA Writer Mono/LICENSE.md" \
                "$out/share/licenses/ia-writer-mono/LICENSE.md"
              runHook postInstall
            '';
          };

        # Installs exactly `//bazel/orlando`'s files, at the output root; never add it to linuxShell.
        # See notes/flake.nix.md#vendored-lua-trees
        wolfOrlando =
          let
            pin = wolfVendoredPins.orlando;
          in
          pkgs.stdenv.mkDerivation {
            pname = "wolf-orlando";
            # The pin is the commit, not a release.
            version = "0-unstable-2025";
            src = pkgs.fetchurl {
              url = "https://github.com/jplein/orlando/archive/${pin.commit}.tar.gz";
              sha256 = pin.sha256;
            };
            sourceRoot = "orlando-${pin.commit}";
            dontConfigure = true;
            dontBuild = true;

            # Keeps `doc/` out of stdenv's `move-docs`, so `:help orlando` works and the tree matches Bazel's.
            # `dontFixup` would be the broad fix and would disable unrelated checks.
            forceShare = [
              "man"
              "info"
            ];

            installPhase = ''
              runHook preInstall
              for d in plugin lua doc; do
                mkdir -p "$out/$d"
                cp -R "$d/." "$out/$d/"
              done
              for f in LICENSE*; do
                install -Dm644 "$f" "$out/$f"
              done
              chmod -R u+w "$out"
              runHook postInstall
            '';
          };

        # Every `wolfOrlando` note applies. A colorscheme: only `colors/` and `LICENSE*`, and its
        # `sourceRoot` carries upstream's `.nvim` suffix, without which the install finds nothing.
        wolfLighthouse =
          let
            pin = wolfVendoredPins.lighthouse;
          in
          pkgs.stdenv.mkDerivation {
            pname = "wolf-lighthouse";
            # The pin is the commit, not a release.
            version = "0-unstable-2025";
            src = pkgs.fetchurl {
              url = "https://github.com/jplein/lighthouse.nvim/archive/${pin.commit}.tar.gz";
              sha256 = pin.sha256;
            };
            sourceRoot = "lighthouse.nvim-${pin.commit}";
            dontConfigure = true;
            dontBuild = true;

            installPhase = ''
              runHook preInstall
              mkdir -p "$out/colors"
              cp -R colors/. "$out/colors/"
              for f in LICENSE*; do
                install -Dm644 "$f" "$out/$f"
              done
              chmod -R u+w "$out"
              runHook postInstall
            '';
          };

        # Must match `bazel/treesitter-parsers/BUILD.bazel`'s `srcs`, in its order; edit both sides.
        # See notes/flake.nix.md#treesitter-grammar-set
        wolfTreesitterLanguages = [
          "bash"
          "c"
          "cpp"
          "css"
          "go"
          "html"
          "java"
          "javascript"
          "json"
          "kotlin"
          "rust"
          "swift"
          "toml"
          "tsx"
          "typescript"
          "yaml"
        ];

        # Each parser derivation holds only `parser/<lang>.so`, so the join is a runtimepath tree.
        # Never `withPlugins` or `withAllGrammars`: see notes/flake.nix.md#treesitter-parsers
        wolfTreesitterParsers = pkgs.symlinkJoin {
          name = "wolf-treesitter-parsers";
          paths = map (
            lang: pkgs.vimPlugins.nvim-treesitter.parsers.${lang}
          ) wolfTreesitterLanguages;
        };

        # One runtimepath tree with `queries/`, `lua/` and `plugin/`; never just `nvim-treesitter/runtime`.
        # See notes/flake.nix.md#treesitter-plugin-tree
        wolfTreesitterPlugin =
          pkgs.runCommand "wolf-nvim-treesitter" { src = pkgs.vimPlugins.nvim-treesitter; }
            ''
              mkdir -p "$out"
              cp -R "$src/runtime/queries" "$out/queries"
              cp -R "$src/lua" "$out/lua"
              cp -R "$src/plugin" "$out/plugin"
              cp "$src/LICENSE" "$out/LICENSE"
            '';

        # NoCC: keeps the stdenv cc-wrapper, xcbuild `xcrun` and a Nix SDKROOT out of the shell.
        # See notes/flake.nix.md#darwin-shell
        darwinShell = pkgs.mkShellNoCC {
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
            wolfNeovim
            wolfRipgrep
            pkgs.pkg-config

            # The `.pc` files the gtk-rs `-sys` crates probe at build time.
            pkgs.gtk4
            pkgs.cairo
            pkgs.pango
            pkgs.glib
            pkgs.gdk-pixbuf
            pkgs.graphene
            pkgs.harfbuzz

            # libadwaita, plus the GApplication runtime data below (schemas, icon themes, SVG and MIME
            # loaders) without which adw::Application aborts on activate; the shellHook exports them.
            pkgs.libadwaita

            # webkitgtk supplies `webkitgtk-6.0.pc` and the web-process helpers; libsoup_3 is a build need.
            # See notes/flake.nix.md#webkitgtk-and-libsoup
            pkgs.webkitgtk_6_0
            pkgs.libsoup_3

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

            # The two editor families wolf-config.yaml can name, so the default config gets no substitute.
            # The fontconfig template still exposes only the pinned trees, keeping renders deterministic.
            pkgs.jetbrains-mono
            iaWriterMono

            # Gate-required: smoke_headless.rs's cage smokes run under plain `cargo test`. `dbus-run-session`
            # gives each cage a private session bus, without which the preview's WebView draws blank.
            pkgs.cage
            pkgs.dbus
            pkgs.wtype
            pkgs.grim
            pkgs.tesseract
            wolfPointerInject

            # prettier backs the gate's Markdown check, pinned so a host prettier of another version never
            # runs against `.prettierrc`.
            pkgs.prettier
          ];

          shellHook = ''
            # WOLF_NVIM_BIN is how wolf-core and its tests locate the hermetic,
            # pinned nvim. Do not rely on PATH.
            export WOLF_NVIM_BIN="${pkgs.lib.getExe wolfNeovim}"
            # WOLF_RG_BIN is how wolf-core's content search + its tests locate the
            # ripgrep binary on the Linux/cargo path. Do not rely on PATH.
            export WOLF_RG_BIN="${pkgs.lib.getExe wolfRipgrep}"
            # init.lua's treesitter gate is a no-op on Linux without these (injected fences lose highlights).
            # Not in `packages`: interpolation builds them. No TEST in the names, unlike Bazel's staging vars.
            export WOLF_NVIM_TREESITTER_DIR="${wolfTreesitterPlugin}"
            export WOLF_PARSERS_DIR="${wolfTreesitterParsers}"

            # Test staging (`WOLF_TEST_*`: no app code reads them) for wolf-ffi's orlando_load.rs and
            # lighthouse_load.rs. Each is the tree itself, and neither lands beside runtime/nvim/init.lua.
            export WOLF_TEST_ORLANDO_DIR="${wolfOrlando}"
            export WOLF_TEST_LIGHTHOUSE_DIR="${wolfLighthouse}"

            export RUST_SRC_PATH="${rustToolchain}/lib/rustlib/src/rust/library"

            # Golden renders pin fontconfig to the three pinned font trees, with no host font directory.
            __wolf_fonts_conf="$(mktemp -t wolf-fonts.XXXXXX.conf)"
            ${pkgs.gnused}/bin/sed \
              -e "s|@DEJAVU_FONTS@|${pkgs.dejavu_fonts}|g" \
              -e "s|@JETBRAINS_MONO@|${pkgs.jetbrains-mono}|g" \
              -e "s|@IA_WRITER_MONO@|${iaWriterMono}|g" \
              "$PWD/crates/wolf-gnome/testdata/fonts.conf" > "$__wolf_fonts_conf"
            export FONTCONFIG_FILE="$__wolf_fonts_conf"

            # --- GApplication runtime discovery: schemas and icon themes, or adw::Application aborts ---
            # Exported directly rather than via wrapGAppsHook4, so `nix develop --command cargo run` works.
            __gsd_schemas="${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}"
            __gtk4_schemas="${pkgs.gtk4}/share/gsettings-schemas/${pkgs.gtk4.name}"
            export XDG_DATA_DIRS="$__gsd_schemas:$__gtk4_schemas:${pkgs.adwaita-icon-theme}/share:${pkgs.hicolor-icon-theme}/share:${pkgs.shared-mime-info}/share''${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"
            export GSETTINGS_SCHEMA_DIR="$__gsd_schemas/glib-2.0/schemas:$__gtk4_schemas/glib-2.0/schemas''${GSETTINGS_SCHEMA_DIR:+:$GSETTINGS_SCHEMA_DIR}"

            # --- Headless Wayland for the cage smokes. WAYLAND_DISPLAY and XDG_RUNTIME_DIR stay unset:
            # the harness gives each cage a private runtime dir, and cage exports WAYLAND_DISPLAY.
            export WLR_BACKENDS=headless
            export WLR_LIBINPUT_NO_DEVICES=1
          '';
        };

        # Each light export is named individually: a space ends a Nix path literal, and this keeps the
        # macOS masters that //crates/wolf-macos owns out of the store copy.
        gnomeIconDir = ./resources/icons + "/Wolf application icon GNOME light";
        gnomeIcon48 = gnomeIconDir + "/icon-silhouette_48x48.png";
        gnomeIcon64 = gnomeIconDir + "/icon-silhouette_64x64.png";
        gnomeIcon128 = gnomeIconDir + "/icon-silhouette_128x128.png";
        gnomeIcon256 = gnomeIconDir + "/icon-silhouette_256x256.png";
        gnomeIconSvg = gnomeIconDir + "/icon-silhouette_scalable.svg";

        # A whitelist: `lib.cleanSource` keeps `target/` (over a gigabyte), so a blocklist mistake
        # silently copies it into the store and `nix build` appears to hang.
        wolfPackageSourceFilter =
          path: _type:
          let
            rel = pkgs.lib.removePrefix (toString ./. + "/") (toString path);
          in
          rel == "Cargo.toml" || rel == "Cargo.lock" || rel == "crates" || pkgs.lib.hasPrefix "crates/" rel;

        # The installed Linux app, built with the flake's own Rust toolchain rather than rustPlatform's
        # default, so the artifact uses the rustc the Linux evidence was taken with.
        wolfGnomePackage =
          (pkgs.makeRustPlatform {
            cargo = rustToolchain;
            rustc = rustToolchain;
          }).buildRustPackage
            {
              pname = "wolf-gnome";
              version = "0.1.0";

              src = pkgs.lib.cleanSourceWith {
                src = ./.;
                name = "wolf-source";
                filter = wolfPackageSourceFilter;
              };

              # No `outputHashes`: `Cargo.lock` carries zero `source = "git+…"`
              # entries, so every crate comes from crates.io.
              cargoLock.lockFile = ./Cargo.lock;

              # Only the front-end binary: `wolf-ffi` is the macOS app's swift-bridge crate.
              cargoBuildFlags = [
                "--package"
                "wolf-gnome"
                "--bin"
                "wolf-gnome"
              ];

              # The default `buildType = "release"` selects `[profile.release]`. Never override `strip`:
              # the profile's contract is that symbols survive.

              nativeBuildInputs = [
                pkgs.pkg-config
                pkgs.wrapGAppsHook4
                pkgs.desktop-file-utils
              ];

              # Build-time `.pc` files as in linuxShell, then runtime data the wrapper hooks find only in
              # `buildInputs` (librsvg: the SVG pixbuf loader). Bundled SQLite: never add a system sqlite.
              buildInputs = [
                pkgs.gtk4
                pkgs.libadwaita
                pkgs.webkitgtk_6_0
                pkgs.libsoup_3
                pkgs.cairo
                pkgs.pango
                pkgs.glib
                pkgs.gdk-pixbuf
                pkgs.graphene
                pkgs.harfbuzz

                pkgs.librsvg
                pkgs.gsettings-desktop-schemas
                pkgs.adwaita-icon-theme
                pkgs.hicolor-icon-theme
                pkgs.shared-mime-info
              ];

              # The suite needs cage, a session bus, a GPU node and nvim, none of them in the build sandbox.
              # See notes/flake.nix.md#no-check-phase
              doCheck = false;

              postInstall = ''
                # --- Captive Neovim tree: init.lua finds its vendored siblings beside itself ----------
                # The destination must match `INSTALLED_INIT_LUA_RELATIVE` in crates/wolf-gnome/src/init_lua.rs.
                install -Dm644 ${./runtime/nvim/init.lua} \
                  "$out/share/wolf/nvim/init.lua"

                mkdir -p "$out/share/wolf/nvim/orlando" "$out/share/wolf/nvim/lighthouse"
                cp -R ${wolfOrlando}/. "$out/share/wolf/nvim/orlando/"
                cp -R ${wolfLighthouse}/. "$out/share/wolf/nvim/lighthouse/"
                chmod -R u+w "$out/share/wolf/nvim"

                # --- Desktop entry: an absolute `Exec`, so PATH cannot launch an unwrapped `target/release` build
                # No `%f`, `%U` or `MimeType`: `app::run` ignores argv. `StartupWMClass` is `app.rs`'s APP_ID.
                mkdir -p "$out/share/applications"
                cat > "$out/share/applications/org.wolf.Wolf.desktop" <<EOF
                [Desktop Entry]
                Type=Application
                Version=1.0
                Name=Wolf
                GenericName=Markdown Editor
                Comment=A Markdown editor that embeds Neovim as its editing engine
                Exec=$out/bin/wolf-gnome
                Icon=org.wolf.Wolf
                Terminal=false
                Categories=Utility;TextEditor;Development;
                StartupNotify=true
                StartupWMClass=org.wolf.Wolf
                EOF

                # --- Icons: hicolor indexes by directory, so each export goes in its own size directory ---
                # Light set only: hicolor has no appearance axis, so a dark set would shadow it or go unused.
                install -Dm644 ${gnomeIcon48} \
                  "$out/share/icons/hicolor/48x48/apps/org.wolf.Wolf.png"
                install -Dm644 ${gnomeIcon64} \
                  "$out/share/icons/hicolor/64x64/apps/org.wolf.Wolf.png"
                install -Dm644 ${gnomeIcon128} \
                  "$out/share/icons/hicolor/128x128/apps/org.wolf.Wolf.png"
                install -Dm644 ${gnomeIcon256} \
                  "$out/share/icons/hicolor/256x256/apps/org.wolf.Wolf.png"
                install -Dm644 ${gnomeIconSvg} \
                  "$out/share/icons/hicolor/scalable/apps/org.wolf.Wolf.svg"

                # --- Editor fonts: `iA Writer Mono S` is `default_config()`'s family, JetBrains Mono the other
                # The two file sets are disjoint, so one directory holds both.
                for f in ${iaWriterMono}/share/fonts/truetype/*.ttf; do
                  install -Dm644 "$f" "$out/share/fonts/truetype/$(basename "$f")"
                done
                for f in ${pkgs.jetbrains-mono}/share/fonts/truetype/*.ttf; do
                  install -Dm644 "$f" "$out/share/fonts/truetype/$(basename "$f")"
                done
                install -Dm644 ${iaWriterMono}/share/licenses/ia-writer-mono/LICENSE.md \
                  "$out/share/licenses/ia-writer-mono-LICENSE.md"

                # --- Additive fontconfig: FONTCONFIG_FILE replaces the root config, so include the system's ---
                # `<dir>` follows it so system fonts win ties; XDG_DATA_DIRS is never scanned, so no `--prefix`.
                mkdir -p "$out/share/wolf"
                cat > "$out/share/wolf/fonts.conf" <<EOF
                <?xml version="1.0"?>
                <!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
                <fontconfig>
                  <include ignore_missing="yes">/etc/fonts/fonts.conf</include>
                  <dir>$out/share/fonts</dir>
                </fontconfig>
                EOF

                # Catches a malformed entry at build time, not as an app missing from a user's menu.
                desktop-file-validate "$out/share/applications/org.wolf.Wolf.desktop"
              '';

              # Appended to `gappsWrapperArgs`, never a second wrap; `--set-default` throughout; never set
              # WOLF_INIT_LUA. See notes/flake.nix.md#wrapper-environment
              preFixup = ''
                gappsWrapperArgs+=(
                  --set-default WOLF_NVIM_BIN "${pkgs.lib.getExe wolfNeovim}"
                  --set-default WOLF_RG_BIN "${pkgs.lib.getExe wolfRipgrep}"
                  --set-default WOLF_NVIM_TREESITTER_DIR "${wolfTreesitterPlugin}"
                  --set-default WOLF_PARSERS_DIR "${wolfTreesitterParsers}"
                  --set-default FONTCONFIG_FILE "$out/share/wolf/fonts.conf"
                )
              '';

              meta = {
                description = "A Markdown editor that embeds Neovim as its editing engine";
                homepage = "https://github.com/jplein/wolf";
                license = with pkgs.lib.licenses; [
                  mit
                  asl20
                ];
                mainProgram = "wolf-gnome";
                platforms = pkgs.lib.platforms.linux;
              };
            };
      in
      {
        devShells.default = if isDarwin then darwinShell else linuxShell;
      }
      // pkgs.lib.optionalAttrs (!isDarwin) {
        # `optionalAttrs` forces only its condition, so Darwin never evaluates `wolfGnomePackage`.
        # The guard is required: see notes/flake.nix.md#darwin-eval-laziness
        packages.wolf-gnome = wolfGnomePackage;
        packages.default = wolfGnomePackage;
      }
    );
}
