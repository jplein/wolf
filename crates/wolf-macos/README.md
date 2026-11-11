# wolf-macos

The macOS front-end: a native AppKit application (Swift) that renders the embedded Neovim character grid and forwards keyboard input over the generated [swift-bridge](../wolf-ffi/README.md) surface. It is a thin native shell — native window chrome and a CoreText/CoreGraphics paint backend — over the unchanged, platform-agnostic [`wolf-core`](../wolf-core/README.md).

**Read [wolf-macos-decisions.md](../../notes/wolf-macos-decisions.md) before changing anything in this crate.** It holds the constraints that a reasonable-looking refactor would otherwise undo.

## Layout

| Directory                                                   | Contents                                                                                                   |
| ----------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| [`Sources/WolfKeyMap/`](Sources/WolfKeyMap/README.md)       | The pure seam: Foundation and the generated FFI glue, no AppKit. Every decision testable without a window. |
| [`Sources/WolfMacOS/`](Sources/WolfMacOS/README.md)         | The AppKit application: windows, views, menus, and the coordinator.                                        |
| [`Sources/WolfMain/`](Sources/WolfMain/README.md)           | The process entry point, a module of its own so a test can link `WolfMacOS`.                               |
| [`Tests/WolfKeyMapTests/`](Tests/WolfKeyMapTests/README.md) | XCTest coverage of the pure seam.                                                                          |
| [`Tests/WolfMacOSTests/`](Tests/WolfMacOSTests/README.md)   | XCTest coverage of what `WolfMacOS` views draw, rendered offscreen.                                        |

## Entry point

`Sources/WolfMain/main.swift` calls `WolfApplication.run()`, which builds the shared `NSApplication`, installs `AppDelegate`, and runs. `AppDelegate` is the coordinator over the crate's projects and views; see [wolf-macos-decisions.md](../../notes/wolf-macos-decisions.md) for that model.

## Building

```sh
bazel build //crates/wolf-macos:WolfRelease        # the optimized (-c opt) .app — build this for real use
bazel build //crates/wolf-macos:Wolf               # the development .app bundle, -Onone
bazel run   //crates/wolf-macos:WolfRelease        # launch the optimized .app
bazel run   //crates/wolf-macos:xcodeproj          # generate the (uncommitted) Xcode project
bazel run   //crates/wolf-macos:InstallWolf        # install the optimized .app to ~/Applications
```

`:Wolf` is the fast-iteration development build (fastbuild, Swift `-Onone`, Rust unoptimized) and is what the test targets and the Xcode project build against; it is substantially slower at runtime. `:WolfRelease` is the one to build for real use, for installing, and for any performance measurement.

`:InstallWolf` builds `:WolfRelease` and installs it to `~/Applications/Wolf.app`, replacing any bundle already there. It depends on `:WolfRelease`, so it can only ever install the optimized build.

Resolve the release `.app` path from `bazel cquery --output=files //crates/wolf-macos:WolfRelease`, never from `bazel-bin`. See [Building Wolf](../../build.md) for the measured penalty and the exact recipe.

## Testing

```sh
bazel test //crates/wolf-macos:WolfKeyMapTests     # the pure WolfKeyMap seams
bazel test //crates/wolf-macos:WolfMacOSTests      # what WolfMacOS views draw, offscreen
bazel test //crates/wolf-macos:bundle_layout_test  # asserts the .app resource layout
```

Both Swift test targets list their sources explicitly rather than globbing, so a new Swift file that is not added to `srcs` never compiles into the test binary and never runs. There is no `cargo`-style fallback here.

`WolfMacOSTests` checks pixels, not events: key-window behaviour, event handling, menus and focus have no automated coverage. See the testing posture in [wolf-macos-decisions.md](../../notes/wolf-macos-decisions.md) for what covers them instead.

## Formatting

The swift-format check in [`//tools/lint`](../../tools/lint/README.md), `:swift_format`, gates `bazel test //...` over this package's hand-written Swift, enumerated by the `:swift_format_srcs` filegroup here, and excludes the machine-generated bindings under [`crates/wolf-ffi/generated/`](../wolf-ffi/README.md). The fix target, `:swift_format.fix`, is `manual` and runs only when named.

```sh
bazel test //tools/lint:swift_format                     # check
bazel run  //tools/lint:swift_format.fix                 # format in place
bazel run  //tools/lint:swift_format.fix -- <file>.swift # format only these
```

## Bundle contents

The `.app` is self-contained, so a double-clicked bundle runs with no shell environment. Everything below is downloaded hermetically and sha256-pinned by a `//bazel` module extension, and asserted by `bundle_layout_test`.

| Bundled at `Contents/Resources/` | Source                                                                                                                             | Consumed by                                                                                          |
| -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `nvim/`, `init.lua`              | [`//bazel/nvim`](../../bazel/nvim/README.md), [`//runtime/nvim`](../../runtime/nvim/README.md)                                     | `wolf-core`, which launches nvim with `-u init.lua`                                                  |
| `orlando/`                       | [`//bazel/orlando`](../../bazel/orlando/README.md)                                                                                 | `init.lua`, which self-locates and sources it                                                        |
| `lighthouse/`                    | [`//bazel/lighthouse`](../../bazel/lighthouse/README.md)                                                                           | `init.lua`, which activates it as the colorscheme                                                    |
| `rg/`                            | [`//bazel/rg`](../../bazel/rg/README.md)                                                                                           | `wolf-core`'s content search, through the `WOLF_RG_BIN` that `Project.swift` defaults to it          |
| `terminfo/`                      | [`//bazel/ghostty-vt`](../../bazel/ghostty-vt/README.md)                                                                           | `wolf-core`'s terminal sessions, through the `WOLF_TERMINFO_DIR` that `Project.swift` defaults to it |
| `fonts/`                         | [`//bazel/jetbrains-mono`](../../bazel/jetbrains-mono/README.md), [`//bazel/ia-writer-mono`](../../bazel/ia-writer-mono/README.md) | Swift: `FontRegistration.swift` CoreText-registers every face at launch                              |
| `Wolf.icon/`                     | `Wolf.icon.json` plus the masters in [`resources/icons/`](../../resources/icons/README.md)                                         | macOS                                                                                                |

Both font families land in the one `fonts/` directory because each TreeArtifact uses an inner `fonts/` directory and their file sets are disjoint. iA Writer Mono S is the config-driven default; JetBrains Mono is registered but non-default; Menlo is the fallback when the configured family does not resolve.

The `WOLF_NVIM_BIN`, `WOLF_RG_BIN`, `WOLF_TERMINFO_DIR` and `WOLF_INIT_LUA` development overrides are honored when exported.
