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
          ];

          shellHook = ''
            # WOLF_NVIM_BIN is how wolf-core and its tests locate the hermetic,
            # pinned nvim. Do not rely on PATH.
            export WOLF_NVIM_BIN="${pkgs.lib.getExe pkgs.neovim}"
            export RUST_SRC_PATH="${rustToolchain}/lib/rustlib/src/rust/library"
          '';
        };
      }
    );
}
