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
# - Darwin (aarch64-darwin only — nixpkgs 26.11 dropped x86_64-darwin, so an
#   Intel Mac builds through Bazel without this shell): a minimal shell that provides
#   bazelisk to drive the Bazel build of `wolf-core`, using the host Xcode SDK
#   for the C/link toolchain (not the older SDK shipped inside Nix). It carries
#   no Rust toolchain (rules_rust downloads a pinned rustc) and no GUI packages.
#
# On Linux it also provides a PACKAGE output — `packages.wolf-gnome`, and
# `packages.default` as the same derivation — which builds the `wolf-gnome`
# binary at the workspace release profile and installs it as a self-contained
# application: its own Neovim, ripgrep, treesitter trees and editor fonts wrapped
# in, a captive `share/wolf/nvim/` runtime tree carrying `init.lua` beside the
# vendored `orlando` and `lighthouse` trees, a desktop entry and the hicolor icon
# set. There is deliberately no Darwin package output; `Wolf.app` is Bazel's.
{
  description = "Wolf — Linux (NixOS) + macOS dev environment (Rust toolchain + pinned hermetic Neovim; bazelisk on Darwin)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    # The RUNTIME ASSETS the shipped application carries — Neovim, its
    # nvim-treesitter grammars and queries, and ripgrep — held apart from the
    # GTK/toolchain `nixpkgs` above and pinned to an explicit revision, so a
    # `nix flake update nixpkgs` taken for a GTK fix cannot move what the Linux
    # suite was run against. The property a packaging unit owes is that the
    # artifact's Neovim IS the one the evidence was taken from; a floating input
    # would make a green Linux suite evidence about one Neovim while the shipped
    # application ran whatever the last update fetched.
    #
    # The asymmetry runs the other way for `nixpkgs` itself, which must NOT lag the
    # host. The GTK process dlopens the host's Mesa from `/run/opengl-driver`, and
    # that Mesa needs a glibc at least as new as the system's. A `nixpkgs` lock
    # older than the NixOS host leaves the process on an older glibc, the driver
    # fails to load, and WebKit's web process aborts with "Could not create
    # default EGL display: EGL_BAD_PARAMETER" — a blank Markdown preview. Neovim
    # runs in a process of its own and is unaffected, which is why it can stay
    # pinned here.
    #
    # IT CARRIES NEOVIM 0.12.2, and `bazel/nvim/extensions.bzl` pins 0.12.3 for
    # macOS. That divergence predates this input and is deliberate rather than
    # unnoticed: the shared `runtime/nvim/init.lua` states its measured claims per
    # version (`vim.uv` on 0.12.3, treesitter starting by itself on a markdown
    # buffer on 0.12.2), and a matching MINOR series is what preserves the API
    # surface those claims rest on. Closing the patch gap costs either building
    # Neovim from source on Linux — off the binary cache, taxing every
    # `nix develop` — or bumping Linux and re-running the whole verification gate
    # plus re-measuring the treesitter readbacks. Recorded in `macos-followups.md`.
    nixpkgs-runtime.url = "github:NixOS/nixpkgs/9ae611a455b90cf061d8f332b977e387bda8e1ca";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      nixpkgs-runtime,
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

        # The runtime-asset package set, from the rev-pinned second input. Its
        # reason for existing is on `inputs.nixpkgs-runtime` above. Only Neovim,
        # ripgrep and nvim-treesitter come from here; every toolchain and GUI
        # package still comes from `pkgs`.
        pkgsRuntime = import nixpkgs-runtime { inherit system; };

        # NOTE: `rustToolchain`, `wolfPointerInject`, `iaWriterMono`,
        # `wolfTreesitterLanguages` / `wolfTreesitterParsers` /
        # `wolfTreesitterPlugin`, `pkgsRuntime` and everything taken from it
        # (`wolfNeovim`, `wolfRipgrep`), `wolfOrlando`, `wolfLighthouse`,
        # `wolfGnomePackage`, and the GTK/Neovim packages below are only
        # ever forced when `linuxShell` or the package output is evaluated. On
        # Darwin `devShells.default` resolves to `darwinShell`, which references
        # none of them, so Nix's laziness keeps these Linux-only derivations off
        # the Darwin eval path. `wolfPointerInject` in particular is a Wayland
        # program that could not build on Darwin at all; it is safe only because
        # nothing outside `linuxShell` forces the thunk. `iaWriterMono` is
        # arch-independent data, and the treesitter grammars are ordinary C that
        # COULD compile on Darwin, so for those the argument is the weaker
        # "safer there than here" one rather than "could not build" — but the
        # rule is the same for all of them. Do not add a top-level reference to
        # any of them, and do not wrap any of them in a `lib.optional` guard: a
        # guard would imply this laziness argument is unsound.
        #
        # THE ONE DELIBERATE EXCEPTION is the `optionalAttrs` on the `packages`
        # output at the bottom of this file, and it is a different thing from the
        # guard forbidden above. That prohibition is about guarding a Linux
        # derivation INSIDE `linuxShell.packages`, where laziness already does the
        # work. The output attribute set is not lazy in the same way: without the
        # guard, `packages.aarch64-darwin.default` would EXIST as an attribute
        # naming a Wayland GTK application that cannot build on Darwin at all, and
        # `nix flake show` and `nix flake check` would try to evaluate it. The
        # guard is on the attribute's existence, not on the derivation's value.
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

        # The captive Neovim the devShell exports and the package wraps in, and
        # the ripgrep behind `wolf-core`'s content search. Both come from
        # `pkgsRuntime` so the tested binary and the shipped binary are the same
        # store path by construction rather than by coincidence.
        wolfNeovim = pkgsRuntime.neovim;
        wolfRipgrep = pkgsRuntime.ripgrep;

        # THE ONE PLACE the Nix side writes down what each vendored asset is
        # pinned at. Every `fetchurl` below reads its commit and its sha256 from
        # here rather than carrying a literal of its own.
        #
        # BUMP BOTH TOGETHER. Each of these three assets is fetched TWICE — once
        # here for the Linux devShell and package, once by
        # `bazel/<asset>/extensions.bzl` for the macOS `.app` — and each side
        # verifies only its OWN sha256. So a one-sided bump is completely silent:
        # both platforms build green while Linux ships one revision of a plugin and
        # macOS bundles another. `crates/wolf-gnome/tests/vendored_pins.rs` is the
        # drift check that would notice, and it runs on every `cargo test`.
        #
        # THE FORMATTING BELOW IS LOAD-BEARING, because that check is a line
        # scanner and not a Nix parser — the same contract the workspace
        # `Cargo.toml` carries for `release_profile.rs`. One attribute set per
        # line, `commit` before `sha256`, both values double-quoted, the closing
        # `};` on the SAME line, and the block's own closing `};` on a line of its
        # own. Reformat this and the check reports every entry it can no longer
        # read, rather than passing.
        wolfVendoredPins = {
          ia-writer-mono = { commit = "f32c04c3058a75d7ce28919ce70fe8800817491b"; sha256 = "826767eaea4ce43ccb7ca71327a5c63ac31206b99df400699e6744df6104e1b5"; };
          orlando = { commit = "c960cf3e1c24fbdd2e6fb30ab9c8022cdc7b2bf9"; sha256 = "794911361108eb509ab00ec64ed03c229418be32f45ad7a385b6d2d6b057f3c5"; };
          lighthouse = { commit = "514dec2c79701b74082defc19090bd1dae57765b"; sha256 = "a452624fee9ce7b82e11877b013f578aa1c3befda6b6564536c3264a40b8e0d2"; };
        };

        # The vendored iA Writer Mono S static faces plus their licence, from the
        # SAME pinned commit tarball `bazel/ia-writer-mono/extensions.bzl`
        # fetches. The pin comes from `wolfVendoredPins` above.
        #
        # Linux-only in practice — see the NOTE above about why it stays off the
        # Darwin eval path.
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

        # The vendored orlando Neovim plugin, from the SAME pinned commit tarball
        # `bazel/orlando/extensions.bzl` fetches. The pin is in
        # `wolfVendoredPins`.
        #
        # THE INSTALLED SUBTREES MIRROR `//bazel/orlando`'s `runtime_files`
        # filegroup exactly — `plugin/`, `lua/`, `doc/` and `LICENSE*` — and the
        # exclusions are the point rather than an economy: `test/`, `flake.nix`,
        # `flake.lock`, `.luarc.json` and `README.md` are dev-only, macOS does not
        # bundle them, and the whole reason to vendor on both platforms is that
        # both ship the SAME files. An inclusive `cp -r` here is a divergence
        # nothing else would report;
        # `crates/wolf-gnome/tests/packaged_layout.rs` asserts the exclusions took.
        #
        # THE OUTPUT ROOT IS THE TREE ITSELF (`$out/plugin/orlando.lua`), with no
        # inner `orlando/` directory. That differs from the Bazel TreeArtifact on
        # purpose: `rules_apple` strips a TreeArtifact's own top-level name when
        # copying it into `Contents/Resources/`, so Bazel has to nest. Nix has no
        # such rule, and `wolfGnomePackage`'s `postInstall` names the destination
        # directory explicitly.
        #
        # DELIBERATELY NOT IN `linuxShell.packages`, and that is not an oversight.
        # The devShell must keep running the compositor smokes with NO orlando and
        # NO lighthouse beside the checkout's `runtime/nvim/init.lua`, because that
        # binary is the CONTROL leg of the packaged-artifact A/B in
        # `smoke_headless.rs::the_packaged_binary_loads_the_vendored_lighthouse_and_orlando_under_cage`.
        # Adding either tree to the shell looks like a convenience and silently
        # destroys that control.
        #
        # Linux-only in practice — see the NOTE above about the Darwin eval path.
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

            # stdenv's `move-docs` setup hook relocates `$out/doc` to
            # `$out/share/doc` by default, which would put orlando's `doc/` one
            # directory away from where the plugin's own `runtimepath` entry
            # expects it — `:help orlando` would find nothing, and the Nix tree
            # would stop matching what `//bazel/orlando` bundles. `forceShare`
            # names exactly which of `man doc info` the hook moves, so dropping
            # `doc` from it is the narrow fix; `dontFixup` would be the broad one
            # and would disable unrelated checks.
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

        # The vendored lighthouse colorscheme, from the SAME pinned commit tarball
        # `bazel/lighthouse/extensions.bzl` fetches. The pin is in
        # `wolfVendoredPins`, and every note on `wolfOrlando` above applies here —
        # including that this tree is deliberately absent from
        # `linuxShell.packages`.
        #
        # lighthouse is a COLORSCHEME: its only runtime files are `colors/` and
        # `LICENSE*`, matching `//bazel/lighthouse`'s filegroup. There is no
        # `plugin/`, no `lua/` and no `doc/` to install.
        #
        # `sourceRoot` IS `lighthouse.nvim-<commit>`, not `lighthouse-<commit>`:
        # the upstream repository name includes the `.nvim`. Getting this wrong
        # makes the install phase find nothing at all. The Bazel extension carries
        # the same warning.
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

        # The curated treesitter grammar set, and the ONE place it is written
        # down on the Nix side.
        #
        # 1. This list is DEFINED BY `bazel/treesitter-parsers/BUILD.bazel`'s
        #    `srcs`, and the two must not diverge: the point of vendoring these
        #    on Linux at all is cross-platform parity, so a divergence is the
        #    bug, not a preference. It is kept in that file's alphabetical order
        #    so a side-by-side diff makes a drift eyeball-visible.
        # 2. `c` is kept even though the pinned Neovim ships a `c` parser of its
        #    own. Taking the parser and the highlights query from the SAME
        #    nvim-treesitter revision is more consistent than mixing sources, and
        #    macOS does exactly that.
        # 3. `markdown` and `markdown_inline` are deliberately ABSENT. Neovim
        #    ships both, macOS excludes both, and re-vendoring them here would
        #    shadow Neovim's own parsers with a second copy.
        # 4. `python` is deliberately ABSENT. Adding it here alone would make
        #    macOS the laggard on a feature macOS already owns; the honest route
        #    is to add it to both sides in one change.
        # 5. ADDING A LANGUAGE MEANS EDITING BOTH SIDES: this list, and
        #    `bazel/treesitter-parsers/BUILD.bazel`'s `srcs` together with the
        #    grammar's pin in that package's `extensions.bzl` and `MODULE.bazel`.
        #    A one-sided edit is a silent platform divergence.
        # 6. Do NOT reach for `pkgs.tree-sitter-grammars.tree-sitter-<lang>`.
        #    Its shared object is named `parser`, not `<lang>.so`, so Neovim
        #    never resolves it; its queries are flat and written in the upstream
        #    tree-sitter dialect rather than nvim-treesitter's; and its grammar
        #    revisions do not match the queries we ship.
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

        # The parser half. Each `nvim-treesitter.parsers.<lang>` derivation
        # contains exactly `parser/<lang>.so`, so the join is
        # `$out/parser/<lang>.so` — precisely the runtimepath layout Neovim
        # resolves a parser from. No two inputs can collide, because each one
        # contributes a single, distinct filename.
        #
        # Do NOT reach for `nvim-treesitter.withPlugins` / `.withAllGrammars`.
        # Both build to the SAME store path as the bare plugin and only set
        # `passthru.dependencies`, which nothing here materializes — so they
        # would deliver no parsers at all. `withAllGrammars` additionally costs
        # 249.9 MiB and 313 local builds for a set far wider than point 1's.
        #
        # The plugin comes from `pkgsRuntime`, not from `pkgs`: the grammars and
        # the queries are runtime assets the application carries, and pinning them
        # to the same revision as Neovim is what keeps the parity claim on
        # `wolfTreesitterPlugin` below from being falsifiable by an unrelated
        # `nix flake update nixpkgs`.
        #
        # Linux-only in practice — see the NOTE above about the Darwin eval path.
        wolfTreesitterParsers = pkgs.symlinkJoin {
          name = "wolf-treesitter-parsers";
          paths = map (
            lang: pkgsRuntime.vimPlugins.nvim-treesitter.parsers.${lang}
          ) wolfTreesitterLanguages;
        };

        # The plugin half: ONE runtimepath-shaped tree carrying the bundled
        # queries, the `lua/nvim-treesitter/` modules and the `plugin/` loader as
        # siblings. Assembly is required because upstream splits them — the
        # queries sit at `runtime/queries/` while `lua/` and `plugin/` sit at the
        # archive root — and a runtimepath entry must expose all three at its own
        # top level.
        #
        # Do NOT "simplify" this back to `${pkgsRuntime.vimPlugins.nvim-treesitter}/runtime`.
        # That path satisfies the query lookup and silently drops the other two,
        # so `vim.cmd.runtime("plugin/nvim-treesitter.lua")` finds nothing,
        # `:TSInstall` never registers, and the pcall'd
        # `require("nvim-treesitter").setup` degrades to a no-op — leaving the
        # durable `parsers_user/` install dir reachable on macOS and not on
        # Linux. `crates/wolf-core/tests/parsers_user.rs` is what catches it.
        #
        # The remap mirrors `bazel/nvim-treesitter/extensions.bzl`, which lifts
        # `runtime/queries` back to `queries` and globs `lua/**` and `plugin/**`
        # to the same end, so both platforms present the same tree shape.
        #
        # Queries stay unfiltered, asymmetric with the curated parser list above
        # on purpose. This is the same upstream revision that extension pins for
        # macOS (4916d6592ede8c07973490d9322f187e07dfefac), so the two platforms
        # get byte-identical queries, and the macOS package globs them with no
        # per-language filter. Parsers stay curated because each one is a
        # compile; queries are data and cost nothing to carry.
        #
        # That parity claim is about a REVISION, so the plugin is taken from
        # `pkgsRuntime` rather than `pkgs`: pinned by the runtime input, it cannot
        # move under a `nixpkgs` bump taken for something else.
        wolfTreesitterPlugin =
          pkgs.runCommand "wolf-nvim-treesitter" { src = pkgsRuntime.vimPlugins.nvim-treesitter; }
            ''
              mkdir -p "$out"
              cp -R "$src/runtime/queries" "$out/queries"
              cp -R "$src/lua" "$out/lua"
              cp -R "$src/plugin" "$out/plugin"
              cp "$src/LICENSE" "$out/LICENSE"
            '';

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
            wolfNeovim
            wolfRipgrep
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

            # webkitgtk does TWO jobs, and both are required. At BUILD time it
            # supplies `webkitgtk-6.0.pc`, which is what `webkit6-sys`'s
            # `system-deps` probe resolves — without it the crate does not
            # configure. At RUN time it supplies the out-of-process
            # `WebKitNetworkProcess` and `WebKitWebProcess` helpers that a
            # `WebKitWebView` spawns; a web view that cannot spawn them renders
            # nothing.
            #
            # Those helpers are found through an ABSOLUTE path compiled into
            # `libwebkitgtk-6.0.so.4`, not through PATH and not through
            # `WEBKIT_EXEC_PATH` — that name does not appear in the library at
            # all. So the packaged wrapper needs no PATH entry and no execution
            # variable of its own, which is a MEASURED fact rather than an
            # inference: `smoke_headless.rs`'s packaged preview smoke launches the
            # store binary with the devShell's exports scrubbed and gets a
            # rendered document.
            #
            # libsoup_3 is NOT optional and not transitive here:
            # `webkitgtk-6.0.pc` declares
            # `Requires: glib-2.0 gtk4 libsoup-3.0 javascriptcoregtk-6.0`, and
            # `libsoup-3.0.pc` is otherwise absent from this devShell, so
            # `soup3-sys` fails to configure. Removing it breaks the build, not
            # just the runtime.
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

            # The two EDITOR font families the shared wolf-config.yaml can name;
            # `iA Writer Mono S` is what `wolf_core::default_config()` asks for,
            # so without these a default config would resolve to a substituted
            # face. The deterministic-render property is preserved because the
            # fontconfig template still exposes only these three pinned trees and
            # no host font directory.
            pkgs.jetbrains-mono
            iaWriterMono

            # GATE-REQUIRED: the fifty-eight headless end-to-end smoke tests in
            # crates/wolf-gnome/tests/smoke_headless.rs (of the sixty-seven that
            # file's default suite runs; the other nine are pure guards that
            # need no compositor) run as part of a plain `cargo test` (no
            # `--ignored`). They launch the real
            # window under a headless Wayland compositor and OCR a screenshot,
            # so these five must be present or the suite fails loud. The three
            # `#[ignore]`d packaged smokes that `./tools/wolf-package-smoke.sh`
            # drives need the same five. cage =
            # headless wlroots compositor, wtype = synthetic keystrokes,
            # grim = screenshot, tesseract = OCR, wolf-pointer-inject =
            # synthetic pointer (motion / button / axis). Not needed to *build*.
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
            export WOLF_NVIM_BIN="${pkgs.lib.getExe wolfNeovim}"
            # WOLF_RG_BIN is how wolf-core's content search + its tests locate the
            # ripgrep binary on the Linux/cargo path. Do not rely on PATH.
            export WOLF_RG_BIN="${pkgs.lib.getExe wolfRipgrep}"
            # The two directories `runtime/nvim/init.lua`'s treesitter gate falls
            # back to when the macOS .app's sibling `nvim-treesitter/` and
            # `parsers/` trees are absent, which on Linux they always are.
            # WITHOUT THESE THE GATE IS A STRICT NO-OP, and what that costs was
            # MEASURED rather than assumed: the two runtimepath entries never
            # land — the vendored grammars, and nvim-treesitter's
            # `queries/markdown/injections.scm` — so the editor loses INJECTED
            # fenced-language highlighting, and the Markdown preview loses every
            # fence the pinned Neovim does not itself ship a parser for. BASE
            # markdown highlighting is unaffected either way: nvim starts
            # treesitter on a markdown buffer by itself, gate or no gate, so
            # `vim.treesitter.highlighter.active[buf]` cannot witness this and
            # must not be cited as evidence for it (runtime/nvim/init.lua).
            # The names deliberately carry no TEST —
            # `WOLF_TEST_NVIM_TREESITTER_DIR` / `WOLF_TEST_PARSERS_DIR` keep
            # their own meaning as the Bazel test staging variables.
            #
            # Neither tree goes into `packages`, and that is deliberate: neither
            # carries a `bin/`, a `lib/` or a `.pc` file, so membership would
            # only add them to PATH and PKG_CONFIG_PATH for nothing. A store path
            # interpolated into `shellHook` acquires a Nix string context, which
            # makes it an input of the shell derivation and therefore built or
            # substituted before the shell is entered — exactly how the
            # `${pkgs.gnused}/bin/sed` call below works, and `gnused` is in no
            # `packages` list either.
            export WOLF_NVIM_TREESITTER_DIR="${wolfTreesitterPlugin}"
            export WOLF_PARSERS_DIR="${wolfTreesitterParsers}"

            # The two vendored Lua trees, staged for
            # `crates/wolf-ffi/tests/orlando_load.rs` and `lighthouse_load.rs`.
            # Both drive a REAL Neovim against a reconstructed sibling layout and
            # are the only automated proof of the two load branches in
            # `runtime/nvim/init.lua`; without a tree to stage each one prints a
            # loud SKIP and passes vacuously, which on Linux it always did before
            # these derivations existed. The names carry TEST because they are
            # test staging variables in the `WOLF_TEST_*` family, alongside
            # `WOLF_TEST_INIT_LUA` / `WOLF_TEST_NVIM_DIR` — no application code
            # reads either, and the packaged smokes scrub both.
            #
            # Each points at the TREE ITSELF rather than at a directory containing
            # one: both tests probe for an inner `<dir>/orlando` or
            # `<dir>/lighthouse` (the shape of the Bazel TreeArtifact) and fall
            # back to the directory they were given, which is the shape these
            # derivations have.
            #
            # This does NOT put either tree next to the checkout's
            # `runtime/nvim/init.lua`: both tests reconstruct the layout in a temp
            # directory, so the compositor smokes' control leg is untouched.
            export WOLF_TEST_ORLANDO_DIR="${wolfOrlando}"
            export WOLF_TEST_LIGHTHOUSE_DIR="${wolfLighthouse}"

            export RUST_SRC_PATH="${rustToolchain}/lib/rustlib/src/rust/library"

            # Pin the font stack for deterministic golden renders. We generate a
            # fontconfig file from the repo-controlled template that exposes ONLY
            # the three pinned font trees — DejaVu, JetBrains Mono and iA Writer
            # Mono S — with no host font dirs, and point FONTCONFIG_FILE at it.
            # This keeps glyph measurement/rasterization reproducible while
            # letting the shared wolf-config.yaml name either editor family.
            __wolf_fonts_conf="$(mktemp -t wolf-fonts.XXXXXX.conf)"
            ${pkgs.gnused}/bin/sed \
              -e "s|@DEJAVU_FONTS@|${pkgs.dejavu_fonts}|g" \
              -e "s|@JETBRAINS_MONO@|${pkgs.jetbrains-mono}|g" \
              -e "s|@IA_WRITER_MONO@|${iaWriterMono}|g" \
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

        # The four GNOME LIGHT icon exports plus the scalable master, named one by
        # one rather than as a directory.
        #
        # `./resources/icons/Wolf application icon GNOME light/...` is NOT a valid
        # Nix path literal: a space terminates one. Hence the path-plus-string
        # concatenation below. Naming each file individually also keeps the
        # sibling macOS 1024x1024 masters — which `//crates/wolf-macos` owns — out
        # of the store copy entirely.
        gnomeIconDir = ./resources/icons + "/Wolf application icon GNOME light";
        gnomeIcon48 = gnomeIconDir + "/icon-silhouette_48x48.png";
        gnomeIcon64 = gnomeIconDir + "/icon-silhouette_64x64.png";
        gnomeIcon128 = gnomeIconDir + "/icon-silhouette_128x128.png";
        gnomeIcon256 = gnomeIconDir + "/icon-silhouette_256x256.png";
        gnomeIconSvg = gnomeIconDir + "/icon-silhouette_scalable.svg";

        # What the source filter KEEPS. A whitelist rather than a blocklist,
        # because the cost of getting a blocklist wrong is silent and large.
        #
        # `lib.cleanSource` alone does NOT exclude `target/`, which on a working
        # checkout is over a gigabyte; an unfiltered `src = ./.` copies all of it
        # into the store on every evaluation. If `nix build` appears to hang
        # before it compiles anything, this is why.
        wolfPackageSourceFilter =
          path: _type:
          let
            rel = pkgs.lib.removePrefix (toString ./. + "/") (toString path);
          in
          rel == "Cargo.toml" || rel == "Cargo.lock" || rel == "crates" || pkgs.lib.hasPrefix "crates/" rel;

        # The installed Linux application: the release binary, its captive Neovim
        # runtime tree, a desktop entry, the hicolor icon set and the two bundled
        # editor font families.
        #
        # Built with the flake's OWN pinned Rust toolchain rather than
        # `rustPlatform`'s nixpkgs default, so the artifact is compiled by the same
        # rustc the Linux evidence was taken with.
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

              # Only the front-end binary. `wolf-ffi` is the swift-bridge crate the
              # macOS app consumes and the shipped Linux application does not use,
              # so building it here would be pure cost.
              cargoBuildFlags = [
                "--package"
                "wolf-gnome"
                "--bin"
                "wolf-gnome"
              ];

              # `buildType` is left at its default `"release"`, which is what
              # selects the workspace `[profile.release]` — `opt-level = 3`,
              # `lto = "thin"`, `panic = "unwind"`, `debug = "line-tables-only"`,
              # `strip = "none"`. No extra flag is needed, and nothing here may
              # override `strip`: the release contract is that symbols survive, so
              # a `dontStrip = false` would break a stated property of the profile.

              nativeBuildInputs = [
                pkgs.pkg-config
                pkgs.wrapGAppsHook4
                pkgs.desktop-file-utils
              ];

              # The first eight are the `.pc` files the gtk-rs and `webkit6` `-sys`
              # crates probe at build time, taken from `linuxShell`'s list, plus
              # `libsoup_3` for the reason that list records (`webkitgtk-6.0.pc`
              # declares `Requires: … libsoup-3.0`, so `soup3-sys` does not
              # configure without it).
              #
              # The last five are RUNTIME, and they belong in `buildInputs` rather
              # than anywhere else because that is how `wrapGAppsHook4` and the
              # `glib` / `hicolor-icon-theme` setup hooks discover them: they
              # accumulate GSETTINGS_SCHEMAS_PATH and XDG_ICON_DIRS from
              # `buildInputs` and write the corresponding `--prefix` flags into the
              # wrapper. `librsvg` is what puts the SVG loader into the pixbuf
              # module cache, without which the scalable icon and Adwaita's own
              # SVGs do not decode.
              #
              # `wolf-core`'s only C dependency is `rusqlite`'s BUNDLED SQLite,
              # compiled by `cc` from stdenv — no system `sqlite` is needed and
              # none must be added.
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

              # THE SUITE `cargo test -p wolf-gnome` RUNS IS NOT A HEADLESS UNIT
              # SUITE, and running it here is not possible rather than merely
              # slow. Most of its tests launch `cage`, a wlroots compositor
              # needing a writable `XDG_RUNTIME_DIR` at mode 0700, alongside
              # `wtype`, `grim`, `tesseract` and this repository's own
              # `wolf-pointer-inject`; every real-nvim test additionally needs
              # `WOLF_NVIM_BIN`; and `wolf-core`'s swap-recovery test needs a
              # writable `$HOME`. A Nix build sandbox provides no
              # `/run/user/<uid>`, no `/dev/dri` (so GTK's GL renderer has no
              # device), and no session bus. Running a curated subset would
              # execute only the pure seams the checkout `cargo test` already
              # covers, and would say nothing about the artifact.
              #
              # THE CONSEQUENCE IS STATED RATHER THAN HIDDEN: every piece of
              # evidence about this artifact comes from OUTSIDE the build sandbox,
              # through `./tools/wolf-package-smoke.sh` —
              # `crates/wolf-gnome/tests/packaged_layout.rs` and the `#[ignore]`d
              # packaged smokes in `crates/wolf-gnome/tests/smoke_headless.rs`.
              doCheck = false;

              postInstall = ''
                # --- The captive Neovim runtime tree ------------------------------
                #
                # `runtime/nvim/init.lua` self-locates its siblings from its OWN
                # directory (`debug.getinfo(1, "S").source`) and gates each on an
                # `fs_stat`, so installing the two vendored trees NEXT TO it is the
                # entire activation mechanism and needs no Lua edit. The captive
                # file is a real copy at mode 644, and
                # `crates/wolf-gnome/tests/packaged_layout.rs` asserts it is
                # byte-identical to the checkout's.
                #
                # The destination is `INSTALLED_INIT_LUA_RELATIVE` joined onto
                # `$out/bin` — the constant `crates/wolf-gnome/src/init_lua.rs`
                # states. Move one and the other must move with it.
                install -Dm644 ${./runtime/nvim/init.lua} \
                  "$out/share/wolf/nvim/init.lua"

                mkdir -p "$out/share/wolf/nvim/orlando" "$out/share/wolf/nvim/lighthouse"
                cp -R ${wolfOrlando}/. "$out/share/wolf/nvim/orlando/"
                cp -R ${wolfLighthouse}/. "$out/share/wolf/nvim/lighthouse/"
                chmod -R u+w "$out/share/wolf/nvim"

                # --- The desktop entry --------------------------------------------
                #
                # `Exec` is the ABSOLUTE store path, never a bare `wolf-gnome`. The
                # entry and the binary are outputs of the same derivation, so the
                # path cannot dangle while the entry is visible, and an absolute
                # path removes any dependence on PATH ordering. A bare name would
                # be resolved through the session's PATH, where a developer's
                # `target/release` ahead of their profile would launch an UNWRAPPED
                # binary carrying no WOLF_NVIM_BIN and no installed init.lua — a
                # silent wrong-artifact launch from the application menu.
                #
                # NO `%f`, NO `%U` and NO `MimeType`: `app::run` calls
                # `run_with_args::<&str>(&[])`, so the process ignores its real
                # argv. A field code would make the desktop hand Wolf a path it
                # silently drops, and a MIME type would put Wolf in the "Open With"
                # list for every `.md` file and then open an empty window. Opening
                # a file from the desktop needs `HANDLES_OPEN`, a real argv parse
                # and an `open` handler — a unit of its own.
                #
                # `StartupWMClass` matches `app.rs`'s APP_ID so the shell
                # associates the window with this entry.
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

                # --- The icons -----------------------------------------------------
                #
                # The hicolor theme is indexed BY DIRECTORY and never re-measures,
                # so each export must land in the directory naming its own pixel
                # size. Only the LIGHT set is installed: hicolor has no appearance
                # axis, and both exports are full-bleed self-contained round icons
                # with their own background plate rather than a symbolic pair, so
                # installing the dark set under any name would either shadow this
                # one or add an icon nothing looks up.
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

                # --- The two editor font families ---------------------------------
                #
                # `iA Writer Mono S` is the family `wolf_core::default_config()`
                # names, so a package without it renders a first-run user's default
                # configuration in whatever face fontconfig substitutes; JetBrains
                # Mono is the other family the shared `wolf-config.yaml` may name.
                # The two file sets are disjoint, which is why one directory holds
                # both — the same argument `crates/wolf-macos/README.md` makes for
                # its single bundled `fonts/`. A NixOS user can also put this
                # package in `fonts.packages` and get both families system-wide.
                for f in ${iaWriterMono}/share/fonts/truetype/*.ttf; do
                  install -Dm644 "$f" "$out/share/fonts/truetype/$(basename "$f")"
                done
                for f in ${pkgs.jetbrains-mono}/share/fonts/truetype/*.ttf; do
                  install -Dm644 "$f" "$out/share/fonts/truetype/$(basename "$f")"
                done
                install -Dm644 ${iaWriterMono}/share/licenses/ia-writer-mono/LICENSE.md \
                  "$out/share/licenses/ia-writer-mono-LICENSE.md"

                # --- The ADDITIVE fontconfig fragment ------------------------------
                #
                # Three things here are each a thing a later reader would undo.
                #
                # (1) It is ADDITIVE, and it is NOT the devShell's exclusionary pin.
                #     `crates/wolf-gnome/testdata/fonts.conf` omits every host font
                #     directory deliberately, for golden-render determinism;
                #     shipping that shape to a user would leave the GTK chrome
                #     rendering in a monospace editor face. The `<include>` of the
                #     system configuration is what keeps the host's own fonts,
                #     aliases and rendering settings.
                # (2) FONTCONFIG_FILE replaces the ROOT configuration rather than
                #     extending it, which is why the include is written out
                #     explicitly. `--prefix XDG_DATA_DIRS` cannot substitute for any
                #     of this: fontconfig's `<dir prefix="xdg">fonts</dir>` covers
                #     XDG_DATA_HOME alone and XDG_DATA_DIRS is never scanned,
                #     verified against `/etc/fonts/fonts.conf` on a NixOS host.
                # (3) The `<dir>` comes AFTER the include, so system fonts win ties.
                mkdir -p "$out/share/wolf"
                cat > "$out/share/wolf/fonts.conf" <<EOF
                <?xml version="1.0"?>
                <!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
                <fontconfig>
                  <include ignore_missing="yes">/etc/fonts/fonts.conf</include>
                  <dir>$out/share/fonts</dir>
                </fontconfig>
                EOF

                # Catches a malformed entry at build time rather than in a user's
                # session, where the symptom is an application that simply does not
                # appear in the menu.
                desktop-file-validate "$out/share/applications/org.wolf.Wolf.desktop"
              '';

              # `wrapGAppsHook4` wraps during `fixupPhase` and CONSUMES the
              # `gappsWrapperArgs` array, so these flags are appended to it rather
              # than applied through a second `wrapProgram` call. A hand-rolled
              # wrap here would double-wrap and shadow the hook's own
              # `--prefix XDG_DATA_DIRS` / GSETTINGS_SCHEMA_DIR / GIO_EXTRA_MODULES
              # / GDK_PIXBUF_MODULE_FILE.
              #
              # `--set-default` for every one, and the reason differs by group.
              # WOLF_NVIM_BIN / WOLF_RG_BIN are direct macOS parity:
              # `Project.swift` sets both with `setenv(name, value, 0)` — overwrite
              # 0 — so a developer's own export wins there, and `--set-default` is
              # the same rule. The two treesitter directories follow it for
              # consistency and because a developer testing a work-in-progress
              # query tree must be able to point the packaged binary at one.
              # FONTCONFIG_FILE follows it so the devShell's exclusionary pin keeps
              # winning INSIDE `nix develop`, which is what keeps `paint_golden.rs`
              # and every OCR smoke reproducible; outside the shell the packaged
              # additive config applies.
              #
              # WOLF_INIT_LUA IS NEVER SET HERE. Setting it would make
              # `init_lua::resolve_init_lua`'s executable-relative branch dead code
              # — branch 1 always wins — and would make the development override
              # unusable on the packaged binary.
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
        # `optionalAttrs` forces its CONDITION, never its value, so the Darwin
        # evaluation path never touches `wolfGnomePackage`. The guard is required
        # rather than tidy, and the NOTE beside `pkgsRuntime` above explains why it
        # is a different thing from the `lib.optional` guard that block forbids.
        packages.wolf-gnome = wolfGnomePackage;
        packages.default = wolfGnomePackage;
      }
    );
}
