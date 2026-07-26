# Wolf

Wolf is a Markdown editor based on [Neovim](https://github.com/neovim/neovim).

Each editor window embeds one captive Neovim per project. If that Neovim crashes or stops responding, the window does not freeze silently: Wolf detects the failure (a three-state Alive / NotResponding / Dead liveness model), tells you, and restarts the captive Neovim in place — reopening the files that were open and recovering unsaved edits via Neovim's built-in swap files. This is surfaced in the macOS app today (a crash/stall overlay with a "Restart Neovim" button); see [Architecture](architecture.md) for the full model and its known limits.

## System requirements

- Linux: Only NixOS is supported. The Rust core (and the GTK front-end) build with Cargo under Nix.
- macOS: The Rust core builds with Bazel (via bazelisk), using the host Xcode SDK.
- macOS app: The `wolf-macos` AppKit app requires macOS Tahoe (26) or newer and Xcode 26+. It ships its own pinned Neovim bundled inside `Wolf.app`, so it needs no separately-installed Neovim and double-clicking it works with no shell environment. See [Building Wolf](build.md).

## Building Wolf

- See [Building Wolf](build.md).

## Architecture

- See [Architecture](architecture.md).
- See [Command lifecycle](command-lifecycle.md) for where a command is configured and how it reaches Neovim.
