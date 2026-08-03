# wolf-macos/Sources

Swift sources for the macOS front-end, split into two modules. The split is load-bearing — see [DECISIONS.md](../DECISIONS.md).

- [`WolfKeyMap/`](WolfKeyMap/README.md) — the pure seam. Foundation and the generated FFI glue only, no AppKit, so every decision it holds is unit-testable without a window server.
- [`WolfMacOS/`](WolfMacOS/README.md) — the AppKit application. Windows, views, menus, and the coordinator.

Each directory's README owns the entries for the files in that directory, and nothing else describes them.
