# Wolf

Wolf is a Markdown editor based on [Neovim](https://github.com/neovim/neovim).

Each editor window embeds one captive Neovim per project. If that Neovim crashes or stops responding, the window does not freeze silently: Wolf detects the failure (a three-state Alive / NotResponding / Dead liveness model), tells you, and restarts the captive Neovim in place — reopening the files that were open and recovering unsaved edits via Neovim's built-in swap files.

**Both front-ends surface this**, as a crash/stall overlay with a "Restart Neovim" button, and both halves of it — a Neovim that died and one that stopped responding — are reachable on each. One Linux-specific limit is worth stating plainly rather than in the small print: the Linux app restores only the set of open files, so a crash while the window was split comes back as a single pane. See [Architecture](specs/architecture.md) for the full model and its known limits.

Each editor pane keeps its own back/forward history of the files you visited in it, including navigation Neovim itself performs such as `:e` or a jump to a definition. It is per-pane rather than per-window, it is not saved between launches, and it is separate from Neovim's own cursor jumplist (`<C-o>` / `<C-i>`), which is untouched.

Both front-ends surface it as two toolbar buttons and as Back and Forward items in the window's menu, with keyboard equivalents: `⌘[` and `⌘]` on macOS, `Alt+Left` and `Alt+Right` on Linux. The macOS History menu also lists the pane's recent files so you can jump straight to one; the Linux menu carries no such list.

## System requirements

- Linux: Only NixOS is supported. The Rust core (and the GTK front-end) build with Cargo under Nix.
- macOS: The Rust core builds with Bazel (via bazelisk), using the host Xcode SDK.
- macOS app: The `wolf-macos` AppKit app requires macOS Tahoe (26) or newer and Xcode 26+. It ships its own pinned Neovim bundled inside `Wolf.app`, so it needs no separately-installed Neovim and double-clicking it works with no shell environment. See [Building Wolf](build.md).

## Building Wolf

- See [Building Wolf](build.md).
- See [macOS follow-ups](notes/macos-followups.md) for work done on Linux that still needs re-verification on a macOS host.

## Architecture

- See [Architecture](specs/architecture.md).
- See [Command lifecycle](specs/command-lifecycle.md) for where a command is configured and how it reaches Neovim.
