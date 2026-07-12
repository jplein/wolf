# Wolf

Wolf is a Markdown editor based on [Neovim](https://github.com/neovim/neovim).

## System requirements

- Linux: Only NixOS is supported. The Rust core (and the GTK front-end) build with Cargo under Nix.
- macOS: The Rust core builds with Bazel (via bazelisk), using the host Xcode SDK.
- macOS app: The `wolf-macos` AppKit app requires macOS Tahoe (26) or newer and Xcode 26+. It ships its own pinned Neovim bundled inside `Wolf.app`, so it needs no separately-installed Neovim and double-clicking it works with no shell environment. See [Building Wolf](build.md).

## Building Wolf

- See [Building Wolf](build.md).

## Architecture

- See [Architecture](architecture.md).
- See [Command lifecycle](command-lifecycle.md) for where a command is configured and how it reaches Neovim.
