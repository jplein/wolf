# Wolf

Wolf is a Markdown editor based on [Neovim](https://github.com/neovim/neovim).

## System requirements

- Linux: Only NixOS is supported. The Rust core (and the GTK front-end) build with Cargo under Nix.
- macOS: The Rust core builds with Bazel (via bazelisk), using the host Xcode SDK.
- macOS app: The `wolf-macos` AppKit app requires macOS Tahoe (26) or newer and Xcode 26+. See [Building Wolf](build.md).

## Building Wolf

- See [Building Wolf](build.md).

## Architecture

- See [Architecture](architecture.md).
