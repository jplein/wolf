# wolf-macos

The macOS front-end: a native AppKit application (Swift) that renders the embedded Neovim character grid and forwards keyboard input over the generated [swift-bridge](../wolf-ffi/README.md) surface. It is a thin native shell — native window chrome and a CoreText/CoreGraphics paint backend — over the unchanged, platform-agnostic [`wolf-core`](../wolf-core/README.md).

**Read [DECISIONS.md](DECISIONS.md) before changing anything in this crate.** It holds the constraints that a reasonable-looking refactor would otherwise undo.

## Layout

| Directory                                                   | Contents                                                                                                   |
| ----------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| [`Sources/WolfKeyMap/`](Sources/WolfKeyMap/README.md)       | The pure seam: Foundation and the generated FFI glue, no AppKit. Every decision testable without a window. |
| [`Sources/WolfMacOS/`](Sources/WolfMacOS/README.md)         | The AppKit application: windows, views, menus, and the coordinator.                                        |
| [`Tests/WolfKeyMapTests/`](Tests/WolfKeyMapTests/README.md) | XCTest coverage of the pure seam.                                                                          |

## Entry point

`Sources/WolfMacOS/main.swift` builds the shared `NSApplication`, installs `AppDelegate`, and runs. `AppDelegate` is the coordinator over the crate's projects and views; see [DECISIONS.md](DECISIONS.md) for that model.

## Building

```sh
bazel build //crates/wolf-macos:WolfRelease        # the optimized (-c opt) .app — build this for real use
bazel build //crates/wolf-macos:Wolf               # the development .app bundle, -Onone
bazel run   //crates/wolf-macos:WolfRelease        # launch the optimized .app
bazel run   //crates/wolf-macos:xcodeproj          # generate the (uncommitted) Xcode project
```

`:Wolf` is the fast-iteration development build (fastbuild, Swift `-Onone`, Rust unoptimized) and is what the test targets and the Xcode project build against; it is substantially slower at runtime. `:WolfRelease` is the one to build for real use, for installing, and for any performance measurement.

Resolve the release `.app` path from `bazel cquery --output=files //crates/wolf-macos:WolfRelease`, never from `bazel-bin`. See [Building Wolf](../../build.md) for the measured penalty and the exact recipe.

## Testing

```sh
bazel test //crates/wolf-macos:WolfKeyMapTests     # the pure WolfKeyMap seams
bazel test //crates/wolf-macos:bundle_layout_test  # asserts the .app resource layout
```

`WolfKeyMapTests` lists its sources explicitly rather than globbing, so a new `*Tests.swift` file that is not added to `srcs` never compiles into the test binary and never runs. There is no `cargo`-style fallback here.

The AppKit half of the crate has no automated coverage — see the testing posture in [DECISIONS.md](DECISIONS.md) for what covers it instead.

## Formatting

The advisory `swift-format` targets in [`//tools/lint`](../../tools/lint/README.md) cover this package's hand-written Swift, enumerated by the `:swift_format_srcs` filegroup here, and exclude the machine-generated bindings under [`crates/wolf-ffi/generated/`](../wolf-ffi/README.md).

```sh
bazel test //tools/lint:swift_format               # check
bazel run  //tools/lint:swift_format.fix           # format in place
```

## Bundle contents

The `.app` is self-contained, so a double-clicked bundle runs with no shell environment. Everything below is downloaded hermetically and sha256-pinned by a `//bazel` module extension, and asserted by `bundle_layout_test`.

| Bundled at `Contents/Resources/` | Source                                                                                                                             | Consumed by                                                             |
| -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `nvim/`, `init.lua`              | [`//bazel/nvim`](../../bazel/nvim/README.md), [`//runtime/nvim`](../../runtime/nvim/README.md)                                     | `wolf-core`, which launches nvim with `-u init.lua`                     |
| `orlando/`                       | [`//bazel/orlando`](../../bazel/orlando/README.md)                                                                                 | `init.lua`, which self-locates and sources it                           |
| `lighthouse/`                    | [`//bazel/lighthouse`](../../bazel/lighthouse/README.md)                                                                           | `init.lua`, which activates it as the colorscheme                       |
| `fonts/`                         | [`//bazel/jetbrains-mono`](../../bazel/jetbrains-mono/README.md), [`//bazel/ia-writer-mono`](../../bazel/ia-writer-mono/README.md) | Swift: `FontRegistration.swift` CoreText-registers every face at launch |
| `Wolf.icon/`                     | `Wolf.icon.json` plus the masters in [`resources/icons/`](../../resources/icons/README.md)                                         | macOS                                                                   |

Both font families land in the one `fonts/` directory because each TreeArtifact uses an inner `fonts/` directory and their file sets are disjoint. iA Writer Mono S is the config-driven default; JetBrains Mono is registered but non-default; Menlo is the fallback when the configured family does not resolve.

The `WOLF_NVIM_BIN` and `WOLF_INIT_LUA` development overrides are honored when exported.
