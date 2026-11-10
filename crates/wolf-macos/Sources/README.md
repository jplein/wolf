# wolf-macos/Sources

Swift sources for the macOS front-end, split into two load-bearing modules — see [wolf-macos-decisions.md](../../../notes/wolf-macos-decisions.md) — and a one-file entry module.

- [`WolfKeyMap/`](WolfKeyMap/README.md) — the pure seam. Foundation and the generated FFI glue only, no AppKit, so every decision it holds is unit-testable without a window server.
- [`WolfMacOS/`](WolfMacOS/README.md) — the AppKit application. Windows, views, menus, and the coordinator.
- [`WolfMain/`](WolfMain/README.md) — the process entry point, apart so that a test can link `WolfMacOS`.

Each directory's README owns the entries for the files in that directory, and nothing else describes them. A module directory's subdirectories, such as `WolfKeyMap/Terminal/`, compile into that module, and each has its own README, linked from the module's README.
