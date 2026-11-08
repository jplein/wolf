# wolf-macos/Sources/WolfMain

The app's entry module, `WolfMain`, built by `:wolf_macos_main` and linked into `:Wolf`.

## Files

#### `main.swift`

Purpose: The process entry point. It calls [`WolfApplication.run()`](../WolfMacOS/README.md#wolfapplicationswift).

**Decisions**

- It is a module of its own so that `WolfMacOS` has no `main.swift`. `WolfMacOS` is then compiled as a library with no `_main`, and a `swift_test`, which generates its own `main` and links its `alwayslink` deps whole, can link it. Folding this file back into `WolfMacOS` breaks [`WolfMacOSTests`](../../Tests/WolfMacOSTests/README.md) at link time with a duplicate `_main`.
